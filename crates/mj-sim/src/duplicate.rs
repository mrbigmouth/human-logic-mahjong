//! Duplicate（複式）對戰：用同一組牌山，讓各方輪流坐每個座位，抵消配牌與摸牌的運氣。
//!
//! 每一「組」使用同一個 seed（第 N 局的牌山只由 seed 與 N 決定），打 4 場：
//! 座位依序輪換，所以每一方在每個座位都坐過。比較兩方時，以「每組的平均差」計算
//! 平均值與 95% 信賴區間——同一組牌山下的成對比較，雜訊比獨立對局小得多。
//!
//! A/A 測試：讓同一個策略和自己比（兩個名稱相同的參賽者），差距應落在信賴區間內。

use mj_engine::{GameConfig, Player, run_game};
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
    let rules = RiichiConfig::TENHOU;
    // 每組：各參賽者的（順位總和, 名次點總和, 座位場數）
    let per_block: Vec<Vec<(f64, f64, u64)>> = (0..blocks)
        .into_par_iter()
        .map(|b| {
            let game_seed = seed.wrapping_add(b);
            let mut acc = vec![(0.0, 0.0, 0u64); names.len()];
            for rot in 0..4 {
                let who: [usize; 4] = std::array::from_fn(|s| pat[(s + rot) % 4]);
                let mut players: [Box<dyn Player>; 4] =
                    std::array::from_fn(|s| make(who[s], game_seed, s as u8));
                let r = run_game(
                    &GameConfig::new(rules.clone(), game_seed),
                    &mut players,
                    false,
                )
                .map_err(|e| format!("第 {b} 組（seed {game_seed}）：{e}"))?;
                for s in 0..4u8 {
                    let e = &mut acc[who[usize::from(s)]];
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
            Entrant {
                name: name.clone(),
                games,
                avg_rank: per_block.iter().map(|b| b[i].0).sum::<f64>() / games as f64,
                avg_points: per_block.iter().map(|b| b[i].1).sum::<f64>() / games as f64,
            }
        })
        .collect();
    let comparison = (names.len() == 2).then(|| {
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
}
