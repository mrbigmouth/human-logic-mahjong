//! 預測（claim）：策略對「牌局中看不到的事實」所做的機率判斷。
//!
//! 預測和大局觀決策不同：在全知視角（看得到所有人的手牌）下，預測對不對可以立刻判定，
//! 所以用預測準確度（Brier 分數、對數損失、校準曲線）來評分與調整，與勝負無關。
//!
//! 每種預測都必須在框架這裡定義「真相怎麼算」，評估工具（`mj predict-eval`）才能自動檢查。
//! 策略的預測函式（`utils/predict/`）在計算完後呼叫 `ctx.claim(來源, 預測)` 記錄下來。

use mj_core::Tile;
use mj_engine::Seat;
use serde::{Deserialize, Serialize};

/// 預測的內容。`p` = 預測「是」的機率（0–1）。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Claim {
    /// `seat` 此刻是否聽牌。真相：他目前的手牌向聽數為 0。
    Tenpai { seat: Seat, p: f64 },
    /// 打出 `tile` 會不會被 `seat` 榮和（不考慮他有沒有役）。
    /// 真相：`tile` 在他目前的聽牌張中，而且他沒有振聽（捨牌、立直、同巡振聽）。
    Waits { seat: Seat, tile: Tile, p: f64 },
    /// 這一局最後是不是由我和牌（押引判斷用，例如「我跟立直者對攻能不能贏」）。
    /// 真相：本局結束時我有沒有和牌；本局由其他人和牌或流局都算「否」。
    IWinThisHand { p: f64 },
    /// 如果 `seat` 這一局和牌，是否為滿貫以上（基本點 ≥ 2000，例如閒家榮和 ≥ 8000 點）。打點預測。
    /// 真相：本局之後 `seat` 和牌時的計分；他沒有和牌時無法判定（不列入評分）。
    BigHand { seat: Seat, p: f64 },
}

impl Claim {
    /// 預測的機率。
    pub fn p(&self) -> f64 {
        match *self {
            Claim::Tenpai { p, .. }
            | Claim::Waits { p, .. }
            | Claim::IWinThisHand { p }
            | Claim::BigHand { p, .. } => p,
        }
    }

    /// 預測的類別名稱（統計分組用）。
    pub fn category(&self) -> &'static str {
        match self {
            Claim::Tenpai { .. } => "聽牌判斷",
            Claim::Waits { .. } => "聽牌張判斷",
            Claim::IWinThisHand { .. } => "本局和牌",
            Claim::BigHand { .. } => "滿貫以上",
        }
    }
}

/// 一筆預測紀錄。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClaimRecord {
    /// 哪一個預測函式做的（例如 `"predict::tenpai"`）
    pub source: String,
    /// 預測內容
    pub claim: Claim,
    /// 做預測的座位
    pub me: Seat,
    /// 做預測時已經發生了幾個事件（對應完整牌譜中的位置，用來重建當時的真相）
    pub at_event: usize,
    /// 被預測者當時的情境（分組統計用）
    pub target: TargetSituation,
}

/// 被預測者（預測的對象；「本局和牌」預測則是自己）當時的情境。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetSituation {
    /// 他打過幾張（巡目）
    pub turn: u8,
    /// 他的副露組數（不含暗槓）
    pub open_melds: u8,
    /// 他是否已立直
    pub riichi: bool,
}

impl Claim {
    /// 預測的對象座位（「本局和牌」為 `None`，代表自己）。
    pub fn target(&self) -> Option<Seat> {
        match *self {
            Claim::Tenpai { seat, .. }
            | Claim::Waits { seat, .. }
            | Claim::BigHand { seat, .. } => Some(seat),
            Claim::IWinThisHand { .. } => None,
        }
    }
}
