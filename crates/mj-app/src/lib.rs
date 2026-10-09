//! 練習 App 的核心（M9）：人機對戰的對局 session、即時提示、給 UI 的牌桌狀態、對局後復盤。
//! 與 UI 無關：桌面版（Tauri command）與開發用的 `mj serve`（HTTP）都呼叫這裡。

pub mod api;
pub mod session;
pub mod table;

pub use api::App;
pub use session::{Ask, GameOver, Hint, ReviewConfig, ReviewState, Session, SessionConfig, Update};
pub use table::{EventView, Frame, TableView};
