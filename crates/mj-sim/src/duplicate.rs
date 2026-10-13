//! Duplicate（複式）對戰：用同一組牌山，讓各方輪流坐每個座位，抵消配牌與摸牌的運氣。
//!
//! 每一「組」使用同一個 seed（第 N 局的牌山只由 seed 與 N 決定），打 4 場：
//! 座位依序輪換，所以每一方在每個座位都坐過。比較兩方時，以「每組的平均差」計算
//! 平均值與 95% 信賴區間——同一組牌山下的成對比較，雜訊比獨立對局小得多。
//!
//! A/A 測試：讓同一個策略和自己比（兩個名稱相同的參賽者），差距應落在信賴區間內。

use mj_engine::{Event, GameConfig, HandEnd, Player, run_game};
use mj_rules::riichi::RiichiConfig;
use rayon::prelude::*;

use crate::counterfactual::placement_points;

/// 建立某一方的玩家：`(參賽者編號, 對局 seed, 座位) -> 玩家`。
pub type MakePlayer<'a> = &'a (dyn Fn(usize, u64, u8) -> Box<dyn Player> + Sync);

/// 一方的成績。
#[derive(Debug, Clone, Default)]
pub struct Entrant {
    /// 名稱
    pub name: String,
    /// 參加的座位場數
    pub games: u64,
    /// 平均順位（1–4）
    pub avg_rank: f64,
    /// 平均名次點
    pub avg_points: f64,
    /// 每局的和牌、放銃統計
    pub hands: HandStats,
}

/// 每局的和牌、放銃統計（印象操作的效果會先出現在這裡：對手放銃給我的次數）。
#[derive(Debug, Clone, Copy, Default)]
pub struct HandStats {
    /// 參加的局數
    pub hands: u64,
    /// 和牌次數：榮和、自摸
    pub ron: u64,
    pub tsumo: u64,
    /// 和牌時的狀態：立直、默聽（門前沒立直）、副露
    pub riichi_wins: u64,
    pub dama_wins: u64,
    pub open_wins: u64,
    /// 和牌得到的點數合計
    pub win_points: i64,
    /// 放銃次數與付出的點數合計
    pub deal_ins: u64,
    pub deal_in_points: i64,
    /// 立直次數
    pub riichi: u64,
}

impl HandStats {
    fn add(&mut self, o: &HandStats) {
        self.hands += o.hands;
        self.ron += o.ron;
        self.tsumo += o.tsumo;
        self.riichi_wins += o.riichi_wins;
        self.dama_wins += o.dama_wins;
        self.open_wins += o.open_wins;
        self.win_points += o.win_points;
        self.deal_ins += o.deal_ins;
        self.deal_in_points += o.deal_in_points;
        self.riichi += o.riichi;
    }

    /// 每局的比率。
    pub fn rate(&self, n: u64) -> f64 {
        n as f64 / self.hands.max(1) as f64
    }
}

/// 由完整事件紀錄統計四個座位的 [`HandStats`]。
pub fn hand_stats(log: &[Event]) -> [HandStats; 4] {
    let mut st = [HandStats::default(); 4];
    let mut riichi = [false; 4];
    let mut open = [false; 4];
    for e in log {
        match e {
            Event::StartKyoku { .. } => {
                riichi = [false; 4];
                open = [false; 4];
                for s in &mut st {
                    s.hands += 1;
                }
            }
            Event::RiichiAccepted { seat } => {
                riichi[usize::from(*seat)] = true;
                st[usize::from(*seat)].riichi += 1;
            }
            Event::Call { seat, meld, .. } if meld.is_open() => open[usize::from(*seat)] = true,
            Event::HandEnd {
                end: HandEnd::Win { winner, from, .. },
                deltas,
            } => {
                let (w, f) = (usize::from(*winner), usize::from(*from));
                let s = &mut st[w];
                if w == f {
                    s.tsumo += 1;
                } else {
                    s.ron += 1;
                    st[f].deal_ins += 1;
                    st[f].deal_in_points -= i64::from(deltas[f]);
                }
                let s = &mut st[w];
                s.win_points += i64::from(deltas[w]);
                if riichi[w] {
                    s.riichi_wins += 1;
                } else if open[w] {
                    s.open_wins += 1;
                } else {
                    s.dama_wins += 1;
                }
            }
            _ => {}
        }
    }
    st
}

/// 兩方的成對比較。
#[derive(Debug, Clone, Default)]
pub struct Comparison {
    /// 平均順位差（A − B；負數代表 A 較好）
    pub rank_diff: f64,
    pub rank_ci: f64,
    /// 平均名次點差（A − B；正數代表 A 較好）
    pub points_diff: f64,
    pub points_ci: f64,
    /// 組數
    pub blocks: u64,
}

/// 對戰結果。
#[derive(Debug, Clone, Default)]
pub struct Report {
    pub entrants: Vec<Entrant>,
    /// 只有兩方對戰時才有
    pub comparison: Option<Comparison>,
}

/// 座位配置：兩方時為 [A, A, B, B]，四方時照順序。
fn pattern(n: usize) -> [usize; 4] {
    match n {
        2 => [0, 0, 1, 1],
        4 => [0, 1, 2, 3],
        _ => panic!("duplicate 對戰需要 2 或 4 個參賽者"),
    }
}

/// 平均值與 95% 信賴區間半寬。
fn mean_ci(xs: &[f64]) -> (f64, f64) {
    let n = xs.len() as f64;
    let mean = xs.iter().sum::<f64>() / n;
    let var = xs.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1.0).max(1.0);
    (mean, 1.96 * (var / n).sqrt())
}

/// 進行 `blocks` 組 duplicate 對戰（每組 4 場）。`names` = 參賽者名稱（2 或 4 個）。
pub fn run(names: &[String], blocks: u64, seed: u64, make: MakePlayer) -> Result<Report, String> {
    let pat = pattern(names.len());
    let lineups: Vec<[usize; 4]> = (0..4)
        .map(|rot| std::array::from_fn(|s| pat[(s + rot) % 4]))
        .collect();
    let entrant_of: Vec<usize> = (0..names.len()).collect();
    let compare = names.len() == 2;
    play(
        names.to_vec(),
        &entrant_of,
        &lineups,
        blocks,
        seed,
        compare,
        make,
    )
}

/// 「場地」對戰：A、B 各自和同一個對手 `field`（坐其他三個位置）在同一組牌山上打，再比較 A 與 B。
/// 每組 8 場：A 輪流坐 4 個座位（另外三家是 field），B 也一樣。
/// 和 [`run`] 的 [A, A, B, B] 不同，A 與 B 不同桌，所以能分別看出「面對這種對手」時的差別
/// （例如詐騙只對會讀牌的對手有效）；也會分別統計 field 和 A 同桌時、和 B 同桌時的成績。
pub fn run_field(
    names: &[String; 3],
    blocks: u64,
    seed: u64,
    make: MakePlayer,
) -> Result<Report, String> {
    let mut lineups = Vec::new();
    for e in 0..2 {
        for rot in 0..4 {
            let mut who = [2 + e; 4];
            who[rot] = e;
            lineups.push(who);
        }
    }
    let labels = vec![
        names[0].clone(),
        names[1].clone(),
        format!("{}（與 {} 同桌）", names[2], names[0]),
        format!("{}（與 {} 同桌）", names[2], names[1]),
    ];
    play(labels, &[0, 1, 2, 2], &lineups, blocks, seed, true, make)
}

/// 依 `lineups`（每場四個座位各記在哪一個統計項目）打 `blocks` 組。
/// `entrant_of[統計項目]` = 這個項目由哪一個參賽者（`make` 的編號）上場。
/// `compare` = 前兩個統計項目做成對比較。
fn play(
    names: Vec<String>,
    entrant_of: &[usize],
    lineups: &[[usize; 4]],
    blocks: u64,
    seed: u64,
    compare: bool,
    make: MakePlayer,
) -> Result<Report, String> {
    let rules = RiichiConfig::TENHOU;
    // 每組：各統計項目的（順位總和, 名次點總和, 座位場數, 每局統計）
    let per_block: Vec<Vec<(f64, f64, u64, HandStats)>> = (0..blocks)
        .into_par_iter()
        .map(|b| {
            let game_seed = seed.wrapping_add(b);
            let mut acc = vec![(0.0, 0.0, 0u64, HandStats::default()); names.len()];
            for who in lineups {
                let mut players: [Box<dyn Player>; 4] =
                    std::array::from_fn(|s| make(entrant_of[who[s]], game_seed, s as u8));
                let r = run_game(
                    &GameConfig::new(rules.clone(), game_seed),
                    &mut players,
                    true,
                )
                .map_err(|e| format!("第 {b} 組（seed {game_seed}）：{e}"))?;
                let stats = hand_stats(&r.log);
                for s in 0..4u8 {
                    let e = &mut acc[who[usize::from(s)]];
                    e.3.add(&stats[usize::from(s)]);
                    e.0 += f64::from(r.ranks[usize::from(s)] + 1);
                    e.1 += placement_points(&rules, &r.scores, &r.ranks, s);
                    e.2 += 1;
                }
            }
            Ok(acc)
        })
        .collect::<Result<_, String>>()?;

    let entrants = names
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let games: u64 = per_block.iter().map(|b| b[i].2).sum();
            let mut hands = HandStats::default();
            for b in &per_block {
                hands.add(&b[i].3);
            }
            Entrant {
                name: name.clone(),
                games,
                avg_rank: per_block.iter().map(|b| b[i].0).sum::<f64>() / games as f64,
                avg_points: per_block.iter().map(|b| b[i].1).sum::<f64>() / games as f64,
                hands,
            }
        })
        .collect();
    let comparison = compare.then(|| {
        let ranks: Vec<f64> = per_block
            .iter()
            .map(|b| b[0].0 / b[0].2 as f64 - b[1].0 / b[1].2 as f64)
            .collect();
        let points: Vec<f64> = per_block
            .iter()
            .map(|b| b[0].1 / b[0].2 as f64 - b[1].1 / b[1].2 as f64)
            .collect();
        let (rank_diff, rank_ci) = mean_ci(&ranks);
        let (points_diff, points_ci) = mean_ci(&points);
        Comparison {
            rank_diff,
            rank_ci,
            points_diff,
            points_ci,
            blocks,
        }
    });
    Ok(Report {
        entrants,
        comparison,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bots::GreedyBot;

    /// A/A：同一種機器人分成兩方對戰，差距應落在信賴區間內（不會顯著偏向任一方）。
    #[test]
    fn aa_test_has_no_bias() {
        let make = |_who: usize, seed: u64, seat: u8| -> Box<dyn Player> {
            Box::new(GreedyBot::new(seed * 4 + u64::from(seat), 0.3))
        };
        let r = run(&["A".into(), "B".into()], 60, 1, &make).unwrap();
        let c = r.comparison.unwrap();
        assert!(c.rank_diff.abs() <= c.rank_ci.max(0.05) * 1.5, "{c:?}");
        let total: f64 = r.entrants.iter().map(|e| e.avg_points).sum();
        assert!(
            total.abs() < 1e-6,
            "名次點兩方平均相加應為 0（每場四家合計 0）"
        );
    }

    /// 場地對戰：A、B 是同一種決定性的機器人時，兩方的對局完全相同，差距恰為 0；
    /// field 和 A 同桌、和 B 同桌的成績也相同。
    #[test]
    fn field_mode_pairs_identical_games() {
        let make = |_who: usize, seed: u64, seat: u8| -> Box<dyn Player> {
            Box::new(GreedyBot::new(seed * 4 + u64::from(seat), 0.0))
        };
        let names = ["A".to_string(), "B".to_string(), "F".to_string()];
        let r = run_field(&names, 6, 3, &make).unwrap();
        let c = r.comparison.unwrap();
        assert_eq!(c.points_diff, 0.0);
        assert_eq!(r.entrants.len(), 4);
        assert_eq!(r.entrants[0].games, 24);
        assert_eq!(r.entrants[2].games, 72);
        assert_eq!(r.entrants[2].avg_points, r.entrants[3].avg_points);
        assert!(r.entrants[0].hands.hands > 0);
    }
}
