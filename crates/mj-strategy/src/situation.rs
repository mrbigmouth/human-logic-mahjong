//! 局勢（L5）：名次、點差、剩餘局數等，由玩家視角推導。

use mj_engine::PlayerView;
use serde::Serialize;

/// 局勢，每次決策計算一次。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Situation {
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

impl Situation {
    /// 由玩家視角計算局勢。
    pub fn from_view(v: &PlayerView) -> Self {
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
        Self {
            rank: rank as u8,
            gap_above,
            gap_below,
            is_dealer: v.dealer() == v.me,
            hands_remaining,
            is_all_last: hands_remaining == 0,
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use mj_core::TileCounts;

    /// 測試用的玩家視角（只填局勢相關欄位）。
    pub(crate) fn view(me: u8, scores: [i32; 4], round_wind: u8, kyoku: u8) -> PlayerView {
        PlayerView {
            me,
            round_wind,
            kyoku,
            honba: 0,
            riichi_sticks: 0,
            scores,
            dora_indicators: vec![],
            hand: TileCounts::default(),
            hand_pais: vec![],
            drawn: None,
            melds: Default::default(),
            rivers: Default::default(),
            riichi: [false; 4],
            tiles_left_in_wall: 70,
            total_kyoku: 8,
            last_discard: None,
        }
    }

    /// 南三局第二名：與第一名差 2000、領先第三名 3000、還剩 1 局。
    #[test]
    fn south_three_second_place() {
        let s = Situation::from_view(&view(1, [30_000, 28_000, 25_000, 17_000], 1, 2));
        assert_eq!(s.rank, 1);
        assert_eq!(s.gap_above, 2_000);
        assert_eq!(s.gap_below, 3_000);
        assert_eq!(s.hands_remaining, 1);
        assert!(!s.is_all_last && !s.is_dealer);
    }

    /// 南四局、座位 3 當莊：最終局、莊家；四家同分時座位 3 排最後。
    #[test]
    fn all_last_dealer() {
        let s = Situation::from_view(&view(3, [25_000; 4], 1, 3));
        assert!(s.is_all_last && s.is_dealer);
        assert_eq!(s.rank, 3);
    }
}
