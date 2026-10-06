//! Game state machine (M1). For now this crate defines the types shared with the
//! strategy layer: seats, actions and the per-player observable view.

use mj_core::{Tile, TileCounts};
use serde::{Deserialize, Serialize};

/// Seat index relative to the dealer of East 1 (0..4).
pub type Seat = u8;

/// Anything a player may choose at a decision point.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Action {
    Discard {
        tile: Tile,
        tsumogiri: bool,
    },
    Riichi {
        discard: Tile,
    },
    Chi {
        tiles: [Tile; 2],
        called: Tile,
    },
    Pon {
        called: Tile,
    },
    Kan {
        tile: Tile,
    },
    Tsumo,
    Ron,
    /// Abortive draw on nine different terminals/honors.
    KyuushuKyuuhai,
    Pass,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscardRecord {
    pub tile: Tile,
    pub tsumogiri: bool,
    pub riichi_declared: bool,
    /// Taken by another player's call.
    pub called: bool,
}

/// Everything one player can see at a decision point. Strategies only ever receive this,
/// never the hidden state, so self-play cannot cheat.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerView {
    pub me: Seat,
    /// 0 = East round, 1 = South round, ...
    pub round_wind: u8,
    /// Hand number within the round wind (0..4); the dealer is seat `kyoku`.
    pub kyoku: u8,
    pub honba: u8,
    pub riichi_sticks: u8,
    pub scores: [i32; 4],
    pub dora_indicators: Vec<Tile>,
    pub hand: TileCounts,
    pub rivers: [Vec<DiscardRecord>; 4],
    pub riichi: [bool; 4],
    pub tiles_left_in_wall: u8,
    /// Total hands in the match (8 for hanchan) — used to know how many hands remain.
    pub total_kyoku: u8,
}

impl PlayerView {
    pub fn dealer(&self) -> Seat {
        self.kyoku % 4
    }

    pub fn current_kyoku_index(&self) -> u8 {
        self.round_wind * 4 + self.kyoku
    }
}
