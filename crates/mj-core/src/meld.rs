use crate::{Pai, Tile};
use serde::{Deserialize, Serialize};

/// 副露（鳴牌）的種類。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MeldKind {
    /// 吃（chi）：用手中兩張與上家打出的牌組成順子
    Chi,
    /// 碰（pon）：用手中兩張與他家打出的牌組成刻子
    Pon,
    /// 大明槓（daiminkan）：用手中三張槓他家打出的牌
    Daiminkan,
    /// 暗槓（ankan）：自己摸齊四張；仍算門前清
    Ankan,
    /// 加槓（kakan）：在已碰的刻子上加上第四張
    Kakan,
}

/// 副露（鳴牌）：已公開宣告的一組面子。`tiles` 包含這組的全部牌（3 或 4 張），含鳴進來的那張。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Meld {
    /// 種類
    pub kind: MeldKind,
    /// 這組面子的全部牌
    pub tiles: Vec<Pai>,
    /// 從他家取得的那張牌（暗槓為 `None`）
    pub called: Option<Pai>,
    /// 被鳴牌者的座位（絕對座位；暗槓為 `None`）
    pub from: Option<u8>,
}

impl Meld {
    /// 這組中最小的牌種（順子的起點，刻子／槓子本身的牌）。
    pub fn base(&self) -> Tile {
        self.tiles
            .iter()
            .map(|p| p.tile)
            .min()
            .expect("meld has tiles")
    }

    /// 是否為槓子（明槓、暗槓、加槓）。
    pub fn is_kan(&self) -> bool {
        matches!(
            self.kind,
            MeldKind::Daiminkan | MeldKind::Ankan | MeldKind::Kakan
        )
    }

    /// 是否破壞門前清：除了暗槓以外的副露都算「鳴牌」。
    pub fn is_open(&self) -> bool {
        self.kind != MeldKind::Ankan
    }

    /// 是否為順子（吃）。
    pub fn is_sequence(&self) -> bool {
        self.kind == MeldKind::Chi
    }
}
