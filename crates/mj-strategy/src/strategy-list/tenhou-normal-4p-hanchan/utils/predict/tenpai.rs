//! 預測：某家此刻是否聽牌。
//!
//! 立直者一定聽牌；其餘依副露數與巡目粗估。準確度由 `mj predict-eval` 對照全知視角評分。

use crate::prelude::*;

/// `seat` 此刻聽牌的機率。
pub fn predict(ctx: &Ctx, seat: Seat) -> f64 {
    let s = usize::from(seat);
    let p = if ctx.view.riichi[s] {
        1.0
    } else {
        let turn = ctx.view.rivers[s].len() as f64;
        let melds = ctx.view.melds[s].iter().filter(|m| m.is_open()).count() as f64;
        // 越後面的巡目、副露越多，越可能聽牌。
        let base: f64 = 0.02 + 0.025 * turn + 0.12 * melds;
        base.min(0.9)
    };
    ctx.claim("predict::tenpai", Claim::Tenpai { seat, p });
    p
}
