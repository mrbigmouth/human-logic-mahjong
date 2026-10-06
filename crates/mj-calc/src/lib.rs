//! Hand-efficiency calculators (M1/M2). This is the performance hot spot of self-play,
//! so everything here works on [`TileCounts`] without allocation.
//!
//! Planned:
//! - `shanten`: standard / chiitoitsu / kokushi shanten (table-based, e.g. Tomohxx-style)
//! - `ukeire`: effective tiles per discard, weighted by a remaining-tile estimate
//! - `win_probability`: chance to win within N draws given a belief over the wall
//! - `expected_value`: win probability × expected hand value

use mj_core::TileCounts;

/// Remaining-tile estimate supplied by the belief layer: expected copies of each kind
/// still drawable. With no reading this is simply `4 - visible`.
pub type RemainingEstimate = [f32; mj_core::NUM_TILE_KINDS];

/// Remaining copies assuming only the player's own visible tiles are gone.
pub fn naive_remaining(visible: &TileCounts) -> RemainingEstimate {
    std::array::from_fn(|i| f32::from(4u8.saturating_sub(visible.0[i])))
}
