//! 反事實實驗（M14）：在對局中或復盤時的任一個選擇點，比較幾個選項「之後依策略打」的結果。
//!
//! 1. 取得那一刻的局面快照，以及四家「旁觀到那一刻」的策略玩家：
//!    - 對局中：session 記錄了四家的每個動作（[`Recorder`]），用同一個 seed 重播（`Scripted` 照紀錄出牌、
//!      策略在旁邊旁觀），在這次詢問保存快照。不必重新計算 AI，對手是 Mortal 也可以。
//!    - 讀入的牌譜：`mj_paifu::replay::capture`（策略旁觀牌譜重播）。
//! 2. `mj_sim::counterfactual::run_experiment`：依決策者看得到的資訊重抽看不到的牌，每個選項強制選定後
//!    四家都依策略打完，比較平均結果、本局和了率與放銃率。對手實際上是誰都一樣：重打時一律用策略。

use mj_engine::{Action, Event, GameConfig, Player, PlayerView, Seat, Snapshot, capture_snapshot};
use mj_rules::riichi::RiichiConfig;
use mj_sim::counterfactual::{Experiment, Metric, OptionStats, Point, run_experiment};
use mj_strategy::framework::{DecisionKind, Strategy, StrategyPlayer};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// 四家到目前為止的動作（依座位、依詢問順序）。
pub type ActionLog = Arc<Mutex<[Vec<Action>; 4]>>;

/// 實驗的起點：快照，以及四家旁觀到快照點的策略玩家。
pub type Prepared = (Snapshot, Vec<Box<dyn StrategyPlayer>>);

/// 包住一個座位的玩家，記下它的每個動作（之後可以照紀錄重播到任一時刻）。
pub struct Recorder {
    pub inner: Box<dyn Player + Send>,
    pub seat: Seat,
    pub log: ActionLog,
}

impl Player for Recorder {
    fn on_event(&mut self, me: Seat, event: &Event) {
        self.inner.on_event(me, event);
    }

    fn act(&mut self, view: &PlayerView, legal: &[Action]) -> Action {
        let a = self.inner.act(view, legal);
        self.log.lock().unwrap_or_else(|e| e.into_inner())[usize::from(self.seat)].push(a.clone());
        a
    }
}

/// 照紀錄出牌的玩家；策略玩家在旁邊旁觀（收到同樣的事件與詢問，記憶與詢問次數都接得上）。
struct Scripted {
    actions: Vec<Action>,
    next: usize,
    shadow: Box<dyn StrategyPlayer>,
}

impl Player for Scripted {
    fn on_event(&mut self, me: Seat, event: &Event) {
        self.shadow.on_event(me, event);
    }

    fn act(&mut self, view: &PlayerView, legal: &[Action]) -> Action {
        self.shadow.act(view, legal);
        let a = self
            .actions
            .get(self.next)
            .cloned()
            .unwrap_or_else(|| legal[0].clone());
        self.next += 1;
        a
    }
}

/// 照動作紀錄重播到「即將第 `ask_index` 次詢問 `seat`」，回傳快照與四家旁觀的策略玩家。
pub fn snapshot_from_actions(
    cfg: &GameConfig,
    actions: &[Vec<Action>; 4],
    strategy: &Arc<dyn Strategy>,
    seat: Seat,
    ask_index: u32,
) -> Result<Prepared, String> {
    let mut players: Vec<Scripted> = actions
        .iter()
        .map(|a| Scripted {
            actions: a.clone(),
            next: 0,
            shadow: strategy.player(false, None),
        })
        .collect();
    let refs: [&mut dyn Player; 4] = {
        let mut it = players.iter_mut().map(|p| p as &mut dyn Player);
        std::array::from_fn(|_| it.next().unwrap())
    };
    let snap = capture_snapshot(cfg, refs, seat, ask_index)
        .map_err(|e| e.to_string())?
        .ok_or("這個時點不能保存局面（例如搶槓）")?;
    Ok((snap, players.into_iter().map(|p| p.shadow).collect()))
}

/// 實驗的請求（API `start_experiment`）。
#[derive(Debug, Clone, Deserialize)]
pub struct ExperimentRequest {
    /// `game` = 目前的對局；`log` = 讀入的牌譜
    #[serde(default = "default_source")]
    pub source: String,
    /// 讀入的牌譜：哪一家（對局中固定是人類的座位）
    #[serde(default)]
    pub seat: Option<Seat>,
    /// 詢問編號（= 第幾次詢問 + 1，與 `Ask::id` 相同）
    pub ask: u64,
    /// 決策種類（與提示、復盤中的決策相同）
    pub kind: DecisionKind,
    /// 這個決策的選項數
    pub option_count: usize,
    /// 要比較的選項（空 = 全部）
    #[serde(default)]
    pub options: Vec<usize>,
    /// 差值的比較基準（通常是策略的選擇）
    pub reference: usize,
    /// 世界數
    #[serde(default = "default_worlds")]
    pub worlds: usize,
    /// points = 本局點數；placement = 名次點（打到整場結束）
    #[serde(default = "default_metric")]
    pub metric: String,
    /// 重打用的策略：顧問中的策略名稱（通常是做這個決策的顧問）；不是策略或沒指定時用基礎策略（見 [`crate::advisor`]）
    #[serde(default)]
    pub strategy: Option<String>,
}

fn default_source() -> String {
    "game".into()
}
fn default_worlds() -> usize {
    64
}
fn default_metric() -> String {
    "points".into()
}

/// 世界數上限（避免一次跑太久）。
pub const MAX_WORLDS: usize = 4096;

/// 實驗的狀態（API `experiment`）。
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ExperimentState {
    Idle,
    Running {
        done: usize,
        total: usize,
        seconds: f64,
    },
    Done {
        /// 與請求相同，讓 UI 確認結果屬於哪個決策
        source: String,
        seat: Seat,
        ask: u64,
        kind: DecisionKind,
        reference: usize,
        metric: String,
        /// 實際完成的世界數（取消時比要求的少）
        worlds: usize,
        cancelled: bool,
        results: Vec<OptionStats>,
        seconds: f64,
    },
    Failed {
        error: String,
    },
}

/// 一個實驗工作（同時只有一個；開始新的會取消舊的）。
#[derive(Default)]
pub struct ExperimentJob {
    inner: Arc<Mutex<JobInner>>,
}

#[derive(Default)]
struct JobInner {
    state: Option<ExperimentState>,
    started: Option<Instant>,
    total: usize,
    /// 這次實驗的進度與取消旗標（每次開始都換新的，舊的執行緒看到不同的旗標就不會覆蓋結果）
    progress: Arc<AtomicUsize>,
    cancel: Arc<AtomicBool>,
}

impl ExperimentJob {
    /// 在背景執行實驗。`prepare` 在背景執行緒中取得快照與旁觀的策略玩家。
    pub fn start(
        &self,
        req: &ExperimentRequest,
        seat: Seat,
        prepare: impl FnOnce() -> Result<Prepared, String> + Send + 'static,
    ) -> Result<(), String> {
        let metric = match req.metric.as_str() {
            "points" => Metric::Points,
            "placement" => Metric::Placement,
            m => return Err(format!("不認得的衡量方式 {m}（points 或 placement）")),
        };
        if req.ask == 0 {
            return Err("詢問編號從 1 開始".into());
        }
        if req.reference >= req.option_count {
            return Err("比較基準超出選項範圍".into());
        }
        let mut options = if req.options.is_empty() {
            (0..req.option_count).collect()
        } else {
            req.options.clone()
        };
        if !options.contains(&req.reference) {
            options.push(req.reference);
        }
        options.sort_unstable();
        options.dedup();
        let worlds = req.worlds.clamp(2, MAX_WORLDS);
        let exp = Experiment {
            point: Point {
                seat,
                act_index: (req.ask - 1) as u32,
                kind: req.kind,
                options: req.option_count,
            },
            options,
            reference: req.reference,
            worlds,
            metric,
            determinize: true,
            seed: mj_engine::hand_seed(req.ask ^ 0x0E5E_ED00, u32::from(seat)),
        };
        let progress = Arc::new(AtomicUsize::new(0));
        let cancel = Arc::new(AtomicBool::new(false));
        {
            let mut g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
            // 取消進行中的實驗。
            g.cancel.store(true, Ordering::Relaxed);
            *g = JobInner {
                state: Some(ExperimentState::Running {
                    done: 0,
                    total: worlds,
                    seconds: 0.0,
                }),
                started: Some(Instant::now()),
                total: worlds,
                progress: progress.clone(),
                cancel: cancel.clone(),
            };
        }
        let inner = self.inner.clone();
        let (source, ask, kind, reference, metric_name) = (
            req.source.clone(),
            req.ask,
            req.kind,
            req.reference,
            req.metric.clone(),
        );
        std::thread::Builder::new()
            .name("mj-experiment".into())
            .spawn(move || {
                let start = Instant::now();
                let r = prepare().and_then(|(snap, base)| {
                    run_experiment(
                        &RiichiConfig::TENHOU,
                        &snap,
                        &base,
                        &exp,
                        &progress,
                        &cancel,
                    )
                });
                let state = match r {
                    Ok(results) => ExperimentState::Done {
                        source,
                        seat,
                        ask,
                        kind,
                        reference,
                        metric: metric_name,
                        worlds: progress.load(Ordering::Relaxed),
                        cancelled: cancel.load(Ordering::Relaxed),
                        results,
                        seconds: start.elapsed().as_secs_f64(),
                    },
                    Err(error) => ExperimentState::Failed { error },
                };
                let mut g = inner.lock().unwrap_or_else(|e| e.into_inner());
                // 已經被新的實驗取代時，不覆蓋新的狀態。
                if Arc::ptr_eq(&g.cancel, &cancel) {
                    g.state = Some(state);
                }
            })
            .map_err(|e| format!("無法建立實驗執行緒：{e}"))?;
        Ok(())
    }

    /// 取消進行中的實驗，並清掉結果（開新對局、讀入新牌譜時：舊的結果不再適用）。
    pub fn reset(&self) {
        let mut g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        g.cancel.store(true, Ordering::Relaxed);
        *g = JobInner::default();
    }

    /// 取消進行中的實驗（以已完成的世界計算結果）。
    pub fn cancel(&self) {
        let g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        g.cancel.store(true, Ordering::Relaxed);
    }

    pub fn state(&self) -> ExperimentState {
        let g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        match &g.state {
            None => ExperimentState::Idle,
            Some(ExperimentState::Running { .. }) => ExperimentState::Running {
                done: g.progress.load(Ordering::Relaxed),
                total: g.total,
                seconds: g.started.map_or(0.0, |t| t.elapsed().as_secs_f64()),
            },
            Some(s) => s.clone(),
        }
    }
}
