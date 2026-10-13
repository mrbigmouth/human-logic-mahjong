//! 讀入的牌譜（M12）：解析（天鳳 XML／JSON、mjai）→ 引擎重播 → 任一座位視角的幀 → 復盤。
//! 剛打完的對局也走同一條路（前端把 session 的牌譜交給 `load_log`），檢視與復盤只有一種做法。

use crate::review::{ReviewConfig, ReviewJob, ReviewState};
use crate::table::{Frame, log_frames};
use mj_engine::{Event, Seat};
use mj_paifu::GameRecord;
use mj_strategy::find_strategy_spec;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// 本專案匯出的 mjai 牌譜在 `start_game` 事件中附加的資訊（欄位名 `mj_meta`；其他程式會忽略）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LogMeta {
    /// 對局的 seed
    #[serde(default)]
    pub seed: Option<u64>,
    /// 人類的座位
    #[serde(default)]
    pub seat: Option<Seat>,
    /// 提示用的策略
    #[serde(default)]
    pub strategy: Option<String>,
}

/// 牌譜的概要（給 UI 選玩家用）。
#[derive(Debug, Clone, Serialize)]
pub struct LogInfo {
    pub id: String,
    /// 四家名稱（依座位）
    pub names: [String; 4],
    /// 局數
    pub hands: usize,
    /// 終局點數
    pub final_scores: [i32; 4],
    /// 本專案匯出的牌譜才有
    pub meta: Option<LogMeta>,
    /// 事件（幀）數
    pub events: usize,
}

/// 某個座位視角的全部幀，以及這個座位每次被詢問時看到的是第幾幀。
#[derive(Debug, Clone, Serialize)]
pub struct LogFrames {
    pub frames: Vec<Frame>,
    /// （詢問編號, 幀數）；詢問編號 = 第幾次詢問 + 1（與對局中的 `Ask::id` 相同），
    /// 幀數 = 詢問當時已收到的事件數（與 `Ask::frame` 相同）
    pub asks: Vec<(u64, usize)>,
}

/// 一份讀入並重播過的牌譜。
pub struct LoadedLog {
    pub record: GameRecord,
    /// 引擎重播的完整事件紀錄
    pub log: Vec<Event>,
    /// 各座位每次被詢問時已收到的事件數（索引 = 第幾次詢問）
    asks: [Vec<usize>; 4],
    pub meta: Option<LogMeta>,
    /// 各座位的復盤（可以分別復盤四家）
    reviews: [ReviewJob; 4],
}

impl LoadedLog {
    /// 解析並以引擎重播（重播的點數必須與牌譜一致）。
    pub fn parse(text: &str, id: &str) -> Result<LoadedLog, String> {
        let record = mj_paifu::parse(text, id)?;
        let replayed = mj_paifu::replay::replay(&record, [None, None, None, None], true)?;
        let mut asks: [Vec<usize>; 4] = Default::default();
        for p in &replayed.points {
            let a = &mut asks[usize::from(p.seat)];
            // 詢問依序記錄；以 ask_index 放到對應的位置。
            let i = p.ask_index as usize;
            if a.len() <= i {
                a.resize(i + 1, p.event_index);
            }
            a[i] = p.event_index;
        }
        Ok(LoadedLog {
            meta: read_meta(text),
            record,
            log: replayed.result.log,
            asks,
            reviews: Default::default(),
        })
    }

    pub fn info(&self) -> LogInfo {
        let final_scores = self
            .log
            .iter()
            .rev()
            .find_map(|e| match e {
                Event::EndGame { scores } => Some(*scores),
                _ => None,
            })
            .unwrap_or_default();
        LogInfo {
            id: self.record.id.clone(),
            names: self.record.players.clone(),
            hands: self.record.hands.len(),
            final_scores,
            meta: self.meta.clone(),
            events: self.log.len(),
        }
    }

    /// `seat` 視角的幀；`open` = 公開四家的手牌。
    pub fn frames(&self, seat: Seat, open: bool) -> Result<LogFrames, String> {
        if seat > 3 {
            return Err("座位必須是 0–3".into());
        }
        Ok(LogFrames {
            frames: log_frames(&self.log, seat, open),
            asks: self.asks[usize::from(seat)]
                .iter()
                .enumerate()
                .map(|(i, &e)| (i as u64 + 1, e))
                .collect(),
        })
    }

    /// 在背景復盤 `seat`，用策略 `strategy`（名稱，可加 `@參數`）。
    pub fn start_review(
        &self,
        seat: Seat,
        strategy: &str,
        rc: &ReviewConfig,
    ) -> Result<(), String> {
        if seat > 3 {
            return Err("座位必須是 0–3".into());
        }
        let st: Arc<dyn mj_strategy::framework::Strategy> =
            Arc::from(find_strategy_spec(strategy)?);
        self.reviews[usize::from(seat)].start(st, self.record.clone(), seat, rc)
    }

    /// `seat` 的復盤狀態。
    pub fn review(&self, seat: Seat) -> ReviewState {
        self.reviews
            .get(usize::from(seat))
            .map_or(ReviewState::Idle, ReviewJob::state)
    }
}

/// 讀出 mjai 牌譜第一行 `start_game` 的 `mj_meta`。
fn read_meta(text: &str) -> Option<LogMeta> {
    let first = text.lines().find(|l| !l.trim().is_empty())?;
    let v: serde_json::Value = serde_json::from_str(first).ok()?;
    if v.get("type")?.as_str()? != "start_game" {
        return None;
    }
    serde_json::from_value(v.get("mj_meta")?.clone()).ok()
}

/// 在 mjai 牌譜的 `start_game` 事件加上 `mj_meta`。
pub fn with_meta(lines: &mut [serde_json::Value], meta: &LogMeta) {
    if let Some(first) = lines.first_mut()
        && first.get("type").and_then(|t| t.as_str()) == Some("start_game")
        && let Some(obj) = first.as_object_mut()
        && let Ok(m) = serde_json::to_value(meta)
    {
        obj.insert("mj_meta".into(), m);
    }
}
