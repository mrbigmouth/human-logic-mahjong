//! 大量對戰的共用核心（M15）：依「對局計畫」平行打很多場，可查進度、可取消，留下每場的結果。
//! `duplicate`（`mj simulate`）與練習 App 的對戰測試都用它。
//!
//! 計畫的每一場是（組、seed、四個座位各記在哪個統計項目）；統計項目由哪個參賽者上場另外對應
//! （場地對戰時，field 和 A 同桌、和 B 同桌是兩個統計項目，但都是同一個參賽者）。
//! 每場結果只留名次、點數等摘要；要看某一場時，用同樣的 seed 與座位配置重打（參賽者都是決定性的），
//! 外部 AI 參加的對局則保留完整事件紀錄（`keep_log`）。

use crate::counterfactual::placement_points;
use crate::duplicate::{Comparison, Entrant, HandStats, Report, hand_stats};
use mj_engine::{Event, GameConfig, Player, run_game};
use mj_rules::riichi::RiichiConfig;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// 建立某個參賽者的玩家：`(參賽者編號, 對局 seed, 座位) -> 玩家`。
pub type MakePlayer<'a> = &'a (dyn Fn(usize, u64, u8) -> Box<dyn Player> + Sync);

/// 計畫中的一場對局。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GamePlan {
    /// 第幾組（同一組用同一組牌山）
    pub block: u64,
    pub seed: u64,
    /// 座位 s 的成績記在哪一個統計項目
    pub slots: [usize; 4],
}

/// 對戰計畫。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    /// 統計項目的名稱
    pub names: Vec<String>,
    /// 統計項目由哪一個參賽者（`MakePlayer` 的編號）上場
    pub entrant_of: Vec<usize>,
    pub games: Vec<GamePlan>,
    /// 每組幾場（成對比較只用完整的組）
    pub per_block: usize,
    /// 前兩個統計項目做成對比較
    pub compare: bool,
}

/// 一場的結果摘要。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameOutcome {
    /// 計畫中的第幾場
    pub index: usize,
    pub block: u64,
    pub seed: u64,
    pub slots: [usize; 4],
    /// 終局點數、名次（0 = 第一名）、名次點（依座位）
    pub scores: [i32; 4],
    pub ranks: [u8; 4],
    pub points: [f64; 4],
    /// 打了幾局
    pub hands: u32,
    /// 和牌、放銃次數（依座位）
    pub wins: [u16; 4],
    pub deal_ins: [u16; 4],
}

/// 打完（或取消時打到一半）的結果。
#[derive(Debug, Clone, Default)]
pub struct Played {
    /// 完成的對局（依計畫順序）
    pub outcomes: Vec<GameOutcome>,
    /// 各統計項目的每局統計
    pub slot_hands: Vec<HandStats>,
    /// `keep_log` 選中的對局的完整事件紀錄（計畫中的第幾場, 紀錄）
    pub logs: Vec<(usize, Vec<Event>)>,
}

/// 進度、取消與要保留紀錄的對局。
#[derive(Default)]
pub struct Control<'a> {
    /// 完成的場數
    pub progress: Option<&'a AtomicUsize>,
    /// 設為 true 後，還沒開始的對局不再打
    pub cancel: Option<&'a AtomicBool>,
    /// 要保留完整事件紀錄的對局
    pub keep_log: Option<&'a (dyn Fn(&GamePlan) -> bool + Sync)>,
}

/// 依計畫平行打所有對局。任何一場出現引擎錯誤（例如不合法動作）都會回傳錯誤。
pub fn play(plan: &Plan, make: MakePlayer, ctl: &Control) -> Result<Played, String> {
    let rules = RiichiConfig::TENHOU;
    type One = Option<(GameOutcome, [HandStats; 4], Option<Vec<Event>>)>;
    let done: Vec<One> = plan
        .games
        .par_iter()
        .enumerate()
        .map(|(index, g)| {
            if ctl.cancel.is_some_and(|c| c.load(Ordering::Relaxed)) {
                return Ok(None);
            }
            let mut players: [Box<dyn Player>; 4] =
                std::array::from_fn(|s| make(plan.entrant_of[g.slots[s]], g.seed, s as u8));
            let r = run_game(&GameConfig::new(rules.clone(), g.seed), &mut players, true)
                .map_err(|e| format!("第 {} 組（seed {}）：{e}", g.block, g.seed))?;
            let stats = hand_stats(&r.log);
            let outcome = GameOutcome {
                index,
                block: g.block,
                seed: g.seed,
                slots: g.slots,
                scores: r.scores,
                ranks: r.ranks,
                points: std::array::from_fn(|s| {
                    placement_points(&rules, &r.scores, &r.ranks, s as u8)
                }),
                hands: r.hands_played,
                wins: std::array::from_fn(|s| (stats[s].ron + stats[s].tsumo) as u16),
                deal_ins: std::array::from_fn(|s| stats[s].deal_ins as u16),
            };
            let keep = ctl.keep_log.is_some_and(|k| k(g));
            if let Some(p) = ctl.progress {
                p.fetch_add(1, Ordering::Relaxed);
            }
            Ok(Some((outcome, stats, keep.then_some(r.log))))
        })
        .collect::<Result<_, String>>()?;
    let mut played = Played {
        slot_hands: vec![HandStats::default(); plan.names.len()],
        ..Played::default()
    };
    for (o, stats, log) in done.into_iter().flatten() {
        for (slot, st) in o.slots.iter().zip(&stats) {
            played.slot_hands[*slot].add(st);
        }
        if let Some(log) = log {
            played.logs.push((o.index, log));
        }
        played.outcomes.push(o);
    }
    Ok(played)
}

/// 平均值與 95% 信賴區間半寬。
pub(crate) fn mean_ci(xs: &[f64]) -> (f64, f64) {
    let n = xs.len() as f64;
    if xs.is_empty() {
        return (0.0, 0.0);
    }
    let mean = xs.iter().sum::<f64>() / n;
    let var = xs.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1.0).max(1.0);
    (mean, 1.96 * (var / n).sqrt())
}

/// 由各場結果彙整成績（取消時只用完成的對局；成對比較只用完整的組）。
pub fn report(plan: &Plan, outcomes: &[GameOutcome], slot_hands: &[HandStats]) -> Report {
    let n = plan.names.len();
    // 每場、每個統計項目：（順位總和, 名次點總和, 座位數）
    let per_game = |o: &GameOutcome, i: usize| {
        let mut acc = (0.0, 0.0, 0u64);
        for s in 0..4 {
            if o.slots[s] == i {
                acc.0 += f64::from(o.ranks[s] + 1);
                acc.1 += o.points[s];
                acc.2 += 1;
            }
        }
        acc
    };
    let entrants = (0..n)
        .map(|i| {
            let (mut rank_sum, mut points_sum, mut games) = (0.0, 0.0, 0u64);
            let mut ranks = [0u64; 4];
            // 每場的平均（同一項目坐兩個位置時取兩者平均），信賴區間用。
            let (mut rank_xs, mut points_xs) = (Vec::new(), Vec::new());
            for o in outcomes {
                let (r, p, c) = per_game(o, i);
                if c == 0 {
                    continue;
                }
                rank_sum += r;
                points_sum += p;
                games += c;
                rank_xs.push(r / c as f64);
                points_xs.push(p / c as f64);
                for s in 0..4 {
                    if o.slots[s] == i {
                        ranks[usize::from(o.ranks[s])] += 1;
                    }
                }
            }
            let g = games.max(1) as f64;
            Entrant {
                name: plan.names[i].clone(),
                games,
                avg_rank: rank_sum / g,
                avg_points: points_sum / g,
                rank_ci: mean_ci(&rank_xs).1,
                points_ci: mean_ci(&points_xs).1,
                ranks,
                hands: slot_hands.get(i).copied().unwrap_or_default(),
            }
        })
        .collect();
    let comparison = (plan.compare && n >= 2)
        .then(|| {
            // 每組：兩方的（順位, 名次點, 座位數）
            let mut blocks: std::collections::BTreeMap<u64, ([f64; 6], usize)> = Default::default();
            for o in outcomes {
                let e = blocks.entry(o.block).or_default();
                for (k, i) in [0, 1].into_iter().enumerate() {
                    let (r, p, c) = per_game(o, i);
                    e.0[3 * k] += r;
                    e.0[3 * k + 1] += p;
                    e.0[3 * k + 2] += c as f64;
                }
                e.1 += 1;
            }
            let full: Vec<[f64; 6]> = blocks
                .into_values()
                .filter(|(_, c)| *c == plan.per_block)
                .map(|(v, _)| v)
                .collect();
            if full.is_empty() {
                return None;
            }
            let ranks: Vec<f64> = full.iter().map(|b| b[0] / b[2] - b[3] / b[5]).collect();
            let points: Vec<f64> = full.iter().map(|b| b[1] / b[2] - b[4] / b[5]).collect();
            let (rank_diff, rank_ci) = mean_ci(&ranks);
            let (points_diff, points_ci) = mean_ci(&points);
            Some(Comparison {
                rank_diff,
                rank_ci,
                points_diff,
                points_ci,
                blocks: full.len() as u64,
            })
        })
        .flatten();
    Report {
        entrants,
        comparison,
    }
}

/// 自我對戰的計畫：`labels` = 四個座位的參賽者名稱（同名的合併統計）；
/// `rotate` = 每場輪換座位（第 g 場的座位 s 使用第 (s + g) mod 4 項），消除座位造成的偏差。
/// 回傳的計畫中，`entrant_of[項目]` = `labels` 中第一個同名的位置。
pub fn selfplay_plan(labels: &[String; 4], games: u64, seed: u64, rotate: bool) -> Plan {
    let mut names: Vec<String> = Vec::new();
    let mut entrant_of = Vec::new();
    for (i, l) in labels.iter().enumerate() {
        if !names.contains(l) {
            names.push(l.clone());
            entrant_of.push(i);
        }
    }
    let item = |l: &String| names.iter().position(|n| n == l).expect("label");
    let games = (0..games)
        .map(|g| {
            let shift = if rotate { g as usize % 4 } else { 0 };
            GamePlan {
                block: g,
                seed: seed.wrapping_add(g),
                slots: std::array::from_fn(|s| item(&labels[(s + shift) % 4])),
            }
        })
        .collect();
    Plan {
        names,
        entrant_of,
        games,
        per_block: 1,
        compare: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bots::GreedyBot;

    /// 取消後只回傳完成的對局；進度與完成數一致；重打同一場得到同樣的結果。
    #[test]
    fn cancel_progress_and_replay() {
        let make = |_who: usize, seed: u64, seat: u8| -> Box<dyn Player> {
            Box::new(GreedyBot::new(seed * 4 + u64::from(seat), 0.3))
        };
        let labels = ["A".to_string(), "B".into(), "A".into(), "B".into()];
        let plan = selfplay_plan(&labels, 12, 5, true);
        assert_eq!(plan.names, ["A", "B"]);
        assert_eq!(plan.entrant_of, [0, 1]);
        let progress = AtomicUsize::new(0);
        let keep = |g: &GamePlan| g.block == 3;
        let played = play(
            &plan,
            &make,
            &Control {
                progress: Some(&progress),
                keep_log: Some(&keep),
                ..Control::default()
            },
        )
        .unwrap();
        assert_eq!(played.outcomes.len(), 12);
        assert_eq!(progress.load(Ordering::Relaxed), 12);
        assert_eq!(played.logs.len(), 1);
        let r = report(&plan, &played.outcomes, &played.slot_hands);
        assert_eq!(r.entrants[0].games + r.entrants[1].games, 48);
        assert_eq!(r.entrants[0].ranks.iter().sum::<u64>(), r.entrants[0].games);
        // 重打第 3 場
        let one = Plan {
            games: vec![plan.games[3]],
            ..plan.clone()
        };
        let again = play(&one, &make, &Control::default()).unwrap();
        assert_eq!(again.outcomes[0].scores, played.outcomes[3].scores);
        // 一開始就取消：沒有對局
        let cancel = AtomicBool::new(true);
        let none = play(
            &plan,
            &make,
            &Control {
                cancel: Some(&cancel),
                ..Control::default()
            },
        )
        .unwrap();
        assert!(none.outcomes.is_empty());
    }
}
