//! 鳴牌：他家打牌時要不要吃、碰、槓。
//!
//! 思考順序：
//! 1. 鳴了沒有役 → 不鳴（無法和牌）。
//! 2. 鳴了向聽數沒有前進 → 不鳴（白白破壞門前）。
//! 3. 有人立直 → 只有鳴了就聽牌才鳴（還沒聽牌就鳴，只是少了手牌、多打一張危險牌）。
//! 4. 不是役牌、鳴了還沒聽牌、寶牌不到 3 張 → 不鳴（又遠又便宜的鳴牌，不如門前做立直）。
//! 5. 其餘選向聽前進最多、進張最多的鳴法。

use super::super::memory::Memory;
use super::super::utils::value;
use crate::prelude::*;

/// 不是役牌的鳴牌，鳴了還在這個向聽數以上時，需要足夠的寶牌才鳴。
/// （M8 以 duplicate 調整：2 → 1，也就是非役牌的鳴牌除非鳴了就聽牌，否則要有足夠寶牌。）
pub const FAR_SHANTEN: Param = Param::new(
    "call.far_shanten",
    1.0,
    1.0,
    4.0,
    "非役牌鳴牌後還在這個向聽以上，需要足夠寶牌",
);
/// 「又遠又便宜」的界線：寶牌少於這麼多張就不鳴。（M8：2 → 3）
pub const MIN_DORA_FOR_FAR_CALL: Param = Param::new(
    "call.min_dora_for_far_call",
    3.0,
    0.0,
    4.0,
    "遠的非役牌鳴牌需要的寶牌張數",
);

/// 回傳鳴牌選擇（必須是 `ctx.call_options()` 之一，含 `CallChoice::Pass`）。
pub fn decide(ctx: &Ctx, _mem: &mut Memory) -> CallChoice {
    let now = ctx.after_call(CallChoice::Pass);
    let riichi = !tools::table::riichi_opponents(ctx.view).is_empty();
    let dora = value::dora_in(ctx, &ctx.hand(), 0);
    let mut best: Option<(CallChoice, AfterEval)> = None;
    for &c in ctx.call_options() {
        if c == CallChoice::Pass {
            continue;
        }
        let action = crate::framework::ctx::call_action(c);
        let path = tools::yaku_path::yaku_after_call(ctx.view, &action);
        if path == Some(OpenYakuPath::None) {
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
        if riichi && after.shanten > 0 {
            ctx.note(format!(
                "{c}：有人立直，鳴了還是 {} 向聽，不鳴",
                after.shanten
            ));
            continue;
        }
        if path != Some(OpenYakuPath::Yakuhai)
            && f64::from(after.shanten) >= ctx.param(&FAR_SHANTEN)
            && f64::from(dora) < ctx.param(&MIN_DORA_FOR_FAR_CALL)
        {
            ctx.note(format!(
                "{c}：鳴了還要 {} 向聽、寶牌只有 {dora} 張，太遠又太便宜，不鳴",
                after.shanten
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
