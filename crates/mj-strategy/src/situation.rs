use mj_engine::PlayerView;
use serde::Serialize;

/// Derived match context (L5), computed once per decision.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Situation {
    /// 0 = first place. Ties are broken by seat order from the East-1 dealer.
    pub rank: u8,
    /// Points needed to pass the player above (0 when first).
    pub gap_above: i32,
    /// Lead over the player below (0 when last).
    pub gap_below: i32,
    pub is_dealer: bool,
    /// Hands left after the current one, ignoring renchan.
    pub hands_remaining: u8,
    pub is_all_last: bool,
}

impl Situation {
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
            rivers: Default::default(),
            riichi: [false; 4],
            tiles_left_in_wall: 70,
            total_kyoku: 8,
        }
    }

    #[test]
    fn south_three_second_place() {
        let s = Situation::from_view(&view(1, [30_000, 28_000, 25_000, 17_000], 1, 2));
        assert_eq!(s.rank, 1);
        assert_eq!(s.gap_above, 2_000);
        assert_eq!(s.gap_below, 3_000);
        assert_eq!(s.hands_remaining, 1);
        assert!(!s.is_all_last && !s.is_dealer);
    }

    #[test]
    fn all_last_dealer() {
        let s = Situation::from_view(&view(3, [25_000; 4], 1, 3));
        assert!(s.is_all_last && s.is_dealer);
        assert_eq!(s.rank, 3);
    }
}
