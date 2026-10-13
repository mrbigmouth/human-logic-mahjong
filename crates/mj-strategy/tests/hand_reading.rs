//! 策略 tenhou-normal-4p-hanchan/hand-reading 的測試：讀牌預測的方向，以及完整對局的合法性。

use mj_core::{Meld, MeldKind, Pai, TileCounts, parse_pais};
use mj_engine::{DiscardRecord, GameConfig, Player, PlayerView, run_game};
use mj_rules::riichi::RiichiConfig;
use mj_strategy::find_strategy;
use mj_strategy::framework::{Ctx, DecisionKind};
use mj_strategy::strategy_list::tenhou_normal_4p_hanchan::hand_reading::{
    decision,
    memory::{Memory, Reveals},
    utils::image,
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

/// 可調參數：建置腳本自動收集、名稱不重複；覆寫會改變判斷；不存在的名稱會報錯；`名稱@覆寫` 可解析。
#[test]
fn params_are_collected_and_overridable() {
    use mj_strategy::framework::ParamSet;
    use std::sync::Arc;
    let st = find_strategy(NAME).unwrap();
    let names: Vec<&str> = st.params().iter().map(|p| p.name).collect();
    for n in [
        "tenpai.per_recent_tedashi",
        "win.later_ukeire",
        "push.later_ukeire",
        "call.far_shanten",
    ] {
        assert!(names.contains(&n), "缺少參數 {n}：{names:?}");
    }
    assert!(
        st.with_params(ParamSet::parse("no.such=1").unwrap())
            .is_err()
    );

    let mut v = view();
    v.melds[1].push(pon("6z"));
    river(
        &mut v,
        &["1z", "9m", "1p", "8s", "2m", "5p", "3s", "9p", "4z", "7m"],
    );
    let mem = Memory::default();
    let base = tenpai::estimate(&Ctx::new(&v, DecisionKind::Discard), &mem, 1);
    let more = ParamSet::parse("tenpai.per_recent_tedashi=1.0").unwrap();
    let ctx = Ctx::new(&v, DecisionKind::Discard).with_params(Arc::new(more));
    assert!(tenpai::estimate(&ctx, &mem, 1) > base);

    let spec = format!("{NAME}@call.far_shanten=3;riichi.min_waits=1");
    let tuned = mj_strategy::find_strategy_spec(&spec).unwrap();
    let far = st
        .params()
        .iter()
        .find(|p| p.name == "call.far_shanten")
        .unwrap();
    assert_eq!(tuned.overrides().get(far), 3.0);
    assert_eq!(st.overrides().get(far), far.default);
}

/// 自己（座位 0）打出 `tiles`（`*` 結尾 = 摸切）。
fn my_river(v: &mut PlayerView, tiles: &[&str]) {
    for (i, t) in tiles.iter().enumerate() {
        v.rivers[0].push(DiscardRecord {
            pai: p(t.trim_end_matches('*')),
            tsumogiri: t.ends_with('*'),
            riichi_declared: false,
            called: false,
            seq: (i * 4) as u16,
        });
    }
}

fn hand(v: &mut PlayerView, tiles: &str) {
    let pais = parse_pais(tiles).unwrap();
    v.hand = TileCounts::from_tiles(pais.iter().map(|p| p.tile));
    v.drawn = pais.last().copied();
    v.hand_pais = pais;
}

/// 手切摸切：聽牌時選「別人眼中較不像聽牌」的打法，離聽牌還遠時選較像聽牌的。
#[test]
fn tedashi_chooses_the_image() {
    let mut v = view();
    v.melds[0].push(pon("6z"));
    // 副露 6z 之後：123m456p 11z 22z ＋摸 2z → 打 2z 聽 1z/2z 對碰。
    hand(&mut v, "123m456p1122z2z");
    my_river(&mut v, &["9m", "1p*", "8s*", "7s", "9s*", "3z*"]);
    let mut mem = Memory::default();
    let ctx = Ctx::new(&v, DecisionKind::Tedashi).with_tedashi(p("2z"), false);
    let tsumo = image::apparent_tenpai(&ctx, &mem, p("2z"), true, false);
    let teda = image::apparent_tenpai(&ctx, &mem, p("2z"), false, false);
    assert!(teda < tsumo, "手切 {teda:.3} 應比摸切 {tsumo:.3} 不像聽牌");
    assert!(decision::tedashi::decide(&ctx, &mut mem));
    let notes = ctx.notes();
    assert!(notes[0].contains("看起來不像聽牌"), "{notes:?}");

    // 離聽牌還遠：選像聽牌的（摸切）。
    let mut far = v.clone();
    hand(&mut far, "159m258p1357s2z2z");
    let ctx = Ctx::new(&far, DecisionKind::Tedashi).with_tedashi(p("2z"), false);
    assert!(!decision::tedashi::decide(&ctx, &mut mem));
    let notes = ctx.notes();
    assert!(notes[0].contains("像聽牌"), "{notes:?}");

    // 關掉印象操作：一律摸切、不寫理由。
    let mut off = mj_strategy::framework::ParamSet::default();
    off.set(decision::tedashi::ENABLED.name, 0.0);
    let ctx = Ctx::new(&v, DecisionKind::Tedashi)
        .with_tedashi(p("2z"), false)
        .with_params(std::sync::Arc::new(off));
    assert!(!decision::tedashi::decide(&ctx, &mut mem));
}

/// 迷彩：兩種單騎聽牌張數相同時，聽自己牌河的筋（別人眼中較安全）。
#[test]
fn meisai_prefers_a_wait_that_looks_safe() {
    let mut v = view();
    // 123m456p789s111z 加 4m、摸 4p：打 4p 單騎 4m（1m、7m 都在牌河 → 4m 是筋），打 4m 單騎 4p。
    hand(&mut v, "123m456p789s111z4m4p");
    my_river(&mut v, &["1m", "9p", "7m", "2s"]);
    let mut mem = Memory::default();
    let ctx = Ctx::new(&v, DecisionKind::Discard);
    let d4m = image::apparent_wait_danger(&ctx, &mem, p("4m"), true).unwrap();
    let d4p = image::apparent_wait_danger(&ctx, &mem, p("4p"), true).unwrap();
    assert!(
        d4p < d4m,
        "聽筋 4m（打 4p）{d4p:.3} 應比聽 4p（打 4m）{d4m:.3} 安全"
    );
    let ctx = Ctx::new(&v, DecisionKind::Discard).with_discards(vec![p("4m"), p("4p")]);
    assert_eq!(decision::discard::decide(&ctx, &mut mem), p("4p"));
    let notes = ctx.notes();
    assert!(notes.iter().any(|n| n.contains("迷彩")), "{notes:?}");
}

/// 反詐騙：流局時常常「看起來沒聽卻聽了」的人，聽牌估計提高；常常「看起來聽了卻沒聽」的人降低。
#[test]
fn reveals_calibrate_tenpai_reading() {
    let mut v = view();
    v.melds[1].push(pon("6z"));
    river(&mut v, &["1z", "9m", "1p", "8s", "2m", "5p", "3s"]);
    let ctx = Ctx::new(&v, DecisionKind::Discard);
    let base = tenpai::estimate(&ctx, &Memory::default(), 1);
    let with = |expected: f64, actual: f64| {
        let mut mem = Memory::default();
        mem.reveals[1] = Reveals {
            n: 3,
            expected,
            actual,
        };
        tenpai::estimate(&ctx, &mem, 1)
    };
    assert!(with(0.6, 3.0) > base, "藏聽牌的人應提高");
    assert!(with(2.4, 0.0) < base, "虛張聲勢的人應降低");
    assert!((with(1.5, 1.5) - base).abs() < 1e-9, "估計準確時不變");
}

/// 荒牌流局時，沒立直的各家公開的聽牌狀態與我最後一次的估計記進記憶；立直者與沒估計過的不列入。
#[test]
fn exhaustive_draw_records_reveals() {
    use mj_engine::{DrawReason, Event, HandEnd};
    use mj_strategy::strategy_list::tenhou_normal_4p_hanchan::hand_reading::memory;
    let mut mem = Memory::default();
    let v = view();
    memory::think(&Ctx::new(&v, DecisionKind::Predict), &mut mem);
    assert!(mem.tenpai_guess[0].is_none() && mem.tenpai_guess[1].is_some());
    memory::observe(0, &Event::RiichiAccepted { seat: 3 }, &mut mem);
    let end = Event::HandEnd {
        end: HandEnd::Draw {
            reason: DrawReason::Exhaustive,
            tenpai: [false, true, false, true],
            nagashi: vec![],
        },
        deltas: [0; 4],
    };
    memory::observe(0, &end, &mut mem);
    assert_eq!(mem.reveals[1].n, 1);
    assert_eq!(mem.reveals[1].actual, 1.0);
    assert_eq!(mem.reveals[2].actual, 0.0);
    assert_eq!(mem.reveals[0].n, 0, "自己不列入");
    assert_eq!(mem.reveals[3].n, 0, "立直者不列入");
    // 新的一局：本局的估計清掉，對照紀錄保留。
    let start = Event::StartKyoku {
        round_wind: 0,
        kyoku: 1,
        honba: 0,
        riichi_sticks: 0,
        scores: [25_000; 4],
        dora_indicator: "1z".parse().unwrap(),
        hands: Default::default(),
    };
    memory::observe(0, &start, &mut mem);
    assert!(mem.tenpai_guess[1].is_none());
    assert_eq!(mem.reveals[1].n, 1);
}
