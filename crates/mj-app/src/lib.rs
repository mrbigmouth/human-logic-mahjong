//! 練習 App 的核心（M9）：人機對戰的對局 session、即時提示、給 UI 的牌桌狀態、對局後復盤；
//! 牌譜的讀入、各視角的檢視與復盤（M12）；反事實實驗（M14）；多個顧問（策略或 Mortal）的提示與復盤（M14.5）；對戰測試平台（M15）。
//! 與 UI 無關：桌面版（Tauri command）與開發用的 `mj serve`（HTTP）都呼叫這裡。

pub mod advisor;
pub mod api;
pub mod experiment;
pub mod paifu;
pub mod review;
pub mod session;
pub mod table;
pub mod testbed;

pub use api::App;
pub use experiment::{ExperimentRequest, ExperimentState};
pub use paifu::{LoadedLog, LogFrames, LogInfo, LogMeta};
pub use session::{Ask, GameOver, Hint, ReviewConfig, ReviewState, Session, SessionConfig, Update};
pub use table::{EventView, Frame, TableView, log_frames, table_mismatch};
