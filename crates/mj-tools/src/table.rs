//! 牌桌狀態：座位、風位、立直、巡目。

use mj_core::Tile;
use mj_engine::{PlayerView, Seat};

/// 其他三家的座位（依下家、對家、上家順序）。
pub fn opponents(view: &PlayerView) -> [Seat; 3] {
    [1, 2, 3].map(|k| (view.me + k) % 4)
}

/// 相對位置：1 = 下家、2 = 對家、3 = 上家。
pub fn relative(view: &PlayerView, seat: Seat) -> u8 {
    (seat + 4 - view.me) % 4
}

/// 已立直的對手座位。
pub fn riichi_opponents(view: &PlayerView) -> Vec<Seat> {
    opponents(view)
        .into_iter()
        .filter(|&s| view.riichi[usize::from(s)])
        .collect()
}

/// 某家宣告立直的那張牌的打牌順序（未立直為 `None`）。
pub fn riichi_seq(view: &PlayerView, seat: Seat) -> Option<u16> {
    view.rivers[usize::from(seat)]
        .iter()
        .find(|d| d.riichi_declared)
        .map(|d| d.seq)
}

/// 自己打過幾張牌（大約等於巡目）。
pub fn my_turn(view: &PlayerView) -> usize {
    view.rivers[usize::from(view.me)].len()
}

/// 自己的門風牌與場風牌。
pub fn my_winds(view: &PlayerView) -> (Tile, Tile) {
    (
        Tile::wind(view.seat_wind(view.me)),
        Tile::wind(view.round_wind),
    )
}

/// 這種牌對自己是不是役牌（三元牌、自風、場風）。
pub fn is_yakuhai_for_me(view: &PlayerView, tile: Tile) -> bool {
    let (seat, round) = my_winds(view);
    tile.is_dragon() || tile == seat || tile == round
}

/// 這種牌對 `seat` 是不是役牌（三元牌、他的自風、場風）。
pub fn is_yakuhai_for(view: &PlayerView, seat: Seat, tile: Tile) -> bool {
    tile.is_dragon()
        || tile == Tile::wind(view.seat_wind(seat))
        || tile == Tile::wind(view.round_wind)
}

/// 自己是否門前清（沒有吃、碰、明槓）。
pub fn i_am_closed(view: &PlayerView) -> bool {
    view.melds[usize::from(view.me)]
        .iter()
        .all(|m| !m.is_open())
}

/// 名次與點差（局勢）。
#[derive(Debug, Clone, PartialEq)]
pub struct Standing {
    /// 目前名次，0 = 第一名。同分時以離起家近的座位為先。
    pub rank: u8,
    /// 與上一名的點差（第一名為 0）
    pub gap_above: i32,
    /// 領先下一名的點差（最後一名為 0）
    pub gap_below: i32,
    /// 是否為莊家
    pub is_dealer: bool,
    /// 本局之後還剩幾局（不計連莊）
    pub hands_remaining: u8,
    /// 是否為最終局（all last，半莊的南四局）
    pub is_all_last: bool,
}

/// 計算自己的名次與點差。
pub fn standing(v: &PlayerView) -> Standing {
    let mut order: Vec<usize> = (0..4).collect();
    order.sort_by_key(|&s| (-v.scores[s], s));
    let me = usize::from(v.me);
    let rank = order.iter().position(|&s| s == me).unwrap_or(0);
    let score = v.scores[me];
    let gap_above = rank
        .checked_sub(1)
        .map_or(0, |r| v.scores[order[r]] - score);
    let gap_below = order.get(rank + 1).map_or(0, |&s| score - v.scores[s]);
    let hands_remaining = v.total_kyoku.saturating_sub(v.current_kyoku_index() + 1);
    Standing {
        rank: rank as u8,
        gap_above,
        gap_below,
        is_dealer: v.dealer() == v.me,
        hands_remaining,
        is_all_last: hands_remaining == 0,
    }
}
