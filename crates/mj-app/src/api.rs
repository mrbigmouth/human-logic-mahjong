//! UI 呼叫的唯一入口：`App::call(名稱, JSON 參數) → JSON`。
//! 開發用的 `mj serve` 把 `POST /api/<名稱>` 轉到這裡，桌面版的 Tauri command 也是；前端只有一種呼叫方式。
//!
//! | 名稱 | 參數 | 回傳 |
//! |---|---|---|
//! | `strategies` | — | 策略清單（`name`、`title`、`summary`、`kind`、`updated`；由新至舊） |
//! | `entrants` | — | 可以上桌的對手：策略、內建機器人、已安裝的 Mortal（同上；`kind` = strategy／bot／mjai） |
//! | `new_game` | [`SessionConfig`] | `{ seed, seat, names, resumed }` |
//! | `saved_game` | — | 上次沒打完的對局（`{ seed, seat, opponents, advisors, asks }`）或 `null`（M16） |
//! | `resume_game` | — | 同 `new_game`：照保存的動作重打到上次的詢問，接著繼續（`mj dev` 重啟後回到對局） |
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
//! | `start_test` | [`testbed::TestRequest`](crate::testbed::TestRequest) | `{ id }`：開始對戰測試（M15；同時只有一個） |
//! | `test_status` | — | [`testbed::TestState`](crate::testbed::TestState)（進度） |
//! | `cancel_test` | — | `null`：停止測試，以已完成的對局計算結果 |
//! | `tests` | — | 已完成的測試（[`testbed::TestSummary`](crate::testbed::TestSummary)，新的在前） |
//! | `test_result` | `{ id }` | 測試結果：`summary`、`request`、`names`（統計項目）、`report`、`predict`、`games` |
//! | `test_games` | [`testbed::GamesQuery`](crate::testbed::GamesQuery) | [`testbed::GamePage`](crate::testbed::GamePage)：對局清單的一頁（可排序） |
//! | `open_test_game` | `{ id, index }` | [`LogInfo`](crate::LogInfo)：把這一場讀入（重打或讀保存的牌譜），之後與 `load_log` 相同 |
//! | `delete_test` | `{ id }` | `null` |
//! | `params` | `{ strategy }` | `{ params, variants }`：策略的可調參數與已保存的參數版本（M16） |
//! | `save_params` | `{ strategy, variant, values }` | [`Variant`](crate::params::Variant)：保存參數版本（之後開的對局、測試立即使用） |
//! | `delete_params` | `{ strategy, variant }` | `null` |

use crate::experiment::{ExperimentJob, ExperimentRequest};
use crate::paifu::LoadedLog;
use crate::params::{ParamStore, params_of};
use crate::session::{ReviewConfig, Session, SessionConfig};
use crate::testbed::{GamesQuery, TestRequest, Testbed};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// 長輪詢最多等多久。
const MAX_WAIT: Duration = Duration::from_secs(30);

/// App 的狀態：目前的對局（同時只有一場；開新局會丟棄舊的）、讀入的牌譜（同時只有一份）、實驗與對戰測試。
pub struct App {
    session: Mutex<Option<Arc<Session>>>,
    loaded: Mutex<Option<Arc<LoadedLog>>>,
    experiment: ExperimentJob,
    testbed: Testbed,
    params: ParamStore,
    /// 對局的保存檔（`<資料目錄>/session.json`；每次詢問人類前寫入，結束時刪除）
    session_file: Option<std::path::PathBuf>,
}

#[derive(Deserialize)]
struct StrategyArgs {
    strategy: String,
}

#[derive(Deserialize)]
struct SaveParamsArgs {
    strategy: String,
    variant: String,
    #[serde(default)]
    values: std::collections::BTreeMap<String, f64>,
}

impl Default for App {
    fn default() -> Self {
        App::with_data_dir(None)
    }
}

#[derive(Deserialize)]
struct IdArgs {
    id: String,
}

#[derive(Deserialize)]
struct TestGameArgs {
    id: String,
    index: usize,
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
    /// 顧問（空 = 牌譜記載的）
    #[serde(default)]
    advisors: Option<Vec<String>>,
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

/// 檔案的修改時間（Unix 秒）。
fn file_mtime(path: &str) -> Option<u64> {
    let t = std::fs::metadata(path).ok()?.modified().ok()?;
    Some(t.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs())
}

/// 策略清單與參數版本，附最後更新時間（`updated`，Unix 秒；原始碼還在時即時計算，見 `mj_strategy::framework::source`；
/// 參數版本取原始碼與參數檔中較晚者）。
fn strategies(store: &ParamStore) -> Vec<Value> {
    let all = mj_strategy::all_strategies();
    let mut out: Vec<Value> = all
        .iter()
        .map(|s| {
            json!({
                "name": s.name(), "title": s.title(), "summary": s.summary(),
                "kind": "strategy", "updated": s.updated(),
            })
        })
        .collect();
    for v in store.all() {
        let Some(s) = all.iter().find(|s| s.name() == v.strategy) else {
            continue;
        };
        let values: Vec<String> = v.values.iter().map(|(k, x)| format!("{k}={x}")).collect();
        out.push(json!({
            "name": v.spec,
            "title": format!("{}（@{}）", s.title(), v.name),
            "summary": format!("{} 的參數版本：{}", s.name(), if values.is_empty() { "全部用預設值".into() } else { values.join("、") }),
            "kind": "strategy",
            "base": s.name(),
            "variant": v.name,
            "updated": s.updated().max(v.updated),
        }));
    }
    out
}

/// 由新至舊排序（沒有時間的放最後，順序不變）。
fn newest_first(mut v: Vec<Value>) -> Vec<Value> {
    v.sort_by_key(|x| std::cmp::Reverse(x["updated"].as_u64()));
    v
}

/// 可以上桌的對手。Mortal 只在從專案目錄執行、而且已用 `tools/mortal/setup.sh` 安裝時列出
/// （更新時間是權重檔的時間）。
fn entrants(store: &ParamStore) -> Vec<Value> {
    let mut out = strategies(store);
    out.push(json!({
        "name": "greedy", "title": "貪婪機器人", "summary": "只看牌效，隨機鳴牌（很弱）",
        "kind": "bot", "updated": null,
    }));
    out.push(json!({
        "name": "random", "title": "隨機機器人", "summary": "隨便打（極弱）",
        "kind": "bot", "updated": null,
    }));
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
                "kind": "mjai",
                "updated": file_mtime(path),
            }));
        }
    }
    newest_first(out)
}

impl App {
    /// 不保存資料（對戰測試的結果只放在記憶體中）。
    pub fn new() -> Self {
        App::default()
    }

    /// `dir` = 資料目錄：對戰測試的結果存在 `<dir>/tests/`。
    pub fn with_data_dir(dir: Option<std::path::PathBuf>) -> Self {
        App {
            session: Mutex::default(),
            loaded: Mutex::default(),
            experiment: ExperimentJob::default(),
            testbed: Testbed::new(dir.as_ref().map(|d| d.join("tests"))),
            params: ParamStore::new(dir.as_ref().map(|d| d.join("params"))),
            session_file: dir.map(|d| d.join("session.json")),
        }
    }

    /// 換成新的對局。
    fn start_session(&self, s: Session, resumed: bool) -> Result<Value, String> {
        self.experiment.reset();
        let out = json!({ "seed": s.seed, "seat": s.me(), "names": s.names, "resumed": resumed });
        // 舊的對局被丟棄：若還在等人類的動作，會自動打完並結束。
        let old = self
            .session
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .replace(Arc::new(s));
        drop(old);
        Ok(out)
    }

    /// 保存的（還沒打完的）對局。
    fn saved_game(&self) -> Option<crate::session::SavedGame> {
        let text = std::fs::read_to_string(self.session_file.as_ref()?).ok()?;
        serde_json::from_str(&text).ok()
    }

    /// 讀入一份牌譜（取代之前的），清掉舊的實驗結果。
    fn set_loaded(&self, log: LoadedLog) -> Result<Value, String> {
        self.experiment.reset();
        let info = log.info();
        *self.loaded.lock().unwrap_or_else(|e| e.into_inner()) = Some(Arc::new(log));
        to_json(&info)
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
            "strategies" => Ok(Value::Array(newest_first(strategies(&self.params)))),
            "entrants" => Ok(Value::Array(entrants(&self.params))),
            "params" => {
                let a: StrategyArgs = args(params)?;
                Ok(json!({
                    "params": params_of(&a.strategy)?,
                    "variants": self.params.variants(&a.strategy),
                }))
            }
            "save_params" => {
                let a: SaveParamsArgs = args(params)?;
                to_json(&self.params.save(&a.strategy, &a.variant, &a.values)?)
            }
            "delete_params" => {
                let a: SaveParamsArgs = args(params)?;
                self.params.delete(&a.strategy, &a.variant)?;
                Ok(Value::Null)
            }
            "new_game" => {
                let cfg: SessionConfig = args(params)?;
                self.start_session(
                    Session::start_with(cfg, None, self.session_file.clone())?,
                    false,
                )
            }
            "saved_game" => Ok(match self.saved_game() {
                Some(g) => json!({
                    "seed": g.config.seed, "seat": g.config.seat, "opponents": g.config.opponents,
                    "advisors": g.config.advisor_names(),
                    "asks": g.actions[usize::from(g.config.seat)].len(),
                }),
                None => Value::Null,
            }),
            "resume_game" => {
                let g = self.saved_game().ok_or("沒有保存的對局")?;
                let s = Session::start_with(g.config, Some(g.actions), self.session_file.clone())?;
                self.start_session(s, true)
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
                self.set_loaded(LoadedLog::parse(&a.text, id)?)
            }
            "log_info" => to_json(&self.loaded()?.info()),
            "log_frames" => {
                let a: FramesArgs = args(params)?;
                to_json(&self.loaded()?.frames(a.seat, a.open)?)
            }
            "start_log_review" => {
                let a: LogReviewArgs = args(params)?;
                let log = self.loaded()?;
                let advisors = log.advisors(a.advisors)?;
                log.start_review(a.seat, &advisors, &a.config)?;
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
                        let st = s.advisors().strategy(req.strategy.as_deref());
                        self.experiment
                            .start(&req, s.me(), s.snapshot_job(ask_index, st))?;
                    }
                    "log" => {
                        let log = self.loaded()?;
                        let seat = req.seat.ok_or("要指定座位")?;
                        let st = log.advisors(None)?.strategy(req.strategy.as_deref());
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
            "start_test" => {
                let req: TestRequest = args(params)?;
                Ok(json!({ "id": self.testbed.start(req)? }))
            }
            "test_status" => to_json(&self.testbed.state()),
            "cancel_test" => {
                self.testbed.cancel();
                Ok(Value::Null)
            }
            "tests" => to_json(&self.testbed.list()),
            "test_result" => {
                let a: IdArgs = args(params)?;
                let f = self.testbed.get(&a.id)?;
                Ok(json!({
                    "summary": f.summary,
                    "request": f.request,
                    "names": f.plan.as_ref().map(|p| p.names.clone()).unwrap_or_default(),
                    "report": f.report,
                    "predict": f.predict,
                    "games": f.outcomes.len(),
                }))
            }
            "test_games" => {
                let q: GamesQuery = args(params)?;
                to_json(&self.testbed.games(&q)?)
            }
            "open_test_game" => {
                let a: TestGameArgs = args(params)?;
                let text = self.testbed.game_log(&a.id, a.index)?;
                self.set_loaded(LoadedLog::parse(&text, &format!("{}-{}", a.id, a.index))?)
            }
            "delete_test" => {
                let a: IdArgs = args(params)?;
                self.testbed.delete(&a.id)?;
                Ok(Value::Null)
            }
            _ => Err(format!("沒有這個 API：{name}")),
        }
    }
}
