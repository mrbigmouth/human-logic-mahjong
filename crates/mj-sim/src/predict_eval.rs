//! 預測準確度評估：讓策略自我對戰（或旁觀牌譜中的真人對局）、收集預測紀錄，再用全知視角判定每個預測的真相。
//!
//! 評分指標（都只看預測本身，與輸贏無關）：
//! - Brier 分數：(p − 真相)² 的平均，越小越好；
//! - 對數損失：−[y·ln p + (1−y)·ln(1−p)] 的平均，越小越好（對「很有把握卻錯」懲罰重）；
//! - 技巧分數：1 − Brier ÷ 基準 Brier（基準 = 永遠預測平均發生率），> 0 代表比瞎猜平均值好；
//! - 校準表：把預測依機率分成 10 組，比較「預測機率」與「實際發生率」。

use crate::replay::{furiten_at, hand_winners_after, hand_wins_after, hands_at};
use mj_engine::{Event, GameConfig, Player, run_game_with};
use mj_paifu::GameRecord;
use mj_paifu::replay::replay;
use mj_rules::riichi::RiichiConfig;
use mj_strategy::framework::{Claim, ClaimRecord, Strategy, StrategyPlayer, TargetSituation};
use rayon::prelude::*;
use std::collections::BTreeMap;
use std::sync::Arc;

/// 一筆已判定真相的預測。
#[derive(Debug, Clone)]
pub struct Judged {
    pub source: String,
    pub category: &'static str,
    pub p: f64,
    /// 真相：1 = 發生，0 = 沒有
    pub truth: bool,
    /// 被預測者當時的情境
    pub target: TargetSituation,
    /// 被預測者的座位（預測自己的和牌等沒有對象時為 `None`）
    pub target_seat: Option<mj_engine::Seat>,
}

/// 預測的分組方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dimension {
    /// 被預測者的巡目
    Turn,
    /// 被預測者的副露組數
    Melds,
    /// 被預測者是否已立直
    Riichi,
}

impl Dimension {
    pub const ALL: [Dimension; 3] = [Dimension::Turn, Dimension::Melds, Dimension::Riichi];

    /// 中文名稱。
    pub fn name(self) -> &'static str {
        match self {
            Dimension::Turn => "巡目",
            Dimension::Melds => "副露",
            Dimension::Riichi => "立直",
        }
    }

    /// 一筆預測屬於哪一組（組名）。
    pub fn bucket(self, j: &Judged) -> String {
        let t = &j.target;
        match self {
            Dimension::Turn => match t.turn {
                0..=5 => "序盤（0–5）".into(),
                6..=11 => "中盤（6–11）".into(),
                _ => "終盤（12+）".into(),
            },
            Dimension::Melds => format!("{} 組", t.open_melds.min(3)),
            Dimension::Riichi => {
                if t.riichi {
                    "已立直".into()
                } else {
                    "未立直".into()
                }
            }
        }
    }
}

/// 某個預測函式依情境分組的指標。
pub fn by_dimension(
    judged: &[Judged],
    source: &str,
    category: &str,
    dim: Dimension,
) -> Vec<(String, Metrics)> {
    let mut groups: BTreeMap<String, Vec<&Judged>> = BTreeMap::new();
    for j in judged
        .iter()
        .filter(|j| j.source == source && j.category == category)
    {
        groups.entry(dim.bucket(j)).or_default().push(j);
    }
    groups.into_iter().map(|(k, v)| (k, metrics(&v))).collect()
}

/// 用全知視角判定一筆預測的真相。無法判定時回傳 `None`。
pub fn judge(log: &[Event], c: &ClaimRecord) -> Option<bool> {
    match c.claim {
        Claim::Tenpai { seat, .. } => {
            let h = hands_at(log, c.at_event)[usize::from(seat)];
            if h.total() % 3 == 1 {
                Some(!mj_calc::waits(&h).is_empty())
            } else {
                Some(mj_calc::shanten(&h) <= 0)
            }
        }
        Claim::Waits { seat, tile, .. } => {
            let h = hands_at(log, c.at_event)[usize::from(seat)];
            if h.total() % 3 != 1 {
                return None;
            }
            let waits = mj_calc::waits(&h);
            Some(waits.contains(&tile) && !furiten_at(log, c.at_event, seat, &waits))
        }
        Claim::IWinThisHand { .. } => Some(hand_winners_after(log, c.at_event).contains(&c.me)),
        Claim::BigHand { seat, .. } => hand_wins_after(log, c.at_event)
            .into_iter()
            .find(|(w, _)| *w == seat)
            .map(|(_, base)| base >= 2000),
    }
}

/// 一組預測的統計。
#[derive(Debug, Clone, Default)]
pub struct Metrics {
    pub n: usize,
    pub mean_p: f64,
    pub base_rate: f64,
    pub brier: f64,
    pub log_loss: f64,
    /// 技巧分數：1 − Brier ÷ 基準 Brier
    pub skill: f64,
    /// 校準表：每組 (預測次數, 平均預測機率, 實際發生率)
    pub calibration: Vec<(usize, f64, f64)>,
}

/// 計算一組已判定預測的指標。
pub fn metrics(items: &[&Judged]) -> Metrics {
    let n = items.len();
    if n == 0 {
        return Metrics::default();
    }
    let nf = n as f64;
    let y = |j: &Judged| if j.truth { 1.0 } else { 0.0 };
    let mean_p = items.iter().map(|j| j.p).sum::<f64>() / nf;
    let base_rate = items.iter().map(|j| y(j)).sum::<f64>() / nf;
    let brier = items.iter().map(|j| (j.p - y(j)).powi(2)).sum::<f64>() / nf;
    let eps = 1e-6;
    let log_loss = items
        .iter()
        .map(|j| {
            let p = j.p.clamp(eps, 1.0 - eps);
            -(y(j) * p.ln() + (1.0 - y(j)) * (1.0 - p).ln())
        })
        .sum::<f64>()
        / nf;
    let base_brier = base_rate * (1.0 - base_rate);
    let skill = if base_brier > 0.0 {
        1.0 - brier / base_brier
    } else {
        0.0
    };
    let mut bins = vec![(0usize, 0.0f64, 0.0f64); 10];
    for j in items {
        let b = ((j.p * 10.0) as usize).min(9);
        bins[b].0 += 1;
        bins[b].1 += j.p;
        bins[b].2 += y(j);
    }
    let calibration = bins
        .into_iter()
        .map(|(c, sp, sy)| {
            if c == 0 {
                (0, 0.0, 0.0)
            } else {
                (c, sp / c as f64, sy / c as f64)
            }
        })
        .collect();
    Metrics {
        n,
        mean_p,
        base_rate,
        brier,
        log_loss,
        skill,
        calibration,
    }
}

/// 取出玩家的全部預測並用完整事件紀錄判定真相。
fn judge_all(players: &mut [Box<dyn StrategyPlayer>], log: &[Event]) -> Vec<Judged> {
    let mut out = Vec::new();
    for sp in players {
        for c in sp.take_claims() {
            if let Some(truth) = judge(log, &c) {
                out.push(Judged {
                    source: c.source.clone(),
                    category: c.claim.category(),
                    p: c.claim.p(),
                    truth,
                    target: c.target,
                    target_seat: c.claim.target(),
                });
            }
        }
    }
    out
}

/// 讓 `strategy` 四家自我對戰 `games` 場，收集並判定所有預測。
pub fn collect(strategy: Arc<dyn Strategy>, games: u64, seed: u64) -> Result<Vec<Judged>, String> {
    let per_game: Vec<Vec<Judged>> = (0..games)
        .into_par_iter()
        .map(|g| {
            let mut sps: Vec<_> = (0..4).map(|_| strategy.player(true, None)).collect();
            let refs: [&mut dyn Player; 4] = {
                let mut it = sps.iter_mut().map(|p| p.as_mut() as &mut dyn Player);
                std::array::from_fn(|_| it.next().unwrap())
            };
            let cfg = GameConfig::new(RiichiConfig::TENHOU, seed.wrapping_add(g));
            let result = run_game_with(&cfg, refs, true).map_err(|e| e.to_string())?;
            Ok(judge_all(&mut sps, &result.log))
        })
        .collect::<Result<_, String>>()?;
    Ok(per_game.into_iter().flatten().collect())
}

/// 四個座位放不同的策略（每場輪換座位：第 g 場座位 s 是 `strategies[(s + g) % 4]`），
/// 收集並判定所有預測。回傳（被預測者是第幾個策略, 預測）；
/// 用來看「讀牌者對某種打法（例如會詐騙的）讀得準不準」。
pub fn collect_mixed(
    strategies: &[Arc<dyn Strategy>; 4],
    games: u64,
    seed: u64,
) -> Result<Vec<(Option<usize>, Judged)>, String> {
    let per_game: Vec<Vec<(Option<usize>, Judged)>> = (0..games)
        .into_par_iter()
        .map(|g| {
            let who = |s: usize| (s + g as usize) % 4;
            let mut sps: Vec<_> = (0..4)
                .map(|s| strategies[who(s)].player(true, None))
                .collect();
            let refs: [&mut dyn Player; 4] = {
                let mut it = sps.iter_mut().map(|p| p.as_mut() as &mut dyn Player);
                std::array::from_fn(|_| it.next().unwrap())
            };
            let cfg = GameConfig::new(RiichiConfig::TENHOU, seed.wrapping_add(g));
            let result = run_game_with(&cfg, refs, true).map_err(|e| e.to_string())?;
            Ok(judge_all(&mut sps, &result.log)
                .into_iter()
                .map(|j| (j.target_seat.map(|s| who(usize::from(s))), j))
                .collect())
        })
        .collect::<Result<_, String>>()?;
    Ok(per_game.into_iter().flatten().collect())
}

/// 在牌譜（例如人類的天鳳對局）上評估預測：四家的位置各放一個策略「旁觀」，
/// 收到同樣的事件與詢問並做預測，但動作照牌譜。預測的對象因此是真人的手牌。
///
/// 回傳（判定後的預測, 無法重播的牌譜與原因）。
pub fn collect_records(
    strategy: Arc<dyn Strategy>,
    records: &[GameRecord],
) -> (Vec<Judged>, Vec<String>) {
    let per_game: Vec<Result<Vec<Judged>, String>> = records
        .par_iter()
        .map(|rec| {
            let mut sps: Vec<_> = (0..4).map(|_| strategy.player(true, None)).collect();
            let shadows: [Option<&mut dyn Player>; 4] = {
                let mut it = sps.iter_mut().map(|p| Some(p.as_mut() as &mut dyn Player));
                std::array::from_fn(|_| it.next().unwrap())
            };
            let r = replay(rec, shadows, false).map_err(|e| format!("{}: {e}", rec.id))?;
            Ok(judge_all(&mut sps, &r.result.log))
        })
        .collect();
    let (mut judged, mut errs) = (Vec::new(), Vec::new());
    for r in per_game {
        match r {
            Ok(j) => judged.extend(j),
            Err(e) => errs.push(e),
        }
    }
    (judged, errs)
}

/// 依（預測函式、類別）分組計算指標。
pub fn summarize(judged: &[Judged]) -> BTreeMap<(String, &'static str), Metrics> {
    let mut groups: BTreeMap<(String, &'static str), Vec<&Judged>> = BTreeMap::new();
    for j in judged {
        groups
            .entry((j.source.clone(), j.category))
            .or_default()
            .push(j);
    }
    groups.into_iter().map(|(k, v)| (k, metrics(&v))).collect()
}
