//! 決策品質評估（反事實模擬）：不看整場輸贏，而是逐一檢查「這個決策選得好不好」。
//!
//! 做法：
//! 1. 正常打一場對局，記錄每個決策（選項、選擇、理由、當時的情境）。
//! 2. 抽樣一些決策點。重播這場對局，在決策點保存局面快照（`capture_snapshot`）。
//! 3. 產生 R 個「可能的世界」：依決策者看得到的資訊，重新分配看不到的牌
//!    （對手的門前牌、未摸的牌山；立直者必須聽牌且沒有振聽）——不偷看對手手牌。
//! 4. 對每個選項，在每個世界中從快照繼續打（強制選這個選項），比較各選項的平均結果：
//!    - 名次點（預設）：打到整場結束，(點數 − 30000) ÷ 1000 ＋ 順位馬 ＋ 第一名的返點 20；
//!    - 點數：只打完這一局，計算這一局的點數得失。
//! 5. 實際選擇與最佳選項的差距 = 這個決策的「損失」。差距不顯著時自動增加世界數，直到上限。
//!
//! 各選項使用同一組世界（共同隨機數），差距的雜訊比各自獨立抽樣小得多。

use mj_engine::{
    GameConfig, Player, Snapshot, Until, capture_snapshot, hand_seed, resume_game, run_game_with,
};
use mj_rules::riichi::RiichiConfig;
use mj_strategy::framework::{DecisionKind, DecisionRecord, Force, Strategy, StrategyPlayer};
use rand::SeedableRng;
use rand::seq::SliceRandom;
use rand_chacha::ChaCha8Rng;
use rayon::prelude::*;
use serde::Serialize;
use std::sync::Arc;

/// 用什麼衡量一個選項的結果。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Metric {
    /// 名次點：打到整場結束（含順位馬與返點）
    Placement,
    /// 這一局的點數得失
    Points,
}

/// 評估設定。
#[derive(Debug, Clone)]
pub struct Options {
    /// 每場對局抽樣幾個決策點
    pub samples: usize,
    /// 每個選項最少重打幾次（世界數）
    pub rollouts: usize,
    /// 差距不顯著時，世界數最多增加到多少
    pub max_rollouts: usize,
    /// 衡量方式
    pub metric: Metric,
    /// 是否重抽看不到的牌（false = 使用真實的對手手牌，即全知；用來比較）
    pub determinize: bool,
    /// 只評估這些決策種類（空 = 全部）
    pub kinds: Vec<DecisionKind>,
}

/// 一個決策點的評估結果。
#[derive(Debug, Clone, Serialize)]
pub struct DecisionEval {
    /// 對局 seed
    pub game_seed: u64,
    /// 做決策的座位
    pub seat: u8,
    /// 原始決策紀錄（選項、選擇、理由、情境）
    pub record: DecisionRecord,
    /// 各選項的平均結果（名次點或點數）
    pub option_ev: Vec<f64>,
    /// 期望值最高的選項
    pub best: usize,
    /// 損失 = 最佳選項的期望值 − 實際選擇的期望值（≥ 0）
    pub loss: f64,
    /// 損失的標準誤（由成對的重打差值估計）
    pub loss_se: f64,
    /// 實際使用的世界數
    pub rollouts: usize,
    /// 重抽時找不到合理立直手牌而保留原手牌的次數
    pub riichi_fallbacks: usize,
}

impl DecisionEval {
    /// 損失是否顯著（大於 2 倍標準誤）。
    pub fn significant(&self) -> bool {
        self.loss > 0.0 && self.loss > 2.0 * self.loss_se
    }
}

/// 名次點：(點數 − 30000) ÷ 1000 ＋ 順位馬 ＋ 第一名的返點 20（天鳳式，四家合計為 0）。
pub fn placement_points(rules: &RiichiConfig, scores: &[i32; 4], ranks: &[u8; 4], seat: u8) -> f64 {
    let s = usize::from(seat);
    let rank = usize::from(ranks[s]);
    let oka = if rank == 0 {
        f64::from(4 * (rules.target_points - rules.starting_points)) / 1000.0
    } else {
        0.0
    };
    f64::from(scores[s] - rules.target_points) / 1000.0 + f64::from(rules.uma[rank]) + oka
}

/// 建立四個策略玩家。
fn players(strategy: &Arc<dyn Strategy>, record: bool) -> Vec<Box<dyn StrategyPlayer>> {
    (0..4).map(|_| strategy.player(record, None)).collect()
}

/// 把玩家借給引擎。
fn refs(ps: &mut [Box<dyn StrategyPlayer>]) -> [&mut dyn Player; 4] {
    let mut it = ps.iter_mut().map(|p| p.as_mut() as &mut dyn Player);
    std::array::from_fn(|_| it.next().unwrap())
}

/// 一個決策點：在一組世界中，每個選項重打的結果。
struct Rollouts {
    per_option: Vec<Vec<f64>>,
    fallbacks: usize,
}

/// 快照點要評估的決策：哪一次詢問的哪一種決策。
#[derive(Debug, Clone, Copy)]
pub struct Point {
    /// 做決策的座位
    pub seat: u8,
    /// 這個座位第幾次被詢問（= `DecisionRecord::act_index`）
    pub act_index: u32,
    /// 決策種類
    pub kind: DecisionKind,
    /// 選項數
    pub options: usize,
}

/// 在 `worlds` 個新世界中，替每個選項各重打一次並累加到 `acc`。
fn run_worlds(
    rules: &RiichiConfig,
    snap: &Snapshot,
    base: &[Box<dyn StrategyPlayer>],
    point: Point,
    seeds: std::ops::Range<u64>,
    opts: &Options,
    acc: &mut Rollouts,
) -> Result<(), String> {
    let seat = point.seat;
    for world_seed in seeds {
        let mut world = snap.clone();
        if opts.determinize {
            acc.fallbacks += world
                .determinize(&mut ChaCha8Rng::seed_from_u64(world_seed))
                .riichi_fallbacks;
        }
        // 之後各局的牌山也依世界而不同。
        let cfg = GameConfig::new(rules.clone(), hand_seed(world_seed, 0x00C0_FFEE));
        let until = match opts.metric {
            Metric::Placement => Until::GameEnd,
            Metric::Points => Until::HandEnd,
        };
        for (option, out) in acc.per_option.iter_mut().enumerate() {
            let mut ps: Vec<Box<dyn StrategyPlayer>> = base.iter().map(|p| p.clone_box()).collect();
            ps[usize::from(seat)].set_force(Some(Force {
                act_index: point.act_index,
                kind: point.kind,
                option,
            }));
            ps[usize::from(seat)].set_record(true);
            let res = resume_game(&cfg, &world, refs(&mut ps), false, until)
                .map_err(|e| e.to_string())?;
            if !ps[usize::from(seat)]
                .take_records()
                .iter()
                .any(|r| r.forced)
            {
                return Err("重播沒有走到決策點".into());
            }
            out.push(match opts.metric {
                Metric::Placement => placement_points(rules, &res.scores, &res.ranks, seat),
                Metric::Points => f64::from(
                    res.scores[usize::from(seat)] - snap.hand_start_scores()[usize::from(seat)],
                ),
            });
        }
    }
    Ok(())
}

/// `a` 選項比 `b` 選項好多少（平均差）與標準誤，以同一組世界的成對差值計算。
pub fn paired_diff(per_option: &[Vec<f64>], a: usize, b: usize) -> (f64, f64) {
    let diffs: Vec<f64> = per_option[a]
        .iter()
        .zip(&per_option[b])
        .map(|(x, y)| x - y)
        .collect();
    let n = diffs.len() as f64;
    let mean = diffs.iter().sum::<f64>() / n;
    let var = diffs.iter().map(|d| (d - mean).powi(2)).sum::<f64>() / (n - 1.0).max(1.0);
    (mean, (var / n).sqrt())
}

/// 由每個選項的結果計算最佳選項、損失與標準誤。
fn summarize_option(per_option: &[Vec<f64>], chosen: usize) -> (Vec<f64>, usize, f64, f64) {
    let ev: Vec<f64> = per_option
        .iter()
        .map(|d| d.iter().sum::<f64>() / d.len() as f64)
        .collect();
    let best = (0..ev.len())
        .max_by(|&a, &b| ev[a].total_cmp(&ev[b]))
        .unwrap();
    let diffs: Vec<f64> = per_option[best]
        .iter()
        .zip(&per_option[chosen])
        .map(|(b, c)| b - c)
        .collect();
    let n = diffs.len() as f64;
    let mean = diffs.iter().sum::<f64>() / n;
    let var = diffs.iter().map(|d| (d - mean).powi(2)).sum::<f64>() / (n - 1.0).max(1.0);
    (ev.clone(), best, ev[best] - ev[chosen], (var / n).sqrt())
}

/// 從快照評估一個決策的各選項。
#[derive(Debug, Clone)]
pub struct SnapshotEval {
    /// 各選項的平均結果（名次點或點數）
    pub option_ev: Vec<f64>,
    /// 期望值最高的選項
    pub best: usize,
    /// 損失 = 最佳選項的期望值 − 被評估選項（`chosen`）的期望值
    pub loss: f64,
    /// 損失的標準誤
    pub loss_se: f64,
    /// 實際使用的世界數
    pub rollouts: usize,
    /// 重抽時保留立直者原手牌的次數
    pub riichi_fallbacks: usize,
    /// 每個選項在各世界的結果（同一個索引 = 同一個世界），可用 [`paired_diff`] 比較任兩個選項
    pub per_option: Vec<Vec<f64>>,
}

/// 何時停止增加世界數。
#[derive(Debug, Clone, Copy)]
pub enum Settle {
    /// `chosen` 就是最佳選項，或它與最佳選項的差距已顯著（決策評估用）
    AgainstBest,
    /// `chosen` 與這個選項的差距已顯著（復盤：玩家的選擇 vs 策略的選擇）
    Against(usize),
}

/// 從快照 `snap` 評估 `point` 的每個選項：`base` 是快照當下的四家策略玩家（記憶、詢問次數都接得上），
/// `chosen` 是要計算損失的選項（實際的選擇），`seed_base` 決定各世界的亂數。
/// 差距不顯著時自動增加世界數。重播沒有走到決策點時回傳 `None`。
#[allow(clippy::too_many_arguments)]
pub fn evaluate_snapshot(
    rules: &RiichiConfig,
    snap: &Snapshot,
    base: &[Box<dyn StrategyPlayer>],
    point: Point,
    chosen: usize,
    settle: Settle,
    seed_base: u64,
    opts: &Options,
) -> Option<SnapshotEval> {
    let mut acc = Rollouts {
        per_option: vec![Vec::new(); point.options],
        fallbacks: 0,
    };
    let mut done = 0u64;
    let step = opts.rollouts.max(2) as u64;
    loop {
        let seeds = seed_base + done..seed_base + done + step;
        run_worlds(rules, snap, base, point, seeds, opts, &mut acc).ok()?;
        done += step;
        let settled = match settle {
            Settle::AgainstBest => {
                let (_, best, loss, se) = summarize_option(&acc.per_option, chosen);
                best == chosen || loss > 2.0 * se
            }
            Settle::Against(other) => {
                let (d, se) = paired_diff(&acc.per_option, other, chosen);
                other == chosen || d.abs() > 2.0 * se
            }
        };
        if settled || done as usize >= opts.max_rollouts {
            break;
        }
    }
    let (option_ev, best, loss, loss_se) = summarize_option(&acc.per_option, chosen);
    Some(SnapshotEval {
        option_ev,
        best,
        loss,
        loss_se,
        rollouts: done as usize,
        riichi_fallbacks: acc.fallbacks,
        per_option: acc.per_option,
    })
}

/// 評估一個決策點。快照點不支援（搶槓）或重播沒有走到決策點時回傳 `None`。
fn evaluate_point(
    strategy: &Arc<dyn Strategy>,
    game_seed: u64,
    seat: u8,
    record: DecisionRecord,
    opts: &Options,
) -> Result<Option<DecisionEval>, String> {
    let rules = RiichiConfig::TENHOU;
    let cfg = GameConfig::new(rules.clone(), game_seed);
    let mut base = players(strategy, false);
    let Some(snap) = capture_snapshot(&cfg, refs(&mut base), seat, record.act_index)
        .map_err(|e| e.to_string())?
    else {
        return Ok(None);
    };
    let point = Point {
        seat,
        act_index: record.act_index,
        kind: record.kind,
        options: record.options.len(),
    };
    let seed_base = hand_seed(game_seed, (u32::from(seat) << 24) ^ record.act_index);
    let Some(e) = evaluate_snapshot(
        &rules,
        &snap,
        &base,
        point,
        record.chosen,
        Settle::AgainstBest,
        seed_base,
        opts,
    ) else {
        return Ok(None);
    };
    Ok(Some(DecisionEval {
        game_seed,
        seat,
        record,
        option_ev: e.option_ev,
        best: e.best,
        loss: e.loss,
        loss_se: e.loss_se,
        rollouts: e.rollouts,
        riichi_fallbacks: e.riichi_fallbacks,
    }))
}

/// 評估一場對局中抽樣的決策點。回傳（評估結果, 無法評估而略過的決策點數）。
pub fn evaluate_game(
    strategy: Arc<dyn Strategy>,
    game_seed: u64,
    opts: &Options,
) -> Result<(Vec<DecisionEval>, usize), String> {
    let cfg = GameConfig::new(RiichiConfig::TENHOU, game_seed);
    let mut ps = players(&strategy, true);
    run_game_with(&cfg, refs(&mut ps), false).map_err(|e| e.to_string())?;
    let mut candidates: Vec<(u8, DecisionRecord)> = ps
        .iter_mut()
        .enumerate()
        .flat_map(|(s, p)| p.take_records().into_iter().map(move |r| (s as u8, r)))
        .filter(|(_, r)| r.options.len() >= 2 && r.kind != DecisionKind::Predict)
        .filter(|(_, r)| opts.kinds.is_empty() || opts.kinds.contains(&r.kind))
        .collect();
    let mut rng = ChaCha8Rng::seed_from_u64(game_seed ^ 0x5EED);
    candidates.shuffle(&mut rng);
    candidates.truncate(opts.samples);

    let results: Vec<Option<DecisionEval>> = candidates
        .into_par_iter()
        .map(|(seat, record)| evaluate_point(&strategy, game_seed, seat, record, opts))
        .collect::<Result<_, String>>()?;
    let skipped = results.iter().filter(|r| r.is_none()).count();
    Ok((results.into_iter().flatten().collect(), skipped))
}

/// 一組決策的統計。
#[derive(Debug, Clone, Default, Serialize)]
pub struct GroupSummary {
    /// 評估的決策數
    pub n: usize,
    /// 實際選擇就是期望值最高選項的比例
    pub agree: f64,
    /// 平均損失
    pub mean_loss: f64,
    /// 平均損失的標準誤（決策之間的差異）
    pub mean_loss_se: f64,
    /// 損失顯著（大於 2 倍標準誤）的比例
    pub significant: f64,
}

/// 計算一組決策的統計。
pub fn group(evals: &[&DecisionEval]) -> GroupSummary {
    if evals.is_empty() {
        return GroupSummary::default();
    }
    let n = evals.len() as f64;
    let mean_loss = evals.iter().map(|e| e.loss).sum::<f64>() / n;
    let var = evals
        .iter()
        .map(|e| (e.loss - mean_loss).powi(2))
        .sum::<f64>()
        / (n - 1.0).max(1.0);
    GroupSummary {
        n: evals.len(),
        agree: evals.iter().filter(|e| e.best == e.record.chosen).count() as f64 / n,
        mean_loss,
        mean_loss_se: (var / n).sqrt(),
        significant: evals.iter().filter(|e| e.significant()).count() as f64 / n,
    }
}

/// 依決策種類彙整。
pub fn summarize(evals: &[DecisionEval]) -> Vec<(DecisionKind, GroupSummary)> {
    DecisionKind::ALL
        .iter()
        .filter_map(|&k| {
            let v: Vec<&DecisionEval> = evals.iter().filter(|e| e.record.kind == k).collect();
            (!v.is_empty()).then(|| (k, group(&v)))
        })
        .collect()
}

/// 情境的分組方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dimension {
    /// 有無立直的對手
    Riichi,
    /// 自己的向聽數
    Shanten,
    /// 巡目
    Turn,
    /// 名次
    Rank,
}

impl Dimension {
    pub const ALL: [Dimension; 4] = [
        Dimension::Riichi,
        Dimension::Shanten,
        Dimension::Turn,
        Dimension::Rank,
    ];

    /// 中文名稱。
    pub fn name(self) -> &'static str {
        match self {
            Dimension::Riichi => "立直的對手",
            Dimension::Shanten => "自己的向聽",
            Dimension::Turn => "巡目",
            Dimension::Rank => "名次",
        }
    }

    /// 一個決策屬於哪一組（組名）。
    pub fn bucket(self, e: &DecisionEval) -> String {
        let s = &e.record.situation;
        match self {
            Dimension::Riichi => {
                if s.riichi_opponents == 0 {
                    "沒有".into()
                } else {
                    "有".into()
                }
            }
            Dimension::Shanten => match s.shanten {
                ..=0 => "聽牌以內".into(),
                1 => "1 向聽".into(),
                2 => "2 向聽".into(),
                _ => "3 向聽以上".into(),
            },
            Dimension::Turn => match s.turn {
                0..=5 => "序盤（0–5）".into(),
                6..=11 => "中盤（6–11）".into(),
                _ => "終盤（12+）".into(),
            },
            Dimension::Rank => format!("第 {} 名", s.rank + 1),
        }
    }
}

/// 依某個情境維度分組（只看某種決策；`kind = None` 表示全部）。
pub fn by_dimension(
    evals: &[DecisionEval],
    kind: Option<DecisionKind>,
    dim: Dimension,
) -> Vec<(String, GroupSummary)> {
    let mut groups: std::collections::BTreeMap<String, Vec<&DecisionEval>> = Default::default();
    for e in evals
        .iter()
        .filter(|e| kind.is_none_or(|k| e.record.kind == k))
    {
        groups.entry(dim.bucket(e)).or_default().push(e);
    }
    groups.into_iter().map(|(k, v)| (k, group(&v))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strategy() -> Arc<dyn Strategy> {
        Arc::from(mj_strategy::find_strategy("tenhou-normal-4p-hanchan/push-fold").unwrap())
    }

    /// 冒煙測試：抽樣的決策點都能從快照重打，每個選項都有期望值。
    #[test]
    fn evaluates_sampled_points() {
        let opts = Options {
            samples: 3,
            rollouts: 2,
            max_rollouts: 2,
            metric: Metric::Points,
            determinize: true,
            kinds: Vec::new(),
        };
        let (evals, skipped) = evaluate_game(strategy(), 5, &opts).expect("replay ok");
        assert_eq!(evals.len() + skipped, 3);
        for e in &evals {
            assert_eq!(e.option_ev.len(), e.record.options.len());
            assert!(e.loss >= 0.0);
        }
    }

    /// 名次點四家合計為 0。
    #[test]
    fn placement_points_sum_to_zero() {
        let rules = RiichiConfig::TENHOU;
        let scores = [42_000, 31_000, 18_000, 9_000];
        let ranks = [0, 1, 2, 3];
        let total: f64 = (0..4)
            .map(|s| placement_points(&rules, &scores, &ranks, s))
            .sum();
        assert!(total.abs() < 1e-9);
        assert_eq!(
            placement_points(&rules, &scores, &ranks, 0),
            12.0 + 20.0 + 20.0
        );
    }
}
