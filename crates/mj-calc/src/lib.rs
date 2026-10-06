//! 牌效計算：向聽數、進張、聽牌張、和牌拆解。
//!
//! 這裡是自我對戰的效能熱點（每次打牌都要計算很多次），所以都以 [`TileCounts`]
//! 運算，盡量避免配置記憶體。這些計算與規則無關，日麻以外的規則也能共用。

pub mod decompose;
pub mod shanten;
pub mod ukeire;

pub use decompose::{Block, Decomposition, decompose, is_agari, is_chiitoi, is_kokushi, waits};
pub use shanten::{shanten, shanten_chiitoi, shanten_kokushi, shanten_standard};
pub use ukeire::{DiscardOption, Ukeire, discard_options, ukeire};

use mj_core::TileCounts;

/// 剩餘牌估計：每種牌預期還能摸到幾張，由讀牌層（L1）提供。
/// 不做讀牌時就是 `4 − 看得到的張數`；讀牌後可以扣掉「推測在別人手上」的牌，所以是小數。
pub type RemainingEstimate = [f32; mj_core::NUM_TILE_KINDS];

/// 最單純的剩餘牌估計：只扣掉看得到的牌（自己的手牌、牌河、副露、寶牌指示牌）。
pub fn naive_remaining(visible: &TileCounts) -> RemainingEstimate {
    std::array::from_fn(|i| f32::from(4u8.saturating_sub(visible.0[i])))
}
