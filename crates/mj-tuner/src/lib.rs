//! 可調參數的自動調整（M8）：策略程式中宣告的具名參數（`framework::Param`），分兩種方式調整——
//! - **預測**的參數：以牌譜上的對數損失擬合（對全知真相，越小越好）；
//! - **決策**的參數：以 duplicate 對戰的名次點調整（同牌山、座位輪換，與基準策略成對比較）。
//!
//! 兩種都用「一次調一個參數」的座標搜尋：參數有名字與意義，調整後依然可以解釋，
//! 而且每一步都看得到是哪個參數、從多少改成多少、目標改善了多少。
//!
//! 決策為什麼不用反事實評估（`mj evaluate`）當目標：反事實損失是對「某個決策點」的局部評分，
//! 雜訊大，而且 M7 發現「和期望值最高的選項比」有挑選偏差；duplicate 直接衡量整場的結果，
//! 是成對比較，雜訊小得多。反事實評估仍用來檢查調整後各種決策的損失有沒有變差。

use mj_engine::Player;
use mj_paifu::GameRecord;
use mj_sim::{duplicate, predict_eval};
use mj_strategy::framework::{Param, ParamSet, Strategy};
use std::sync::Arc;

/// 調整的目標。
pub enum Objective {
    /// 預測：在牌譜上，某個預測函式（例如 `predict::tenpai`）的對數損失（越小越好）
    LogLoss {
        source: String,
        records: Vec<GameRecord>,
    },
    /// 決策：與 `baseline` 進行 duplicate 對戰的平均名次點差（越大越好）
    Duplicate {
        baseline: Arc<dyn Strategy>,
        blocks: u64,
        seed: u64,
    },
}

/// 一次評估的結果。
#[derive(Debug, Clone, Copy)]
pub struct Score {
    /// 目標值（已換成「越大越好」：對數損失取負號）
    pub value: f64,
    /// 標準誤（duplicate 才有；對數損失在同一組牌譜上是確定的，為 0）
    pub se: f64,
}

impl Objective {
    /// 評估策略（套用 `params` 覆寫）。
    pub fn score(&self, strategy: &Arc<dyn Strategy>, params: &ParamSet) -> Result<Score, String> {
        let st: Arc<dyn Strategy> = Arc::from(strategy.with_params(params.clone())?);
        match self {
            Objective::LogLoss { source, records } => {
                let (judged, errors) = predict_eval::collect_records(st, records);
                if let Some(e) = errors.first() {
                    return Err(e.clone());
                }
                let mine: Vec<&predict_eval::Judged> =
                    judged.iter().filter(|j| &j.source == source).collect();
                if mine.is_empty() {
                    return Err(format!("牌譜中沒有 {source} 的預測"));
                }
                let m = predict_eval::metrics(&mine);
                Ok(Score {
                    value: -m.log_loss,
                    se: 0.0,
                })
            }
            Objective::Duplicate {
                baseline,
                blocks,
                seed,
            } => {
                let both = [st, baseline.clone()];
                let make = |who: usize, _seed: u64, _seat: u8| -> Box<dyn Player> {
                    both[who].player(false, None)
                };
                let r = duplicate::run(&["A".into(), "B".into()], *blocks, *seed, &make)?;
                let c = r.comparison.expect("two entrants");
                Ok(Score {
                    value: c.points_diff,
                    se: c.points_ci / 1.96,
                })
            }
        }
    }

    /// 兩個參數組合之間的差距：`candidate` 比 `current` 好多少（越大越好）與標準誤。
    ///
    /// duplicate 時直接讓兩者對戰（成對比較，雜訊比分別對基準各打一次小）。
    pub fn compare(
        &self,
        strategy: &Arc<dyn Strategy>,
        candidate: &ParamSet,
        current: &ParamSet,
    ) -> Result<Score, String> {
        match self {
            Objective::LogLoss { .. } => {
                let a = self.score(strategy, candidate)?;
                let b = self.score(strategy, current)?;
                Ok(Score {
                    value: a.value - b.value,
                    se: 0.0,
                })
            }
            Objective::Duplicate { blocks, seed, .. } => {
                let pair = Objective::Duplicate {
                    baseline: Arc::from(strategy.with_params(current.clone())?),
                    blocks: *blocks,
                    seed: *seed,
                };
                pair.score(strategy, candidate)
            }
        }
    }
}

/// 調整設定。
pub struct Options {
    /// 要調整的參數名稱
    pub params: Vec<String>,
    /// 座標搜尋的輪數（每一輪把每個參數各調一次）
    pub rounds: usize,
    /// duplicate：候選值要比目前值好多少個標準誤才採用
    pub accept_z: f64,
}

/// 調整結果。
pub struct Tuned {
    /// 調整前的參數（策略原本的覆寫值）
    pub initial: ParamSet,
    /// 調整後的參數
    pub tuned: ParamSet,
}

/// 參數是否當作整數處理（預設值與範圍都是整數、範圍不大，例如「幾向聽」「幾張」）。
pub fn is_integer(p: &Param) -> bool {
    p.default.fract() == 0.0
        && p.min.fract() == 0.0
        && p.max.fract() == 0.0
        && p.max - p.min <= 20.0
}

/// 依名稱（或名稱前綴，例如 `tenpai.`）選出參數。
pub fn select_params(all: &[Param], names: &[String]) -> Result<Vec<Param>, String> {
    let mut out: Vec<Param> = Vec::new();
    for n in names {
        let found: Vec<&Param> = all
            .iter()
            .filter(|p| p.name == n || (n.ends_with('.') && p.name.starts_with(n.as_str())))
            .collect();
        if found.is_empty() {
            return Err(format!("沒有名為 {n} 的參數（用 `mj params <策略>` 查看）"));
        }
        for p in found {
            if !out.iter().any(|q| q.name == p.name) {
                out.push(*p);
            }
        }
    }
    Ok(out)
}

/// 座標搜尋：每一輪把每個參數各調一次，其他參數固定在目前的值。`log` 收到每一步的說明。
///
/// - 對數損失（確定、平滑）：在範圍內做黃金分割搜尋，取損失最小的值；
/// - duplicate（有雜訊）：試「目前值 ± 一步」（整數參數 ±1，其他為範圍的 1/8），
///   候選值與目前值直接對戰，好過 `accept_z` 個標準誤才採用；之後一步縮小一半。
pub fn tune(
    strategy: &Arc<dyn Strategy>,
    objective: &Objective,
    opts: &Options,
    log: &mut dyn FnMut(String),
) -> Result<Tuned, String> {
    let params = select_params(strategy.params(), &opts.params)?;
    let initial = strategy.overrides().clone();
    let mut current = initial.clone();
    let mut steps: Vec<f64> = params
        .iter()
        .map(|p| {
            if is_integer(p) {
                1.0
            } else {
                (p.max - p.min) / 8.0
            }
        })
        .collect();
    for round in 0..opts.rounds {
        log(format!("── 第 {} 輪 ──", round + 1));
        for (i, p) in params.iter().enumerate() {
            let now = current.get(p);
            match objective {
                Objective::LogLoss { .. } => {
                    let best = golden_section(p, |x| {
                        let mut c = current.clone();
                        c.set(p.name, x);
                        objective.score(strategy, &c).map(|s| s.value)
                    })?;
                    let mut c = current.clone();
                    c.set(p.name, best);
                    let before = objective.score(strategy, &current)?.value;
                    let after = objective.score(strategy, &c)?.value;
                    // 只差一點點（例如四捨五入造成的）不算改善。
                    if after > before + 1e-5 {
                        log(format!(
                            "{}：{} → {}（對數損失 {:.5} → {:.5}）",
                            p.name,
                            fmt(now),
                            fmt(best),
                            -before,
                            -after
                        ));
                        current = c;
                    } else {
                        log(format!(
                            "{}：維持 {}（對數損失 {:.5}）",
                            p.name,
                            fmt(now),
                            -before
                        ));
                    }
                }
                Objective::Duplicate { .. } => {
                    let mut best: Option<(f64, Score)> = None;
                    for dir in [-1.0, 1.0] {
                        let mut x = now + dir * steps[i];
                        if is_integer(p) {
                            x = x.round();
                        }
                        if x < p.min || x > p.max || x == now {
                            continue;
                        }
                        let mut c = current.clone();
                        c.set(p.name, x);
                        let s = objective.compare(strategy, &c, &current)?;
                        log(format!(
                            "{}：{} → {}：名次點 {:+.2} ± {:.2}",
                            p.name,
                            fmt(now),
                            fmt(x),
                            s.value,
                            s.se
                        ));
                        if s.value > opts.accept_z * s.se
                            && best.is_none_or(|(_, b)| s.value > b.value)
                        {
                            best = Some((x, s));
                        }
                    }
                    match best {
                        Some((x, _)) => {
                            log(format!("  → 採用 {} = {}", p.name, fmt(x)));
                            current.set(p.name, x);
                        }
                        None => {
                            if !is_integer(p) {
                                steps[i] /= 2.0;
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(Tuned {
        initial,
        tuned: current,
    })
}

/// 在參數範圍內找使目標最大的值（黃金分割搜尋，假設在範圍內大致是單峰）。整數參數最後取整數。
fn golden_section(p: &Param, mut f: impl FnMut(f64) -> Result<f64, String>) -> Result<f64, String> {
    let r = (5f64.sqrt() - 1.0) / 2.0;
    let (mut a, mut b) = (p.min, p.max);
    let mut c = b - r * (b - a);
    let mut d = a + r * (b - a);
    let (mut fc, mut fd) = (f(c)?, f(d)?);
    // 範圍縮到原本的 1% 以下即可（約 10 次評估）。
    while b - a > (p.max - p.min) * 0.01 {
        if fc > fd {
            b = d;
            d = c;
            fd = fc;
            c = b - r * (b - a);
            fc = f(c)?;
        } else {
            a = c;
            c = d;
            fc = fd;
            d = a + r * (b - a);
            fd = f(d)?;
        }
    }
    let x = (a + b) / 2.0;
    Ok(if is_integer(p) {
        x.round()
    } else {
        round_sig(x)
    })
}

/// 取 3 位有效數字（參數寫回程式時好讀）。
fn round_sig(x: f64) -> f64 {
    if x == 0.0 {
        return 0.0;
    }
    let mag = 10f64.powi(x.abs().log10().floor() as i32 - 2);
    (x / mag).round() * mag
}

/// 數值的顯示（去掉多餘的小數）。
pub fn fmt(x: f64) -> String {
    let s = format!("{x:.4}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn golden_section_finds_peak() {
        let p = Param::new("x", 0.5, 0.0, 2.0, "");
        let x = golden_section(&p, |x| Ok(-(x - 1.3f64).powi(2))).unwrap();
        assert!((x - 1.3).abs() < 0.03, "{x}");
        let q = Param::new("n", 2.0, 0.0, 6.0, "");
        assert!(is_integer(&q));
        assert_eq!(
            golden_section(&q, |x| Ok(-(x - 4.2f64).powi(2))).unwrap(),
            4.0
        );
    }

    #[test]
    fn rounding() {
        assert_eq!(round_sig(0.123456), 0.123);
        assert_eq!(fmt(2.0), "2");
        assert_eq!(fmt(0.7300), "0.73");
    }
}
