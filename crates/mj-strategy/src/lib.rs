//! 策略：把人類打麻將的思路寫成程式。
//!
//! - `src/strategy-list/<規則>/<策略>/`：**策略編輯者從這裡開始**。第一層是規則，第二層是該規則下的各個策略，
//!   每個策略內含 `README.md`（策略說明，GUI 選策略時顯示）、`main.rs`（登記什麼情況呼叫哪個決策）、`memory.rs`（對局記憶）、
//!   `decision/`（各種決策，每個檔案匯出一個 `decide` 函式）、`utils/`（輔助判斷與預測）。
//!   新增檔案或目錄不需要寫 `mod` 宣告，建置腳本（build.rs）會自動產生。
//! - [`framework`]：決定如何呼叫策略（把引擎的詢問拆成各種決策、記錄理由與預測）。
//! - [`prelude`]：策略檔案開頭 `use crate::prelude::*;` 即可使用框架型別與工具。
//! - 工具清單：`TOOLS.md`。

pub mod framework;
pub mod prelude;

// 由 build.rs 依 src/strategy-list/ 的目錄結構自動產生：
// `pub mod strategy_list { ... }`、`pub fn all_strategies()` 與 `pub fn all_rules()`。
include!(concat!(env!("OUT_DIR"), "/strategy_list.rs"));

/// 依名稱找策略（名稱 =「規則／策略」，例如 `tenhou-normal-4p-hanchan/push-fold`）。
pub fn find_strategy(name: &str) -> Option<Box<dyn framework::Strategy>> {
    all_strategies().into_iter().find(|s| s.name() == name)
}

/// 依「策略名稱＋參數覆寫」找策略：`名稱`、`名稱@參數檔`（每行一個「參數 = 值」）或
/// `名稱@參數=值,參數=值`。例如 `tenhou-normal-4p-hanchan/hand-reading@call.far_shanten=3`。
pub fn find_strategy_spec(spec: &str) -> Result<Box<dyn framework::Strategy>, String> {
    let (name, overrides) = match spec.split_once('@') {
        Some((n, o)) => (n, Some(o)),
        None => (spec, None),
    };
    let st = find_strategy(name)
        .ok_or_else(|| format!("找不到策略 {name}（用 `mj strategies` 查看可用的策略）"))?;
    let Some(o) = overrides else {
        return Ok(st);
    };
    let text = if o.contains('=') {
        o.to_string()
    } else {
        std::fs::read_to_string(o).map_err(|e| format!("讀取參數檔 {o} 失敗：{e}"))?
    };
    st.with_params(framework::ParamSet::parse(&text)?)
}
