//! 對戰測試平台（M15）：在 App 中執行自我對戰、duplicate、場地對戰、預測評估，查看結果與每一場的細節。
//!
//! - 計算與 CLI 共用：`mj_sim::arena`（對戰的平行執行、進度、取消、每場結果）、`mj_sim::duplicate`（計畫與彙整）、
//!   `mj_sim::predict_eval`；參賽者一律經由參賽者登錄 `mj_sim::entrant::EntrantSpec`。
//! - 同時只有一個測試在跑（都是吃滿 CPU 的工作）；完成或取消後存成 JSON 檔（有資料目錄時），重開 App 後還能查看。
//! - 每場只存摘要。要看某一場時用同樣的 seed 與座位配置重打（參賽者都是決定性的），並確認終局點數相同；
//!   有外部 AI（Mortal）的對局不能重現，另外保存牌譜檔。

use mj_sim::arena::{self, Control, GameOutcome, Plan};
use mj_sim::duplicate::{self, Report};
use mj_sim::entrant::{EntrantSpec, mjai_jobs};
use mj_sim::predict_eval::{Judged, Metrics, collect_mixed_with, metrics};
use mj_strategy::find_strategy_spec;
use mj_strategy::framework::Strategy;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// 測試的種類。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TestMode {
    /// 自我對戰：四個座位各一個參賽者（可重複），每場輪換座位
    Selfplay,
    /// duplicate：2 個（各坐兩個位置）或 4 個參賽者，每組同一牌山輪換座位打 4 場
    Duplicate,
    /// 場地對戰：A、B 各自和三家 field 同桌，每組 8 場
    Field,
    /// 預測評估：四個策略輪換座位，評估讀牌預測對每個被預測者準不準
    Predict,
}

/// 測試的請求（API `start_test`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestRequest {
    pub mode: TestMode,
    /// 參賽者名稱（見 `mj_sim::entrant`）
    pub entrants: Vec<String>,
    /// 場地對戰的對手
    #[serde(default)]
    pub field: Option<String>,
    /// 場數（自我對戰、預測評估）或組數（duplicate、場地對戰）
    pub count: u64,
    /// 牌山 seed；`None` = 依時間決定
    #[serde(default)]
    pub seed: Option<u64>,
}

/// 測試的概要（清單用）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestSummary {
    pub id: String,
    /// 開始時間（Unix 秒）
    pub created: u64,
    pub mode: TestMode,
    pub entrants: Vec<String>,
    pub field: Option<String>,
    pub count: u64,
    pub seed: u64,
    /// 完成的場數與總場數
    pub games_done: usize,
    pub games_total: usize,
    pub seconds: f64,
    /// 中途停止
    pub cancelled: bool,
}

/// 預測評估的一列：某個預測函式、類別，對某個被預測者的準確度。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictRow {
    pub source: String,
    pub category: String,
    /// 被預測者（參賽者名稱；「全部」= 不分）
    pub target: String,
    /// 只算沒立直的被預測者（立直者一定聽牌，印象操作只在沒立直時有作用）
    pub unriichi: bool,
    pub metrics: Metrics,
    /// 實際發生時、沒發生時的平均預測（被騙時前者偏低、後者偏高）
    pub when_true: f64,
    pub when_false: f64,
}

/// 一個測試的完整結果（存檔的內容）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestFile {
    pub summary: TestSummary,
    pub request: TestRequest,
    /// 對戰的計畫（預測評估沒有）
    pub plan: Option<Plan>,
    /// 參賽者（`plan.entrant_of` 的編號 → 名稱）
    pub specs: Vec<String>,
    pub report: Option<Report>,
    #[serde(default)]
    pub predict: Vec<PredictRow>,
    /// 每場的結果（依計畫順序）
    #[serde(default)]
    pub outcomes: Vec<GameOutcome>,
    /// 保存了牌譜的場次（有外部 AI 的對局）
    #[serde(default)]
    pub logs: Vec<usize>,
}

/// 測試的進行狀態（API `test_status`）。
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum TestState {
    Idle,
    Running {
        id: String,
        done: usize,
        total: usize,
        seconds: f64,
    },
    Done {
        id: String,
    },
    Failed {
        error: String,
    },
}

/// 一場的一個座位（對局清單用）。
#[derive(Debug, Clone, Serialize)]
pub struct SeatRow {
    /// 統計項目
    pub item: usize,
    pub name: String,
    pub score: i32,
    /// 名次（0 = 第一名）
    pub rank: u8,
    pub points: f64,
    pub wins: u16,
    pub deal_ins: u16,
}

/// 對局清單的一列。
#[derive(Debug, Clone, Serialize)]
pub struct GameRow {
    pub index: usize,
    pub block: u64,
    pub seed: u64,
    pub hands: u32,
    pub seats: Vec<SeatRow>,
    /// 有保存牌譜（外部 AI 的對局）
    pub stored: bool,
}

/// 對局清單的一頁（API `test_games`）。
#[derive(Debug, Clone, Serialize)]
pub struct GamePage {
    pub total: usize,
    pub rows: Vec<GameRow>,
}

/// 對局清單的排序方式。
#[derive(Debug, Clone, Deserialize)]
pub struct GamesQuery {
    pub id: String,
    /// `index`（預設）、`hands`、`item:<統計項目>`（該項目這場的平均名次點）
    #[serde(default)]
    pub sort: Option<String>,
    #[serde(default)]
    pub desc: bool,
    #[serde(default)]
    pub offset: usize,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    100
}

/// 場數上限（避免一次開太大的工作）。
pub const MAX_GAMES: usize = 200_000;

#[derive(Default)]
struct JobInner {
    state: Option<TestState>,
    started: Option<Instant>,
    total: usize,
    progress: Arc<AtomicUsize>,
    cancel: Arc<AtomicBool>,
}

/// 對戰測試：目前的工作與已完成的結果。
pub struct Testbed {
    /// 存檔目錄（`None` = 只放在記憶體中）
    dir: Option<PathBuf>,
    job: Arc<Mutex<JobInner>>,
    /// 已完成的結果（記憶體中的快取；有存檔目錄時也會寫檔）
    done: Arc<Mutex<HashMap<String, Arc<TestFile>>>>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// 同名的參賽者（例如 A/A 測試）加上編號以便區分（與 `mj simulate` 相同）。
fn distinct(names: &[String]) -> Vec<String> {
    names
        .iter()
        .enumerate()
        .map(|(i, n)| {
            if names.iter().filter(|o| *o == n).count() > 1 {
                format!("{n}#{}", i + 1)
            } else {
                n.clone()
            }
        })
        .collect()
}

/// 由請求建立計畫與參賽者清單。
fn plan_of(req: &TestRequest, seed: u64) -> Result<(Plan, Vec<String>), String> {
    let n = req.count;
    Ok(match req.mode {
        TestMode::Selfplay => {
            let labels: [String; 4] = req
                .entrants
                .clone()
                .try_into()
                .map_err(|_| "自我對戰要四個參賽者（依座位）")?;
            (
                arena::selfplay_plan(&labels, n, seed, true),
                req.entrants.clone(),
            )
        }
        TestMode::Duplicate => (
            duplicate::plan(&distinct(&req.entrants), n, seed)?,
            req.entrants.clone(),
        ),
        TestMode::Field => {
            let field = req.field.clone().ok_or("場地對戰要指定場地的對手")?;
            let [a, b]: [String; 2] = req
                .entrants
                .clone()
                .try_into()
                .map_err(|_| "場地對戰要兩個參賽者")?;
            let names = distinct(&[a.clone(), b.clone()]);
            (
                duplicate::field_plan(
                    &[names[0].clone(), names[1].clone(), field.clone()],
                    n,
                    seed,
                ),
                vec![a, b, field],
            )
        }
        TestMode::Predict => return Err("預測評估沒有對戰計畫".into()),
    })
}

/// 預測評估的結果表。
fn predict_rows(judged: &[(Option<usize>, Judged)], names: &[String; 4]) -> Vec<PredictRow> {
    let mut targets: Vec<String> = Vec::new();
    for n in names {
        if !targets.contains(n) {
            targets.push(n.clone());
        }
    }
    let mut groups: Vec<(String, &'static str)> = judged
        .iter()
        .filter(|(t, _)| t.is_some())
        .map(|(_, j)| (j.source.clone(), j.category))
        .collect();
    groups.sort();
    groups.dedup();
    let mut rows = Vec::new();
    for (source, category) in &groups {
        let all = std::iter::once(None).chain(targets.iter().map(Some));
        for target in all {
            for unriichi in [false, true] {
                let items: Vec<&Judged> = judged
                    .iter()
                    .filter(|(e, j)| {
                        e.is_some_and(|e| target.is_none_or(|t| names[e] == *t))
                            && j.source == *source
                            && j.category == *category
                            && !(unriichi && j.target.riichi)
                    })
                    .map(|(_, j)| j)
                    .collect();
                if items.is_empty() {
                    continue;
                }
                let cond = |truth: bool| {
                    let v: Vec<f64> = items
                        .iter()
                        .filter(|j| j.truth == truth)
                        .map(|j| j.p)
                        .collect();
                    v.iter().sum::<f64>() / v.len().max(1) as f64
                };
                rows.push(PredictRow {
                    source: source.clone(),
                    category: category.to_string(),
                    target: target.cloned().unwrap_or_else(|| "全部".into()),
                    unriichi,
                    metrics: metrics(&items),
                    when_true: cond(true),
                    when_false: cond(false),
                });
            }
        }
    }
    rows
}

impl Testbed {
    pub fn new(dir: Option<PathBuf>) -> Testbed {
        Testbed {
            dir,
            job: Arc::default(),
            done: Arc::default(),
        }
    }

    fn file_path(&self, id: &str) -> Option<PathBuf> {
        self.dir.as_ref().map(|d| d.join(format!("{id}.json")))
    }

    /// 保存牌譜的位置：`<資料目錄>/<id>/<場次>.mjson`。
    fn log_path(&self, id: &str, index: usize) -> Option<PathBuf> {
        self.dir
            .as_ref()
            .map(|d| d.join(id).join(format!("{index}.mjson")))
    }

    /// 開始一個測試（背景執行）；已經有測試在跑時回傳錯誤。回傳測試的 ID。
    pub fn start(&self, req: TestRequest) -> Result<String, String> {
        if req.count == 0 {
            return Err("場數（組數）至少 1".into());
        }
        let seed = req.seed.unwrap_or_else(|| now_secs() & ((1 << 53) - 1));
        // 先解析參賽者與計畫：錯誤立即回報。
        let (plan, specs) = match req.mode {
            TestMode::Predict => {
                if req.entrants.len() != 4 {
                    return Err("預測評估要四個策略（依座位）".into());
                }
                (None, req.entrants.clone())
            }
            _ => {
                let (p, s) = plan_of(&req, seed)?;
                (Some(p), s)
            }
        };
        let entrants: Vec<EntrantSpec> = specs
            .iter()
            .map(|s| EntrantSpec::parse(s))
            .collect::<Result<_, _>>()?;
        let strategies: Option<[Arc<dyn Strategy>; 4]> = match req.mode {
            TestMode::Predict => {
                let v: Vec<Arc<dyn Strategy>> = specs
                    .iter()
                    .map(|s| find_strategy_spec(s).map(Arc::from))
                    .collect::<Result<_, _>>()
                    .map_err(|e| format!("預測評估只能用策略：{e}"))?;
                Some(v.try_into().map_err(|_| "四個策略")?)
            }
            _ => None,
        };
        let total = plan.as_ref().map_or(req.count as usize, |p| p.games.len());
        if total > MAX_GAMES {
            return Err(format!("一次最多 {MAX_GAMES} 場（目前 {total} 場）"));
        }
        let created = now_secs();
        let id = format!("{}-{:?}-{}", created, req.mode, seed % 10_000).to_lowercase();
        let progress = Arc::new(AtomicUsize::new(0));
        let cancel = Arc::new(AtomicBool::new(false));
        {
            let mut g = lock(&self.job);
            if matches!(g.state, Some(TestState::Running { .. })) {
                return Err("已經有測試在進行中".into());
            }
            *g = JobInner {
                state: Some(TestState::Running {
                    id: id.clone(),
                    done: 0,
                    total,
                    seconds: 0.0,
                }),
                started: Some(Instant::now()),
                total,
                progress: progress.clone(),
                cancel: cancel.clone(),
            };
        }
        let job = self.job.clone();
        let done = self.done.clone();
        let file_path = self.file_path(&id);
        let log_dir = self.dir.as_ref().map(|d| d.join(&id));
        let id2 = id.clone();
        // 有外部 AI 時限制同時進行的對局數（每個 Mortal 行程約 380 MB）。
        let mjai = entrants.iter().any(EntrantSpec::is_mjai);
        std::thread::Builder::new()
            .name("mj-test".into())
            .spawn(move || {
                let start = Instant::now();
                let threads = if mjai {
                    mjai_jobs()
                } else {
                    std::thread::available_parallelism().map_or(1, |n| n.get())
                };
                let run = || -> Result<TestFile, String> {
                    let pool = rayon::ThreadPoolBuilder::new()
                        .num_threads(threads)
                        .build()
                        .map_err(|e| e.to_string())?;
                    let mut file = TestFile {
                        summary: TestSummary {
                            id: id2.clone(),
                            created,
                            mode: req.mode,
                            entrants: req.entrants.clone(),
                            field: req.field.clone(),
                            count: req.count,
                            seed,
                            games_done: 0,
                            games_total: total,
                            seconds: 0.0,
                            cancelled: false,
                        },
                        request: req.clone(),
                        plan: plan.clone(),
                        specs: specs.clone(),
                        report: None,
                        predict: Vec::new(),
                        outcomes: Vec::new(),
                        logs: Vec::new(),
                    };
                    if let Some(st) = &strategies {
                        let judged = pool.install(|| {
                            collect_mixed_with(st, req.count, seed, Some(&progress), Some(&cancel))
                        })?;
                        let names: [String; 4] = std::array::from_fn(|s| specs[s].clone());
                        file.predict = predict_rows(&judged, &names);
                    } else if let Some(plan) = &plan {
                        let make = |who: usize, seed: u64, seat: u8| {
                            entrants[who].player(seed, u64::from(seat))
                        };
                        let keep = |g: &arena::GamePlan| {
                            g.slots
                                .iter()
                                .any(|&i| entrants[plan.entrant_of[i]].is_mjai())
                        };
                        let played = pool.install(|| {
                            arena::play(
                                plan,
                                &make,
                                &Control {
                                    progress: Some(&progress),
                                    cancel: Some(&cancel),
                                    keep_log: Some(&keep),
                                },
                            )
                        })?;
                        file.report =
                            Some(arena::report(plan, &played.outcomes, &played.slot_hands));
                        // 外部 AI 的對局：存成 mjai 牌譜（沒有存檔目錄時不保存，之後無法檢視）。
                        if let Some(dir) = &log_dir
                            && !played.logs.is_empty()
                        {
                            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
                            for (index, log) in &played.logs {
                                let names = seat_names(plan, &specs, &plan.games[*index].slots);
                                let text = mjai_text(log, &names);
                                std::fs::write(dir.join(format!("{index}.mjson")), text)
                                    .map_err(|e| e.to_string())?;
                                file.logs.push(*index);
                            }
                        }
                        file.outcomes = played.outcomes;
                    }
                    file.summary.games_done = progress.load(Ordering::Relaxed);
                    file.summary.cancelled = cancel.load(Ordering::Relaxed);
                    file.summary.seconds = start.elapsed().as_secs_f64();
                    if let Some(path) = &file_path {
                        if let Some(parent) = path.parent() {
                            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                        }
                        let json = serde_json::to_string(&file).map_err(|e| e.to_string())?;
                        std::fs::write(path, json).map_err(|e| format!("無法寫入結果：{e}"))?;
                    }
                    Ok(file)
                };
                let state = match run() {
                    Ok(file) => {
                        lock(&done).insert(id2.clone(), Arc::new(file));
                        TestState::Done { id: id2 }
                    }
                    Err(error) => TestState::Failed { error },
                };
                lock(&job).state = Some(state);
            })
            .map_err(|e| format!("無法建立測試執行緒：{e}"))?;
        Ok(id)
    }

    /// 停止進行中的測試（以已完成的對局計算結果）。
    pub fn cancel(&self) {
        lock(&self.job).cancel.store(true, Ordering::Relaxed);
    }

    pub fn state(&self) -> TestState {
        let g = lock(&self.job);
        match &g.state {
            None => TestState::Idle,
            Some(TestState::Running { id, .. }) => TestState::Running {
                id: id.clone(),
                done: g.progress.load(Ordering::Relaxed),
                total: g.total,
                seconds: g.started.map_or(0.0, |t| t.elapsed().as_secs_f64()),
            },
            Some(s) => s.clone(),
        }
    }

    /// 已完成的測試（新的在前）。
    pub fn list(&self) -> Vec<TestSummary> {
        let mut out: HashMap<String, TestSummary> = lock(&self.done)
            .iter()
            .map(|(k, v)| (k.clone(), v.summary.clone()))
            .collect();
        if let Some(dir) = &self.dir
            && let Ok(entries) = std::fs::read_dir(dir)
        {
            for e in entries.flatten() {
                let p = e.path();
                let Some(id) = p
                    .file_name()
                    .and_then(|n| n.to_str())
                    .and_then(|n| n.strip_suffix(".json"))
                else {
                    continue;
                };
                if out.contains_key(id) {
                    continue;
                }
                if let Ok(f) = self.get(id) {
                    out.insert(id.to_string(), f.summary.clone());
                }
            }
        }
        let mut v: Vec<TestSummary> = out.into_values().collect();
        v.sort_by(|a, b| b.created.cmp(&a.created).then(b.id.cmp(&a.id)));
        v
    }

    /// 一個測試的結果（記憶體中沒有時讀檔）。
    pub fn get(&self, id: &str) -> Result<Arc<TestFile>, String> {
        if let Some(f) = lock(&self.done).get(id) {
            return Ok(f.clone());
        }
        if id.contains(['/', '\\', '.']) {
            return Err("不合法的測試 ID".into());
        }
        let path = self.file_path(id).ok_or("找不到這個測試")?;
        let text = std::fs::read_to_string(&path).map_err(|_| format!("找不到測試 {id}"))?;
        let f: TestFile =
            serde_json::from_str(&text).map_err(|e| format!("測試 {id} 的檔案損壞：{e}"))?;
        let f = Arc::new(f);
        lock(&self.done).insert(id.to_string(), f.clone());
        Ok(f)
    }

    /// 刪除一個測試（與保存的牌譜）。
    pub fn delete(&self, id: &str) -> Result<(), String> {
        let f = self.get(id)?;
        if matches!(lock(&self.job).state, Some(TestState::Running { id: ref r, .. }) if r == id) {
            return Err("測試還在進行中".into());
        }
        lock(&self.done).remove(id);
        if let Some(p) = self.file_path(&f.summary.id) {
            let _ = std::fs::remove_file(p);
        }
        if let Some(d) = self.dir.as_ref().map(|d| d.join(&f.summary.id)) {
            let _ = std::fs::remove_dir_all(d);
        }
        Ok(())
    }

    /// 對局清單的一頁。
    pub fn games(&self, q: &GamesQuery) -> Result<GamePage, String> {
        let f = self.get(&q.id)?;
        let plan = f.plan.as_ref().ok_or("這個測試沒有對局清單")?;
        let item_points = |o: &GameOutcome, i: usize| -> Option<f64> {
            let v: Vec<f64> = (0..4)
                .filter(|&s| o.slots[s] == i)
                .map(|s| o.points[s])
                .collect();
            (!v.is_empty()).then(|| v.iter().sum::<f64>() / v.len() as f64)
        };
        let mut order: Vec<&GameOutcome> = f.outcomes.iter().collect();
        match q.sort.as_deref() {
            Some("hands") => order.sort_by_key(|o| o.hands),
            Some(s) if s.starts_with("item:") => {
                let i: usize = s["item:".len()..].parse().map_err(|_| "排序方式錯誤")?;
                // 沒有這個項目的對局排最後（不論升降冪）。
                order.sort_by(|a, b| {
                    let key = |o: &GameOutcome| item_points(o, i);
                    match (key(a), key(b)) {
                        (Some(x), Some(y)) => {
                            if q.desc {
                                y.total_cmp(&x)
                            } else {
                                x.total_cmp(&y)
                            }
                        }
                        (Some(_), None) => std::cmp::Ordering::Less,
                        (None, Some(_)) => std::cmp::Ordering::Greater,
                        (None, None) => a.index.cmp(&b.index),
                    }
                });
            }
            _ => {}
        }
        if q.desc && !q.sort.as_deref().is_some_and(|s| s.starts_with("item:")) {
            order.reverse();
        }
        let rows = order
            .iter()
            .skip(q.offset)
            .take(q.limit.min(1000))
            .map(|o| GameRow {
                index: o.index,
                block: o.block,
                seed: o.seed,
                hands: o.hands,
                seats: (0..4)
                    .map(|s| SeatRow {
                        item: o.slots[s],
                        name: plan.names[o.slots[s]].clone(),
                        score: o.scores[s],
                        rank: o.ranks[s],
                        points: o.points[s],
                        wins: o.wins[s],
                        deal_ins: o.deal_ins[s],
                    })
                    .collect(),
                stored: f.logs.contains(&o.index),
            })
            .collect();
        Ok(GamePage {
            total: f.outcomes.len(),
            rows,
        })
    }

    /// 某一場的 mjai 牌譜：保存的牌譜，或用同樣的 seed 與座位配置重打（確認終局點數相同）。
    pub fn game_log(&self, id: &str, index: usize) -> Result<String, String> {
        let f = self.get(id)?;
        let plan = f.plan.as_ref().ok_or("這個測試沒有對局清單")?;
        let outcome = f
            .outcomes
            .iter()
            .find(|o| o.index == index)
            .ok_or("沒有這一場（可能測試中途停止了）")?;
        if f.logs.contains(&index) {
            let path = self.log_path(id, index).ok_or("沒有保存牌譜")?;
            return std::fs::read_to_string(path).map_err(|e| format!("無法讀取牌譜：{e}"));
        }
        let entrants: Vec<EntrantSpec> = f
            .specs
            .iter()
            .map(|s| EntrantSpec::parse(s))
            .collect::<Result<_, _>>()?;
        if outcome
            .slots
            .iter()
            .any(|&i| entrants[plan.entrant_of[i]].is_mjai())
        {
            return Err("這一場有外部 AI，沒有保存牌譜（測試時沒有資料目錄），無法重現".into());
        }
        let one = Plan {
            games: vec![plan.games[index]],
            ..plan.clone()
        };
        let make = |who: usize, seed: u64, seat: u8| entrants[who].player(seed, u64::from(seat));
        let keep = |_: &arena::GamePlan| true;
        let played = arena::play(
            &one,
            &make,
            &Control {
                keep_log: Some(&keep),
                ..Control::default()
            },
        )?;
        let (o, (_, log)) = played
            .outcomes
            .first()
            .zip(played.logs.first())
            .ok_or("重打失敗")?;
        if o.scores != outcome.scores {
            return Err(format!(
                "重打的結果與測試時不同（終局點數 {:?} ≠ {:?}）：策略或參數可能在測試之後改過",
                o.scores, outcome.scores
            ));
        }
        Ok(mjai_text(log, &seat_names(plan, &f.specs, &outcome.slots)))
    }
}

/// 每個座位的參賽者名稱（牌譜的玩家名稱）。
fn seat_names(plan: &Plan, specs: &[String], slots: &[usize; 4]) -> [String; 4] {
    std::array::from_fn(|s| specs[plan.entrant_of[slots[s]]].clone())
}

/// 完整事件紀錄 → mjai 牌譜文字。
fn mjai_text(log: &[mj_engine::Event], names: &[String; 4]) -> String {
    let lines: Vec<String> = mj_paifu::mjai::events_to_mjai(log, None, names)
        .iter()
        .map(|v| v.to_string())
        .collect();
    lines.join("\n") + "\n"
}
