//! UI 呼叫的唯一入口：`App::call(名稱, JSON 參數) → JSON`。
//! 開發用的 `mj serve` 把 `POST /api/<名稱>` 轉到這裡，桌面版的 Tauri command 也是；前端只有一種呼叫方式。
//!
//! | 名稱 | 參數 | 回傳 |
//! |---|---|---|
//! | `strategies` | — | 策略清單（`name`、`title`、`summary`） |
//! | `entrants` | — | 可以上桌的對手：策略、內建機器人、已安裝的 Mortal（`name`、`title`、`summary`） |
//! | `new_game` | [`SessionConfig`] | `{ seed, seat, names }` |
//! | `poll` | `{ since, wait_ms }` | [`Update`](crate::Update)（長輪詢：沒有新東西時最多等 `wait_ms`） |
//! | `act` | `{ ask, index }` | `null` |
//! | `start_review` | [`ReviewConfig`] | `null` |
//! | `review` | — | [`ReviewState`](crate::ReviewState) |
//! | `log` | — | mjai 格式的牌譜（字串） |

use crate::session::{ReviewConfig, Session, SessionConfig};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// 長輪詢最多等多久。
const MAX_WAIT: Duration = Duration::from_secs(30);

/// App 的狀態：目前的對局（同時只有一場；開新局會丟棄舊的）。
#[derive(Default)]
pub struct App {
    session: Mutex<Option<Arc<Session>>>,
}

#[derive(Deserialize)]
struct PollArgs {
    #[serde(default)]
    since: usize,
    #[serde(default)]
    wait_ms: u64,
}

#[derive(Deserialize)]
struct ActArgs {
    ask: u64,
    index: usize,
}

fn args<T: for<'de> Deserialize<'de>>(v: Value) -> Result<T, String> {
    // 沒有參數時當作空物件（各欄位用預設值）。
    let v = if v.is_null() { json!({}) } else { v };
    serde_json::from_value(v).map_err(|e| format!("參數錯誤：{e}"))
}

fn to_json<T: serde::Serialize>(v: &T) -> Result<Value, String> {
    serde_json::to_value(v).map_err(|e| e.to_string())
}

/// 可以上桌的對手。Mortal 只在從專案目錄執行、而且已用 `tools/mortal/setup.sh` 安裝時列出。
fn entrants() -> Vec<Value> {
    let mut out: Vec<Value> = mj_strategy::all_strategies()
        .iter()
        .map(|s| json!({ "name": s.name(), "title": s.title(), "summary": s.summary() }))
        .collect();
    out.push(
        json!({ "name": "greedy", "title": "貪婪機器人", "summary": "只看牌效，隨機鳴牌（很弱）" }),
    );
    out.push(json!({ "name": "random", "title": "隨機機器人", "summary": "隨便打（極弱）" }));
    let installed = std::path::Path::new("tools/mortal/.venv").is_dir();
    for (model, path, summary) in [
        (
            "s42",
            "data/mortal/s42/weights/s42-inference.pth",
            "深度強化學習 AI（社群權重 S42），遠強於本專案的策略；每個約占 400 MB 記憶體",
        ),
        (
            "hhim8826",
            "data/mortal/hhim8826/mortal.pth",
            "深度強化學習 AI（社群權重 hhim8826）；每個約占 400 MB 記憶體",
        ),
    ] {
        if installed && std::path::Path::new(path).is_file() {
            out.push(json!({
                "name": format!("mjai:tools/mortal/run.sh {model}"),
                "title": format!("Mortal {model}"),
                "summary": summary,
            }));
        }
    }
    out
}

impl App {
    pub fn new() -> Self {
        App::default()
    }

    fn current(&self) -> Result<Arc<Session>, String> {
        self.session
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .ok_or_else(|| "還沒有開始對局".to_string())
    }

    /// 執行一個 API 呼叫。
    pub fn call(&self, name: &str, params: Value) -> Result<Value, String> {
        match name {
            "strategies" => Ok(Value::Array(
                mj_strategy::all_strategies()
                    .iter()
                    .map(
                        |s| json!({ "name": s.name(), "title": s.title(), "summary": s.summary() }),
                    )
                    .collect(),
            )),
            "entrants" => Ok(Value::Array(entrants())),
            "new_game" => {
                let cfg: SessionConfig = args(params)?;
                let s = Session::start(cfg)?;
                let out = json!({ "seed": s.seed, "seat": s.me(), "names": s.names });
                // 舊的對局被丟棄：若還在等人類的動作，會自動打完並結束。
                *self.session.lock().unwrap_or_else(|e| e.into_inner()) = Some(Arc::new(s));
                Ok(out)
            }
            "poll" => {
                let a: PollArgs = args(params)?;
                let s = self.current()?;
                to_json(&s.poll(a.since, Duration::from_millis(a.wait_ms).min(MAX_WAIT)))
            }
            "act" => {
                let a: ActArgs = args(params)?;
                self.current()?.act(a.ask, a.index)?;
                Ok(Value::Null)
            }
            "start_review" => {
                let rc: ReviewConfig = args(params)?;
                self.current()?.start_review(&rc)?;
                Ok(Value::Null)
            }
            "review" => to_json(&self.current()?.review()),
            "log" => self
                .current()?
                .mjai_log()
                .map(Value::String)
                .ok_or_else(|| "對局還沒結束".to_string()),
            _ => Err(format!("沒有這個 API：{name}")),
        }
    }
}
