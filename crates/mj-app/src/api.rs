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
//! | `log` | — | 剛打完的對局的 mjai 牌譜（字串，`start_game` 附 `mj_meta`） |
//! | `load_log` | `{ text, name }` | [`LogInfo`](crate::LogInfo)：讀入牌譜（天鳳 XML／JSON、mjai），以引擎重播；取代之前讀入的 |
//! | `log_info` | — | 目前讀入的牌譜的 [`LogInfo`](crate::LogInfo) |
//! | `log_frames` | `{ seat, open }` | [`LogFrames`](crate::LogFrames)：該座位視角的全部幀；`open` = 公開四家手牌 |
//! | `start_log_review` | `{ seat, strategy?, ...ReviewConfig }` | `null`：復盤讀入的牌譜（策略預設為牌譜記載的提示策略） |
//! | `log_review` | `{ seat }` | 這個座位的 [`ReviewState`](crate::ReviewState) |
//! | `start_experiment` | [`ExperimentRequest`] | `null`：反事實實驗（對局中或讀入的牌譜；開始新的會取消舊的） |
//! | `experiment` | — | [`ExperimentState`](crate::ExperimentState)（進度或結果） |
//! | `cancel_experiment` | — | `null`：停止實驗，以已完成的世界計算結果 |

use crate::experiment::{ExperimentJob, ExperimentRequest};
use crate::paifu::LoadedLog;
use crate::session::{ReviewConfig, Session, SessionConfig};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// 長輪詢最多等多久。
const MAX_WAIT: Duration = Duration::from_secs(30);

/// App 的狀態：目前的對局（同時只有一場；開新局會丟棄舊的）與讀入的牌譜（同時只有一份）。
#[derive(Default)]
pub struct App {
    session: Mutex<Option<Arc<Session>>>,
    loaded: Mutex<Option<Arc<LoadedLog>>>,
    experiment: ExperimentJob,
}

#[derive(Deserialize)]
struct LoadArgs {
    text: String,
    #[serde(default)]
    name: String,
}

#[derive(Deserialize)]
struct FramesArgs {
    seat: u8,
    #[serde(default)]
    open: bool,
}

#[derive(Deserialize)]
struct SeatArgs {
    seat: u8,
}

#[derive(Deserialize)]
struct LogReviewArgs {
    seat: u8,
    #[serde(default)]
    strategy: Option<String>,
    #[serde(flatten)]
    config: ReviewConfig,
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

    fn loaded(&self) -> Result<Arc<LoadedLog>, String> {
        self.loaded
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .ok_or_else(|| "還沒有讀入牌譜".to_string())
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
                self.experiment.reset();
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
            "load_log" => {
                let a: LoadArgs = args(params)?;
                let id = a.name.rsplit(['/', '\\']).next().unwrap_or_default();
                let id = id.rsplit_once('.').map_or(id, |(stem, _)| stem);
                let log = LoadedLog::parse(&a.text, id)?;
                self.experiment.reset();
                let info = log.info();
                *self.loaded.lock().unwrap_or_else(|e| e.into_inner()) = Some(Arc::new(log));
                to_json(&info)
            }
            "log_info" => to_json(&self.loaded()?.info()),
            "log_frames" => {
                let a: FramesArgs = args(params)?;
                to_json(&self.loaded()?.frames(a.seat, a.open)?)
            }
            "start_log_review" => {
                let a: LogReviewArgs = args(params)?;
                let log = self.loaded()?;
                let strategy = log.strategy_name(a.strategy);
                log.start_review(a.seat, &strategy, &a.config)?;
                Ok(Value::Null)
            }
            "log_review" => {
                let a: SeatArgs = args(params)?;
                to_json(&self.loaded()?.review(a.seat))
            }
            "start_experiment" => {
                let req: ExperimentRequest = args(params)?;
                let ask_index = req.ask.saturating_sub(1) as u32;
                match req.source.as_str() {
                    "game" => {
                        let s = self.current()?;
                        self.experiment
                            .start(&req, s.me(), s.snapshot_job(ask_index))?;
                    }
                    "log" => {
                        let log = self.loaded()?;
                        let seat = req.seat.ok_or("要指定座位")?;
                        let name = log.strategy_name(req.strategy.clone());
                        let st: Arc<dyn mj_strategy::framework::Strategy> =
                            Arc::from(mj_strategy::find_strategy_spec(&name)?);
                        self.experiment
                            .start(&req, seat, log.snapshot_job(seat, ask_index, st))?;
                    }
                    s => return Err(format!("不認得的來源 {s}（game 或 log）")),
                }
                Ok(Value::Null)
            }
            "experiment" => to_json(&self.experiment.state()),
            "cancel_experiment" => {
                self.experiment.cancel();
                Ok(Value::Null)
            }
            _ => Err(format!("沒有這個 API：{name}")),
        }
    }
}
