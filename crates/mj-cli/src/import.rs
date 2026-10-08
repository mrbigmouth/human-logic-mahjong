//! `mj import`：匯入牌譜，用引擎重播並與原牌譜的點數比對。

use mj_engine::{Event, HandEnd};
use mj_paifu::replay::replay;
use mj_paifu::{GameRecord, load_all};
use rayon::prelude::*;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// 一場重播的摘要。
struct Summary {
    hands: usize,
    wins: usize,
    double_rons: usize,
    draws: usize,
    decisions: usize,
}

fn summarize(log: &[Event], decisions: usize) -> Summary {
    let mut s = Summary {
        hands: 0,
        wins: 0,
        double_rons: 0,
        draws: 0,
        decisions,
    };
    let mut wins_this_hand = 0;
    for e in log {
        match e {
            Event::StartKyoku { .. } => {
                s.hands += 1;
                wins_this_hand = 0;
            }
            Event::HandEnd {
                end: HandEnd::Win { .. },
                ..
            } => {
                s.wins += 1;
                wins_this_hand += 1;
                if wins_this_hand == 2 {
                    s.double_rons += 1;
                }
            }
            Event::HandEnd {
                end: HandEnd::Draw { .. },
                ..
            } => s.draws += 1,
            _ => {}
        }
    }
    s
}

fn one(rec: &GameRecord, dump: Option<&Path>) -> Result<Summary, String> {
    let r = replay(rec, [None, None, None, None], true).map_err(|e| format!("{}: {e}", rec.id))?;
    if let Some(dir) = dump {
        let path = dir.join(format!("{}.events.json", rec.id));
        let text = serde_json::to_string(&r.result).map_err(|e| e.to_string())?;
        std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(summarize(&r.result.log, r.points.len()))
}

pub fn run(paths: &[PathBuf], dump: Option<PathBuf>, show_errors: usize) -> Result<(), String> {
    let start = Instant::now();
    let (records, mut errors) = load_all(paths);
    let parse_failures = errors.len();
    if let Some(d) = &dump {
        std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
    }
    let results: Vec<Result<Summary, String>> = records
        .par_iter()
        .map(|r| one(r, dump.as_deref()))
        .collect();
    let mut ok = Vec::new();
    for r in results {
        match r {
            Ok(s) => ok.push(s),
            Err(e) => errors.push(e),
        }
    }
    let sum = |f: fn(&Summary) -> usize| ok.iter().map(f).sum::<usize>();
    println!(
        "牌譜 {} 場：重播成功 {} 場，解析失敗 {}，重播或比對失敗 {}（{:.1} 秒）",
        records.len() + parse_failures,
        ok.len(),
        parse_failures,
        errors.len() - parse_failures,
        start.elapsed().as_secs_f64()
    );
    println!(
        "成功的對局共 {} 局：和牌 {}（雙響 {} 次）、流局 {}；決策點（詢問）{} 個",
        sum(|s| s.hands),
        sum(|s| s.wins),
        sum(|s| s.double_rons),
        sum(|s| s.draws),
        sum(|s| s.decisions)
    );
    println!("重播成功 = 每局開始的局數、本場、供託、四家點數，每局結束與終局點數都與原牌譜一致。");
    if let Some(d) = &dump {
        println!("引擎事件紀錄已寫入 {}", d.display());
    }
    for e in errors.iter().take(show_errors) {
        println!("  ✗ {e}");
    }
    if errors.len() > show_errors {
        println!("  ……另有 {} 個失敗", errors.len() - show_errors);
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(format!("{} 個牌譜無法匯入", errors.len()))
    }
}
