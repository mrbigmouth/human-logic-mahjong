//! 手切摸切：打出的牌和剛摸到的牌同種時，摸切還是從手中打出同一種牌。
//! 手牌結果完全相同，差別只在對手看到的牌河（印象操作，見 `utils/image.rs`）。
//!
//! 思考順序：
//! 1. 立直宣言牌：讀牌模型不看宣言牌是手切還是摸切 → 照常摸切。
//! 2. 用自己的讀牌模型看自己，算兩種打法下「別人眼中我聽牌的機率」：
//!    - 聽牌或一向聽 → 選看起來較不像聽牌的（對手比較敢推，之後比較可能放銃給我）；
//!    - 離聽牌還遠（[`BLUFF_SHANTEN`] 以上）→ 選看起來較像聽牌的（推進的人會收手）。
//! 3. 差不多時照常摸切。

use super::super::memory::Memory;
use super::super::utils::image;
use crate::prelude::*;

/// 是否做印象操作（0 = 一律摸切，用來和不做的版本比較）。
pub const ENABLED: Param = Param::new(
    "deceive.tedashi",
    1.0,
    0.0,
    1.0,
    "手切摸切的印象操作（1 = 開、0 = 一律摸切）",
);
/// 打出後的向聽數達到這個值，就改為「看起來像聽牌」（虛張聲勢）。
pub const BLUFF_SHANTEN: Param = Param::new(
    "deceive.bluff_shanten",
    2.0,
    1.0,
    9.0,
    "打出後幾向聽以上改為看起來像聽牌",
);
/// 兩種打法的「看起來聽牌的機率」差距小於這個值時，照常摸切。
pub const MIN_DIFF: f64 = 0.005;

/// 回傳 true = 手切。
pub fn decide(ctx: &Ctx, mem: &mut Memory) -> bool {
    let Some(pai) = ctx.tedashi_pai() else {
        return false;
    };
    if ctx.param(&ENABLED) < 0.5 || ctx.declaring_riichi() {
        return false;
    }
    let as_tsumogiri = image::apparent_tenpai(ctx, mem, pai, true, false);
    let as_tedashi = image::apparent_tenpai(ctx, mem, pai, false, false);
    if (as_tsumogiri - as_tedashi).abs() < MIN_DIFF {
        return false;
    }
    let shanten = ctx.after_discard(pai).shanten;
    let bluff = f64::from(shanten) >= ctx.param(&BLUFF_SHANTEN);
    let tedashi = if bluff {
        as_tedashi > as_tsumogiri
    } else {
        as_tedashi < as_tsumogiri
    };
    let goal = if bluff {
        "離聽牌還遠，讓自己看起來像聽牌"
    } else {
        "讓自己看起來不像聽牌"
    };
    ctx.note(format!(
        "{goal}：摸切時別人看我聽牌約 {:.0}%、手切時約 {:.0}% → {}",
        as_tsumogiri * 100.0,
        as_tedashi * 100.0,
        if tedashi { "手切" } else { "摸切" }
    ));
    tedashi
}
