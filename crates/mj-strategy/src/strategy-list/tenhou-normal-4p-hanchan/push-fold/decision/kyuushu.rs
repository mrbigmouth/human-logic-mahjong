//! 九種九牌：第一巡手上有 9 種以上幺九牌時，要不要流局重來。

use super::super::memory::Memory;
use crate::prelude::*;

/// 回傳 true = 流局。
pub fn decide(ctx: &Ctx, _mem: &mut Memory) -> bool {
    let shanten = ctx.shanten();
    if shanten <= 3 {
        ctx.note(format!(
            "手牌 {shanten} 向聽（可能是國士無雙的路線），繼續打"
        ));
        return false;
    }
    ctx.note(format!("手牌 {shanten} 向聽，太差了，流局重來"));
    true
}
