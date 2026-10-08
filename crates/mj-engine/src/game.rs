//! 一整場對局（半莊或東風戰）：一局接一局進行，直到符合結束條件。
//!
//! 每局結束後：
//! - 本場：非莊家和牌時歸零，其餘（莊家和牌、流局）加一；
//! - 連莊時局數不變，否則進到下一局（東四之後是南一……）；
//! - 判斷是否結束：擊飛、最終局（南四）以後有人達到返點、西入的最後一局。

use crate::Player;
use crate::Seat;
use crate::event::Event;
use crate::kyoku::{EngineError, Halt, Kyoku, KyokuSetup, Table};
use crate::snapshot::Snapshot;
use crate::wall::{Wall, hand_seed};
use mj_core::Pai;
use mj_rules::riichi::RiichiConfig;
use serde::{Deserialize, Serialize};

/// 對局設定。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameConfig {
    /// 規則選項
    pub rules: RiichiConfig,
    /// 對局 seed：每一手的牌山都由它推導而來，同一個 seed 會重現同一場牌局。
    pub seed: u64,
    /// 前幾手改用指定的牌山，而不是由 seed 洗牌（測試、重播牌譜用）。
    #[serde(default)]
    pub preset_walls: Vec<Vec<Pai>>,
    /// 打完這麼多手就結束（測試、單局練習用）。
    #[serde(default)]
    pub max_hands: Option<u32>,
}

impl GameConfig {
    /// 一般對局設定（不指定牌山、不限手數）。
    pub fn new(rules: RiichiConfig, seed: u64) -> Self {
        Self {
            rules,
            seed,
            preset_walls: Vec::new(),
            max_hands: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameResult {
    /// 最終點數，依座位排列（座位 0 是起家，即東一局的莊家）。
    pub scores: [i32; 4],
    /// 各座位的名次，0 = 第一名。同分時離起家近的座位名次較前（上家優先）。
    pub ranks: [u8; 4],
    pub hands_played: u32,
    /// 完整的事件紀錄（牌譜）；呼叫時沒有要求就是空的。
    pub log: Vec<Event>,
    /// 每一局使用的牌山（與 `log` 一起保留；可用來重播或做反事實模擬）
    pub walls: Vec<Vec<Pai>>,
}

/// 進行一整場對局。`players[0]` 是起家。`keep_log` = 是否保留完整牌譜與牌山。
pub fn run_game(
    cfg: &GameConfig,
    players: &mut [Box<dyn Player>; 4],
    keep_log: bool,
) -> Result<GameResult, EngineError> {
    run_game_with(
        cfg,
        players.each_mut().map(|p| p.as_mut() as &mut dyn Player),
        keep_log,
    )
}

/// 同 [`run_game`]，但只借用玩家；對局結束後呼叫端還能取回玩家的狀態（例如決策紀錄）。
pub fn run_game_with(
    cfg: &GameConfig,
    players: [&mut dyn Player; 4],
    keep_log: bool,
) -> Result<GameResult, EngineError> {
    let mut table = Table::new(players, keep_log, None);
    match play(cfg, &mut table, Start::Fresh) {
        Ok(r) => Ok(r),
        Err(Halt::Error(e)) => Err(e),
        Err(Halt::Snapshot(_) | Halt::Unsupported) => unreachable!("no snapshot target was set"),
    }
}

/// 重播一場對局，在「即將第 `ask_index` 次詢問座位 `seat`」時保存局面並停止。
/// 玩家的狀態（例如對局記憶）停在詢問之前，呼叫端可以接著複製這些玩家來繼續打。
/// 對局在那之前就結束，或詢問點落在不支援繼續的位置（搶槓）時回傳 `None`。
pub fn capture_snapshot(
    cfg: &GameConfig,
    players: [&mut dyn Player; 4],
    seat: Seat,
    ask_index: u32,
) -> Result<Option<Snapshot>, EngineError> {
    let mut table = Table::new(players, false, Some((seat, ask_index)));
    match play(cfg, &mut table, Start::Fresh) {
        Ok(_) | Err(Halt::Unsupported) => Ok(None),
        Err(Halt::Snapshot(s)) => Ok(Some(*s)),
        Err(Halt::Error(e)) => Err(e),
    }
}

/// 從快照繼續要打到哪裡。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Until {
    /// 打完這一局就停止（結果的點數不含整場結束時的供託分配）
    HandEnd,
    /// 打完整場對局（之後的牌山由 `cfg.seed` 決定）
    GameEnd,
}

/// 從快照繼續打。玩家應該是保存快照時玩家狀態的複本（詢問次數、記憶都要接得上）。
/// 回傳的 `log` 只包含快照之後的事件。
pub fn resume_game(
    cfg: &GameConfig,
    snapshot: &Snapshot,
    players: [&mut dyn Player; 4],
    keep_log: bool,
    until: Until,
) -> Result<GameResult, EngineError> {
    let mut table = Table::new(players, keep_log, None);
    match play(cfg, &mut table, Start::Resume(snapshot, until)) {
        Ok(r) => Ok(r),
        Err(Halt::Error(e)) => Err(e),
        Err(Halt::Snapshot(_) | Halt::Unsupported) => unreachable!("no snapshot target was set"),
    }
}

/// 對局從哪裡開始。
enum Start<'s> {
    /// 新的一場
    Fresh,
    /// 從快照繼續
    Resume(&'s Snapshot, Until),
}

/// 對局主迴圈。
fn play(cfg: &GameConfig, table: &mut Table, start: Start) -> Result<GameResult, Halt> {
    let rules = &cfg.rules;
    let keep_log = table.log.is_some();
    let mut walls = Vec::new();

    // 正規局數：半莊 8 局（東 4 南 4），東風戰 4 局。
    let regular_hands: u8 = if rules.hanchan { 8 } else { 4 };
    // index = 局序（0 = 東一、4 = 南一、8 = 西一）；honba = 本場；sticks = 供託。
    let (mut scores, mut index, mut honba, mut sticks, mut hands_played);
    let (mut pending, until) = match start {
        Start::Fresh => {
            table.emit(Event::StartGame { seed: cfg.seed });
            (scores, index, honba, sticks, hands_played) =
                ([rules.starting_points; 4], 0u8, 0u8, 0u8, 0u32);
            (None, Until::GameEnd)
        }
        Start::Resume(snap, until) => {
            let su = &snap.state.setup;
            (scores, index, honba, sticks, hands_played) = (
                su.scores,
                su.round_wind * 4 + su.kyoku,
                su.honba,
                su.riichi_sticks,
                su.hands_played,
            );
            (Some(snap), until)
        }
    };

    loop {
        let dealer = index % 4;
        let out = match pending.take() {
            Some(snap) => Kyoku::resume(snap.state.clone(), rules, table, snap.resume.clone())?,
            None => {
                let setup = KyokuSetup {
                    round_wind: index / 4,
                    kyoku: index % 4,
                    honba,
                    riichi_sticks: sticks,
                    scores,
                    total_kyoku: regular_hands,
                    hands_played,
                };
                // 第 N 手的牌山只由 (seed, N) 決定，與打法無關（duplicate 對戰需要）。
                let wall = match cfg.preset_walls.get(hands_played as usize) {
                    Some(tiles) => Wall::from_tiles(tiles.clone()),
                    None => Wall::shuffled(hand_seed(cfg.seed, hands_played), rules.red_fives),
                };
                if keep_log {
                    walls.push(wall.tiles().to_vec());
                }
                Kyoku::new(setup, rules, wall, table).run()?
            }
        };
        hands_played += 1;
        scores = out.scores;
        sticks = out.riichi_sticks;
        // 閒家和牌時本場歸零；莊家和牌或流局時本場加一。
        let dealer_won = out.winners.contains(&dealer);
        honba = if !out.winners.is_empty() && !dealer_won {
            0
        } else {
            honba.saturating_add(1)
        };

        if until == Until::HandEnd {
            return Ok(GameResult {
                scores,
                ranks: ranking(&scores),
                hands_played,
                log: table.log.take().unwrap_or_default(),
                walls,
            });
        }
        let ended = cfg.max_hands.is_some_and(|m| hands_played >= m)
            || match_over(
                rules,
                index,
                &scores,
                dealer,
                out.dealer_continues,
                out.aborted,
            );
        if ended {
            // 場上剩下的供託歸第一名。
            let top = ranking(&scores).iter().position(|&r| r == 0).unwrap();
            scores[top] += 1000 * i32::from(sticks);
            table.emit(Event::EndGame { scores });
            return Ok(GameResult {
                scores,
                ranks: ranking(&scores),
                hands_played,
                log: table.log.take().unwrap_or_default(),
                walls,
            });
        }
        // 沒有連莊就輪到下一位當莊（進到下一局）。
        if !out.dealer_continues {
            index += 1;
        }
    }
}

/// 第 `index` 局（0 = 東一局）結束、點數為 `scores` 時，對局是否結束。
///
/// - 擊飛：有人低於 0 點立刻結束。
/// - 最終局（半莊為南四局）以後，有人達到返點（30000）就結束；
///   但莊家連莊且不是第一名時繼續（莊家有權繼續追分）。莊家和牌或流局聽牌而連莊、且為第一名時，
///   和了止め（agari-yame，天鳳也包含聽牌止め）讓對局結束；途中流局（`aborted`）的連莊不適用。
/// - 延長的風場（西入）是突然死亡制，最後一局（西四）結束時一定結束（莊家連莊除外）。
fn match_over(
    rules: &RiichiConfig,
    index: u8,
    scores: &[i32; 4],
    dealer: u8,
    dealer_continues: bool,
    aborted: bool,
) -> bool {
    let regular: u8 = if rules.hanchan { 8 } else { 4 };
    if rules.bust_ends_game && scores.iter().any(|&s| s < 0) {
        return true;
    }
    let last_phase = index + 1 >= regular;
    let reached = scores.iter().any(|&s| s >= rules.target_points);
    let dealer_top = ranking(scores)[usize::from(dealer)] == 0;
    if last_phase && reached && (!dealer_continues || (rules.agari_yame && dealer_top && !aborted))
    {
        return true;
    }
    !dealer_continues && index + 1 >= regular + 4
}

/// 計算各座位名次：點數高者在前，同分時座位編號小（離起家近）者在前。
pub fn ranking(scores: &[i32; 4]) -> [u8; 4] {
    let mut order: Vec<usize> = (0..4).collect();
    order.sort_by_key(|&s| (-scores[s], s));
    let mut ranks = [0u8; 4];
    for (r, &s) in order.iter().enumerate() {
        ranks[s] = r as u8;
    }
    ranks
}

#[cfg(test)]
mod tests {
    use super::*;

    const R: RiichiConfig = RiichiConfig::TENHOU;
    /// 南四局的局序
    const S4: u8 = 7;
    /// 西四局的局序
    const W4: u8 = 11;

    /// 各種結束條件。
    #[test]
    fn end_conditions() {
        // 還不是最終局：即使大幅領先也繼續。
        assert!(!match_over(
            &R,
            3,
            &[45_000, 20_000, 20_000, 15_000],
            3,
            false,
            false
        ));
        // 南四局，有人超過 30000 → 結束。
        assert!(match_over(
            &R,
            S4,
            &[35_000, 25_000, 20_000, 20_000],
            3,
            false,
            false
        ));
        // 南四局，沒人達到 30000 → 西入。
        assert!(!match_over(
            &R,
            S4,
            &[28_000, 26_000, 24_000, 22_000],
            3,
            false,
            false
        ));
        // 莊家連莊且為第一名、超過 30000 → 和了止め，結束。
        assert!(match_over(
            &R,
            S4,
            &[20_000, 20_000, 25_000, 35_000],
            3,
            true,
            false
        ));
        // 莊家連莊但不是第一名 → 莊家繼續。
        assert!(!match_over(
            &R,
            S4,
            &[32_000, 20_000, 23_000, 25_000],
            3,
            true,
            false
        ));
        // 西四局：莊家沒有連莊就一定結束。
        assert!(match_over(
            &R,
            W4,
            &[28_000, 26_000, 24_000, 22_000],
            3,
            false,
            false
        ));
        // 莊家第一名但連莊是因為途中流局（九種九牌等）→ 不適用和了止め，繼續（天鳳規則）。
        assert!(!match_over(
            &R,
            S4,
            &[20_000, 20_000, 25_000, 35_000],
            3,
            true,
            true
        ));
        // 有人低於 0 點 → 立刻結束（擊飛）。
        assert!(match_over(
            &R,
            1,
            &[-100, 40_100, 30_000, 30_000],
            1,
            false,
            false
        ));
    }

    /// 同分時座位編號小者名次在前。
    #[test]
    fn ranking_breaks_ties_by_seat() {
        assert_eq!(ranking(&[25_000, 30_000, 25_000, 20_000]), [1, 0, 2, 3]);
    }
}
