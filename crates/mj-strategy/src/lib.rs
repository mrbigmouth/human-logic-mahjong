//! 可解釋的效用式 AI（Utility AI）。
//!
//! 每個合法動作都由一組「考量」（[`Consideration`]，一條人類的思路）打分。
//! 每個考量回傳原始值，以及機器可讀的理由（[`Reason`]）；原始值經過響應曲線（[`Curve`]）
//! 轉換、乘上權重後加總，就是這個動作的分數。
//! 權重會依局勢由「局勢修正器」（[`Modifier`]）動態調整。
//! 完整的分數拆解保存在決策紀錄（[`Trace`]）中，復盤時比較兩個動作的紀錄，
//! 就能說明「為什麼」一個選擇比另一個好。
//!
//! 詳細設計見 `docs/03-strategy-model.md`。

pub mod curve;
pub mod decide;
pub mod profile;
pub mod situation;

pub use curve::Curve;
pub use decide::{ActionTrace, Decider, Decision, TermTrace, Trace};
pub use profile::{Profile, ProfileError, Registry};
pub use situation::Situation;

use mj_engine::{Action, PlayerView};
use serde::Serialize;

/// 策略層，依資訊流的順序排列（前面層的結果會提供給後面的層）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum Layer {
    /// L1 讀牌：推測對手手牌、聽牌機率、各張牌的放銃危險度。
    Belief,
    /// L2 牌效：向聽數、進張、和牌速度。
    Efficiency,
    /// L3 牌型・打點：役種路線與和牌點數。
    Value,
    /// L4 防守・押引：放銃風險與和牌收益的取捨。
    Defense,
    /// L5 局勢：名次、點差、剩餘局數（主要透過修正器調整其他層的權重）。
    Situation,
    /// L6 印象操作：對手從我們的牌河讀到什麼（詐騙、迷彩）。
    Image,
}

impl Layer {
    /// 全部策略層。
    pub const ALL: [Layer; 6] = [
        Layer::Belief,
        Layer::Efficiency,
        Layer::Value,
        Layer::Defense,
        Layer::Situation,
        Layer::Image,
    ];

    /// 編號 0..6（[`Weights::layer`] 的索引）。
    pub fn index(self) -> usize {
        self as usize
    }
}

/// 機器可讀的理由。`code` 選擇訊息模板（由復盤層翻成各語言的文字），`args` 填入參數。
/// 例：`("ukeire.drop", [("before", 24.0), ("after", 16.0)])` →「進張從 24 張降到 16 張」。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Reason {
    /// 理由代碼
    pub code: &'static str,
    /// 參數（名稱、數值）
    pub args: Vec<(&'static str, f64)>,
}

impl Reason {
    /// 建立沒有參數的理由。
    pub fn new(code: &'static str) -> Self {
        Self {
            code,
            args: Vec::new(),
        }
    }

    /// 加上一個參數（可串接）。
    pub fn arg(mut self, name: &'static str, value: f64) -> Self {
        self.args.push((name, value));
        self
    }
}

/// 一個考量對一個動作的評估結果。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Eval {
    /// 原始值（單位由各考量自行定義，例如進張張數、期望點數）
    pub value: f64,
    /// 理由
    pub reason: Reason,
}

/// 讀牌層（L1）的輸出：對手模型，每次決策計算一次（M5 實作）。
#[derive(Debug, Clone, Default)]
pub struct Belief {}

/// 決策情境：每次決策計算一次，提供給所有考量共用。
pub struct Ctx<'a> {
    /// 玩家看得到的資訊
    pub view: &'a PlayerView,
    /// 局勢（名次、點差、剩餘局數……）
    pub situation: Situation,
    /// 讀牌結果
    pub belief: Belief,
}

impl<'a> Ctx<'a> {
    /// 由玩家視角建立決策情境。
    pub fn new(view: &'a PlayerView) -> Self {
        Self {
            view,
            situation: Situation::from_view(view),
            belief: Belief::default(),
        }
    }
}

/// 考量：把一條人類的思路寫成程式。要新增策略，就實作這個 trait。
pub trait Consideration: Send + Sync {
    /// 固定的識別碼，用於策略側寫（Profile）與決策紀錄，例如 `"efficiency.ukeire"`。
    fn id(&self) -> &'static str;
    /// 屬於哪一層（權重依層調整）。
    fn layer(&self) -> Layer;
    /// 評估一個動作。
    fn evaluate(&self, ctx: &Ctx, action: &Action) -> Eval;
}

/// 本次決策實際使用的權重。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Weights {
    /// 每一層的倍率，以 [`Layer::index`] 為索引；預設全部 1.0。
    pub layer: [f64; 6],
}

impl Default for Weights {
    fn default() -> Self {
        Self { layer: [1.0; 6] }
    }
}

impl Weights {
    /// 把某一層的倍率乘上 `factor`。
    pub fn scale(&mut self, layer: Layer, factor: f64) {
        self.layer[layer.index()] *= factor;
    }
}

/// 局勢修正器：依局勢調整權重，例如「最終局落後 → 打點 ×1.5」。
/// 生效時回傳理由，讓決策紀錄能說明權重為什麼改變。
pub trait Modifier: Send + Sync {
    /// 固定的識別碼，例如 `"situation.all_last_behind"`。
    fn id(&self) -> &'static str;
    /// 調整權重；沒有生效時回傳 `None`。
    fn adjust(&self, ctx: &Ctx, weights: &mut Weights) -> Option<Reason>;
}
