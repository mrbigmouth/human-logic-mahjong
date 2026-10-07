//! `mj evaluate`：決策品質評估（反事實模擬），依決策種類統計平均損失。

use mj_sim::counterfactual::{DecisionEval, Options as CfOptions, evaluate_game, summarize};
use mj_strategy::find_strategy;
use mj_strategy::framework::explain::{pad, pad_left, render_record};
use std::sync::Arc;
use std::time::Instant;

/// 評估設定。
pub struct Options<'a> {
    pub strategy: &'a str,
    pub games: u64,
    pub seed: u64,
    pub samples: usize,
    pub rollouts: usize,
    pub worst: usize,
}

pub fn run(o: &Options) -> Result<(), String> {
    let st: Arc<dyn mj_strategy::framework::Strategy> =
        Arc::from(find_strategy(o.strategy).ok_or_else(|| format!("找不到策略 {}", o.strategy))?);
    let start = Instant::now();
    let mut evals: Vec<DecisionEval> = Vec::new();
    for g in 0..o.games {
        evals.extend(evaluate_game(
            st.clone(),
            o.seed.wrapping_add(g),
            CfOptions {
                samples: o.samples,
                rollouts: o.rollouts,
            },
        )?);
    }
    println!(
        "策略 {}：{} 場對局，評估 {} 個決策點，每個選項重打 {} 次（{:.1} 秒）",
        o.strategy,
        o.games,
        evals.len(),
        o.rollouts,
        start.elapsed().as_secs_f64()
    );
    println!();
    println!(
        "{}{}{}{}{}",
        pad("決策種類", 12),
        pad_left("評估數", 8),
        pad_left("選到最佳", 10),
        pad_left("平均損失(點)", 14),
        pad_left("顯著損失比例", 14)
    );
    for (kind, s) in summarize(&evals) {
        println!(
            "{}{}{}{}{}",
            pad(kind.name(), 12),
            pad_left(&s.n.to_string(), 8),
            pad_left(&format!("{:.1}%", s.agree * 100.0), 10),
            pad_left(&format!("{:.0}", s.mean_loss), 14),
            pad_left(&format!("{:.1}%", s.significant * 100.0), 14)
        );
    }
    println!();
    println!("損失 = 期望值最高的選項 − 實際選擇的期望值（這一局的點數得失）。");
    println!("「顯著」= 損失大於 2 倍標準誤；其餘的差距可能只是重打次數不夠造成的雜訊。");

    evals.sort_by(|a, b| b.loss.total_cmp(&a.loss));
    if o.worst > 0 && !evals.is_empty() {
        println!();
        println!("損失最大的 {} 個決策：", o.worst.min(evals.len()));
        for e in evals.iter().take(o.worst) {
            println!();
            println!(
                "seed {}、第 {} 局、座位 {}：損失 {:.0} ± {:.0} 點",
                e.game_seed, e.record.hand_index, e.seat, e.loss, e.loss_se
            );
            print!("{}", render_record(&e.record));
            for (i, (label, ev)) in e.record.options.iter().zip(&e.option_ev).enumerate() {
                let mark = match (i == e.record.chosen, i == e.best) {
                    (true, true) => "◎ 選擇＝最佳",
                    (true, false) => "← 實際選擇",
                    (false, true) => "★ 最佳",
                    _ => "",
                };
                println!("    {} {:>8.0}  {mark}", pad(label, 16), ev);
            }
        }
    }
    Ok(())
}
