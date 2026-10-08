//! 牌譜（paifu）匯入：把外部牌譜轉成本專案引擎的對局，之後的重播、預測評估、復盤都直接使用。
//!
//! 流程：
//! 1. 解析：`tenhou_xml`（天鳳 mjlog XML）或 `tenhou_json`（天鳳 JSON，tenhou.net/6 格式）
//!    → 共通的 [`GameRecord`]（每局的配牌、依時間排序的摸牌與動作、寶牌指示牌、原牌譜的點數）。
//! 2. 重播（[`replay`]）：由配牌與摸牌順序**還原每局的牌山**，再讓引擎實際打一次，
//!    四家都是「照牌譜出牌」的玩家（[`replay::ScriptedPlayer`]）。
//!    - 引擎會檢查每個動作是否合法，並自己計算役、點數、流局、連莊與終局；
//!    - 結束後比對每局開始的局數、本場、供託、點數與終局點數，必須與原牌譜完全一致。
//! 3. 結果是一般的 [`mj_engine::GameResult`]（完整事件紀錄＋各局牌山），
//!    可以在每個決策點取得當時的 [`mj_engine::PlayerView`]（[`replay::DecisionPoint`]），
//!    也可以讓策略「旁觀」（shadow）：策略收到同樣的事件與詢問，做出預測，但動作照牌譜。

pub mod replay;
pub mod tenhou_json;
pub mod tenhou_xml;

use mj_core::Pai;
use mj_engine::{Action, Seat};
use mj_rules::riichi::RiichiConfig;
use std::path::Path;

/// 一場對局的牌譜（與格式無關的中間表示）。
#[derive(Debug, Clone)]
pub struct GameRecord {
    /// 牌譜 ID 或檔名
    pub id: String,
    /// 四家名稱（依座位；座位 0 = 起家）
    pub players: [String; 4],
    /// 規則
    pub rules: RiichiConfig,
    /// 每一局
    pub hands: Vec<HandRecord>,
    /// 終局點數（已含剩餘供託的分配，不含順位馬）
    pub final_scores: Option<[i32; 4]>,
}

/// 一局的牌譜。
#[derive(Debug, Clone)]
pub struct HandRecord {
    /// 場風：0 = 東場、1 = 南場……
    pub round_wind: u8,
    /// 局（0..4），莊家座位 = kyoku
    pub kyoku: u8,
    /// 本場數
    pub honba: u8,
    /// 供託（立直棒）數
    pub riichi_sticks: u8,
    /// 開局時四家點數
    pub scores: [i32; 4],
    /// 四家配牌（依座位）
    pub haipai: [Vec<Pai>; 4],
    /// 寶牌指示牌，依翻開順序（第一張是開局的寶牌指示牌，之後是槓寶牌）
    pub dora_indicators: Vec<Pai>,
    /// 裏寶牌指示牌（只有立直和牌時才會公開；其餘為空）
    pub ura_indicators: Vec<Pai>,
    /// 依時間排序的摸牌與動作
    pub steps: Vec<Step>,
    /// 原牌譜記載的本局結束後點數（有記載時才有）
    pub end_scores: Option<[i32; 4]>,
}

/// 牌譜中的一步。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// 摸牌（開槓後的下一次摸牌是嶺上牌）
    Draw { seat: Seat, pai: Pai },
    /// 玩家的動作（打牌、立直、吃碰槓、和牌、九種九牌）。
    /// 「跳過」不會出現在牌譜中：沒有記載的回應一律視為跳過。
    Act { seat: Seat, action: Action },
}

/// 讀取一個牌譜檔，依內容自動判斷格式（`<` 開頭為 mjlog XML、`{` 開頭為 JSON）。
pub fn load(path: &Path) -> Result<GameRecord, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if bytes.starts_with(&[0x1f, 0x8b]) {
        return Err(format!(
            "{}: 這是 gzip 壓縮的檔案（天鳳用戶端儲存的 .mjlog），請先解壓縮（gunzip）",
            path.display()
        ));
    }
    let text =
        String::from_utf8(bytes).map_err(|_| format!("{}: 不是 UTF-8 文字檔", path.display()))?;
    let id = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    parse(&text, &id).map_err(|e| format!("{}: {e}", path.display()))
}

/// 解析牌譜文字，依內容自動判斷格式。
pub fn parse(text: &str, id: &str) -> Result<GameRecord, String> {
    let mut rec = match text.trim_start().chars().next() {
        Some('<') => tenhou_xml::parse(text, id),
        Some('{') => tenhou_json::parse(text, id),
        _ => Err("無法辨識的牌譜格式（只支援天鳳 mjlog XML 與 JSON）".into()),
    }?;
    // 吃碰的兩張牌依牌排序，不同格式解析出的動作才會完全相同。
    for h in &mut rec.hands {
        for s in &mut h.steps {
            if let Step::Act {
                action: Action::Chi { consumed } | Action::Pon { consumed },
                ..
            } = s
            {
                consumed.sort();
            }
        }
    }
    Ok(rec)
}

/// 展開路徑：檔案直接採用，目錄則取其中的 `.xml`／`.mjlog`／`.json`（不遞迴），依檔名排序。
pub fn find_files(paths: &[std::path::PathBuf]) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    for p in paths {
        if p.is_dir() {
            let mut v: Vec<_> = std::fs::read_dir(p)
                .map(|d| d.flatten().map(|e| e.path()).collect())
                .unwrap_or_default();
            v.retain(|f: &std::path::PathBuf| {
                f.extension()
                    .is_some_and(|e| e == "xml" || e == "mjlog" || e == "json")
            });
            v.sort();
            out.extend(v);
        } else {
            out.push(p.clone());
        }
    }
    out
}

/// 讀取多個牌譜檔。同一個牌譜 ID 出現多次（例如同時有 XML 與 JSON）時只保留第一個。
/// 回傳（成功的牌譜, 失敗訊息）。
pub fn load_all(paths: &[std::path::PathBuf]) -> (Vec<GameRecord>, Vec<String>) {
    let mut seen = std::collections::HashSet::new();
    let (mut recs, mut errs) = (Vec::new(), Vec::new());
    for f in find_files(paths) {
        match load(&f) {
            Ok(r) => {
                if seen.insert(r.id.clone()) {
                    recs.push(r);
                }
            }
            Err(e) => errs.push(e),
        }
    }
    (recs, errs)
}
