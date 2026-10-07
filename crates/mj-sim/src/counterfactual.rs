//! 決策品質評估（反事實模擬）：不看整場輸贏，而是逐一檢查「這個決策選得好不好」。
//!
//! 做法：
//! 1. 正常打一場對局，記錄每個決策（選項、選擇、理由）。
//! 2. 抽樣一些決策點。對每個選項（含實際的選擇），重打這一局 R 次：之前的局照原樣重播，
//!    到這個決策點時強制選這個選項，之後牌山中「還沒摸到的部分」每次重新洗牌。
//! 3. 比較各選項在這一局的平均點數得失。實際選擇與最佳選項的差距 = 這個決策的「損失」。
//!
//! 各選項使用同一組 R 種洗牌（共同隨機數），差距的變異比各自獨立抽樣小得多。
//!
//! 限制（第一版）：對手的手牌維持真實的牌（全知），只重洗未摸的牌山；王牌（嶺上、寶牌指示牌）不變。
//! 也就是評估「在實際局面下，哪個選擇的期望值較高」。

use mj_engine::{Event, GameConfig, Player, run_game_with};
use mj_rules::riichi::RiichiConfig;
use mj_strategy::framework::{DecisionKind, DecisionRecord, Force, Strategy};
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use rayon::prelude::*;
use std::sync::Arc;

/// 配牌張數（4 家 × 13）。
const DEAL: usize = 52;
/// 牌山中可摸的部分（136 − 王牌 14）。
const LIVE: usize = 122;

/// 評估設定。
#[derive(Debug, Clone, Copy)]
pub struct Options {
    /// 每場對局抽樣幾個決策點
    pub samples: usize,
    /// 每個選項重打幾次
    pub rollouts: usize,
}

/// 一個決策點的評估結果。
#[derive(Debug, Clone)]
pub struct DecisionEval {
    /// 對局 seed
    pub game_seed: u64,
    /// 做決策的座位
    pub seat: u8,
    /// 原始決策紀錄（選項、選擇、理由）
    pub record: DecisionRecord,
    /// 各選項在這一局的平均點數得失
    pub option_ev: Vec<f64>,
    /// 期望值最高的選項
    pub best: usize,
    /// 損失 = 最佳選項的期望值 − 實際選擇的期望值（≥ 0）
    pub loss: f64,
    /// 損失的標準誤（由成對的重打差值估計）
    pub loss_se: f64,
}

/// 第 `hand` 局在 `event_index` 之前，從牌山（非嶺上）摸了幾張牌。
fn draws_before(log: &[Event], hand: u32, event_index: usize) -> usize {
    let mut current: i64 = -1;
    let mut draws = 0;
    for ev in &log[..event_index.min(log.len())] {
        match ev {
            Event::StartKyoku { .. } => {
                current += 1;
                draws = 0;
            }
            Event::Draw { rinshan: false, .. } if current == i64::from(hand) => draws += 1,
            _ => {}
        }
    }
    draws
}

/// 最後一局（`hand`）開始與結束時的點數，回傳 `seat` 的得失。
fn hand_delta(log: &[Event], seat: u8) -> Option<f64> {
    let start = log.iter().rev().find_map(|e| match e {
        Event::StartKyoku { scores, .. } => Some(*scores),
        _ => None,
    })?;
    let end = log.iter().rev().find_map(|e| match e {
        Event::EndKyoku { scores } => Some(*scores),
        _ => None,
    })?;
    Some(f64::from(end[usize::from(seat)] - start[usize::from(seat)]))
}

/// 用 4 個（可能有一個被強制指定決策的）策略玩家打一場，回傳對局結果與該座位的決策紀錄。
fn play(
    strategy: &Arc<dyn Strategy>,
    cfg: &GameConfig,
    forced_seat: Option<(u8, Force)>,
    record_all: bool,
) -> Result<(mj_engine::GameResult, Vec<Vec<DecisionRecord>>), String> {
    let mut sps: Vec<_> = (0..4u8)
        .map(|s| match forced_seat {
            Some((fs, f)) if fs == s => strategy.player(true, Some(f)),
            _ => strategy.player(record_all, None),
        })
        .collect();
    let refs: [&mut dyn Player; 4] = {
        let mut it = sps.iter_mut().map(|p| p.as_mut() as &mut dyn Player);
        std::array::from_fn(|_| it.next().unwrap())
    };
    let result = run_game_with(cfg, refs, true).map_err(|e| e.to_string())?;
    let records = sps.iter_mut().map(|p| p.take_records()).collect();
    Ok((result, records))
}

/// 評估一場對局中抽樣的決策點。
pub fn evaluate_game(
    strategy: Arc<dyn Strategy>,
    game_seed: u64,
    opts: Options,
) -> Result<Vec<DecisionEval>, String> {
    let base_cfg = GameConfig::new(RiichiConfig::TENHOU, game_seed);
    let (original, records) = play(&strategy, &base_cfg, None, true)?;

    // 只評估有兩個以上選項、而且這一局有正常結束的決策。
    let mut candidates: Vec<(u8, DecisionRecord)> = records
        .into_iter()
        .enumerate()
        .flat_map(|(s, rs)| rs.into_iter().map(move |r| (s as u8, r)))
        .filter(|(_, r)| r.options.len() >= 2 && (r.hand_index as usize) < original.walls.len())
        .collect();
    let mut rng = ChaCha8Rng::seed_from_u64(game_seed ^ 0x5EED);
    candidates.shuffle(&mut rng);
    candidates.truncate(opts.samples);

    candidates
        .into_par_iter()
        .map(|(seat, record)| {
            let hand = record.hand_index;
            let drawn = draws_before(&original.log, hand, record.event_index);
            let keep = (DEAL + drawn).min(LIVE);
            // 每次重打的洗牌：所有選項共用同一組（共同隨機數）。
            let mut shuffle_rng = ChaCha8Rng::seed_from_u64(
                game_seed
                    ^ (u64::from(hand) << 32)
                    ^ u64::from(record.act_index)
                    ^ (u64::from(seat) << 48),
            );
            let walls: Vec<Vec<mj_core::Pai>> = (0..opts.rollouts)
                .map(|_| {
                    let mut w = original.walls[hand as usize].clone();
                    w[keep..LIVE].shuffle(&mut shuffle_rng);
                    // 讓洗牌序列不依賴選項數目。
                    let _: u64 = shuffle_rng.random();
                    w
                })
                .collect();
            let mut per_option: Vec<Vec<f64>> = Vec::with_capacity(record.options.len());
            for option in 0..record.options.len() {
                let force = Force {
                    act_index: record.act_index,
                    kind: record.kind,
                    option,
                };
                let mut deltas = Vec::with_capacity(opts.rollouts);
                for w in &walls {
                    let mut cfg = base_cfg.clone();
                    cfg.preset_walls = original.walls[..hand as usize].to_vec();
                    cfg.preset_walls.push(w.clone());
                    cfg.max_hands = Some(hand + 1);
                    let (res, recs) = play(&strategy, &cfg, Some((seat, force)), false)?;
                    // 確認重播確實走到同一個決策點並套用了指定的選項。
                    if !recs[usize::from(seat)].iter().any(|r| r.forced) {
                        return Err(format!(
                            "重播沒有走到決策點（seed {game_seed}、第 {hand} 局、座位 {seat}、{}）",
                            record.kind.name()
                        ));
                    }
                    deltas.push(hand_delta(&res.log, seat).ok_or("找不到本局點數")?);
                }
                per_option.push(deltas);
            }
            let option_ev: Vec<f64> = per_option
                .iter()
                .map(|d| d.iter().sum::<f64>() / d.len() as f64)
                .collect();
            let best = (0..option_ev.len())
                .max_by(|&a, &b| option_ev[a].total_cmp(&option_ev[b]))
                .unwrap();
            let diffs: Vec<f64> = per_option[best]
                .iter()
                .zip(&per_option[record.chosen])
                .map(|(b, c)| b - c)
                .collect();
            let n = diffs.len() as f64;
            let mean = diffs.iter().sum::<f64>() / n;
            let var = diffs.iter().map(|d| (d - mean).powi(2)).sum::<f64>() / (n - 1.0).max(1.0);
            Ok(DecisionEval {
                game_seed,
                seat,
                loss: option_ev[best] - option_ev[record.chosen],
                loss_se: (var / n).sqrt(),
                record,
                option_ev,
                best,
            })
        })
        .collect()
}

/// 依決策種類彙整的統計。
#[derive(Debug, Clone, Default)]
pub struct KindSummary {
    /// 評估的決策數
    pub n: usize,
    /// 實際選擇就是期望值最高選項的比例
    pub agree: f64,
    /// 平均損失（點）
    pub mean_loss: f64,
    /// 損失顯著（大於 2 倍標準誤）的比例
    pub significant: f64,
}

/// 依決策種類彙整。
pub fn summarize(evals: &[DecisionEval]) -> Vec<(DecisionKind, KindSummary)> {
    DecisionKind::ALL
        .iter()
        .filter_map(|&k| {
            let v: Vec<&DecisionEval> = evals.iter().filter(|e| e.record.kind == k).collect();
            if v.is_empty() {
                return None;
            }
            let n = v.len() as f64;
            Some((
                k,
                KindSummary {
                    n: v.len(),
                    agree: v.iter().filter(|e| e.best == e.record.chosen).count() as f64 / n,
                    mean_loss: v.iter().map(|e| e.loss).sum::<f64>() / n,
                    significant: v
                        .iter()
                        .filter(|e| e.loss > 2.0 * e.loss_se && e.loss > 0.0)
                        .count() as f64
                        / n,
                },
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 冒煙測試：抽樣的決策點都能重播並套用指定選項，每個選項都有期望值。
    #[test]
    fn replays_reach_the_decision_point() {
        let st: Arc<dyn Strategy> =
            Arc::from(mj_strategy::find_strategy("tenhou-normal-4p-hanchan").unwrap());
        let evals = evaluate_game(
            st,
            5,
            Options {
                samples: 4,
                rollouts: 2,
            },
        )
        .expect("replay ok");
        assert_eq!(evals.len(), 4);
        for e in &evals {
            assert_eq!(e.option_ev.len(), e.record.options.len());
            assert!(e.loss >= 0.0);
        }
    }
}
