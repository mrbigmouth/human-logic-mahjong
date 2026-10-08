//! 策略 tenhou-normal-4p-hanchan/hand-reading 的測試：讀牌預測的方向，以及完整對局的合法性。

use mj_core::{Meld, MeldKind, Pai, TileCounts, parse_pais};
use mj_engine::{DiscardRecord, GameConfig, Player, PlayerView, run_game};
use mj_rules::riichi::RiichiConfig;
use mj_strategy::find_strategy;
use mj_strategy::framework::{Ctx, DecisionKind};
use mj_strategy::strategy_list::tenhou_normal_4p_hanchan::hand_reading::{
    memory::Memory,
    utils::predict::{hand_value, tenpai, waits},
};

const NAME: &str = "tenhou-normal-4p-hanchan/hand-reading";

fn p(s: &str) -> Pai {
    s.parse().unwrap()
}

fn view() -> PlayerView {
    let pais = parse_pais("123m456p789s1122z5z").unwrap();
    PlayerView {
        me: 0,
        round_wind: 0,
        kyoku: 0,
        honba: 0,
        riichi_sticks: 0,
        scores: [25_000; 4],
        dora_indicators: vec!["1z".parse().unwrap()],
        hand: TileCounts::from_tiles(pais.iter().map(|p| p.tile)),
        drawn: pais.last().copied(),
        hand_pais: pais,
        melds: Default::default(),
        rivers: Default::default(),
        riichi: [false; 4],
        tiles_left_in_wall: 40,
        total_kyoku: 8,
        last_discard: None,
    }
}

/// 座位 1 打出 `tiles`（`*` 結尾 = 摸切）。
fn river(v: &mut PlayerView, tiles: &[&str]) {
    for (i, t) in tiles.iter().enumerate() {
        let tsumogiri = t.ends_with('*');
        v.rivers[1].push(DiscardRecord {
            pai: p(t.trim_end_matches('*')),
            tsumogiri,
            riichi_declared: false,
            called: false,
            seq: (i * 4 + 1) as u16,
        });
    }
}

fn pon(tile: &str) -> Meld {
    Meld {
        kind: MeldKind::Pon,
        tiles: vec![p(tile); 3],
        called: Some(p(tile)),
        from: Some(0),
    }
}

/// 副露者：最近都摸切比一直手切更像聽牌；有別人立直時較不像聽牌。
#[test]
fn tenpai_follows_river_clues() {
    let mut a = view();
    a.melds[1].push(pon("6z"));
    let mut b = a.clone();
    river(
        &mut a,
        &[
            "1z", "9m", "1p", "8s", "2m", "5p", "3s*", "9p*", "4z*", "7m*",
        ],
    );
    river(
        &mut b,
        &["1z", "9m", "1p", "8s", "2m", "5p", "3s", "9p", "4z", "7m"],
    );
    let mem = Memory::default();
    let pa = tenpai::estimate(&Ctx::new(&a, DecisionKind::Discard), &mem, 1);
    let pb = tenpai::estimate(&Ctx::new(&b, DecisionKind::Discard), &mem, 1);
    assert!(pa > pb + 0.1, "摸切 {pa:.2} vs 手切 {pb:.2}");
    let mut c = a.clone();
    c.riichi[2] = true;
    let pc = tenpai::estimate(&Ctx::new(&c, DecisionKind::Discard), &mem, 1);
    assert!(pc < pa);
}

/// 聽牌張：現物 0；筋比無筋安全；寶牌比一般牌危險。
#[test]
fn waits_follow_safety_clues() {
    let mut v = view();
    river(&mut v, &["4m", "9p", "1s", "2z"]);
    v.riichi[1] = true;
    let ctx = Ctx::new(&v, DecisionKind::Discard);
    let mem = Memory::default();
    let w = |t: &str| waits::estimate(&ctx, &mem, 1, t.parse().unwrap());
    assert_eq!(w("4m"), 0.0);
    assert!(w("1m") < w("1p"), "筋 1m 應比無筋 1p 安全");
    assert!(w("7m") < w("7p"));
    // 寶牌：指示牌 4s → 寶牌 5s，比同位置的 5p 危險。
    let mut d = v.clone();
    d.dora_indicators = vec!["4s".parse().unwrap()];
    let ctx_d = Ctx::new(&d, DecisionKind::Discard);
    let dora = waits::estimate(&ctx_d, &mem, 1, "5s".parse().unwrap());
    let plain = waits::estimate(&ctx_d, &mem, 1, "5p".parse().unwrap());
    assert!(dora > plain, "寶牌 5s {dora:.3} 應比 5p {plain:.3} 危險");
}

/// 打點：副露中有寶牌刻子的人和牌多半是滿貫；莊家的點數是 1.5 倍。
#[test]
fn hand_value_reads_open_dora() {
    let mut v = view();
    v.dora_indicators = vec!["5z".parse().unwrap()]; // 寶牌 6z（發）
    v.melds[1].push(pon("6z"));
    let mem = Memory::default();
    let ctx = Ctx::new(&v, DecisionKind::Discard);
    let (points, mangan) = hand_value::estimate(&ctx, &mem, 1);
    assert!(mangan > 0.5);
    let mut cheap = view();
    cheap.melds[1].push(pon("7z"));
    let ctx_c = Ctx::new(&cheap, DecisionKind::Discard);
    let (cheap_points, _) = hand_value::estimate(&ctx_c, &mem, 1);
    assert!(points > cheap_points);
    // 第 2 局（kyoku = 1）座位 1 是莊家
    let mut dealer = cheap.clone();
    dealer.kyoku = 1;
    let ctx_d = Ctx::new(&dealer, DecisionKind::Discard);
    let (dealer_points, _) = hand_value::estimate(&ctx_d, &mem, 1);
    assert!((dealer_points - cheap_points * 1.5).abs() < 1e-6);
}

/// 四家都用這個策略打完整場：引擎檢查每個動作都合法。
#[test]
fn plays_full_games_legally() {
    let st = find_strategy(NAME).unwrap();
    for seed in 0..3 {
        let mut players: [Box<dyn Player>; 4] =
            std::array::from_fn(|_| st.player(false, None) as Box<dyn Player>);
        let r = run_game(
            &GameConfig::new(RiichiConfig::TENHOU, seed),
            &mut players,
            false,
        )
        .expect("all legal");
        assert_eq!(r.scores.iter().sum::<i32>(), 100_000);
    }
}
