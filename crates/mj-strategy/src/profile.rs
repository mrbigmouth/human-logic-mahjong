//! 策略側寫（Profile）：一個策略用了哪些考量與局勢修正器，以及各自的權重、響應曲線、參數。
//! 以 TOML 檔儲存，方便版本控管、比較差異與自動調參。範例：
//!
//! ```toml
//! name = "speed"
//! [[consideration]]
//! id = "efficiency.ukeire"
//! weight = 1.0
//! curve = [[0, 0], [40, 1]]
//! params = { lookahead = 2 }
//! [[modifier]]
//! id = "situation.all_last_behind"
//! params = { value_scale = 1.5 }
//! ```

use crate::decide::{Decider, Term};
use crate::{Consideration, Curve, Modifier};
use serde::Deserialize;
use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;

/// 模組參數（TOML 表格），由各模組的建構函式自行解讀。
pub type Params = toml::Table;

/// 策略側寫。
#[derive(Debug, Clone, Deserialize)]
pub struct Profile {
    /// 名稱
    pub name: String,
    /// 說明
    #[serde(default)]
    pub description: String,
    /// 使用的考量（TOML 中寫成 `[[consideration]]`）
    #[serde(default, rename = "consideration")]
    pub considerations: Vec<TermSpec>,
    /// 使用的局勢修正器（TOML 中寫成 `[[modifier]]`）
    #[serde(default, rename = "modifier")]
    pub modifiers: Vec<ModifierSpec>,
}

/// 側寫中的一個考量設定。
#[derive(Debug, Clone, Deserialize)]
pub struct TermSpec {
    /// 考量的識別碼
    pub id: String,
    /// 基礎權重（預設 1.0）
    #[serde(default = "one")]
    pub weight: f64,
    /// 響應曲線（預設恆等映射）
    #[serde(default)]
    pub curve: Curve,
    /// 模組參數
    #[serde(default)]
    pub params: Params,
}

/// 側寫中的一個局勢修正器設定。
#[derive(Debug, Clone, Deserialize)]
pub struct ModifierSpec {
    /// 修正器的識別碼
    pub id: String,
    /// 模組參數
    #[serde(default)]
    pub params: Params,
}

/// 權重預設值。
fn one() -> f64 {
    1.0
}

/// 側寫載入錯誤。
#[derive(Debug, Clone, PartialEq)]
pub enum ProfileError {
    /// TOML 格式錯誤
    Parse(String),
    /// 找不到這個識別碼的考量（未註冊）
    UnknownConsideration(String),
    /// 找不到這個識別碼的修正器（未註冊）
    UnknownModifier(String),
    /// 響應曲線的控制點沒有依 x 遞增
    UnsortedCurve(String),
    /// 模組參數不正確
    BadParams { id: String, message: String },
}

impl fmt::Display for ProfileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(m) => write!(f, "profile parse error: {m}"),
            Self::UnknownConsideration(id) => write!(f, "unknown consideration: {id}"),
            Self::UnknownModifier(id) => write!(f, "unknown modifier: {id}"),
            Self::UnsortedCurve(id) => write!(f, "curve points of {id} must be sorted by x"),
            Self::BadParams { id, message } => write!(f, "bad params for {id}: {message}"),
        }
    }
}

impl std::error::Error for ProfileError {}

impl FromStr for Profile {
    type Err = ProfileError;

    /// 從 TOML 文字解析（只檢查格式；模組是否存在要到 [`Profile::build`] 才檢查）。
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        toml::from_str(s).map_err(|e| ProfileError::Parse(e.to_string()))
    }
}

/// 模組建構函式的回傳值：失敗時附上錯誤訊息，由註冊表補上模組識別碼。
pub type FactoryResult<T> = Result<Box<T>, String>;
type ConsiderationFactory = fn(&Params) -> FactoryResult<dyn Consideration>;
type ModifierFactory = fn(&Params) -> FactoryResult<dyn Modifier>;

/// 註冊表：識別碼 → 模組建構函式。內建模組與使用者自訂的模組都在這裡註冊。
#[derive(Default)]
pub struct Registry {
    considerations: HashMap<&'static str, ConsiderationFactory>,
    modifiers: HashMap<&'static str, ModifierFactory>,
}

impl Registry {
    /// 註冊一個考量。
    pub fn add_consideration(&mut self, id: &'static str, f: ConsiderationFactory) {
        self.considerations.insert(id, f);
    }

    /// 註冊一個局勢修正器。
    pub fn add_modifier(&mut self, id: &'static str, f: ModifierFactory) {
        self.modifiers.insert(id, f);
    }
}

impl Profile {
    /// 依側寫從註冊表建立模組，組成可執行的決策器。
    pub fn build(&self, registry: &Registry) -> Result<Decider, ProfileError> {
        let terms = self
            .considerations
            .iter()
            .map(|spec| {
                let factory = registry
                    .considerations
                    .get(spec.id.as_str())
                    .ok_or_else(|| ProfileError::UnknownConsideration(spec.id.clone()))?;
                if !spec.curve.is_sorted() {
                    return Err(ProfileError::UnsortedCurve(spec.id.clone()));
                }
                Ok(Term {
                    consideration: factory(&spec.params).map_err(|m| bad_params(&spec.id, m))?,
                    weight: spec.weight,
                    curve: spec.curve.clone(),
                })
            })
            .collect::<Result<_, _>>()?;
        let modifiers = self
            .modifiers
            .iter()
            .map(|spec| {
                let factory = registry
                    .modifiers
                    .get(spec.id.as_str())
                    .ok_or_else(|| ProfileError::UnknownModifier(spec.id.clone()))?;
                factory(&spec.params).map_err(|m| bad_params(&spec.id, m))
            })
            .collect::<Result<_, _>>()?;
        Ok(Decider {
            name: self.name.clone(),
            terms,
            modifiers,
        })
    }
}

/// 把模組回報的參數錯誤附上識別碼。
fn bad_params(id: &str, message: String) -> ProfileError {
    ProfileError::BadParams {
        id: id.to_string(),
        message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 範例側寫 profiles/example.toml 可以解析。
    #[test]
    fn parses_example_profile() {
        let src = include_str!("../../../profiles/example.toml");
        let p: Profile = src.parse().unwrap();
        assert_eq!(p.name, "example");
        assert!(!p.considerations.is_empty());
        assert!(p.considerations.iter().all(|c| c.curve.is_sorted()));
    }

    /// 引用未註冊的考量會報錯。
    #[test]
    fn unknown_id_is_an_error() {
        let p: Profile = "name = \"x\"\n[[consideration]]\nid = \"nope\""
            .parse()
            .unwrap();
        assert_eq!(
            p.build(&Registry::default()).err(),
            Some(ProfileError::UnknownConsideration("nope".into()))
        );
    }
}
