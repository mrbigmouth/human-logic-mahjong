//! 看得到的牌與剩餘張數。

use mj_calc::{RemainingEstimate, naive_remaining};
use mj_core::{Tile, TileCounts};
use mj_engine::PlayerView;

/// 自己看得到的牌：手牌、四家牌河（被鳴走的牌改由副露計算，不重複）、四家副露、寶牌指示牌。
pub fn visible_counts(view: &PlayerView) -> TileCounts {
    let mut v = view.hand;
    for river in &view.rivers {
        for d in river.iter().filter(|d| !d.called) {
            v.add(d.pai.tile);
        }
    }
    for melds in &view.melds {
        for m in melds {
            m.tiles.iter().for_each(|p| v.add(p.tile));
        }
    }
    for &t in &view.dora_indicators {
        v.add(t);
    }
    v
}

/// 某種牌看得到幾張（0–4）。
pub fn visible_count(view: &PlayerView, tile: Tile) -> u8 {
    visible_counts(view).get(tile)
}

/// 不做讀牌時的剩餘牌估計：每種牌 4 − 看得到的張數（包含在別人手上、牌山、王牌中的）。
pub fn remaining(view: &PlayerView) -> RemainingEstimate {
    naive_remaining(&visible_counts(view))
}
