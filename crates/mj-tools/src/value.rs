//! 和牌點數：給定明確條件的計分，以及「目前手牌如果和了值多少」。

use crate::table::i_am_closed;
use mj_core::{Tile, TileCounts};
use mj_engine::PlayerView;
pub use mj_rules::riichi::{Limit, RiichiConfig, RiichiState, Score, WinContext, Yaku, score};

/// 用自己的視角組出和牌狀況（風位、寶牌、是否立直、赤五數）。
/// 一發、海底、嶺上等偶發狀況一律當作沒有；立直時不計裏寶牌（未知）。
pub fn my_win_context(view: &PlayerView, win_tile: Tile, tsumo: bool) -> WinContext {
    let me = usize::from(view.me);
    let mut ctx = WinContext::new(win_tile, tsumo);
    ctx.dealer = view.dealer() == view.me;
    ctx.seat_wind = view.seat_wind(view.me);
    ctx.round_wind = view.round_wind;
    ctx.dora_indicators = view.dora_indicators.clone();
    ctx.riichi = if view.riichi[me] {
        RiichiState::Riichi
    } else {
        RiichiState::None
    };
    let melded = view.melds[me].iter().flat_map(|m| m.tiles.iter());
    ctx.red_fives = view
        .hand_pais
        .iter()
        .chain(melded)
        .filter(|p| p.red)
        .count() as u8;
    ctx
}

/// 聽牌中的手牌（3k+1 張 `hand`）以 `win_tile` 和牌時的分數；無役或不是和牌型時回傳 `None`。
/// `assume_riichi` = 假設已立直（用來比較「立直 vs 默聽」）。
pub fn win_value(
    view: &PlayerView,
    hand: &TileCounts,
    win_tile: Tile,
    tsumo: bool,
    assume_riichi: bool,
) -> Option<Score> {
    let mut concealed = *hand;
    concealed.add(win_tile);
    let mut ctx = my_win_context(view, win_tile, tsumo);
    if assume_riichi && i_am_closed(view) {
        ctx.riichi = RiichiState::Riichi;
    }
    score(
        &concealed,
        &view.melds[usize::from(view.me)],
        &ctx,
        &RiichiConfig::TENHOU,
    )
}

/// 榮和時對方要付的點數（不含本場、供託）。
pub fn ron_points(view: &PlayerView, s: &Score) -> i32 {
    s.ron_points(view.dealer() == view.me, 0)
}

/// 聽牌手牌的價值（[`tenpai_value`] 的結果）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TenpaiValue {
    /// 聽牌張還剩幾張（扣掉自己看得到的牌）
    pub waits: f32,
    /// 其中有役、能榮和的聽牌張剩幾張（沒有役的聽牌張只能靠門前清自摸）
    pub ron_waits: f32,
    /// 能榮和時的平均點數（依各聽牌張的剩餘張數加權；剩餘都是 0 時取平均）。沒有能榮和的聽牌張時為 0
    pub avg_ron: f32,
}

/// 聽牌中的手牌（3k+1 張 `hand`）各聽牌張的榮和點數，依剩餘張數加權平均。
/// `assume_riichi` = 假設已立直（比較立直與默聽）。不是聽牌時回傳 `None`。
pub fn tenpai_value(
    view: &PlayerView,
    hand: &TileCounts,
    assume_riichi: bool,
) -> Option<TenpaiValue> {
    let waits = crate::hand::waits_with_remaining(view, hand);
    if waits.is_empty() {
        return None;
    }
    let (mut total, mut ron_waits, mut weighted, mut weight, mut sum, mut n) =
        (0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    for (t, left) in waits {
        total += left;
        if let Some(s) = win_value(view, hand, t, false, assume_riichi) {
            let pts = ron_points(view, &s) as f32;
            ron_waits += left;
            weighted += pts * left;
            weight += left;
            sum += pts;
            n += 1.0;
        }
    }
    let avg_ron = if weight > 0.0 {
        weighted / weight
    } else if n > 0.0 {
        sum / n
    } else {
        0.0
    };
    Some(TenpaiValue {
        waits: total,
        ron_waits,
        avg_ron,
    })
}

/// 依番數粗估榮和點數（假設 30 符；滿貫以上照規定）。`dealer` = 莊家（1.5 倍）。
/// 用於還沒聽牌、無法精確計分時的估計。
pub fn points_for_han(han: u8, dealer: bool) -> i32 {
    let base = match han {
        0 => 0,
        1 => 1000,
        2 => 2000,
        3 => 3900,
        4 => 7700,
        5 => 8000,
        6 | 7 => 12000,
        8..=10 => 16000,
        11 | 12 => 24000,
        _ => 32000,
    };
    if dealer {
        // 莊家：1000 → 1500、2000 → 2900、3900 → 5800、7700 → 11600、滿貫以上 1.5 倍
        match han {
            1 => 1500,
            2 => 2900,
            3 => 5800,
            4 => 11600,
            _ => base * 3 / 2,
        }
    } else {
        base
    }
}
