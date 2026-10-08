//! `mj simulate`：duplicate（複式）對戰。同一組牌山輪換座位打 4 場，比較各方的平均順位與名次點；
//! 兩方對戰時另外輸出成對差距與 95% 信賴區間。

use crate::selfplay::{make_player, parse_entrant};
use mj_engine::Player;
use mj_sim::duplicate;
use mj_strategy::framework::explain::{display_width, pad, pad_left};
use std::time::Instant;

pub fn run(entrants: &[String], blocks: u64, seed: u64) -> Result<(), String> {
    if entrants.len() != 2 && entrants.len() != 4 {
        return Err("simulate 需要 2 個（兩方各坐兩個位置）或 4 個參賽者".into());
    }
    let seats = entrants
        .iter()
        .map(|e| parse_entrant(e))
        .collect::<Result<Vec<_>, _>>()?;
    // 同名的參賽者（例如 A/A 測試）加上編號以便區分。
    let names: Vec<String> = seats
        .iter()
        .enumerate()
        .map(|(i, s)| {
            if seats.iter().filter(|o| o.label == s.label).count() > 1 {
                format!("{}#{}", s.label, i + 1)
            } else {
                s.label.clone()
            }
        })
        .collect();
    let make = |who: usize, seed: u64, seat: u8| -> Box<dyn Player> {
        make_player(&seats[who], seed, u64::from(seat))
    };
    let start = Instant::now();
    let report = duplicate::run(&names, blocks, seed, &make)?;
    println!(
        "duplicate 對戰：{blocks} 組 × 4 場（每組同一牌山、座位輪換），{:.1} 秒",
        start.elapsed().as_secs_f64()
    );
    println!();
    let w = names
        .iter()
        .map(|n| display_width(n))
        .max()
        .unwrap_or(8)
        .max(8)
        + 2;
    println!(
        "{}{}{}{}",
        pad("參賽者", w),
        pad_left("座位場數", 10),
        pad_left("平均順位", 10),
        pad_left("平均名次點", 12)
    );
    for e in &report.entrants {
        println!(
            "{}{}{}{}",
            pad(&e.name, w),
            pad_left(&e.games.to_string(), 10),
            pad_left(&format!("{:.3}", e.avg_rank), 10),
            pad_left(&format!("{:+.2}", e.avg_points), 12)
        );
    }
    if let Some(c) = report.comparison {
        println!();
        let verdict = |diff: f64, ci: f64, better_when_negative: bool| {
            if diff.abs() <= ci {
                "差距在誤差範圍內（不顯著）"
            } else if (diff < 0.0) == better_when_negative {
                "前者顯著較好"
            } else {
                "後者顯著較好"
            }
        };
        println!(
            "{} − {}：平均順位差 {:+.3} ± {:.3}（{}）",
            names[0],
            names[1],
            c.rank_diff,
            c.rank_ci,
            verdict(c.rank_diff, c.rank_ci, true)
        );
        println!(
            "{} − {}：名次點差 {:+.2} ± {:.2}（{}）",
            names[0],
            names[1],
            c.points_diff,
            c.points_ci,
            verdict(c.points_diff, c.points_ci, false)
        );
        println!("（± 為 95% 信賴區間，以每組的成對差距計算）");
    }
    Ok(())
}
