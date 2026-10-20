//! 提示與復盤的顧問（M14.5）：開局時選最多三個策略或 mjai AI（Mortal），對局中同時列出它們的選擇，
//! 復盤時第一個顧問是比較基準。
//!
//! 決策的結構（有哪些決策、選項）與重打（反事實評估、實驗）都要用策略：用顧問中的第一個策略
//! （「基礎策略」）；顧問都是 AI 時用預設策略，但不顯示它的理由。

use crate::session::DEFAULT_STRATEGY;
use mj_review::advisor::{Advisor, AdvisorKind};
use mj_strategy::find_strategy_spec;
use mj_strategy::framework::Strategy;
use serde::Serialize;
use std::sync::Arc;

/// 顧問數上限。
pub const MAX_ADVISORS: usize = 3;

/// 顧問的概要（給 UI 標示是誰的選擇）。
#[derive(Debug, Clone, Serialize)]
pub struct AdvisorInfo {
    pub name: String,
    pub title: String,
    /// mjai AI（沒有理由，只有機率）
    pub ai: bool,
}

/// 一組顧問。
#[derive(Clone)]
pub struct Advisors {
    pub list: Vec<Advisor>,
    /// 基礎策略：決策結構與重打用
    pub base: Arc<dyn Strategy>,
    /// 基礎策略是第幾個顧問（`None` = 顧問都是 AI，基礎策略是預設策略）
    pub base_index: Option<usize>,
}

/// Mortal 的包裝程式（`tools/mortal/run.sh`）。
fn is_mortal(cmd: &[String]) -> bool {
    cmd.first().is_some_and(|p| p.ends_with("mortal/run.sh"))
}

/// 由名稱建立顧問：策略名稱（可加 `@參數`）或 `mjai:<程式 參數…>`。
/// Mortal 自動加上 `--q-values`（每個回應附 Q 值，才算得出機率）。
pub fn parse_advisor(name: &str) -> Result<Advisor, String> {
    let name = name.trim();
    if let Some(cmd) = name.strip_prefix("mjai:") {
        let mut cmd: Vec<String> = cmd.split_whitespace().map(String::from).collect();
        if cmd.is_empty() {
            return Err("mjai: 後面要接外部程式".into());
        }
        if is_mortal(&cmd) && !cmd.iter().any(|a| a == "--q-values") {
            cmd.push("--q-values".into());
        }
        return Ok(Advisor {
            name: name.to_string(),
            kind: AdvisorKind::Mjai(cmd),
        });
    }
    if matches!(name, "greedy" | "random") {
        return Err(format!("{name} 不能當提示與復盤的策略"));
    }
    Ok(Advisor {
        name: name.to_string(),
        kind: AdvisorKind::Strategy(Arc::from(find_strategy_spec(name)?)),
    })
}

/// 由名稱清單建立一組顧問（1–3 個，不可重複）。
pub fn advisors(names: &[String]) -> Result<Advisors, String> {
    if names.is_empty() || names.len() > MAX_ADVISORS {
        return Err(format!("提示與復盤的策略要選 1–{MAX_ADVISORS} 個"));
    }
    for (i, n) in names.iter().enumerate() {
        if names[..i].contains(n) {
            return Err(format!("{n} 重複了"));
        }
    }
    let list: Vec<Advisor> = names
        .iter()
        .map(|n| parse_advisor(n))
        .collect::<Result<_, _>>()?;
    let base_index = list
        .iter()
        .position(|a| matches!(a.kind, AdvisorKind::Strategy(_)));
    let base = match base_index.map(|i| &list[i].kind) {
        Some(AdvisorKind::Strategy(s)) => s.clone(),
        _ => Arc::from(find_strategy_spec(DEFAULT_STRATEGY)?),
    };
    Ok(Advisors {
        list,
        base,
        base_index,
    })
}

impl Advisors {
    pub fn names(&self) -> Vec<String> {
        self.list.iter().map(|a| a.name.clone()).collect()
    }

    pub fn infos(&self) -> Vec<AdvisorInfo> {
        self.list.iter().map(info).collect()
    }

    /// 名稱為 `name` 的策略顧問（實驗用它重打）；不是策略或沒有時用基礎策略。
    pub fn strategy(&self, name: Option<&str>) -> Arc<dyn Strategy> {
        self.list
            .iter()
            .find_map(|a| match &a.kind {
                AdvisorKind::Strategy(s) if Some(a.name.as_str()) == name => Some(s.clone()),
                _ => None,
            })
            .unwrap_or_else(|| self.base.clone())
    }
}

/// 顧問的標題：策略的 README 標題，或「Mortal s42」。
pub fn info(a: &Advisor) -> AdvisorInfo {
    let (title, ai) = match &a.kind {
        // 參數覆寫（`名稱@…`）附在標題後面，才分得出同一個策略的不同版本。
        AdvisorKind::Strategy(s) => match a.name.split_once('@') {
            // 參數版本（`名稱@參數檔`）顯示檔名（版本名稱）。
            Some((_, p)) if p.ends_with(".params") => {
                let stem = std::path::Path::new(p)
                    .file_stem()
                    .map_or(p.into(), |s| s.to_string_lossy());
                (format!("{}（@{stem}）", s.title()), false)
            }
            Some((_, p)) => (format!("{}（@{p}）", s.title()), false),
            None => (s.title().to_string(), false),
        },
        AdvisorKind::Mjai(cmd) if is_mortal(cmd) => (
            format!("Mortal {}", cmd.get(1).map_or("", String::as_str)),
            true,
        ),
        AdvisorKind::Mjai(cmd) => (cmd.join(" "), true),
    };
    AdvisorInfo {
        name: a.name.clone(),
        title,
        ai,
    }
}
