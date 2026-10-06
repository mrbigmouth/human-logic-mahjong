//! 從標準輸入每行讀一手牌（緊湊記法），輸出向聽數。
//! 用於與其他實作做差異測試（見 tools/oracle/check_shanten.py）。

use mj_calc::shanten;
use mj_core::{TileCounts, parse_tiles};
use std::io::{BufRead, Write};

fn main() {
    let stdin = std::io::stdin();
    let mut out = std::io::BufWriter::new(std::io::stdout());
    for line in stdin.lock().lines() {
        let line = line.unwrap();
        let hand = TileCounts::from_tiles(parse_tiles(line.trim()).expect("valid hand"));
        writeln!(out, "{}", shanten(&hand)).unwrap();
    }
}
