//! `mj predict-eval`：預測準確度評估（與勝負無關，只看預測對不對）。

use mj_sim::predict_eval::{Dimension, by_dimension, collect, collect_records, summarize};
use mj_strategy::find_strategy_spec;
use mj_strategy::framework::explain::{pad, pad_left};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

/// `logs` 不是空的時改用牌譜（策略旁觀真人對局），否則自我對戰 `games` 場。
pub fn run(strategy: &str, logs: &[PathBuf], games: u64, seed: u64) -> Result<(), String> {
    let st: Arc<dyn mj_strategy::framework::Strategy> = Arc::from(find_strategy_spec(strategy)?);
    let start = Instant::now();
    let judged = if logs.is_empty() {
        let judged = collect(st, games, seed)?;
        println!(
            "策略 {strategy}：自我對戰 {games} 場，判定 {} 筆預測（{:.1} 秒）",
            judged.len(),
            start.elapsed().as_secs_f64()
        );
        judged
    } else {
        let (records, mut errors) = mj_paifu::load_all(logs);
        let (judged, replay_errors) = collect_records(st, &records);
        let usable = records.len() - replay_errors.len();
        errors.extend(replay_errors);
        println!(
            "策略 {strategy}：旁觀牌譜 {usable} 場（無法使用 {} 場），判定 {} 筆預測（{:.1} 秒）",
            errors.len(),
            judged.len(),
            start.elapsed().as_secs_f64()
        );
        for e in errors.iter().take(5) {
            println!("  ✗ {e}");
        }
        judged
    };
    println!();
    println!(
        "{}{}{}{}{}{}{}",
        pad("預測函式", 20),
        pad("類別", 12),
        pad_left("筆數", 9),
        pad_left("平均預測", 10),
        pad_left("實際發生率", 12),
        pad_left("Brier", 9),
        pad_left("技巧分數", 10)
    );
    let groups = summarize(&judged);
    for ((source, category), m) in &groups {
        println!(
            "{}{}{}{}{}{}{}",
            pad(source, 20),
            pad(category, 12),
            pad_left(&m.n.to_string(), 9),
            pad_left(&format!("{:.3}", m.mean_p), 10),
            pad_left(&format!("{:.3}", m.base_rate), 12),
            pad_left(&format!("{:.4}", m.brier), 9),
            pad_left(&format!("{:+.3}", m.skill), 10)
        );
    }
    println!();
    println!("技巧分數 = 1 − Brier ÷ 基準 Brier（基準 = 永遠猜平均發生率）；> 0 代表比基準好。");
    for (source, category) in groups.keys() {
        for dim in Dimension::ALL {
            println!();
            println!(
                "{}{}{}{}{}{}",
                pad(&format!("{source}×{}", dim.name()), 28),
                pad_left("筆數", 9),
                pad_left("平均預測", 10),
                pad_left("實際發生率", 12),
                pad_left("Brier", 9),
                pad_left("技巧分數", 10)
            );
            for (label, m) in by_dimension(&judged, source, category, dim) {
                println!(
                    "{}{}{}{}{}{}",
                    pad(&format!("  {label}"), 28),
                    pad_left(&m.n.to_string(), 9),
                    pad_left(&format!("{:.3}", m.mean_p), 10),
                    pad_left(&format!("{:.3}", m.base_rate), 12),
                    pad_left(&format!("{:.4}", m.brier), 9),
                    pad_left(&format!("{:+.3}", m.skill), 10)
                );
            }
        }
    }
    for ((source, category), m) in &groups {
        println!();
        println!("校準表：{source}（{category}）  對數損失 {:.4}", m.log_loss);
        println!("  預測機率區間   筆數     平均預測   實際發生率");
        for (i, (c, p, y)) in m.calibration.iter().enumerate() {
            if *c > 0 {
                println!(
                    "  {:.1}–{:.1}   {:>9}   {:>8.3}   {:>8.3}",
                    i as f64 / 10.0,
                    (i + 1) as f64 / 10.0,
                    c,
                    p,
                    y
                );
            }
        }
    }
    Ok(())
}
