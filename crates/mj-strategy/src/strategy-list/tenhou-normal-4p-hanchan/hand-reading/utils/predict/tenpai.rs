//! 預測：某家此刻是否聽牌。
//!
//! 判斷順序：
//! 1. 立直者一定聽牌。
//! 2. 基準值：依副露組數與巡目查表（[`BASE`]）。
//! 3. 依牌河的線索調整勝算（odds，見 `utils/odds.rs`）：
//!    - 最近 3 張中手切越多 → 手牌還在變化，越不像聽牌；
//!    - 最後一次手切的是中張牌（3–7）→ 拆搭子、手牌成形，較像聽牌；是字牌 → 還在整理孤立牌，較不像；
//!    - 連續摸切 → 手牌沒有變化，較像聽牌；
//!    - 有其他人立直 → 沒聽牌的人多半在防守，較不像聽牌（聽牌的人才會繼續推）。
//!
//! 數值以天鳳鳳凰卓牌譜（2026-10-06，434 場）的統計校準：`mj read-stats` 匯出特徵，
//! 以邏輯迴歸估計各線索的勝算倍數，再在另一天的牌譜上驗證（見 docs/03「讀牌」）。

use super::super::super::memory::Memory;
use super::super::odds;
use crate::prelude::*;

/// 巡目分組：0–3、4–5、6–7、8–9、10–11、12–13、14–15、16 以上（該家打過幾張）。
const TURN_GROUP: [usize; 8] = [0, 4, 6, 8, 10, 12, 14, 16];

/// 未立直者的基準聽牌率：`BASE[副露組數（3 以上算 3）][巡目分組]`。
/// 基準情況：最近 3 張手切 1 張、最後手切的不是中張也不是字牌、沒有連續摸切、沒有其他人立直。
pub const BASE: [[f64; 8]; 4] = [
    // 門前（沒立直）：天鳳的高手聽牌多半會立直，默聽很少
    [0.001, 0.010, 0.024, 0.043, 0.072, 0.098, 0.136, 0.171],
    // 副露 1 組
    [0.03, 0.09, 0.18, 0.26, 0.38, 0.45, 0.46, 0.48],
    // 副露 2 組
    [0.29, 0.29, 0.44, 0.50, 0.57, 0.64, 0.64, 0.64],
    // 副露 3 組以上
    [0.50, 0.55, 0.77, 0.82, 0.82, 0.86, 0.86, 0.86],
];
/// 最近 3 張中，每多 1 張手切（相對於 1 張），勝算乘上這個值。
pub const PER_RECENT_TEDASHI: f64 = 0.73;
/// 最後一次手切的是中張牌（3–7）。
pub const LAST_TEDASHI_MIDDLE: f64 = 1.44;
/// 最後一次手切的是字牌。
pub const LAST_TEDASHI_HONOR: f64 = 0.62;
/// 每 1 次連續摸切（最多算 4 次）。
pub const PER_TRAILING_TSUMOGIRI: f64 = 1.12;
/// 每 1 位其他人立直（最多算 2 位）。
pub const PER_OTHER_RIICHI: f64 = 0.67;

/// 常駐預測：每次輪到自己時，判斷其他三家是否聽牌（只為了評估預測準確度，不影響動作）。
pub fn each_turn(ctx: &Ctx, mem: &mut Memory) {
    for seat in tools::table::opponents(ctx.view) {
        predict(ctx, mem, seat);
    }
}

/// `seat` 此刻聽牌的機率，並記錄成預測與理由。
pub fn predict(ctx: &Ctx, mem: &Memory, seat: Seat) -> f64 {
    let p = estimate(ctx, mem, seat);
    let v = ctx.view;
    if !v.riichi[usize::from(seat)] && p >= 0.3 {
        let mut why = vec![format!(
            "副露 {} 組、第 {} 巡",
            tools::river::open_melds(v, seat),
            tools::river::turn(v, seat)
        )];
        let trail = tools::river::trailing_tsumogiri(v, seat);
        if trail >= 2 {
            why.push(format!("連續摸切 {trail} 次"));
        }
        if let Some((_, pai)) = tools::river::last_tedashi(v, seat) {
            why.push(format!("最後手切 {pai}"));
        }
        ctx.note(format!(
            "座位 {seat} 可能已聽牌（{:.0}%）：{}",
            p * 100.0,
            why.join("、")
        ));
    }
    ctx.claim("predict::tenpai", Claim::Tenpai { seat, p });
    p
}

/// `seat` 此刻聽牌的機率（只計算，不記錄預測也不寫理由；決策中反覆使用時用這個）。
pub fn estimate(ctx: &Ctx, _mem: &Memory, seat: Seat) -> f64 {
    let v = ctx.view;
    let s = usize::from(seat);
    if v.riichi[s] {
        return 1.0;
    }
    let melds = tools::river::open_melds(v, seat).min(3);
    let turn = tools::river::turn(v, seat);
    let group = TURN_GROUP.iter().rposition(|&t| turn >= t).unwrap_or(0);
    let mut factor = 1.0;
    let tedashi = tools::river::recent_tedashi(v, seat, 3) as i32;
    factor *= PER_RECENT_TEDASHI.powi(tedashi - 1);
    match tools::river::last_tedashi(v, seat) {
        Some((_, pai)) if tools::river::is_middle(pai.tile) => factor *= LAST_TEDASHI_MIDDLE,
        Some((_, pai)) if pai.tile.is_honor() => factor *= LAST_TEDASHI_HONOR,
        _ => {}
    }
    let trail = tools::river::trailing_tsumogiri(v, seat).min(4) as i32;
    factor *= PER_TRAILING_TSUMOGIRI.powi(trail);
    let others_riichi = (0..4).filter(|&o| o != s && v.riichi[o]).count().min(2) as i32;
    factor *= PER_OTHER_RIICHI.powi(others_riichi);
    odds::scale(BASE[melds][group], factor)
}
