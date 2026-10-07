//! 何切：打哪一張。
//!
//! 思考順序：
//! 1. 有人立直、自己離聽牌還遠（2 向聽以上）→ 棄和，打最安全的牌。
//! 2. 否則照牌效：向聽數最低 → 進張最多 → 保留寶牌 → 先打字牌、老頭牌。

use super::super::memory::Memory;
use super::super::utils::{defense, efficiency};
use crate::prelude::*;

/// 回傳要打的牌（必須是 `ctx.discard_candidates()` 之一）。
pub fn decide(ctx: &Ctx, mem: &mut Memory) -> Pai {
    let candidates = ctx.discard_candidates();
    let ranked = efficiency::rank_by_efficiency(ctx, candidates);
    let (best, best_eval) = ranked[0];

    let riichi = tools::table::riichi_opponents(ctx.view);
    if !riichi.is_empty() && best_eval.shanten >= 2 {
        // 棄和：放銃機率最低的牌；同樣安全時選牌效較好的。
        let safest = ranked
            .iter()
            .map(|&(p, e)| (p, e, defense::deal_in_risk(ctx, mem, p)))
            .min_by(|a, b| a.2.total_cmp(&b.2))
            .unwrap();
        ctx.note(format!(
            "對手立直，自己還要 {} 向聽才聽牌 → 棄和，打最安全的 {}（放銃率約 {:.0}%）",
            best_eval.shanten,
            safest.0,
            safest.2 * 100.0
        ));
        return safest.0;
    }

    if best_eval.shanten == 0 {
        ctx.note(format!("打 {best} 聽牌，聽牌張剩 {} 張", best_eval.ukeire));
    } else {
        ctx.note(format!(
            "打 {best}：{} 向聽、進張 {} 張（牌效最好）",
            best_eval.shanten, best_eval.ukeire
        ));
    }
    if let Some((second, e)) = ranked.get(1) {
        if e.shanten == best_eval.shanten && e.ukeire == best_eval.ukeire {
            let why = if efficiency::dora_of(ctx, *second) > efficiency::dora_of(ctx, best) {
                format!("保留寶牌 {second}")
            } else {
                format!("{second} 牌效相同，先打較難用的 {best}")
            };
            ctx.note(why);
        }
    }
    best
}
