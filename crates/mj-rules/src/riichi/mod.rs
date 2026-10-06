//! 日本立直麻將。預設值採用天鳳段位戰規則（[`RiichiConfig::TENHOU`]）。
//!
//! - `yaku`：役種列表與番數
//! - `score`：和牌的番、符、點數計算

mod score;
mod yaku;

pub use score::{Limit, RiichiState, Score, WinContext, dora_from_indicator, score};
pub use yaku::Yaku;

use crate::RuleSet;
use serde::{Deserialize, Serialize};

/// 規則選項（各平台、各雀莊的差異）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RiichiConfig {
    /// true = 半莊（hanchan，東場＋南場），false = 東風戰（tonpuu，只打東場）。
    pub hanchan: bool,
    /// 赤五數量：3 = 赤五萬、赤五筒、赤五索各一張。
    pub red_fives: u8,
    /// 食斷（kuitan）：副露後是否仍承認斷么九。
    pub kuitan: bool,
    /// 起始點數。
    pub starting_points: i32,
    /// 返點：最終局結束時需要有人達到這個點數才結束，否則進入下一個風場（西入）。
    pub target_points: i32,
    /// 順位馬（uma），單位千點，依序為第 1–4 名。
    pub uma: [i32; 4],
    /// 切上滿貫（kiriage）：4 番 30 符、3 番 60 符算滿貫。
    pub kiriage: bool,
    /// 累計役滿（kazoe yakuman）：13 番以上算役滿。
    pub kazoe_yakuman: bool,
    /// 役滿複合：同一手有多個役滿時累加（例如字一色＋大三元 = 雙倍役滿）。
    pub composite_yakuman: bool,
    /// 雙響：允許兩家同時榮和同一張牌；三家同時榮和則途中流局（三家和）。
    /// false 時採用截和（atamahane），只有離放銃者最近的一家和牌。
    pub double_ron: bool,
    /// 擊飛（tobi）：有人點數低於 0 時立刻結束。
    pub bust_ends_game: bool,
    /// 和了止め（agari-yame）：最終局莊家連莊且為第一名時結束對局。
    pub agari_yame: bool,
}

impl RiichiConfig {
    /// 天鳳段位戰規則：半莊、赤寶牌 3 張、食斷、25000 起 30000 返、
    /// 不採用切上滿貫、有累計役滿與役滿複合、雙響、擊飛、和了止め。
    pub const TENHOU: RiichiConfig = RiichiConfig {
        hanchan: true,
        red_fives: 3,
        kuitan: true,
        starting_points: 25_000,
        target_points: 30_000,
        uma: [20, 10, -10, -20],
        kiriage: false,
        kazoe_yakuman: true,
        composite_yakuman: true,
        double_ron: true,
        bust_ends_game: true,
        agari_yame: true,
    };
}

impl Default for RiichiConfig {
    fn default() -> Self {
        Self::TENHOU
    }
}

/// 日本立直麻將規則。
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
