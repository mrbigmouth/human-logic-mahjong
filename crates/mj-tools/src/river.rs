//! 牌河與副露的「事實」：手切／摸切的順序、各花色打了幾張、副露的內容。
//! 讀牌（某家是否聽牌、聽什麼、多大）的機率判斷屬於策略的預測函式，這裡只整理看得到的資料。

use crate::dora::pai_dora_value;
use mj_core::{Pai, Suit, Tile};
use mj_engine::{DiscardRecord, PlayerView, Seat};

fn river(view: &PlayerView, seat: Seat) -> &[DiscardRecord] {
    &view.rivers[usize::from(seat)]
}

/// 某家打過幾張牌（大約等於巡目）。
pub fn turn(view: &PlayerView, seat: Seat) -> usize {
    river(view, seat).len()
}

/// 牌河最後連續摸切的張數（最後一次手切之後）。剛手切時為 0。
pub fn trailing_tsumogiri(view: &PlayerView, seat: Seat) -> usize {
    river(view, seat)
        .iter()
        .rev()
        .take_while(|d| d.tsumogiri)
        .count()
}

/// 最近 `k` 張打牌中手切的張數。
pub fn recent_tedashi(view: &PlayerView, seat: Seat, k: usize) -> usize {
    river(view, seat)
        .iter()
        .rev()
        .take(k)
        .filter(|d| !d.tsumogiri)
        .count()
}

/// 最後一次手切：（第幾張，從 0 起算；那張牌）。
pub fn last_tedashi(view: &PlayerView, seat: Seat) -> Option<(usize, Pai)> {
    river(view, seat)
        .iter()
        .enumerate()
        .rev()
        .find(|(_, d)| !d.tsumogiri)
        .map(|(i, d)| (i, d.pai))
}

/// 第 `from` 張（從 0 起算）以後手切的中張牌（數牌 3–7）張數。
/// 中盤以後還在手切中張牌，代表手牌正在成形（從孤立的幺九牌整理到搭子）。
pub fn middle_tedashi_since(view: &PlayerView, seat: Seat, from: usize) -> usize {
    river(view, seat)
        .iter()
        .skip(from)
        .filter(|d| !d.tsumogiri && is_middle(d.pai.tile))
        .count()
}

/// 數牌 3–7（中張牌）。
pub fn is_middle(tile: Tile) -> bool {
    !tile.is_honor() && (3..=7).contains(&tile.number())
}

/// 各花色打出的張數：`[萬, 筒, 索, 字]`。
pub fn suit_counts(view: &PlayerView, seat: Seat) -> [u8; 4] {
    let mut c = [0u8; 4];
    for d in river(view, seat) {
        c[suit_index(d.pai.tile.suit())] += 1;
    }
    c
}

/// 花色的編號：萬 0、筒 1、索 2、字 3。
pub fn suit_index(suit: Suit) -> usize {
    match suit {
        Suit::Man => 0,
        Suit::Pin => 1,
        Suit::Sou => 2,
        Suit::Honor => 3,
    }
}

/// 副露（不含暗槓）組數。
pub fn open_melds(view: &PlayerView, seat: Seat) -> usize {
    view.melds[usize::from(seat)]
        .iter()
        .filter(|m| m.is_open())
        .count()
}

/// 副露中看得到的寶牌張數（寶牌、赤五；含暗槓）。
pub fn meld_dora(view: &PlayerView, seat: Seat) -> u8 {
    view.melds[usize::from(seat)]
        .iter()
        .flat_map(|m| m.tiles.iter())
        .map(|&p| pai_dora_value(view, p))
        .sum()
}

/// 副露中有沒有對 `seat` 而言是役牌的刻子／槓子（三元牌、他的自風、場風）。
pub fn has_yakuhai_meld(view: &PlayerView, seat: Seat) -> bool {
    view.melds[usize::from(seat)]
        .iter()
        .any(|m| !m.is_sequence() && crate::table::is_yakuhai_for(view, seat, m.base()))
}

/// 副露用到的數牌花色（位元：萬 1、筒 2、索 4）。只有一種花色時可能是染手（混一色、清一色）。
pub fn meld_suits(view: &PlayerView, seat: Seat) -> u8 {
    view.melds[usize::from(seat)]
        .iter()
        .map(|m| m.base().suit())
        .filter(|&s| s != Suit::Honor)
        .fold(0, |acc, s| acc | (1 << suit_index(s)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mj_core::TileCounts;

    fn view() -> PlayerView {
        PlayerView {
            me: 0,
            round_wind: 0,
            kyoku: 0,
            honba: 0,
            riichi_sticks: 0,
            scores: [25_000; 4],
            dora_indicators: vec![],
            hand: TileCounts::default(),
            hand_pais: vec![],
            drawn: None,
            melds: Default::default(),
            rivers: Default::default(),
            riichi: [false; 4],
            tiles_left_in_wall: 60,
            total_kyoku: 8,
            last_discard: None,
        }
    }

    fn push(v: &mut PlayerView, tile: &str, tsumogiri: bool) {
        let seq = v.rivers[1].len() as u16;
        v.rivers[1].push(DiscardRecord {
            pai: tile.parse().unwrap(),
            tsumogiri,
            riichi_declared: false,
            called: false,
            seq,
        });
    }

    /// 手切 1z、9m、5p，再摸切 2 張：連續摸切 2、最近 3 張手切 1、最後手切 5p（第 2 張）。
    #[test]
    fn tedashi_and_tsumogiri() {
        let mut v = view();
        push(&mut v, "1z", false);
        push(&mut v, "9m", false);
        push(&mut v, "5p", false);
        push(&mut v, "8s", true);
        push(&mut v, "2z", true);
        assert_eq!(turn(&v, 1), 5);
        assert_eq!(trailing_tsumogiri(&v, 1), 2);
        assert_eq!(recent_tedashi(&v, 1, 3), 1);
        assert_eq!(
            last_tedashi(&v, 1).map(|(i, p)| (i, p.tile)),
            Some((2, "5p".parse().unwrap()))
        );
        assert_eq!(middle_tedashi_since(&v, 1, 0), 1);
        assert_eq!(suit_counts(&v, 1), [1, 1, 1, 2]);
    }
}
