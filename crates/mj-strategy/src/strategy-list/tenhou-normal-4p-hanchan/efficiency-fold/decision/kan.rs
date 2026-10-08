//! 開槓：能暗槓或加槓時要不要槓。

use super::super::memory::Memory;
use crate::prelude::*;

/// 回傳要槓的選項；`None` = 不槓。
pub fn decide(ctx: &Ctx, _mem: &mut Memory) -> Option<KanChoice> {
    let riichi_opponents = tools::table::riichi_opponents(ctx.view);
    let now = ctx.shanten();
    for &k in ctx.kan_options() {
        let action = match k {
            KanChoice::Ankan(tile) => Action::Ankan { tile },
            KanChoice::Kakan(pai) => Action::Kakan { pai },
        };
        let after = ctx.after(&action);
        if after.shanten > now {
            ctx.note(format!(
                "{k} 會讓向聽數變差（{now} → {}），不槓",
                after.shanten
            ));
            continue;
        }
        if matches!(k, KanChoice::Kakan(_)) && !riichi_opponents.is_empty() {
            ctx.note(format!(
                "有人立直，{k} 會多翻一張寶牌給對手、也可能被搶槓，不槓"
            ));
            continue;
        }
        ctx.note(format!("{k} 不影響牌效，還能多一張寶牌，槓"));
        return Some(k);
    }
    None
}
