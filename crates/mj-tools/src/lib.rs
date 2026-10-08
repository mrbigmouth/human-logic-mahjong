//! 策略編輯者的工具箱：只做計算、不做決定的函式。
//!
//! 給同樣的輸入一定得到同樣的結果，也不帶任何「該怎麼打」的判斷——判斷寫在策略程式裡
//! （`mj-strategy/src/strategy-list/`）。完整清單與範例見 `crates/mj-strategy/TOOLS.md`。
//!
//! | 模組 | 內容 |
//! |---|---|
//! | [`visible`] | 看得到的牌、剩餘張數 |
//! | [`after`] | 做了某個動作後的向聽數與進張 |
//! | [`hand`] | 向聽、進張、聽牌張、何切排序（`mj-calc` 的整理） |
//! | [`yaku_path`] | 鳴牌後還有沒有役 |
//! | [`dora`] | 寶牌、赤五 |
//! | [`safety`] | 現物、筋、No Chance、One Chance 等安全度「事實」（不含機率推測） |
//! | [`river`] | 牌河與副露的事實：手切／摸切順序、各花色張數、副露的寶牌與役牌 |
//! | [`value`] | 和牌點數計算、目前手牌若和牌值多少 |
//! | [`table`] | 牌桌狀態：誰立直、第幾巡、座位與風位 |

pub mod after;
pub mod dora;
pub mod hand;
pub mod river;
pub mod safety;
pub mod table;
pub mod value;
pub mod visible;
pub mod yaku_path;

pub use mj_calc::RemainingEstimate;
pub use mj_core::{Meld, MeldKind, Pai, Suit, Tile, TileCounts, parse_pais, parse_tiles};
pub use mj_engine::{Action, DiscardRecord, PlayerView, Seat};
