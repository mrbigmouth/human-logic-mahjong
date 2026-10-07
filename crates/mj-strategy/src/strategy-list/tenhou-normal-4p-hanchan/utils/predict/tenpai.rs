//! 預測：某家此刻是否聽牌。
//!
//! 判斷順序：
//! 1. 立直者一定聽牌。
//! 2. 依巡目與副露數粗估基準值。
//! 3. 中巡以後連續摸切 2 次以上（記在對局記憶中）→ 手牌沒有變化，提高聽牌機率：
//!    - 副露者連續摸切：很可能已聽牌在等，每次 +5%；
//!    - 門前不立直者連續摸切：多半是棄和或手牌卡住（聽牌的人通常會立直），每次只 +1%。
//!
//!    依據（300 局自我對戰的分組統計）：副露 1 組、第 10–13 巡，摸切 0 次時實際聽牌 55%、
//!    4 次以上 73%；門前不立直者不論摸切幾次，實際聽牌都在 2% 以下。
//!
//! 準確度由 `mj predict-eval` 對照全知視角評分。

use super::super::super::memory::Memory;
use crate::prelude::*;

/// 常駐預測：每次輪到自己時，判斷其他三家是否聽牌（只為了評估預測準確度，不影響動作）。
pub fn each_turn(ctx: &Ctx, mem: &mut Memory) {
    for seat in tools::table::opponents(ctx.view) {
        predict(ctx, mem, seat);
    }
}

/// `seat` 此刻聽牌的機率。
pub fn predict(ctx: &Ctx, mem: &Memory, seat: Seat) -> f64 {
    let s = usize::from(seat);
    let p = if ctx.view.riichi[s] {
        1.0
    } else {
        let turn = ctx.view.rivers[s].len() as f64;
        let melds = ctx.view.melds[s].iter().filter(|m| m.is_open()).count() as f64;
        // 越後面的巡目、副露越多，越可能聽牌。
        let mut p: f64 = (0.02 + 0.025 * turn + 0.12 * melds).min(0.9);
        // 中巡以後連續摸切：手牌沒有變化。
        let streak = mem.tsumogiri_streak[s].min(4);
        if streak >= 2 {
            if melds > 0.0 {
                p = (p + 0.05 * f64::from(streak)).min(0.95);
                ctx.note(format!(
                    "座位 {seat} 副露後連續摸切 {streak} 次，可能已聽牌（{:.0}%）",
                    p * 100.0
                ));
            } else {
                p = (p + 0.01 * f64::from(streak)).min(0.95);
                ctx.note(format!(
                    "座位 {seat} 門前連續摸切 {streak} 次但沒立直，多半還沒聽牌（{:.0}%）",
                    p * 100.0
                ));
            }
        }
        p
    };
    ctx.claim("predict::tenpai", Claim::Tenpai { seat, p });
    p
}
