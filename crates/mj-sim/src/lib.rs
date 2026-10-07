//! 自我對戰模擬。
//!
//! - M1：參考機器人（`bots`），用於引擎的模糊測試，也當作比較強弱的基準。
//! - M3：duplicate（複式）模擬器——同一組牌山讓各策略輪流坐每個座位以抵消運氣，
//!   平行執行並輸出統計（平均順位、和牌率、放銃率、信賴區間）。

pub mod bots;
pub mod counterfactual;
pub mod predict_eval;
pub mod replay;
