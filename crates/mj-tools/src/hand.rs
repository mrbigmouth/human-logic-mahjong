//! 手牌計算：向聽、進張、聽牌張、何切排序（整理自 `mj-calc`，加上「看得到的牌」）。

use crate::visible::remaining;
use mj_calc::{DiscardOption, Ukeire};
pub use mj_calc::{
    decompose, is_agari, is_chiitoi, is_kokushi, shanten, shanten_chiitoi, shanten_kokushi,
    shanten_standard,
};
use mj_core::{Tile, TileCounts};
use mj_engine::PlayerView;

/// 手牌的進張（3k+1 張），剩餘張數扣掉自己看得到的牌。
pub fn ukeire(view: &PlayerView, hand: &TileCounts) -> Ukeire {
    mj_calc::ukeire(hand, &remaining(view))
}

/// 摸牌後（3k+2 張）每種打法打出後的向聽與進張，由好到壞排序（向聽低、進張多優先）。
pub fn discard_options(view: &PlayerView, hand: &TileCounts) -> Vec<DiscardOption> {
    mj_calc::discard_options(hand, &remaining(view))
}

/// 聽牌張（只看牌型，不看役），以及每種還剩幾張（扣掉自己看得到的牌）。
pub fn waits_with_remaining(view: &PlayerView, hand: &TileCounts) -> Vec<(Tile, f32)> {
    let rem = remaining(view);
    mj_calc::waits(hand)
        .into_iter()
        .map(|t| (t, rem[t.index()]))
        .collect()
}

/// 是否聽牌（3k+1 張）。
pub fn is_tenpai(hand: &TileCounts) -> bool {
    !mj_calc::waits(hand).is_empty()
}

/// 自己目前的向聽數（摸牌後是 3k+2 張時，取打出最好一張後的向聽數）。
pub fn my_shanten(view: &PlayerView) -> i8 {
    let h = view.hand;
    if h.total() % 3 == 2 {
        discard_options(view, &h)
            .first()
            .map_or(shanten(&h), |o| o.after.shanten)
    } else {
        shanten(&h)
    }
}
