//! `mj agree`：一致率。在牌譜（或策略四家自我對戰）的每個決策點，讓策略與一個說 mjai 的 AI（例如 Mortal）
//! 一起旁觀，比較策略、玩家與 AI 的選擇，依決策種類與「有沒有對手立直」分組（M8）。

use crate::review::print_agreement;
use mj_engine::{GameConfig, Player, run_game};
use mj_paifu::GameRecord;
use mj_review::{GameReview, Options as ReviewOptions, agreement, review_game};
use mj_rules::riichi::RiichiConfig;
use mj_sim::counterfactual::{Metric, Options as CfOptions};
use mj_strategy::find_strategy_spec;
use mj_strategy::framework::Strategy;
use rayon::prelude::*;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

/// 一致率設定。
pub struct Options {
    pub logs: Vec<PathBuf>,
    pub mjai: Vec<String>,
    pub strategy: String,
    pub selfplay: u64,
    pub seed: u64,
    pub json: Option<PathBuf>,
}

/// 策略四家自我對戰 `n` 場，轉成牌譜（經 mjai 牌譜格式，與匯入的牌譜走同一條路）。
fn selfplay_records(st: &Arc<dyn Strategy>, n: u64, seed: u64) -> Result<Vec<GameRecord>, String> {
    (0..n)
        .into_par_iter()
        .map(|g| {
            let mut players: [Box<dyn Player>; 4] =
                std::array::from_fn(|_| st.player(false, None) as Box<dyn Player>);
            let cfg = GameConfig::new(RiichiConfig::TENHOU, seed.wrapping_add(g));
            let r = run_game(&cfg, &mut players, true).map_err(|e| format!("第 {g} 場：{e}"))?;
            let names = std::array::from_fn(|s| format!("{}#{s}", st.name()));
            let text: Vec<String> = mj_paifu::mjai::events_to_mjai(&r.log, None, &names)
                .iter()
                .map(|v| v.to_string())
                .collect();
            mj_paifu::parse(
                &text.join("\n"),
                &format!("selfplay-{}", seed.wrapping_add(g)),
            )
        })
        .collect()
}

pub fn run(opts: &Options) -> Result<(), String> {
    if opts.mjai.is_empty() {
        return Err("--mjai 要指定 AI 的程式".into());
    }
    let st: Arc<dyn Strategy> = Arc::from(find_strategy_spec(&opts.strategy)?);
    let start = Instant::now();
    let (records, source) = if opts.selfplay > 0 {
        let recs = selfplay_records(&st, opts.selfplay, opts.seed)?;
        (
            recs,
            format!("自我對戰 {} 場（seed {}）", opts.selfplay, opts.seed),
        )
    } else {
        let (recs, errors) = mj_paifu::load_all(&opts.logs);
        for e in &errors {
            eprintln!("✗ {e}");
        }
        (recs, "牌譜".to_string())
    };
    if records.is_empty() {
        return Err("沒有牌譜（給牌譜路徑或 --selfplay 場數）".into());
    }
    let ro = ReviewOptions {
        seats: vec![0, 1, 2, 3],
        all: false,
        // 不做反事實評估（evaluate: false），這些設定用不到。
        eval: CfOptions {
            samples: 0,
            rollouts: 0,
            max_rollouts: 0,
            metric: Metric::Points,
            determinize: true,
            kinds: Vec::new(),
        },
        evaluate: false,
        mjai: Some(opts.mjai.clone()),
        advisors: Vec::new(),
        base_advisor: None,
    };
    let results: Vec<Result<GameReview, String>> = records
        .par_iter()
        .map(|rec| review_game(&st, rec, &ro).map_err(|e| format!("{}：{e}", rec.id)))
        .collect();
    let mut reviews = Vec::new();
    for r in results {
        match r {
            Ok(r) => reviews.push(r),
            Err(e) => eprintln!("✗ {e}"),
        }
    }
    let errors: usize = reviews.iter().map(|r| r.ai_errors).sum();
    if let Some(e) = reviews.iter().find_map(|r| r.ai_first_error.as_ref()) {
        println!("（AI 的回應有 {errors} 次無法對應到合法動作，例如：{e}）");
    }
    let rows = agreement(&reviews);
    println!(
        "策略 {}；AI {}；{source}，{} 場；{:.1} 秒",
        opts.strategy,
        opts.mjai.join(" "),
        reviews.len(),
        start.elapsed().as_secs_f64()
    );
    println!();
    // 自我對戰時「玩家」就是策略自己，不列玩家欄。
    print_agreement(&rows, if opts.selfplay > 0 { "" } else { "玩家" });
    if let Some(path) = &opts.json {
        let out = serde_json::json!({
            "strategy": opts.strategy,
            "ai": opts.mjai.join(" "),
            "source": source,
            "games": reviews.len(),
            "rows": rows,
        });
        let body = serde_json::to_string_pretty(&out).map_err(|e| e.to_string())?;
        std::fs::write(path, body).map_err(|e| format!("寫入 {} 失敗：{e}", path.display()))?;
        println!("已寫入 {}", path.display());
    }
    Ok(())
}
