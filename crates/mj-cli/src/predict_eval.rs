//! `mj predict-eval`：預測準確度評估（與勝負無關，只看預測對不對）。

use mj_sim::predict_eval::{
    Dimension, Judged, by_dimension, collect, collect_mixed, collect_records, metrics, summarize,
};
use mj_strategy::find_strategy_spec;
use mj_strategy::framework::explain::{pad, pad_left};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

/// `--seats`：四個座位放不同的策略（輪換座位），依「被預測的是哪個策略」分組，
/// 看讀牌者對每種打法讀得準不準（例如印象操作有沒有騙到人）。
pub fn run_mixed(seats: &str, games: u64, seed: u64) -> Result<(), String> {
    let names: Vec<&str> = seats.split(',').map(str::trim).collect();
    let [a, b, c, d] = names[..] else {
        return Err("--seats 要四個策略，以逗號分隔".into());
    };
    let names = [a, b, c, d];
    let mut strategies = Vec::new();
    for n in names {
        strategies.push(Arc::from(find_strategy_spec(n)?));
    }
    let strategies: [Arc<dyn mj_strategy::framework::Strategy>; 4] =
        strategies.try_into().map_err(|_| "四個策略")?;
    let start = Instant::now();
    let judged = collect_mixed(&strategies, games, seed)?;
    println!(
        "四家 {seats}：{games} 場（輪換座位），判定 {} 筆預測（{:.1} 秒）",
        judged.len(),
        start.elapsed().as_secs_f64()
    );
    // 同名的座位合併。
    let mut targets: Vec<&str> = Vec::new();
    for n in names {
        if !targets.contains(&n) {
            targets.push(n);
        }
    }
    let sources: std::collections::BTreeSet<(String, &'static str)> = judged
        .iter()
        .filter(|(t, _)| t.is_some())
        .map(|(_, j)| (j.source.clone(), j.category))
        .collect();
    for (source, category) in &sources {
        println!();
        println!("{source}（{category}）：依被預測者分組");
        println!(
            "{}{}{}{}{}{}{}{}",
            pad("被預測者", 44),
            pad_left("筆數", 9),
            pad_left("平均預測", 10),
            pad_left("實際發生率", 12),
            pad_left("Brier", 9),
            pad_left("技巧分數", 10),
            pad_left("發生時預測", 12),
            pad_left("沒發生時預測", 14)
        );
        for t in &targets {
            // 全部，以及只看沒立直的被預測者（立直者一定聽牌，印象操作只在沒立直時有作用）。
            for (label, unriichi_only) in [("", false), ("（未立直）", true)] {
                let items: Vec<&Judged> = judged
                    .iter()
                    .filter(|(e, j)| {
                        e.is_some_and(|e| names[e] == *t)
                            && j.source == *source
                            && j.category == *category
                            && !(unriichi_only && j.target.riichi)
                    })
                    .map(|(_, j)| j)
                    .collect();
                if items.is_empty() {
                    continue;
                }
                let m = metrics(&items);
                let cond = |truth: bool| {
                    let v: Vec<f64> = items
                        .iter()
                        .filter(|j| j.truth == truth)
                        .map(|j| j.p)
                        .collect();
                    v.iter().sum::<f64>() / v.len().max(1) as f64
                };
                println!(
                    "{}{}{}{}{}{}{}{}",
                    pad(&format!("{t}{label}"), 44),
                    pad_left(&m.n.to_string(), 9),
                    pad_left(&format!("{:.3}", m.mean_p), 10),
                    pad_left(&format!("{:.3}", m.base_rate), 12),
                    pad_left(&format!("{:.4}", m.brier), 9),
                    pad_left(&format!("{:+.3}", m.skill), 10),
                    pad_left(&format!("{:.3}", cond(true)), 12),
                    pad_left(&format!("{:.3}", cond(false)), 14)
                );
            }
        }
    }
    println!();
    println!(
        "「發生時預測」= 實際聽牌（或聽這張、或大牌）時讀牌者給的平均機率；被騙時會偏低。「沒發生時預測」被騙時會偏高。"
    );
    Ok(())
}

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
