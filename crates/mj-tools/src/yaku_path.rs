//! 役的路線：鳴牌之後還能不能和牌（有沒有役）。
//!
//! 目前是粗略判斷三條最常見的副露役：役牌、斷么九（食斷）、染手（混一色／清一色）。

use crate::table::is_yakuhai_for_me;
use mj_core::{Tile, TileCounts};
use mj_engine::{Action, PlayerView};

/// 鳴牌後能成立的役。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenYakuPath {
    /// 沒有役：鳴了也無法和牌（無役）
    None,
    /// 役牌：三元牌、自風、場風的刻子
    Yakuhai,
    /// 斷么九（食斷）：全部是 2–8 的數牌
    Tanyao,
    /// 混一色／清一色：只有一種數牌花色（＋字牌）
    Flush,
}

/// 判斷副露 `meld_sets`（每組的牌）加上門前 `concealed` 有沒有現實的役：
/// - 役牌：副露中有役牌刻子，或門前已有役牌暗刻；
/// - 斷么九：副露全是中張牌，且門前幺九牌最多 1 張（之後打掉）；
/// - 染手：副露全是同一種花色或字牌，且門前其他花色的牌最多 2 張。
pub fn open_yaku_path(
    view: &PlayerView,
    meld_sets: &[Vec<Tile>],
    concealed: &TileCounts,
) -> OpenYakuPath {
    let yakuhai = |t: Tile| is_yakuhai_for_me(view, t);
    let is_triplet = |s: &Vec<Tile>| s.len() >= 3 && s.iter().all(|&t| t == s[0]);
    if meld_sets.iter().any(|s| is_triplet(s) && yakuhai(s[0]))
        || Tile::all().any(|t| yakuhai(t) && concealed.get(t) >= 3)
    {
        return OpenYakuPath::Yakuhai;
    }
    let meld_tiles = meld_sets.iter().flatten();
    let concealed_yaochu = concealed.tiles().filter(|t| t.is_yaochu()).count();
    if meld_tiles.clone().all(|t| !t.is_yaochu()) && concealed_yaochu <= 1 {
        return OpenYakuPath::Tanyao;
    }
    let suits: Vec<usize> = meld_tiles
        .filter(|t| !t.is_honor())
        .map(|t| t.index() / 9)
        .collect();
    if let Some(&suit) = suits.first() {
        if suits.iter().all(|&s| s == suit) {
            let off_suit = concealed
                .tiles()
                .filter(|t| !t.is_honor() && t.index() / 9 != suit)
                .count();
            if off_suit <= 2 {
                return OpenYakuPath::Flush;
            }
        }
    }
    OpenYakuPath::None
}

/// 吃、碰、明槓之後的副露（每組的牌）與門前剩牌；不是這類動作時回傳 `None`。
pub fn after_call(view: &PlayerView, action: &Action) -> Option<(Vec<Vec<Tile>>, TileCounts)> {
    let (_, called) = view.last_discard?;
    let mut concealed = view.hand;
    let new_set: Vec<Tile> = match action {
        Action::Chi { consumed } | Action::Pon { consumed } => {
            consumed.iter().for_each(|p| {
                concealed.remove(p.tile);
            });
            let mut s = vec![consumed[0].tile, consumed[1].tile, called.tile];
            s.sort();
            s
        }
        Action::Daiminkan => {
            (0..3).for_each(|_| {
                concealed.remove(called.tile);
            });
            vec![called.tile; 4]
        }
        _ => return None,
    };
    let mut sets: Vec<Vec<Tile>> = view.melds[usize::from(view.me)]
        .iter()
        .map(|m| m.tiles.iter().map(|p| p.tile).collect())
        .collect();
    sets.push(new_set);
    Some((sets, concealed))
}

/// 便利版本：這個鳴牌動作之後有沒有役。不是鳴牌動作時回傳 `None`。
pub fn yaku_after_call(view: &PlayerView, action: &Action) -> Option<OpenYakuPath> {
    let (sets, concealed) = after_call(view, action)?;
    Some(open_yaku_path(view, &sets, &concealed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mj_core::{Pai, parse_pais};

    /// 南家（座位 1）、東場，手牌 `hand`，回應座位 0 打出的 `discard`。
    fn responding(hand: &str, discard: &str) -> PlayerView {
        let pais = parse_pais(hand).unwrap();
        PlayerView {
            me: 1,
            round_wind: 0,
            kyoku: 0,
            honba: 0,
            riichi_sticks: 0,
            scores: [25_000; 4],
            dora_indicators: vec![],
            hand: TileCounts::from_tiles(pais.iter().map(|p| p.tile)),
            hand_pais: pais,
            drawn: None,
            melds: Default::default(),
            rivers: Default::default(),
            riichi: [false; 4],
            tiles_left_in_wall: 60,
            total_kyoku: 8,
            last_discard: Some((0, discard.parse().unwrap())),
        }
    }

    fn pon(s: &str) -> Action {
        let p: Pai = s.parse().unwrap();
        Action::Pon { consumed: [p, p] }
    }

    /// 碰中（役牌）有役；碰 9m（手上很多幺九、多種花色）無役。
    #[test]
    fn yakuhai_and_none() {
        let v = responding("77z123m456p789s19s", "7z");
        assert_eq!(yaku_after_call(&v, &pon("7z")), Some(OpenYakuPath::Yakuhai));
        let v = responding("99m123m456p789s19s", "9m");
        assert_eq!(yaku_after_call(&v, &pon("9m")), Some(OpenYakuPath::None));
    }

    /// 斷么九：碰 5p 且門前全是中張牌。
    #[test]
    fn tanyao() {
        let v = responding("55p234m678s3456s", "5p");
        assert_eq!(yaku_after_call(&v, &pon("5p")), Some(OpenYakuPath::Tanyao));
    }

    /// 南家的自風是南（2z）：碰南是役牌；碰西不是。
    #[test]
    fn seat_wind() {
        let v = responding("22z33z19m19p19s12m", "2z");
        assert_eq!(yaku_after_call(&v, &pon("2z")), Some(OpenYakuPath::Yakuhai));
        let v = responding("22z33z19m19p19s12m", "3z");
        assert_eq!(yaku_after_call(&v, &pon("3z")), Some(OpenYakuPath::None));
    }
}
