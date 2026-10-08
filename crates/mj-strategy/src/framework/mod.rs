//! 框架：決定「如何呼叫 strategy-list 中的策略」。策略編輯者通常不需要修改這裡。

pub mod book;
pub mod choice;
pub mod claim;
pub mod ctx;
pub mod explain;
pub mod player;

pub use book::Book;
pub use choice::{CallChoice, DecisionKind, KanChoice, WinKind};
pub use claim::{Claim, ClaimRecord, TargetSituation};
pub use ctx::Ctx;
pub use player::{
    BookPlayer, DecisionRecord, Force, Situation, Strategy, StrategyPlayer, book_strategy,
    option_of_action,
};
