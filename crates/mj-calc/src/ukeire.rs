//! 進張（ukeire，受け入れ）：摸到哪些牌能讓向聽數前進，以及這些牌還剩幾張。
//!
//! 剩餘張數由讀牌層提供（[`RemainingEstimate`]），可以是小數，代表「預期還能摸到幾張」。
//! 沒有讀牌時就是 4 − 自己看得到的張數。

use crate::RemainingEstimate;
use crate::shanten::shanten;
use mj_core::{Tile, TileCounts};

/// 進張計算結果。
#[derive(Debug, Clone, PartialEq)]
pub struct Ukeire {
    /// 目前的向聽數
    pub shanten: i8,
    /// 能讓向聽數前進的牌種，以及預期剩餘張數。
    pub tiles: Vec<(Tile, f32)>,
    /// 進張總數（剩餘張數合計）
    pub total: f32,
}

/// 等待摸牌的手牌（3k+1 張）的進張。
pub fn ukeire(hand: &TileCounts, remaining: &RemainingEstimate) -> Ukeire {
    let base = shanten(hand);
    let mut h = *hand;
    let mut tiles = Vec::new();
    for t in Tile::all() {
        let left = remaining[t.index()];
        if left <= 0.0 || h.get(t) >= 4 {
            continue;
        }
        h.add(t);
        if shanten(&h) < base {
            tiles.push((t, left));
        }
        h.remove(t);
    }
    let total = tiles.iter().map(|&(_, n)| n).sum();
    Ukeire {
        shanten: base,
        tiles,
        total,
    }
}

/// 何切（打哪張）的一個選項。
#[derive(Debug, Clone, PartialEq)]
pub struct DiscardOption {
    /// 打出的牌
    pub discard: Tile,
    /// 打出後的向聽數與進張
    pub after: Ukeire,
}

/// 何切：摸牌後的手牌（3k+2 張）每種可打的牌，以及打出後的進張。
/// 由好到壞排序：向聽數低者優先，其次進張多者，最後依牌的編號。
pub fn discard_options(hand: &TileCounts, remaining: &RemainingEstimate) -> Vec<DiscardOption> {
    let mut h = *hand;
    let mut out: Vec<DiscardOption> = Tile::all()
        .filter(|&t| hand.get(t) > 0)
        .map(|t| {
            h.remove(t);
            let after = ukeire(&h, remaining);
            h.add(t);
            DiscardOption { discard: t, after }
        })
        .collect();
    out.sort_by(|a, b| {
        a.after
            .shanten
            .cmp(&b.after.shanten)
            .then(b.after.total.total_cmp(&a.after.total))
            .then(a.discard.cmp(&b.discard))
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::naive_remaining;
    use mj_core::parse_tiles;

    fn h(s: &str) -> TileCounts {
        TileCounts::from_tiles(parse_tiles(s).unwrap())
    }

    /// 兩面聽 23m：聽 1m、4m，各剩 4 張，共 8 張。
    #[test]
    fn ryanmen_wait() {
        let hand = h("23m456p789s11122z");
        let u = ukeire(&hand, &naive_remaining(&hand));
        assert_eq!(u.shanten, 0);
        let waits: Vec<String> = u.tiles.iter().map(|(t, _)| t.to_string()).collect();
        assert_eq!(waits, ["1m", "4m"]);
        assert_eq!(u.total, 8.0);
    }

    /// 打孤立的字牌（中）可以聽牌，應排第一。
    #[test]
    fn best_discard_is_isolated_honor() {
        let hand = h("23m456p789s111z55z7z");
        let best = &discard_options(&hand, &naive_remaining(&hand))[0];
        assert_eq!(best.after.shanten, 0);
        assert!(best.discard.is_honor());
    }
}
