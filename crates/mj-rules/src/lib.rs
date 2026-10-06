//! Rule sets. Everything rule-specific (tile set, yaku, scoring, draws, end conditions)
//! goes behind [`RuleSet`] so other variants (Taiwanese 16-tile, MCR, ...) can be added
//! without touching the engine, strategy or simulator crates.

pub mod riichi;

/// A mahjong variant. The engine is generic over this trait.
pub trait RuleSet: Send + Sync {
    /// Short identifier, e.g. `"riichi"`.
    fn id(&self) -> &'static str;

    /// Tiles in a concealed hand before drawing (13 for riichi, 16 for Taiwanese).
    fn hand_size(&self) -> usize;

    /// Number of physical copies of each tile kind in the wall.
    fn copies_per_kind(&self) -> u8 {
        4
    }

    // M1: wall construction, legal calls, win detection, scoring, game-end conditions.
}
