//! `mj tune`：可調參數的自動調整（預測以牌譜上的對數損失、決策以 duplicate 對戰），
//! 最後在另一組資料上驗證，並輸出參數檔與建議修改的預設值。

use mj_strategy::find_strategy_spec;
use mj_strategy::framework::Strategy;
use mj_tuner::{Objective, Options as TuneOptions, fmt, select_params, tune};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

/// 調整設定。
pub struct Options {
    pub strategy: String,
    pub params: Vec<String>,
    pub objective: String,
    pub source: String,
    pub logs: Vec<PathBuf>,
    pub validate_logs: Vec<PathBuf>,
    pub baseline: Option<String>,
    pub blocks: u64,
    pub seed: u64,
    pub validate_blocks: u64,
    pub validate_seed: u64,
    pub rounds: usize,
    pub accept_z: f64,
    pub out: Option<PathBuf>,
}

pub fn run(o: &Options) -> Result<(), String> {
    let st: Arc<dyn Strategy> = Arc::from(find_strategy_spec(&o.strategy)?);
    let load = |paths: &[PathBuf]| -> Result<Vec<mj_paifu::GameRecord>, String> {
        let (records, errors) = mj_paifu::load_all(paths);
        if let Some(e) = errors.first() {
            return Err(e.clone());
        }
        Ok(records)
    };
    let (objective, validation) = match o.objective.as_str() {
        "logloss" => {
            if o.logs.is_empty() {
                return Err("--objective logloss 需要 --logs（擬合用的牌譜）".into());
            }
            let fit = Objective::LogLoss {
                source: o.source.clone(),
                records: load(&o.logs)?,
            };
            let val = (!o.validate_logs.is_empty())
                .then(|| -> Result<Objective, String> {
                    Ok(Objective::LogLoss {
                        source: o.source.clone(),
                        records: load(&o.validate_logs)?,
                    })
                })
                .transpose()?;
            (fit, val)
        }
        "duplicate" => {
            let base_spec = o.baseline.clone().unwrap_or_else(|| o.strategy.clone());
            let baseline: Arc<dyn Strategy> = Arc::from(find_strategy_spec(&base_spec)?);
            let fit = Objective::Duplicate {
                baseline: baseline.clone(),
                blocks: o.blocks,
                seed: o.seed,
            };
            let val = Objective::Duplicate {
                baseline,
                blocks: o.validate_blocks,
                seed: o.validate_seed,
            };
            (fit, Some(val))
        }
        other => return Err(format!("未知的目標 {other}（可用 logloss 或 duplicate）")),
    };
    let params = select_params(st.params(), &o.params)?;
    println!(
        "調整 {}：{}",
        o.strategy,
        params.iter().map(|p| p.name).collect::<Vec<_>>().join("、")
    );
    let start = Instant::now();
    let tuned = tune(
        &st,
        &objective,
        &TuneOptions {
            params: o.params.clone(),
            rounds: o.rounds,
            accept_z: o.accept_z,
        },
        &mut |line| println!("{line}  [{:.0} 秒]", start.elapsed().as_secs_f64()),
    )?;
    println!();
    if tuned.tuned == tuned.initial {
        println!("沒有找到更好的值，參數維持不變。");
        return Ok(());
    }
    println!("調整後的參數（與原本不同的）：");
    for p in &params {
        let (a, b) = (tuned.initial.get(p), tuned.tuned.get(p));
        if a != b {
            println!(
                "  {} = {}（原本 {}，預設 {}）",
                p.name,
                fmt(b),
                fmt(a),
                fmt(p.default)
            );
        }
    }
    if let Some(val) = validation {
        println!();
        match &val {
            Objective::LogLoss { .. } => {
                let a = val.score(&st, &tuned.initial)?;
                let b = val.score(&st, &tuned.tuned)?;
                println!(
                    "驗證（另一組牌譜）：{} 對數損失 {:.5} → {:.5}",
                    o.source, -a.value, -b.value
                );
            }
            Objective::Duplicate { blocks, seed, .. } => {
                let s = val.compare(&st, &tuned.tuned, &tuned.initial)?;
                println!(
                    "驗證（新的牌山 seed {seed}，{blocks} 組）：調整後 − 調整前 名次點 {:+.2} ± {:.2}（95% 信賴區間 ±{:.2}）",
                    s.value,
                    s.se,
                    1.96 * s.se
                );
            }
        }
    }
    if let Some(path) = &o.out {
        std::fs::write(path, tuned.tuned.to_text())
            .map_err(|e| format!("寫入 {} 失敗：{e}", path.display()))?;
        println!(
            "參數檔已寫入 {}（`{}@{}` 即可使用；確定採用後請把程式中的預設值改掉）",
            path.display(),
            st.name(),
            path.display()
        );
    }
    Ok(())
}
