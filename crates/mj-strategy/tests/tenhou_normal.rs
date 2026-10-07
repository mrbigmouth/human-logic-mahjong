//! 策略 tenhou-normal-4p-hanchan 的測試：各決策的判斷，以及完整對局的合法性。

use mj_core::{Pai, Tile, TileCounts, parse_pais};
use mj_engine::{DiscardRecord, GameConfig, Player, PlayerView, run_game};
use mj_rules::riichi::RiichiConfig;
use mj_strategy::framework::{CallChoice, Ctx, DecisionKind};
use mj_strategy::strategy_list::tenhou_normal_4p_hanchan::{decision, memory::Memory};
use mj_strategy::{all_strategies, find_strategy};
use rand::SeedableRng;
use rand::seq::SliceRandom;
use rand_chacha::ChaCha8Rng;

const NAME: &str = "tenhou-normal-4p-hanchan";

fn view(hand: &str) -> PlayerView {
    let pais = parse_pais(hand).unwrap();
    PlayerView {
        me: 1,
        round_wind: 0,
        kyoku: 0,
        honba: 0,
        riichi_sticks: 0,
        scores: [25_000; 4],
        dora_indicators: vec![],
        hand: TileCounts::from_tiles(pais.iter().map(|p| p.tile)),
        drawn: pais.last().copied(),
        hand_pais: pais,
        melds: Default::default(),
        rivers: Default::default(),
        riichi: [false; 4],
        tiles_left_in_wall: 60,
        total_kyoku: 8,
        last_discard: None,
    }
}

fn distinct(v: &PlayerView) -> Vec<Pai> {
    let mut out: Vec<Pai> = Vec::new();
    for &p in &v.hand_pais {
        if !out.contains(&p) {
            out.push(p);
        }
    }
    out
}

fn p(s: &str) -> Pai {
    s.parse().unwrap()
}

/// build.rs 自動產生的策略清單包含這個策略。
#[test]
fn strategy_is_registered() {
    assert!(all_strategies().iter().any(|s| s.name() == NAME));
}

/// 何切一致性：沒有寶牌、沒人立直時，選的打法必須符合牌理定義（向聽最低、其中進張最多）。
#[test]
fn discards_follow_tile_efficiency() {
    let mut rng = ChaCha8Rng::seed_from_u64(7);
    let mut checked = 0;
    for _ in 0..800 {
        let mut wall: Vec<Tile> = Tile::all().flat_map(|t| [t; 4]).collect();
        wall.shuffle(&mut rng);
        let hand: String = wall[..14].iter().map(|t| t.to_string()).collect();
        let v = view(&hand);
        if mj_calc::is_agari(&v.hand) {
            continue;
        }
        let ctx = Ctx::new(&v, DecisionKind::Discard).with_discards(distinct(&v));
        let chosen = decision::discard::decide(&ctx, &mut Memory::default());
        let opts = mj_tools::hand::discard_options(&v, &v.hand);
        let mine = opts.iter().find(|o| o.discard == chosen.tile).unwrap();
        assert_eq!(
            (mine.after.shanten, mine.after.total),
            (opts[0].after.shanten, opts[0].after.total),
            "hand {hand}"
        );
        checked += 1;
    }
    assert!(checked > 700);
}

/// 對手立直、自己 2 向聽以上 → 棄和打現物。
#[test]
fn folds_against_riichi() {
    let mut v = view("147m258p369s1234z5z");
    v.riichi[2] = true;
    v.rivers[2].push(DiscardRecord {
        pai: p("9s"),
        tsumogiri: false,
        riichi_declared: true,
        called: false,
        seq: 0,
    });
    let ctx = Ctx::new(&v, DecisionKind::Discard).with_discards(distinct(&v));
    let chosen = decision::discard::decide(&ctx, &mut Memory::default());
    assert_eq!(chosen, p("9s"));
    assert!(ctx.notes()[0].contains("棄和"));
}

/// 聽牌張只剩 1 張時默聽；4 張時立直。
#[test]
fn riichi_needs_enough_waits() {
    let v = view("123m456p789s11225z");
    let ctx = Ctx::new(&v, DecisionKind::Riichi).with_riichi_discard(p("5z"));
    assert!(decision::riichi::decide(&ctx, &mut Memory::default()));
    let mut v = view("123m456p789s11225z");
    // 對碰聽 1z、2z。讓寶牌指示牌露出 1z 兩張、2z 一張 → 聽牌張只剩 1 張。
    v.dora_indicators = parse_pais("1z1z2z")
        .unwrap()
        .iter()
        .map(|p| p.tile)
        .collect();
    let ctx = Ctx::new(&v, DecisionKind::Riichi).with_riichi_discard(p("5z"));
    assert!(!decision::riichi::decide(&ctx, &mut Memory::default()));
}

/// 鳴了沒有役就不鳴；碰役牌讓向聽前進就鳴。
#[test]
fn calls_need_yaku_and_progress() {
    let mut v = view("99m123m456p78s19s1z");
    v.last_discard = Some((0, p("9m")));
    let pon = CallChoice::Pon([p("9m"), p("9m")]);
    let ctx = Ctx::new(&v, DecisionKind::Call).with_calls(vec![CallChoice::Pass, pon]);
    assert_eq!(
        decision::call::decide(&ctx, &mut Memory::default()),
        CallChoice::Pass
    );

    let mut v = view("77z123m456p78s19s1z");
    v.last_discard = Some((0, p("7z")));
    let pon = CallChoice::Pon([p("7z"), p("7z")]);
    let ctx = Ctx::new(&v, DecisionKind::Call).with_calls(vec![CallChoice::Pass, pon]);
    assert_eq!(decision::call::decide(&ctx, &mut Memory::default()), pon);
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

/// 有記錄時，每個決策都在選項範圍內、沒有不合法的回傳值。
#[test]
fn records_are_valid() {
    let st = find_strategy(NAME).unwrap();
    let mut players: Vec<_> = (0..4).map(|_| st.player(true, None)).collect();
    let refs: [&mut dyn Player; 4] = {
        let mut it = players.iter_mut().map(|p| p.as_mut() as &mut dyn Player);
        std::array::from_fn(|_| it.next().unwrap())
    };
    mj_engine::run_game_with(&GameConfig::new(RiichiConfig::TENHOU, 11), refs, false).unwrap();
    let records: Vec<_> = players.iter_mut().flat_map(|p| p.take_records()).collect();
    assert!(records.len() > 100);
    assert!(
        records
            .iter()
            .all(|r| !r.invalid && r.chosen < r.options.len())
    );
    assert!(
        records
            .iter()
            .any(|r| r.kind == DecisionKind::Discard && !r.notes.is_empty())
    );
}
