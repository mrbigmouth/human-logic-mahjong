//! 防守輔助：每張牌對立直者的放銃機率（取各立直者中最危險的值）。

use super::predict::waits;
use crate::prelude::*;

/// 打出 `pai` 對所有立直者的放銃機率（取最大值）。沒有人立直時為 0。
pub fn deal_in_risk(ctx: &Ctx, mem: &super::super::memory::Memory, pai: Pai) -> f64 {
    tools::table::riichi_opponents(ctx.view)
        .into_iter()
        .map(|seat| waits::predict(ctx, mem, seat, pai.tile))
        .fold(0.0, f64::max)
}
