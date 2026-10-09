//! 可調參數：策略中有名字、有預設值與合理範圍的數值。
//!
//! 策略照常把數值寫成具名常數，只是型別改成 [`Param`]，讀取時用 `ctx.param(&名稱)`：
//!
//! ```ignore
//! /// 最近 3 張中，每多 1 張手切，聽牌的勝算乘上這個值。
//! pub const PER_RECENT_TEDASHI: Param =
//!     Param::new("tenpai.per_recent_tedashi", 0.73, 0.4, 1.0, "最近 3 張每多 1 張手切的勝算倍數");
//!
//! let factor = ctx.param(&PER_RECENT_TEDASHI);
//! ```
//!
//! 預設值就寫在程式裡（讀程式就知道策略怎麼想）；執行時可以用 [`ParamSet`] 覆寫，
//! 不必改程式就能比較不同的值。自動調整工具（`mj tune`）就是靠覆寫來搜尋更好的值，
//! 找到後再把程式裡的預設值改掉。
//!
//! 建置腳本會自動找出每個策略中所有 `pub const 名稱: Param = Param::new(...)`，
//! 列在 `Strategy::params()`（`mj params <策略>` 可以查看）。

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// 一個可調參數。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Param {
    /// 唯一的名稱，例如 `"tenpai.per_recent_tedashi"`（覆寫與調整時用）
    pub name: &'static str,
    /// 預設值
    pub default: f64,
    /// 合理範圍的下限（自動調整只在範圍內搜尋）
    pub min: f64,
    /// 合理範圍的上限
    pub max: f64,
    /// 一句話說明
    pub doc: &'static str,
}

impl Param {
    /// 宣告一個可調參數。
    pub const fn new(
        name: &'static str,
        default: f64,
        min: f64,
        max: f64,
        doc: &'static str,
    ) -> Self {
        Param {
            name,
            default,
            min,
            max,
            doc,
        }
    }
}

/// 一組參數覆寫值（參數名稱 → 值）。沒有覆寫的參數使用預設值。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ParamSet(pub BTreeMap<String, f64>);

impl ParamSet {
    /// 參數的值：有覆寫用覆寫值，否則用預設值。
    pub fn get(&self, p: &Param) -> f64 {
        self.0.get(p.name).copied().unwrap_or(p.default)
    }

    /// 設定覆寫值。
    pub fn set(&mut self, name: &str, value: f64) {
        self.0.insert(name.to_string(), value);
    }

    /// 是否沒有任何覆寫。
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// 解析「名稱=值」的清單（以逗號、分號或換行分隔；`#` 之後是註解），例如
    /// `"tenpai.per_recent_tedashi=0.7, call.far_shanten=3"`。參數檔就是每行一個「名稱 = 值」。
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut set = ParamSet::default();
        for item in text.split([',', ';', '\n']) {
            let item = item.split('#').next().unwrap_or("").trim();
            if item.is_empty() {
                continue;
            }
            let (k, v) = item
                .split_once('=')
                .ok_or_else(|| format!("參數「{item}」應寫成 名稱=值"))?;
            let v: f64 = v
                .trim()
                .parse()
                .map_err(|_| format!("參數 {} 的值「{}」不是數字", k.trim(), v.trim()))?;
            set.set(k.trim(), v);
        }
        Ok(set)
    }

    /// 寫成參數檔的格式（每行一個「名稱 = 值」）。
    pub fn to_text(&self) -> String {
        self.0.iter().map(|(k, v)| format!("{k} = {v}\n")).collect()
    }

    /// 檢查每個覆寫的名稱都是 `params` 中的參數。
    pub fn check(&self, params: &[Param]) -> Result<(), String> {
        for k in self.0.keys() {
            if !params.iter().any(|p| p.name == k) {
                return Err(format!("沒有名為 {k} 的參數（用 `mj params <策略>` 查看）"));
            }
        }
        Ok(())
    }
}

/// 整數參數的值（四捨五入），例如「幾向聽」「幾張」。
pub fn round(v: f64) -> i64 {
    v.round() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: Param = Param::new("a", 1.5, 0.0, 3.0, "測試");

    #[test]
    fn parse_and_get() {
        let s = ParamSet::parse("a = 2.0 # 註解\n b=3, ").unwrap();
        assert_eq!(s.get(&A), 2.0);
        assert_eq!(ParamSet::default().get(&A), 1.5);
        assert!(s.check(&[A]).is_err());
        assert!(ParamSet::parse("a").is_err());
        assert_eq!(ParamSet::parse(&s.to_text()).unwrap(), s);
    }
}
