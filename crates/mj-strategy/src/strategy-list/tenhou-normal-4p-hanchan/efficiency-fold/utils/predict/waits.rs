//! 預測：某家此刻是否聽某種牌（打出去會不會放銃給他）。
//!
//! 依序套用人類常用的安全度判斷：現物 → 字牌 → 筋 → No Chance → 一般危險牌。
//! 記憶中若此人用過筋牌騙，筋牌的安全度打折。

use super::super::super::memory::Memory;
use crate::prelude::*;

/// `seat` 此刻聽 `tile` 的機率（假設他已聽牌，例如已立直）。
pub fn predict(ctx: &Ctx, mem: &Memory, seat: Seat, tile: Tile) -> f64 {
    let v = ctx.view;
    let p = if tools::safety::is_genbutsu(v, seat, tile) {
        0.0
    } else if tile.is_honor() {
        // 字牌：看得到越多張越安全（3 張以上只剩單騎、且最多 1 張）。
        match tools::safety::honor_visible(v, tile) {
            0 | 1 => 0.06,
            2 => 0.03,
            _ => 0.01,
        }
    } else if tools::safety::is_suji(v, seat, tile) {
        // 筋牌：只剩單騎、對碰、嵌張等聽法。用過筋牌騙的人，筋牌不可信。
        let trap = mem.suji_trap_wins[usize::from(seat)] > 0;
        let base = if matches!(tile.number(), 1 | 9) {
            0.02
        } else {
            0.04
        };
        if trap { base * 2.5 } else { base }
    } else if tools::safety::is_no_chance(v, tile) {
        0.03
    } else {
        // 無筋的數牌：中張比老頭牌危險。
        match tile.number() {
            1 | 9 => 0.06,
            2 | 8 => 0.09,
            _ => 0.12,
        }
    };
    ctx.claim("predict::waits", Claim::Waits { seat, tile, p });
    p
}
