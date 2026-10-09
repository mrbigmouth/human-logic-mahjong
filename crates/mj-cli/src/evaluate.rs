//! `mj evaluate`：決策品質評估（反事實模擬），依決策種類與情境統計平均損失。

use mj_sim::counterfactual::{
    DecisionEval, Dimension, GroupSummary, Metric, Options as CfOptions, by_dimension,
    evaluate_game, summarize,
};
use mj_strategy::find_strategy_spec;
use mj_strategy::framework::DecisionKind;
use mj_strategy::framework::explain::{pad, pad_left, render_record};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

/// 評估設定。
pub struct Options<'a> {
    pub strategy: &'a str,
    pub games: u64,
    pub seed: u64,
    pub samples: usize,
    pub rollouts: usize,
    pub max_rollouts: usize,
    pub metric: Metric,
    pub omniscient: bool,
    pub worst: usize,
    pub json: Option<PathBuf>,
    pub kinds: Vec<DecisionKind>,
}

/// 一列統計。
fn row(label: &str, s: &GroupSummary, unit: &str) -> String {
    format!(
        "{}{}{}{}{}",
        pad(label, 16),
        pad_left(&s.n.to_string(), 8),
        pad_left(&format!("{:.1}%", s.agree * 100.0), 10),
        pad_left(
            &format!("{:.2}±{:.2}{unit}", s.mean_loss, s.mean_loss_se),
            22
        ),
        pad_left(&format!("{:.1}%", s.significant * 100.0), 14)
    )
}

fn header(first: &str) -> String {
    format!(
        "{}{}{}{}{}",
        pad(first, 16),
        pad_left("評估數", 8),
        pad_left("選到最佳", 10),
        pad_left("平均損失", 22),
        pad_left("顯著損失比例", 14)
    )
}

pub fn run(o: &Options) -> Result<(), String> {
    let st: Arc<dyn mj_strategy::framework::Strategy> = Arc::from(find_strategy_spec(o.strategy)?);
    let opts = CfOptions {
        samples: o.samples,
        rollouts: o.rollouts,
        max_rollouts: o.max_rollouts.max(o.rollouts),
        metric: o.metric,
        determinize: !o.omniscient,
        kinds: o.kinds.clone(),
    };
    let unit = match o.metric {
        Metric::Placement => "pt",
        Metric::Points => "點",
    };
    let start = Instant::now();
    let mut evals: Vec<DecisionEval> = Vec::new();
    let mut skipped = 0;
    for g in 0..o.games {
        let (e, s) = evaluate_game(st.clone(), o.seed.wrapping_add(g), &opts)?;
        evals.extend(e);
        skipped += s;
    }
    let fallbacks: usize = evals.iter().map(|e| e.riichi_fallbacks).sum();
    let avg_rollouts =
        evals.iter().map(|e| e.rollouts).sum::<usize>() as f64 / evals.len().max(1) as f64;
    println!(
        "策略 {}：{} 場對局，評估 {} 個決策點（略過 {}），平均每個選項重打 {:.1} 次（{:.1} 秒）",
        o.strategy,
        o.games,
        evals.len(),
        skipped,
        avg_rollouts,
        start.elapsed().as_secs_f64()
    );
    println!(
        "衡量方式：{}；對手手牌：{}{}",
        match o.metric {
            Metric::Placement => "名次點（打到整場結束，含順位馬與返點）",
            Metric::Points => "這一局的點數得失",
        },
        if o.omniscient {
            "使用真實手牌（全知）"
        } else {
            "依公開資訊重抽"
        },
        if fallbacks > 0 {
            format!("（立直者重抽失敗 {fallbacks} 次）")
        } else {
            String::new()
        }
    );
    println!();
    println!("{}", header("決策種類"));
    for (kind, s) in summarize(&evals) {
        println!("{}", row(kind.name(), &s, unit));
    }

    for kind in [DecisionKind::Discard, DecisionKind::Call] {
        if !evals.iter().any(|e| e.record.kind == kind) {
            continue;
        }
        for dim in Dimension::ALL {
            println!();
            println!("{}", header(&format!("{}×{}", kind.name(), dim.name())));
            for (label, s) in by_dimension(&evals, Some(kind), dim) {
                println!("{}", row(&label, &s, unit));
            }
        }
    }
    println!();
    println!("損失 = 期望值最高的選項 − 實際選擇的期望值。「顯著」= 損失大於 2 倍標準誤；");
    println!(
        "差距不顯著時會自動增加重打次數（最多 {} 次），仍不顯著的差距可能只是雜訊。",
        opts.max_rollouts
    );

    if let Some(path) = &o.json {
        let text = serde_json::to_string_pretty(&evals).map_err(|e| e.to_string())?;
        std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))?;
        println!("評估結果已寫入 {}", path.display());
    }

    let mut sorted: Vec<&DecisionEval> = evals.iter().filter(|e| e.significant()).collect();
    sorted.sort_by(|a, b| b.loss.total_cmp(&a.loss));
    if o.worst > 0 && !sorted.is_empty() {
        println!();
        println!(
            "損失最大的顯著決策（前 {} 個）：",
            o.worst.min(sorted.len())
        );
        for e in sorted.iter().take(o.worst) {
            println!();
            println!(
                "seed {}、第 {} 局、座位 {}：損失 {:.2} ± {:.2}{unit}（重打 {} 次）",
                e.game_seed, e.record.hand_index, e.seat, e.loss, e.loss_se, e.rollouts
            );
            print!("{}", render_record(&e.record));
            for (i, (label, ev)) in e.record.options.iter().zip(&e.option_ev).enumerate() {
                let mark = match (i == e.record.chosen, i == e.best) {
                    (true, true) => "◎ 選擇＝最佳",
                    (true, false) => "← 實際選擇",
                    (false, true) => "★ 最佳",
                    _ => "",
                };
                println!("    {} {:>9.2}  {mark}", pad(label, 16), ev);
            }
        }
    }
    Ok(())
}
