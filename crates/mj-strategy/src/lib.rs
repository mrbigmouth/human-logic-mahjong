//! 策略：把人類打麻將的思路寫成程式。
//!
//! - `src/strategy-list/`：**策略編輯者從這裡開始**。每個子目錄是一個策略（以規則命名），
//!   內含 `main.rs`（登記什麼情況呼叫哪個決策）、`memory.rs`（對局記憶）、
//!   `decision/`（各種決策，每個檔案匯出一個 `decide` 函式）、`utils/`（輔助判斷與預測）。
//!   新增檔案或目錄不需要寫 `mod` 宣告，建置腳本（build.rs）會自動產生。
//! - [`framework`]：決定如何呼叫策略（把引擎的詢問拆成各種決策、記錄理由與預測）。
//! - [`prelude`]：策略檔案開頭 `use crate::prelude::*;` 即可使用框架型別與工具。
//! - 工具清單：`TOOLS.md`。

pub mod framework;
pub mod prelude;

// 由 build.rs 依 src/strategy-list/ 的目錄結構自動產生：
// `pub mod strategy_list { ... }` 與 `pub fn all_strategies()`。
include!(concat!(env!("OUT_DIR"), "/strategy_list.rs"));

/// 依名稱找策略（名稱 = strategy-list 下的目錄名稱）。
pub fn find_strategy(name: &str) -> Option<Box<dyn framework::Strategy>> {
    all_strategies().into_iter().find(|s| s.name() == name)
}
