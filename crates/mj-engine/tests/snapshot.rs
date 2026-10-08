//! 局面快照的測試：快照後繼續打必須與不中斷的對局完全一致；重抽看不到的牌不能改變決策者的視角。

use mj_engine::{
    Action, GameConfig, Player, PlayerView, Snapshot, Until, capture_snapshot, resume_game,
    run_game_with,
};
use mj_rules::riichi::RiichiConfig;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

/// 決定性的簡單玩家：能和就和、能立直就立直、打第一張可打的牌、不鳴牌。
#[derive(Clone, Default)]
struct Simple;

impl Player for Simple {
    fn act(&mut self, _view: &PlayerView, legal: &[Action]) -> Action {
        for a in [Action::Tsumo, Action::Ron] {
            if legal.contains(&a) {
                return a;
            }
        }
        if let Some(a) = legal.iter().find(|a| matches!(a, Action::Riichi { .. })) {
            return a.clone();
        }
        if let Some(a) = legal.iter().find(|a| matches!(a, Action::Discard { .. })) {
            return a.clone();
        }
        Action::Pass
    }
}

fn refs(ps: &mut [Simple; 4]) -> [&mut dyn Player; 4] {
    ps.each_mut().map(|p| p as &mut dyn Player)
}

fn cfg(seed: u64) -> GameConfig {
    GameConfig::new(RiichiConfig::TENHOU, seed)
}

/// 從快照繼續打完整場，最終結果必須和不中斷的對局完全相同。
#[test]
fn resume_reproduces_the_original_game() {
    for seed in [1, 2, 3] {
        let mut ps: [Simple; 4] = Default::default();
        let full = run_game_with(&cfg(seed), refs(&mut ps), false).unwrap();
        for (seat, ask) in [(0, 0), (1, 5), (2, 40), (3, 120)] {
            let mut ps: [Simple; 4] = Default::default();
            let Some(snap) = capture_snapshot(&cfg(seed), refs(&mut ps), seat, ask).unwrap() else {
                continue;
            };
            assert_eq!(snap.asked_seat(), seat);
            let r = resume_game(&cfg(seed), &snap, refs(&mut ps), false, Until::GameEnd).unwrap();
            assert_eq!(r.scores, full.scores, "seed {seed}, seat {seat}, ask {ask}");
            assert_eq!(r.hands_played, full.hands_played);
        }
    }
}

/// 找出一個「有對手已立直」的快照（多個測試共用，只搜尋一次）。
fn snapshot_with_riichi_opponent() -> Snapshot {
    static CACHE: std::sync::OnceLock<Snapshot> = std::sync::OnceLock::new();
    CACHE
        .get_or_init(find_snapshot_with_riichi_opponent)
        .clone()
}

fn find_snapshot_with_riichi_opponent() -> Snapshot {
    for seed in 0..50 {
        for ask in (20..200).step_by(7) {
            let mut ps: [Simple; 4] = Default::default();
            if let Some(s) = capture_snapshot(&cfg(seed), refs(&mut ps), 0, ask).unwrap() {
                let v = s.view();
                if v.riichi[1..].iter().any(|&r| r) {
                    return s;
                }
            }
        }
    }
    panic!("no snapshot with a riichi opponent found");
}

/// 重抽後決策者的視角完全不變，而且可以繼續打完（引擎的守恆檢查會驗證牌數與點數）。
#[test]
fn determinize_keeps_the_deciders_view() {
    let snap = snapshot_with_riichi_opponent();
    let before = serde_json::to_string(&snap.view()).unwrap();
    for r in 0..20 {
        let mut s = snap.clone();
        let report = s.determinize(&mut ChaCha8Rng::seed_from_u64(r));
        assert_eq!(report.riichi_fallbacks, 0);
        assert_eq!(serde_json::to_string(&s.view()).unwrap(), before);
        let mut ps: [Simple; 4] = Default::default();
        // 新的玩家詢問次數從 0 開始，這裡只確認重抽後的局面能正常打完這一局。
        resume_game(&cfg(r), &s, refs(&mut ps), false, Until::HandEnd).expect("plays on");
    }
}

/// 重抽通常會改變對手的手牌（確認真的有重抽，而不是原封不動）。
#[test]
fn determinize_changes_hidden_tiles() {
    let snap = snapshot_with_riichi_opponent();
    let mut a = snap.clone();
    let mut b = snap.clone();
    a.determinize(&mut ChaCha8Rng::seed_from_u64(1));
    b.determinize(&mut ChaCha8Rng::seed_from_u64(2));
    let mut pa: [Simple; 4] = Default::default();
    let mut pb: [Simple; 4] = Default::default();
    let ra = resume_game(&cfg(9), &a, refs(&mut pa), true, Until::HandEnd).unwrap();
    let rb = resume_game(&cfg(9), &b, refs(&mut pb), true, Until::HandEnd).unwrap();
    assert_ne!(
        serde_json::to_string(&ra.log).unwrap(),
        serde_json::to_string(&rb.log).unwrap()
    );
}

/// 能吃就吃、但不碰的玩家（其餘同 `Simple`）：讓回應階段中常有「下家已決定吃、接著詢問對家要不要碰」，
/// 而對家不碰，所以下家的吃會真的執行。
#[derive(Clone, Default)]
struct Caller;

impl Player for Caller {
    fn act(&mut self, view: &PlayerView, legal: &[Action]) -> Action {
        if let Some(a) = legal.iter().find(|a| matches!(a, Action::Chi { .. })) {
            return a.clone();
        }
        Simple.act(view, legal)
    }
}

/// 在回應階段保存的快照：之前的回應者已依真實手牌決定吃牌，重抽後要依新手牌重新詢問，
/// 不能沿用舊的決定（否則會用到已經不在手上的牌）。
#[test]
fn determinize_reasks_earlier_responders() {
    let mut checked = 0;
    for seed in 0..12 {
        for seat in 0..4 {
            for ask in (0..80).step_by(3) {
                let mut ps: [Caller; 4] = Default::default();
                let refs = ps.each_mut().map(|p| p as &mut dyn Player);
                let Some(snap) = capture_snapshot(&cfg(seed), refs, seat, ask).unwrap() else {
                    continue;
                };
                if snap.view().last_discard.is_none() {
                    continue;
                }
                for r in 0..3 {
                    let mut s = snap.clone();
                    s.determinize(&mut ChaCha8Rng::seed_from_u64(r));
                    let mut ps: [Caller; 4] = Default::default();
                    let refs = ps.each_mut().map(|p| p as &mut dyn Player);
                    resume_game(&cfg(seed), &s, refs, false, Until::HandEnd).expect("plays on");
                    checked += 1;
                }
            }
        }
    }
    assert!(checked > 100, "only {checked} response snapshots");
}
