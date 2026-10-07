//! 各種決策的選項與結果。每種決策的結果都是有限的：只能從框架列出的選項中挑一個。

use mj_core::{Pai, Tile};
use serde::{Deserialize, Serialize};
use std::fmt;

/// 決策的種類（依框架呼叫的先後順序排列）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum DecisionKind {
    /// 九種九牌：要不要流局重來
    Kyuushu,
    /// 能和牌時：要不要和（自摸、榮和、搶槓）
    Win,
    /// 能開槓時（暗槓、加槓）：要不要槓、槓哪個
    Kan,
    /// 何切：打哪一張
    Discard,
    /// 打出的這張可以立直時：要不要立直
    Riichi,
    /// 他家打牌可以吃碰槓時：要不要鳴、怎麼鳴
    Call,
}

impl DecisionKind {
    pub const ALL: [DecisionKind; 6] = [
        DecisionKind::Kyuushu,
        DecisionKind::Win,
        DecisionKind::Kan,
        DecisionKind::Discard,
        DecisionKind::Riichi,
        DecisionKind::Call,
    ];

    /// 中文名稱。
    pub fn name(self) -> &'static str {
        match self {
            DecisionKind::Kyuushu => "九種九牌",
            DecisionKind::Win => "和牌與否",
            DecisionKind::Kan => "開槓",
            DecisionKind::Discard => "何切",
            DecisionKind::Riichi => "立直與否",
            DecisionKind::Call => "鳴牌",
        }
    }
}

/// 和牌的方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WinKind {
    /// 自摸
    Tsumo,
    /// 榮和他家打出的牌
    Ron,
    /// 搶槓（榮和他家加槓或國士搶暗槓的牌）
    Chankan,
}

/// 開槓的選項。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum KanChoice {
    /// 暗槓這種牌
    Ankan(Tile),
    /// 把這張牌加到已碰的刻子上
    Kakan(Pai),
}

/// 鳴牌的選項。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CallChoice {
    /// 不鳴
    Pass,
    /// 用手中這兩張吃
    Chi([Pai; 2]),
    /// 用手中這兩張碰
    Pon([Pai; 2]),
    /// 大明槓
    Daiminkan,
}

impl fmt::Display for KanChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KanChoice::Ankan(t) => write!(f, "暗槓 {t}"),
            KanChoice::Kakan(p) => write!(f, "加槓 {p}"),
        }
    }
}

impl fmt::Display for CallChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CallChoice::Pass => write!(f, "不鳴"),
            CallChoice::Chi([a, b]) => write!(f, "吃（{a}{b}）"),
            CallChoice::Pon([a, b]) => write!(f, "碰（{a}{b}）"),
            CallChoice::Daiminkan => write!(f, "明槓"),
        }
    }
}
