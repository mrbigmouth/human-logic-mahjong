//! Japanese riichi mahjong (M-League / Tenhou-style defaults).

use crate::RuleSet;
use serde::{Deserialize, Serialize};

/// Common house-rule switches. Defaults follow Tenhou ranked games.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RiichiConfig {
    /// true = hanchan (East + South), false = tonpuu (East only).
    pub hanchan: bool,
    pub red_fives: u8,
    pub kuitan: bool,
    pub starting_points: i32,
    /// Placement bonus (uma) in thousands of points, 1st..4th.
    pub uma: [i32; 4],
}

impl Default for RiichiConfig {
    fn default() -> Self {
        Self {
            hanchan: true,
            red_fives: 3,
            kuitan: true,
            starting_points: 25_000,
            uma: [20, 10, -10, -20],
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Riichi {
    pub config: RiichiConfig,
}

impl RuleSet for Riichi {
    fn id(&self) -> &'static str {
        "riichi"
    }

    fn hand_size(&self) -> usize {
        13
    }
}
