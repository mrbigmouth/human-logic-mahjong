//! 鳴牌：他家打牌時要不要吃、碰、槓。
//!
//! 思考順序：
//! 1. 有人立直、自己離聽牌還遠 → 不鳴（棄和中）。
//! 2. 鳴了沒有役 → 不鳴（無法和牌）。
//! 3. 鳴了向聽數沒有前進 → 不鳴（白白破壞門前）。
//! 4. 其餘選向聽前進最多、進張最多的鳴法。

use super::super::memory::Memory;
use crate::prelude::*;

/// 回傳鳴牌選擇（必須是 `ctx.call_options()` 之一，含 `CallChoice::Pass`）。
pub fn decide(ctx: &Ctx, _mem: &mut Memory) -> CallChoice {
    let now = ctx.after_call(CallChoice::Pass);
    if !tools::table::riichi_opponents(ctx.view).is_empty() && now.shanten >= 2 {
        ctx.note(format!("對手立直，自己 {} 向聽，棄和中不鳴", now.shanten));
        return CallChoice::Pass;
    }
    let mut best: Option<(CallChoice, AfterEval)> = None;
    for &c in ctx.call_options() {
        if c == CallChoice::Pass {
            continue;
        }
        let action = crate::framework::ctx::call_action(c);
        if tools::yaku_path::yaku_after_call(ctx.view, &action) == Some(OpenYakuPath::None) {
            ctx.note(format!("{c}：鳴了沒有役，無法和牌"));
            continue;
        }
        let after = ctx.after_call(c);
        if after.shanten >= now.shanten {
            ctx.note(format!(
                "{c}：向聽數沒有前進（{} → {}），不值得破壞門前",
                now.shanten, after.shanten
            ));
            continue;
        }
        let better =
            best.is_none_or(|(_, b)| (after.shanten, -after.ukeire) < (b.shanten, -b.ukeire));
        if better {
            best = Some((c, after));
        }
    }
    match best {
        Some((c, after)) => {
            ctx.note(format!(
                "{c}：{} 向聽 → {} 向聽，有役，鳴",
                now.shanten, after.shanten
            ));
            c
        }
        None => CallChoice::Pass,
    }
}
