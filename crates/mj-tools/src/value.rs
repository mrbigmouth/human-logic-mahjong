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
