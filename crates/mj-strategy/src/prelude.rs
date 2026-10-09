//! 策略編輯者在每個檔案開頭寫 `use crate::prelude::*;`，就能使用框架型別與全部工具。
//!
//! 工具的完整清單與範例見 `crates/mj-strategy/TOOLS.md`。

pub use crate::framework::{Book, CallChoice, Claim, Ctx, DecisionKind, KanChoice, Param, WinKind};
pub use mj_engine::Event;
/// 工具箱：`tools::hand`、`tools::safety`、`tools::value`……
pub use mj_tools as tools;
pub use mj_tools::after::AfterEval;
pub use mj_tools::table::Standing;
pub use mj_tools::yaku_path::OpenYakuPath;
pub use mj_tools::{Action, Meld, MeldKind, Pai, PlayerView, Seat, Suit, Tile, TileCounts};
