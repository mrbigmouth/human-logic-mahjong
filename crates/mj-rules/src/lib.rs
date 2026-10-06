//! 規則集。與特定規則相關的計算（役、符、點數、規則選項）放在各規則的子模組，
//! 目前只有日本立直麻將（[`riichi`]）。之後加入台灣 16 張、國標麻將時新增對應模組，
//! 策略、模擬、復盤只依賴 `mj-engine` 的共通介面，不必修改。

pub mod riichi;

/// 一種麻將規則的基本資訊。
pub trait RuleSet: Send + Sync {
    /// 簡短識別碼，例如 `"riichi"`。
    fn id(&self) -> &'static str;

    /// 摸牌前的手牌張數（日麻 13 張，台灣麻將 16 張）。
    fn hand_size(&self) -> usize;

    /// 每種牌有幾張。
    fn copies_per_kind(&self) -> u8 {
        4
    }

    // 對局流程目前由 mj-engine 針對日麻實作；之後視需要再把共通部分提到這裡。
}
