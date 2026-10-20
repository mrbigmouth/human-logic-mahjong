//! 策略的參數版本（M16 A）：在 GUI 編輯策略的可調參數（`Param`），存成參數檔，不必編譯就能用於對局與測試。
//!
//! 參數檔放在 `<資料目錄>/params/<規則>/<策略>/<版本>.params`（`mj params` 的格式：每行一個「名稱 = 值」）。
//! 版本的參賽者名稱是 `<策略>@<參數檔路徑>`（`find_strategy_spec` 的格式），所以 CLI 也能直接用，
//! 也會列在參賽者登錄（開局設定、提示與復盤、對戰測試的選單）中；更新時間取策略原始碼與參數檔中較晚者。

use mj_strategy::find_strategy;
use mj_strategy::framework::ParamSet;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// 一個可調參數（給 UI）。
#[derive(Debug, Clone, Serialize)]
pub struct ParamInfo {
    pub name: &'static str,
    pub doc: &'static str,
    pub default: f64,
    pub min: f64,
    pub max: f64,
}

/// 一個參數版本。
#[derive(Debug, Clone, Serialize)]
pub struct Variant {
    /// 策略名稱（「規則／策略」）
    pub strategy: String,
    /// 版本名稱
    pub name: String,
    /// 參賽者名稱：`<策略>@<參數檔路徑>`
    pub spec: String,
    /// 覆寫的值
    pub values: BTreeMap<String, f64>,
    /// 參數檔的修改時間（Unix 秒）
    pub updated: Option<u64>,
}

/// 參數檔的存放處。
pub struct ParamStore {
    /// `<資料目錄>/params`（`None` = 沒有資料目錄，不能存版本）
    dir: Option<PathBuf>,
}

fn mtime(p: &Path) -> Option<u64> {
    let t = std::fs::metadata(p).ok()?.modified().ok()?;
    Some(t.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs())
}

/// 版本名稱：1–40 個字，只能是文字、數字、`-`、`_`（會成為檔名）。
fn check_name(name: &str) -> Result<(), String> {
    let ok = !name.is_empty()
        && name.chars().count() <= 40
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_');
    if ok {
        Ok(())
    } else {
        Err("版本名稱要 1–40 個字，只能用文字、數字、- 與 _".into())
    }
}

/// 策略的可調參數。
pub fn params_of(strategy: &str) -> Result<Vec<ParamInfo>, String> {
    let st = find_strategy(strategy).ok_or_else(|| format!("找不到策略 {strategy}"))?;
    Ok(st
        .params()
        .iter()
        .map(|p| ParamInfo {
            name: p.name,
            doc: p.doc,
            default: p.default,
            min: p.min,
            max: p.max,
        })
        .collect())
}

impl ParamStore {
    pub fn new(dir: Option<PathBuf>) -> ParamStore {
        ParamStore { dir }
    }

    fn strategy_dir(&self, strategy: &str) -> Result<PathBuf, String> {
        let dir = self.dir.as_ref().ok_or("沒有資料目錄，不能保存參數版本")?;
        if find_strategy(strategy).is_none() {
            return Err(format!("找不到策略 {strategy}"));
        }
        // 策略名稱是「規則／策略」，兩層目錄。
        Ok(dir.join(strategy))
    }

    fn variant_at(&self, strategy: &str, path: &Path) -> Option<Variant> {
        let name = path.file_stem()?.to_str()?.to_string();
        let text = std::fs::read_to_string(path).ok()?;
        let values = ParamSet::parse(&text).ok()?.0;
        Some(Variant {
            strategy: strategy.to_string(),
            spec: format!("{strategy}@{}", path.display()),
            name,
            values,
            updated: mtime(path),
        })
    }

    /// 一個策略的所有版本（依名稱）。
    pub fn variants(&self, strategy: &str) -> Vec<Variant> {
        let Ok(dir) = self.strategy_dir(strategy) else {
            return Vec::new();
        };
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return Vec::new();
        };
        let mut v: Vec<Variant> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "params"))
            .filter_map(|p| self.variant_at(strategy, &p))
            .collect();
        v.sort_by(|a, b| a.name.cmp(&b.name));
        v
    }

    /// 所有策略的所有版本。
    pub fn all(&self) -> Vec<Variant> {
        mj_strategy::all_strategies()
            .iter()
            .flat_map(|s| self.variants(s.name()))
            .collect()
    }

    /// 保存（新增或覆蓋）一個版本。參數名稱必須是這個策略的參數，值必須是有限的數字。
    pub fn save(
        &self,
        strategy: &str,
        name: &str,
        values: &BTreeMap<String, f64>,
    ) -> Result<Variant, String> {
        check_name(name)?;
        let dir = self.strategy_dir(strategy)?;
        let st = find_strategy(strategy).ok_or_else(|| format!("找不到策略 {strategy}"))?;
        if let Some((k, _)) = values.iter().find(|(_, v)| !v.is_finite()) {
            return Err(format!("參數 {k} 的值不是有限的數字"));
        }
        let set = ParamSet(values.clone());
        set.check(st.params())?;
        std::fs::create_dir_all(&dir).map_err(|e| format!("無法建立 {}：{e}", dir.display()))?;
        let path = dir.join(format!("{name}.params"));
        let text = format!(
            "# {strategy} 的參數版本「{name}」（App 的策略參數畫面儲存）\n{}",
            set.to_text()
        );
        std::fs::write(&path, text).map_err(|e| format!("無法寫入 {}：{e}", path.display()))?;
        self.variant_at(strategy, &path)
            .ok_or_else(|| "保存後讀不回來".to_string())
    }

    /// 刪除一個版本。
    pub fn delete(&self, strategy: &str, name: &str) -> Result<(), String> {
        check_name(name)?;
        let path = self.strategy_dir(strategy)?.join(format!("{name}.params"));
        std::fs::remove_file(&path).map_err(|e| format!("無法刪除 {}：{e}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HR: &str = "tenhou-normal-4p-hanchan/hand-reading";

    #[test]
    fn save_list_load_delete() {
        let dir = std::env::temp_dir().join(format!("mj-params-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = ParamStore::new(Some(dir.clone()));
        let p = params_of(HR).unwrap();
        assert!(!p.is_empty());
        let mut values = BTreeMap::new();
        values.insert(p[0].name.to_string(), p[0].max);
        let v = store.save(HR, "試驗-1", &values).unwrap();
        assert_eq!(store.variants(HR).len(), 1);
        // 參賽者名稱可以直接交給 find_strategy_spec。
        let st = mj_strategy::find_strategy_spec(&v.spec).unwrap();
        assert_eq!(st.overrides().0, values);
        // 不合法的名稱、參數、值
        assert!(store.save(HR, "a/b", &values).is_err());
        let mut bad = values.clone();
        bad.insert("沒有這個參數".into(), 1.0);
        assert!(store.save(HR, "x", &bad).is_err());
        let mut nan = BTreeMap::new();
        nan.insert(p[0].name.to_string(), f64::NAN);
        assert!(store.save(HR, "x", &nan).is_err());
        store.delete(HR, "試驗-1").unwrap();
        assert!(store.variants(HR).is_empty());
        assert!(ParamStore::new(None).save(HR, "x", &values).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
