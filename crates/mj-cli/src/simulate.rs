//! `mj simulate`：duplicate（複式）對戰。同一組牌山輪換座位打 4 場，比較各方的平均順位與名次點；
//! 兩方對戰時另外輸出成對差距與 95% 信賴區間。`--field X`：兩方各自和三家 X 同桌（場地對戰）。

use crate::selfplay::{make_player, parse_entrant};
use mj_engine::Player;
use mj_sim::duplicate;
use mj_strategy::framework::explain::{display_width, pad, pad_left};
use std::time::Instant;

pub fn run(entrants: &[String], blocks: u64, seed: u64, field: Option<&str>) -> Result<(), String> {
    if field.is_some() && entrants.len() != 2 {
        return Err("--field 需要 2 個參賽者".into());
    }
    if entrants.len() != 2 && entrants.len() != 4 {
        return Err("simulate 需要 2 個（兩方各坐兩個位置）或 4 個參賽者".into());
    }
    let all: Vec<String> = entrants
        .iter()
        .cloned()
        .chain(field.map(String::from))
        .collect();
    let seats = all
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
    let report = match field {
        Some(_) => {
            let names = [names[0].clone(), names[1].clone(), names[2].clone()];
            let r = duplicate::run_field(&names, blocks, seed, &make)?;
            println!(
                "場地對戰：{blocks} 組 × 8 場（每組同一牌山；兩方各自和三家 {} 同桌、輪換座位），{:.1} 秒",
                names[2],
                start.elapsed().as_secs_f64()
            );
            r
        }
        None => {
            let r = duplicate::run(&names, blocks, seed, &make)?;
            println!(
                "duplicate 對戰：{blocks} 組 × 4 場（每組同一牌山、座位輪換），{:.1} 秒",
                start.elapsed().as_secs_f64()
            );
            r
        }
    };
    println!();
    let w = report
        .entrants
        .iter()
        .map(|e| &e.name)
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
    println!();
    println!(
        "{}{}{}{}{}{}{}{}",
        pad("每局統計", w),
        pad_left("和了率", 9),
        pad_left("榮和", 8),
        pad_left("默聽和", 8),
        pad_left("副露和", 8),
        pad_left("平均和了", 10),
        pad_left("放銃率", 9),
        pad_left("平均放銃", 10)
    );
    for e in &report.entrants {
        let h = &e.hands;
        let pct = |n: u64| format!("{:.1}%", h.rate(n) * 100.0);
        let avg = |sum: i64, n: u64| format!("{:.0}", sum as f64 / n.max(1) as f64);
        println!(
            "{}{}{}{}{}{}{}{}",
            pad(&e.name, w),
            pad_left(&pct(h.ron + h.tsumo), 9),
            pad_left(&pct(h.ron), 8),
            pad_left(&pct(h.dama_wins), 8),
            pad_left(&pct(h.open_wins), 8),
            pad_left(&avg(h.win_points, h.ron + h.tsumo), 10),
            pad_left(&pct(h.deal_ins), 9),
            pad_left(&avg(h.deal_in_points, h.deal_ins), 10)
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
