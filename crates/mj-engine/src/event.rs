//! 事件紀錄：對局中發生的每件事（牌譜）。用於通知玩家、重播、統計與復盤。

use crate::Seat;
use mj_core::{Meld, Pai, Tile};
use mj_rules::riichi::Score;
use serde::{Deserialize, Serialize};

/// 流局（ryuukyoku）原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DrawReason {
    /// 荒牌流局：牌山摸完
    Exhaustive,
    /// 九種九牌
    KyuushuKyuuhai,
    /// 四風連打：第一巡四家都打出同一種風牌
    SuufonRenda,
    /// 四家立直
    SuuchaRiichi,
    /// 四槓散了：兩人以上合計開了四槓
    Suukaikan,
    /// 三家和：三家同時榮和同一張牌
    Sanchahou,
}

/// 一手牌的結束方式。

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum HandEnd {
    /// 和牌
    Win {
        /// 和牌者
        winner: Seat,
        /// 放銃者；自摸時等於 `winner`
        from: Seat,
        /// 和牌時的門前手牌（含和牌張）
        hand: Vec<Pai>,
        /// 和牌張
        win_tile: Pai,
        /// 計分結果
        score: Score,
        /// 裏寶牌指示牌（未立直時為空）
        ura_indicators: Vec<Tile>,
    },
    /// 流局
    Draw {
        /// 流局原因
        reason: DrawReason,
        /// 四家是否聽牌（依座位）
        tenpai: [bool; 4],
        /// 達成流局滿貫（nagashi mangan）的座位
        nagashi: Vec<Seat>,
    },
}

/// 完整資訊的對局紀錄（上帝視角）。玩家收到的是 [`Event::masked_for`] 遮蔽後的版本。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Event {
    /// 對局開始
    StartGame { seed: u64 },
    /// 一局開始（配牌）
    StartKyoku {
        /// 場風：0 = 東場、1 = 南場……
        round_wind: u8,
        /// 局（0..4），莊家座位 = kyoku
        kyoku: u8,
        /// 本場數
        honba: u8,
        /// 供託（立直棒）數
        riichi_sticks: u8,
        /// 四家點數
        scores: [i32; 4],
        /// 第一張寶牌指示牌
        dora_indicator: Tile,
        /// 四家配牌；遮蔽版本中別人的手牌為空
        hands: [Vec<Pai>; 4],
    },
    /// 摸牌；遮蔽版本中別人摸到的牌為 `None`。`rinshan` = 是否為嶺上牌。
    Draw {
        seat: Seat,
        pai: Option<Pai>,
        rinshan: bool,
    },
    /// 打牌；`tsumogiri` = 摸切，`riichi` = 立直宣言牌
    Discard {
        seat: Seat,
        pai: Pai,
        tsumogiri: bool,
        riichi: bool,
    },
    /// 副露（吃、碰、槓）
    Call { seat: Seat, meld: Meld },
    /// 立直成立（宣言牌沒有被榮和，已支付 1000 點供託）
    RiichiAccepted { seat: Seat },
    /// 翻開新的寶牌指示牌（槓寶牌）
    Dora { indicator: Tile },
    /// 一手牌結束：每個和牌者一筆（雙響時有兩筆），之後是 `EndKyoku`。
    /// `deltas` = 這筆結算的四家點數增減（不含立直時已支付的 1000 點）。
    HandEnd { end: HandEnd, deltas: [i32; 4] },
    /// 一局結束，附上結算後的四家點數
    EndKyoku { scores: [i32; 4] },
    /// 對局結束，附上最終點數（剩下的供託已歸第一名）
    EndGame { scores: [i32; 4] },
}

impl Event {
    /// 這個事件中 `seat` 看得到的部分：別人的配牌與摸牌會被隱藏。
    pub fn masked_for(&self, seat: Seat) -> Event {
        match self {
            Event::StartKyoku {
                round_wind,
                kyoku,
                honba,
                riichi_sticks,
                scores,
                dora_indicator,
                hands,
            } => {
                let mut hands = hands.clone();
                for (s, h) in hands.iter_mut().enumerate() {
                    if s != usize::from(seat) {
                        h.clear();
                    }
                }
                Event::StartKyoku {
                    round_wind: *round_wind,
                    kyoku: *kyoku,
                    honba: *honba,
                    riichi_sticks: *riichi_sticks,
                    scores: *scores,
                    dora_indicator: *dora_indicator,
                    hands,
                }
            }
            Event::Draw {
                seat: s, rinshan, ..
            } if *s != seat => Event::Draw {
                seat: *s,
                pai: None,
                rinshan: *rinshan,
            },
            e => e.clone(),
        }
    }
}
