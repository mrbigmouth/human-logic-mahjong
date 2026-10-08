//! 立直與否：打出 `ctx.riichi_discard()` 後聽牌，要不要立直。

use super::super::memory::Memory;
use crate::prelude::*;

/// 回傳 true = 立直。
pub fn decide(ctx: &Ctx, _mem: &mut Memory) -> bool {
    let pai = ctx.riichi_discard().expect("立直決策一定有要打的牌");
    let waits = ctx.after_discard(pai).ukeire;
    if waits < 2.0 {
        ctx.note(format!("聽牌張只剩 {waits} 張，默聽等改聽"));
        return false;
    }
    ctx.note(format!("聽牌張剩 {waits} 張，立直"));
    true
}
