//! 日麻對局引擎：由 [`Player`]（玩家／AI）驅動的確定性狀態機。
//!
//! - 玩家只看得到 [`PlayerView`]（自己的手牌＋公開資訊）與遮蔽過的 [`Event`] 事件流，
//!   所以策略不可能偷看牌山或別人的手牌。
//! - 每場對局都能由 seed 完全重現。
//!
//! 模組：`wall` 牌山、`kyoku` 一局的流程、`game` 整場（半莊）的流程、`event` 事件紀錄。

mod event;
mod game;
mod kyoku;
mod wall;

pub use event::{DrawReason, Event, HandEnd};
pub use game::{GameConfig, GameResult, run_game};
pub use kyoku::EngineError;
pub use wall::{Wall, full_set, hand_seed};

use mj_core::{Meld, Pai, Tile, TileCounts};
use serde::{Deserialize, Serialize};

/// 座位編號 0..4，以東一局的莊家為 0（起家），依逆時針（下家方向）遞增。
pub type Seat = u8;

/// 玩家在決策點可以選擇的動作。從手牌拿出的牌用 [`Pai`] 表示，才能分清楚是不是赤五。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Action {
    /// 打牌（dahai）。`tsumogiri` = 摸切（打出剛摸到的牌），其他玩家看得到，是讀牌的線索。
    Discard { pai: Pai, tsumogiri: bool },
    /// 立直（riichi）：宣告立直並打出這張牌。
    Riichi { pai: Pai, tsumogiri: bool },
    /// 吃（chi）：用手中這兩張牌吃上家剛打出的牌。
    Chi { consumed: [Pai; 2] },
    /// 碰（pon）：用手中這兩張牌碰剛打出的牌。
    Pon { consumed: [Pai; 2] },
    /// 大明槓（daiminkan）：用手中三張相同的牌槓剛打出的牌。
    Daiminkan,
    /// 暗槓（ankan）：把手中四張相同的牌槓出來。
    Ankan { tile: Tile },
    /// 加槓（kakan）：把這張牌加到已碰的刻子上。
    Kakan { pai: Pai },
    /// 自摸（tsumo）和牌
    Tsumo,
    /// 榮和（ron）：和別人打出的牌
    Ron,
    /// 九種九牌（kyuushu kyuuhai）：第一巡手上有 9 種以上幺九牌時宣告流局。
    KyuushuKyuuhai,
    /// 跳過（不吃、不碰、不和）
    Pass,
}

/// 牌河（捨牌區）中的一張牌。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscardRecord {
    /// 打出的牌
    pub pai: Pai,
    /// 是否為摸切（反之為手切）
    pub tsumogiri: bool,
    /// 是否為宣告立直的那張牌（立直宣言牌）
    pub riichi_declared: bool,
    /// 是否被別人吃碰槓拿走
    pub called: bool,
}

/// 玩家在決策點看得到的全部資訊（自己的手牌＋公開資訊）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerView {
    /// 自己的座位
    pub me: Seat,
    /// 場風（round wind）：0 = 東場、1 = 南場……
    pub round_wind: u8,
    /// 局（kyoku）：該場的第幾局（0..4）；莊家座位 = `kyoku`
    pub kyoku: u8,
    /// 本場數（honba）
    pub honba: u8,
    /// 場上的供託（立直棒）數
    pub riichi_sticks: u8,
    /// 四家目前點數（依座位）
    pub scores: [i32; 4],
    /// 寶牌指示牌
    pub dora_indicators: Vec<Tile>,
    /// 門前手牌（含剛摸到的牌）
    pub hand: TileCounts,
    /// 同一手牌，保留赤五資訊，已排序
    pub hand_pais: Vec<Pai>,
    /// 自己回合剛摸到的牌
    pub drawn: Option<Pai>,
    /// 四家的副露
    pub melds: [Vec<Meld>; 4],
    /// 四家的牌河
    pub rivers: [Vec<DiscardRecord>; 4],
    /// 四家是否已立直
    pub riichi: [bool; 4],
    /// 牌山剩餘張數（可摸的部分，不含王牌）
    pub tiles_left_in_wall: u8,
    /// 正規局數（半莊 8 局）——用來判斷還剩幾局
    pub total_kyoku: u8,
    /// 正在回應的那張牌（他家打出的牌或加槓的牌）與其座位
    pub last_discard: Option<(Seat, Pai)>,
}

impl PlayerView {
    /// 莊家（oya）的座位。
    pub fn dealer(&self) -> Seat {
        self.kyoku % 4
    }

    /// 從東一局起算的局序（東一 = 0、南一 = 4……）。
    pub fn current_kyoku_index(&self) -> u8 {
        self.round_wind * 4 + self.kyoku
    }

    /// 某座位的門風：0 = 東 .. 3 = 北。
    pub fn seat_wind(&self, seat: Seat) -> u8 {
        (seat + 4 - self.dealer()) % 4
    }
}

/// 對局參與者：AI 策略、人類操作介面、遠端 AI……
pub trait Player {
    /// 每個事件都會通知，內容已遮蔽成這個座位看得到的部分。
    fn on_event(&mut self, _me: Seat, _event: &Event) {}

    /// 從合法動作 `legal`（不會是空的）中選一個。回傳不合法的動作會讓對局以錯誤結束。
    fn act(&mut self, view: &PlayerView, legal: &[Action]) -> Action;
}
