//! 和牌與否：能自摸或榮和時要不要和。

use super::super::memory::Memory;
use crate::prelude::*;

/// 回傳 true = 和牌。
pub fn decide(ctx: &Ctx, _mem: &mut Memory) -> bool {
    // 目前一律和牌。之後可以加入：最終局和了反而名次下降時不和（見逃）等判斷。
    ctx.note(match ctx.win_kind() {
        Some(WinKind::Tsumo) => "自摸，和牌",
        Some(WinKind::Chankan) => "搶槓，和牌",
        _ => "榮和，和牌",
    });
    true
}
