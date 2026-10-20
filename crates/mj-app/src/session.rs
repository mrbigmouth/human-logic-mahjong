//! 對局 session：人類一個座位、其餘座位是 AI。對局在背景執行緒進行，輪到人類時暫停，
//! 等 UI 送來動作（[`Session::act`]）。UI 以 [`Session::poll`] 取得新的事件幀與目前的詢問。

use crate::advisor::{AdvisorInfo, Advisors, advisors};
use crate::experiment::{ActionLog, Prepared, Recorder, snapshot_from_actions};
use crate::paifu::{LogMeta, with_meta};
use crate::review::ReviewJob;
pub use crate::review::{ReviewConfig, ReviewState};
use crate::table::{Frame, TableBuilder, seat_name, table_mismatch};
use mj_core::Pai;
use mj_engine::{Action, Event, GameConfig, Player, PlayerView, Seat, run_game};
use mj_review::advisor::{Answer, Policy, Watcher, pick};
use mj_rules::riichi::RiichiConfig;
use mj_sim::entrant::EntrantSpec;
use mj_strategy::framework::{DecisionRecord, Strategy, StrategyPlayer};
use serde::{Deserialize, Serialize};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// 預設的提示與復盤策略。
pub const DEFAULT_STRATEGY: &str = "tenhou-normal-4p-hanchan/hand-reading";

/// 開一場對局的設定。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SessionConfig {
    /// 牌山 seed（同一個 seed、同樣的打法會重現同一場對局）；`None` = 依時間決定
    pub seed: Option<u64>,
    /// 人類的座位（0 = 起家）
    pub seat: Seat,
    /// 其他三家（從人類的下家開始）：策略名稱、`greedy`、`random` 或 `mjai:<程式 參數…>`
    pub opponents: [String; 3],
    /// 提示與復盤用的策略（`advisors` 是空的時候用）
    pub strategy: String,
    /// 提示與復盤的顧問（1–3 個：策略名稱或 `mjai:<程式 參數…>`）；第一個是主要的（提示標示的動作、復盤的比較基準）
    pub advisors: Vec<String>,
    /// 打完這麼多手就結束（單局練習用；`None` = 打完整場半莊）
    pub max_hands: Option<u32>,
}

impl SessionConfig {
    /// 顧問的名稱：`advisors`，空的時候是 `strategy`。
    pub fn advisor_names(&self) -> Vec<String> {
        if self.advisors.is_empty() {
            vec![self.strategy.clone()]
        } else {
            self.advisors.clone()
        }
    }
}

impl Default for SessionConfig {
    fn default() -> Self {
        SessionConfig {
            seed: None,
            seat: 0,
            opponents: std::array::from_fn(|_| DEFAULT_STRATEGY.to_string()),
            strategy: DEFAULT_STRATEGY.to_string(),
            advisors: Vec::new(),
            max_hands: None,
        }
    }
}

/// UI 用的一個合法動作。
#[derive(Debug, Clone, Serialize)]
pub struct ActionView {
    /// 在合法動作中的編號（送出動作時用）
    pub index: usize,
    /// discard、riichi、chi、pon、daiminkan、ankan、kakan、tsumo、ron、kyuushu、pass
    #[serde(rename = "type")]
    pub kind: &'static str,
    /// 打出、加槓的牌；暗槓的牌種
    pub pai: Option<String>,
    /// 摸切
    pub tsumogiri: bool,
    /// 吃碰用的手牌
    pub consumed: Vec<String>,
    /// 中文說明，例如「打 5m」「吃 4m6m」
    pub label: String,
}

/// 把一個動作轉成 UI 用的形式。
pub fn action_view(index: usize, a: &Action) -> ActionView {
    let s = |p: &Pai| p.to_string();
    let (kind, pai, tsumogiri, consumed, label) = match a {
        Action::Discard { pai, tsumogiri } => (
            "discard",
            Some(s(pai)),
            *tsumogiri,
            vec![],
            format!("打 {pai}{}", if *tsumogiri { "（摸切）" } else { "" }),
        ),
        Action::Riichi { pai, tsumogiri } => (
            "riichi",
            Some(s(pai)),
            *tsumogiri,
            vec![],
            format!("立直 打 {pai}"),
        ),
        Action::Chi { consumed } => (
            "chi",
            None,
            false,
            consumed.iter().map(s).collect(),
            format!("吃 {}{}", consumed[0], consumed[1]),
        ),
        Action::Pon { consumed } => (
            "pon",
            None,
            false,
            consumed.iter().map(s).collect(),
            format!("碰 {}{}", consumed[0], consumed[1]),
        ),
        Action::Daiminkan => ("daiminkan", None, false, vec![], "明槓".into()),
        Action::Ankan { tile } => (
            "ankan",
            Some(tile.to_string()),
            false,
            vec![],
            format!("暗槓 {tile}"),
        ),
        Action::Kakan { pai } => ("kakan", Some(s(pai)), false, vec![], format!("加槓 {pai}")),
        Action::Tsumo => ("tsumo", None, false, vec![], "自摸".into()),
        Action::Ron => ("ron", None, false, vec![], "榮和".into()),
        Action::KyuushuKyuuhai => ("kyuushu", None, false, vec![], "九種九牌".into()),
        Action::Pass => ("pass", None, false, vec![], "跳過".into()),
    };
    ActionView {
        index,
        kind,
        pai,
        tsumogiri,
        consumed,
        label,
    }
}

/// 策略的一個決策（提示用）。
#[derive(Debug, Clone, Serialize)]
pub struct HintDecision {
    /// 決策種類，例如「何切」「立直與否」
    pub kind: String,
    /// 決策種類的代號（反事實實驗指定決策時用）
    pub kind_key: mj_strategy::framework::DecisionKind,
    pub options: Vec<String>,
    pub chosen: usize,
    /// 決策函式寫下的理由
    pub notes: Vec<String>,
    /// 每個顧問在這個決策的選擇（依 `Hint::advisors`；走了另一條路時為 `None`）
    pub picks: Vec<Option<usize>>,
}

/// 一個顧問在這次詢問的建議。
#[derive(Debug, Clone, Serialize)]
pub struct Advice {
    #[serde(flatten)]
    pub info: AdvisorInfo,
    /// 選的動作（在 `Ask::actions` 中的編號；AI 回應不合法時為 `None`）
    pub action: Option<usize>,
    pub label: String,
    /// 決策（策略：自己的決策與理由；AI：基礎策略的決策結構，`chosen` 是 AI 的選擇、沒有理由）
    pub decisions: Vec<HintDecision>,
    /// AI：各動作的機率
    pub policy: Option<Policy>,
    pub error: Option<String>,
}

/// 牌效參考：打出某張牌後的向聽數與進張（剩餘張數扣掉自己看得到的牌）。
#[derive(Debug, Clone, Serialize)]
pub struct Efficiency {
    pub pai: String,
    pub shanten: i8,
    /// 進張種類
    pub tiles: Vec<String>,
    /// 進張總數
    pub ukeire: f32,
}

/// 即時提示：各顧問在這次詢問的選擇與理由（或機率）。
#[derive(Debug, Clone, Serialize)]
pub struct Hint {
    /// 第一個顧問選的動作（在 `Ask::actions` 中的編號）
    pub action: Option<usize>,
    pub label: String,
    pub advisors: Vec<Advice>,
    /// 要打牌時，每種打法的牌效（由好到壞）
    pub efficiency: Vec<Efficiency>,
    /// 要打牌時，離和牌最近的幾條路線（向聽數 ≤ 2 時才有；見 `mj_tools::routes`）
    pub routes: Vec<RouteView>,
}

/// 一條和牌路線（給 UI）。
#[derive(Debug, Clone, Serialize)]
pub struct RouteView {
    /// 要打掉的牌（第一張是現在要打的）
    pub discards: Vec<String>,
    /// 還要摸到的牌
    pub draws: Vec<String>,
    /// 和了時的門前手牌（含和牌張）
    pub hand: Vec<String>,
    /// 點數最高的和牌張
    pub win_tile: String,
    /// 役（名稱與番數）；無役時是空的
    pub yaku: Vec<(String, u8)>,
    pub han: u8,
    pub fu: u8,
    /// 滿貫、跳滿……
    pub limit: String,
    /// 榮和點數（無役為 0）
    pub points: i32,
    /// 點數假設立直
    pub riichi: bool,
    /// 相對機會（剩餘張數的組合數乘積）
    pub chance: f64,
}

/// 提示列出的和牌路線條數。
const HINT_ROUTES: usize = 6;

/// 等待人類回答的詢問。
#[derive(Debug, Clone, Serialize)]
pub struct Ask {
    /// 詢問編號（送出動作時帶上，避免送到過期的詢問）
    pub id: u64,
    /// 這次詢問時已經發生的事件幀數
    pub frame: usize,
    pub actions: Vec<ActionView>,
    pub hint: Option<Hint>,
}

/// 對局結果。
#[derive(Debug, Clone, Serialize)]
pub struct GameOver {
    pub scores: [i32; 4],
    /// 名次（0 = 第一名）
    pub ranks: [u8; 4],
    pub hands_played: u32,
}

/// [`Session::poll`] 的回應。
#[derive(Debug, Clone, Serialize)]
pub struct Update {
    /// 從 `since` 開始的新事件幀
    pub frames: Vec<Frame>,
    /// 目前總共有幾幀（下次 poll 的 `since`）
    pub next: usize,
    /// 等待人類回答的詢問
    pub ask: Option<Ask>,
    pub game_over: Option<GameOver>,
    pub error: Option<String>,
}

/// 共用狀態。
#[derive(Default)]
struct State {
    frames: Vec<Frame>,
    ask: Option<Ask>,
    /// 對應 `ask` 的合法動作
    legal: Vec<Action>,
    asks: u64,
    game_over: Option<GameOver>,
    error: Option<String>,
    /// 完整事件紀錄（對局結束後才有）
    log: Vec<Event>,
    /// 人類每次被詢問時的視角（索引 = 第幾次詢問）；反事實實驗用來確認重播到的局面相同
    views: Vec<PlayerView>,
}

#[derive(Default)]
struct Shared {
    state: Mutex<State>,
    changed: Condvar,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn update(&self, f: impl FnOnce(&mut State)) {
        f(&mut self.lock());
        self.changed.notify_all();
    }
}

/// 人類座位：把事件轉成幀，詢問時等待 UI 的動作。
struct HumanSeat {
    shared: Arc<Shared>,
    actions: Receiver<Action>,
    builder: TableBuilder,
    /// 旁觀的基礎策略（提示的決策結構）
    hint: Box<dyn StrategyPlayer>,
    /// 其他顧問（依顧問順序；基礎策略的位置是 `None`）
    watchers: Vec<Option<Watcher>>,
    advisors: Vec<AdvisorInfo>,
    /// session 已被丟棄：之後自動打完（摸切或跳過）
    abandoned: bool,
    /// 剛自動摸切：下一個自己的打牌幀標記為自動
    auto_pending: bool,
}

impl HumanSeat {
    fn hint(&mut self, view: &PlayerView, legal: &[Action]) -> Option<Hint> {
        let chosen = self.hint.act(view, legal);
        let base_records = self.hint.take_records();
        for w in self.watchers.iter_mut().flatten() {
            w.act(view, legal);
        }
        let answers: Vec<Answer> = (0..self.advisors.len())
            .map(|k| match &self.watchers[k] {
                Some(w) => w.last().cloned().unwrap_or_default(),
                None => Answer {
                    action: Some(chosen.clone()),
                    records: base_records.clone(),
                    ..Answer::default()
                },
            })
            .collect();
        let advisors: Vec<Advice> = self
            .advisors
            .iter()
            .zip(&answers)
            .enumerate()
            .map(|(k, (info, answer))| {
                let action = answer
                    .action
                    .as_ref()
                    .and_then(|a| legal.iter().position(|b| b == a));
                let decisions = if info.ai {
                    // AI 沒有自己的決策：用基礎策略的決策結構，選擇換成 AI 的，不附理由。
                    hint_decisions(&base_records, legal, &answers)
                        .into_iter()
                        .filter_map(|mut d| {
                            d.chosen = d.picks[k]?;
                            d.notes.clear();
                            Some(d)
                        })
                        .collect()
                } else {
                    hint_decisions(&answer.records, legal, &answers)
                };
                Advice {
                    info: info.clone(),
                    action,
                    label: answer
                        .action
                        .as_ref()
                        .map_or_else(|| "（無法得知）".into(), |a| action_view(0, a).label),
                    decisions,
                    policy: answer.policy.clone(),
                    error: answer.error.clone(),
                }
            })
            .collect();
        // 只列可以打的牌（食替禁止的牌不列）。
        let can_discard = |t: mj_core::Tile| {
            legal
                .iter()
                .any(|a| matches!(a, Action::Discard { pai, .. } if pai.tile == t))
        };
        let efficiency = if legal.iter().any(|a| matches!(a, Action::Discard { .. })) {
            mj_tools::hand::discard_options(view, &view.hand)
                .into_iter()
                .filter(|o| can_discard(o.discard))
                .map(|o| Efficiency {
                    pai: o.discard.to_string(),
                    shanten: o.after.shanten,
                    tiles: o.after.tiles.iter().map(|(t, _)| t.to_string()).collect(),
                    ukeire: o.after.total,
                })
                .collect()
        } else {
            Vec::new()
        };
        let routes = if legal.iter().any(|a| matches!(a, Action::Discard { .. })) {
            mj_tools::routes::win_routes(view, &view.hand, HINT_ROUTES)
                .into_iter()
                .filter(|r| r.discards.first().is_none_or(|&d| can_discard(d)))
                .map(route_view)
                .collect()
        } else {
            Vec::new()
        };
        Some(Hint {
            action: advisors.first().and_then(|a| a.action),
            label: advisors
                .first()
                .map_or_else(String::new, |a| a.label.clone()),
            advisors,
            efficiency,
            routes,
        })
    }
}

/// 決策紀錄 → 提示的決策，附上每個顧問在這個決策的選擇。
fn hint_decisions(
    records: &[DecisionRecord],
    legal: &[Action],
    answers: &[Answer],
) -> Vec<HintDecision> {
    records
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let riichi_pai = mj_review::strategy_discard(records, i, legal);
            HintDecision {
                kind: r.kind.name().to_string(),
                kind_key: r.kind,
                options: r.options.clone(),
                chosen: r.chosen,
                notes: r.notes.clone(),
                picks: answers
                    .iter()
                    .map(|a| pick(r.kind, &r.options, legal, a, riichi_pai))
                    .collect(),
            }
        })
        .collect()
}

fn route_view(r: mj_tools::routes::WinRoute) -> RouteView {
    let names = |ts: &[mj_core::Tile]| ts.iter().map(|t| t.to_string()).collect();
    RouteView {
        discards: names(&r.discards),
        draws: names(&r.draws),
        hand: names(&r.hand),
        win_tile: r.win_tile.to_string(),
        // 役之後列寶牌、赤寶牌（不含裏寶牌：和了前不知道），番數才對得起來。
        yaku: r.score.as_ref().map_or(Vec::new(), |s| {
            let mut v: Vec<(String, u8)> = s
                .yaku
                .iter()
                .map(|(y, h)| (y.name_ja().to_string(), *h))
                .collect();
            for (name, n) in [("寶牌", s.dora), ("赤寶牌", s.aka)] {
                if n > 0 {
                    v.push((name.to_string(), n));
                }
            }
            v
        }),
        han: r.score.as_ref().map_or(0, |s| s.han),
        fu: r.score.as_ref().map_or(0, |s| s.fu),
        limit: r
            .score
            .as_ref()
            .map_or(String::new(), |s| crate::table::limit_name(s.limit)),
        points: r.points,
        riichi: r.assumes_riichi,
        chance: r.chance,
    }
}

impl HumanSeat {
    /// 除錯建置時確認由事件推算的牌桌與引擎的視角一致。
    fn check_table(&self, view: &PlayerView) {
        if cfg!(debug_assertions)
            && let Some(e) = table_mismatch(self.builder.table(), view)
        {
            panic!("{e}");
        }
    }
}

/// 沒有人操作時的預設動作：能跳過就跳過，否則摸切（或第一個打牌選項）。
fn auto_action(legal: &[Action]) -> Action {
    legal
        .iter()
        .find(|a| matches!(a, Action::Pass))
        .or_else(|| {
            legal.iter().find(|a| {
                matches!(
                    a,
                    Action::Discard {
                        tsumogiri: true,
                        ..
                    }
                )
            })
        })
        .or_else(|| legal.iter().find(|a| matches!(a, Action::Discard { .. })))
        .unwrap_or(&legal[0])
        .clone()
}

impl Player for HumanSeat {
    fn on_event(&mut self, me: Seat, event: &Event) {
        self.hint.on_event(me, event);
        for w in self.watchers.iter_mut().flatten() {
            w.on_event(me, event);
        }
        let mut frame = self.builder.apply(event);
        if self.auto_pending && matches!(event, Event::Discard { seat, .. } if *seat == me) {
            frame.auto = true;
            self.auto_pending = false;
        }
        self.shared.update(|s| s.frames.push(frame));
    }

    fn act(&mut self, view: &PlayerView, legal: &[Action]) -> Action {
        // 提示策略一律要經過每次詢問（它的記憶與詢問次數才會正確）。
        self.check_table(view);
        let frame = self.shared.lock().frames.len();
        self.shared.update(|s| s.views.push(view.clone()));
        let hint = self.hint(view, legal);
        if self.abandoned {
            return auto_action(legal);
        }
        // 立直後只能摸切（不能自摸、不能暗槓）：自動打出，不詢問。
        // 詢問編號照樣遞增，維持「編號 − 1 = 第幾次詢問（act_index）」的對應（復盤用）。
        if let [
            a @ Action::Discard {
                tsumogiri: true, ..
            },
        ] = legal
        {
            self.auto_pending = true;
            self.shared.update(|s| s.asks += 1);
            return a.clone();
        }
        self.shared.update(|s| {
            s.asks += 1;
            s.ask = Some(Ask {
                id: s.asks,
                frame,
                actions: legal
                    .iter()
                    .enumerate()
                    .map(|(i, a)| action_view(i, a))
                    .collect(),
                hint,
            });
            s.legal = legal.to_vec();
        });

        match self.actions.recv() {
            Ok(a) => a,
            Err(_) => {
                self.abandoned = true;
                auto_action(legal)
            }
        }
    }
}

/// 一場進行中的對局。丟棄時，若還在等人類的動作，對局會自動打完並結束。
pub struct Session {
    pub config: SessionConfig,
    /// 實際使用的 seed
    pub seed: u64,
    /// 四家名稱（依座位）
    pub names: [String; 4],
    advisors: Advisors,
    shared: Arc<Shared>,
    actions: Sender<Action>,
    review: ReviewJob,
    /// 對局設定（重播用）
    cfg: GameConfig,
    /// 四家到目前為止的動作
    history: ActionLog,
}

impl Session {
    /// 開始一場對局（立即在背景執行緒開打）。
    pub fn start(config: SessionConfig) -> Result<Session, String> {
        if config.seat > 3 {
            return Err("座位必須是 0–3".into());
        }
        let advisors = advisors(&config.advisor_names())?;
        let seed = config.seed.unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                // 限制在 53 位元內：前端（JavaScript 的數字）才能精確顯示並重新輸入。
                .map_or(0, |d| d.as_nanos() as u64 & ((1 << 53) - 1))
        });
        let me = config.seat;
        let names: [String; 4] = std::array::from_fn(|s| {
            let s = s as u8;
            if s == me {
                "你".to_string()
            } else {
                config.opponents[usize::from((s + 3 - me) % 4)].clone()
            }
        });
        let shared = Arc::new(Shared::default());
        let (tx, rx) = channel();
        let mut rx = Some(rx);
        let actions: ActionLog = Arc::default();
        let mut players: Vec<Box<dyn Player + Send>> = Vec::with_capacity(4);
        for s in 0..4u8 {
            let inner: Box<dyn Player + Send> = if s == me {
                Box::new(HumanSeat {
                    shared: shared.clone(),
                    actions: rx.take().expect("one human seat"),
                    builder: TableBuilder::new(me),
                    hint: advisors.base.player(true, None),
                    watchers: advisors
                        .list
                        .iter()
                        .enumerate()
                        .map(|(k, a)| {
                            if Some(k) == advisors.base_index {
                                Ok(None)
                            } else {
                                Watcher::new(a, &names).map(Some)
                            }
                        })
                        .collect::<Result<_, String>>()?,
                    advisors: advisors.infos(),
                    abandoned: false,
                    auto_pending: false,
                })
            } else {
                // 參賽者登錄與對戰測試、CLI 共用（機器人的 seed 由對局 seed 與座位推導）。
                EntrantSpec::parse(&names[usize::from(s)])?.try_player(seed, u64::from(s))?
            };
            // 記下每個動作：反事實實驗照紀錄重播到任一時刻。
            players.push(Box::new(Recorder {
                inner,
                seat: s,
                log: actions.clone(),
            }));
        }
        let mut cfg = GameConfig::new(RiichiConfig::TENHOU, seed);
        cfg.max_hands = config.max_hands;
        let game_cfg = cfg.clone();
        let sh = shared.clone();
        std::thread::Builder::new()
            .name("mj-session".into())
            .spawn(move || {
                let mut players: [Box<dyn Player>; 4] = players
                    .into_iter()
                    .map(|p| p as Box<dyn Player>)
                    .collect::<Vec<_>>()
                    .try_into()
                    .unwrap_or_else(|_| unreachable!("four seats"));
                let r = run_game(&cfg, &mut players, true);
                sh.update(|s| match r {
                    Ok(r) => {
                        s.game_over = Some(GameOver {
                            scores: r.scores,
                            ranks: r.ranks,
                            hands_played: r.hands_played,
                        });
                        s.log = r.log;
                    }
                    Err(e) => s.error = Some(format!("對局錯誤：{e}")),
                });
            })
            .map_err(|e| format!("無法建立對局執行緒：{e}"))?;
        Ok(Session {
            config,
            seed,
            names,
            advisors,
            shared,
            actions: tx,
            review: ReviewJob::default(),
            cfg: game_cfg,
            history: actions,
        })
    }

    /// 人類的座位。
    pub fn me(&self) -> Seat {
        self.config.seat
    }

    /// 取得第 `since` 幀之後的新事件。沒有新東西時最多等 `wait`（長輪詢）。
    pub fn poll(&self, since: usize, wait: Duration) -> Update {
        let deadline = Instant::now() + wait;
        let mut s = self.shared.lock();
        loop {
            let fresh = s.frames.len() > since
                || s.ask.is_some()
                || s.game_over.is_some()
                || s.error.is_some();
            let now = Instant::now();
            if fresh || now >= deadline {
                break;
            }
            s = self
                .shared
                .changed
                .wait_timeout(s, deadline - now)
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
        let since = since.min(s.frames.len());
        Update {
            frames: s.frames[since..].to_vec(),
            next: s.frames.len(),
            ask: s.ask.clone(),
            game_over: s.game_over.clone(),
            error: s.error.clone(),
        }
    }

    /// 回答詢問 `ask_id`：選第 `index` 個合法動作。
    pub fn act(&self, ask_id: u64, index: usize) -> Result<(), String> {
        // 在同一個鎖內清掉詢問，避免同一個詢問被回答兩次（第二個動作會被下一次詢問收到）。
        let action = {
            let mut s = self.shared.lock();
            match &s.ask {
                Some(a) if a.id == ask_id => {}
                Some(a) => return Err(format!("詢問 {ask_id} 已過期（目前是 {}）", a.id)),
                None => return Err("目前沒有等待中的詢問".into()),
            }
            let action = s
                .legal
                .get(index)
                .cloned()
                .ok_or_else(|| format!("沒有第 {index} 個動作"))?;
            s.ask = None;
            s.legal.clear();
            action
        };
        self.actions
            .send(action)
            .map_err(|_| "對局已經結束".to_string())
    }

    /// 開始復盤人類座位（背景執行；對局結束後才能開始）。
    pub fn start_review(&self, rc: &ReviewConfig) -> Result<(), String> {
        let text = self.mjai_log().ok_or("對局還沒結束")?;
        let record = mj_paifu::parse(&text, &format!("session-{}", self.seed))?;
        self.review.start(&self.advisors, record, self.me(), rc)
    }

    /// 復盤狀態。
    pub fn review(&self) -> ReviewState {
        self.review.state()
    }

    /// 對局結束後的完整事件紀錄（mjai 格式，一行一個事件），可存成牌譜檔。
    /// `start_game` 附上 `mj_meta`（seed、人類的座位、提示策略），讀回來時用。
    pub fn mjai_log(&self) -> Option<String> {
        let s = self.shared.lock();
        s.game_over.as_ref()?;
        let mut events = mj_paifu::mjai::events_to_mjai(&s.log, None, &self.names);
        with_meta(
            &mut events,
            &LogMeta {
                seed: Some(self.seed),
                seat: Some(self.me()),
                strategy: Some(self.base_name()),
                advisors: self.advisors.names(),
            },
        );
        let lines: Vec<String> = events.iter().map(|v| v.to_string()).collect();
        Some(lines.join("\n") + "\n")
    }

    /// 反事實實驗用：「即將第 `ask_index` 次詢問人類」時的快照與四家旁觀的策略玩家（照動作紀錄重播）。
    /// 回傳一個在背景執行緒中計算的函式。
    pub fn snapshot_job(
        &self,
        ask_index: u32,
        strategy: Arc<dyn Strategy>,
    ) -> impl FnOnce() -> Result<Prepared, String> + Send + 'static {
        let cfg = self.cfg.clone();
        let actions = self
            .history
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let me = self.me();
        let expected = self.shared.lock().views.get(ask_index as usize).cloned();
        move || {
            let expected = expected.ok_or("還沒有這次詢問")?;
            let (snap, base) = snapshot_from_actions(&cfg, &actions, &strategy, me, ask_index)?;
            // 重播到的局面必須和當時看到的一樣（否則代表重播走了另一條路）。
            let same =
                serde_json::to_value(snap.view()).ok() == serde_json::to_value(&expected).ok();
            if !same {
                return Err("重播到的局面與對局不一致".into());
            }
            Ok((snap, base))
        }
    }

    /// 顧問。
    pub fn advisors(&self) -> &Advisors {
        &self.advisors
    }

    /// 基礎策略的名稱（含參數覆寫）。
    fn base_name(&self) -> String {
        match self.advisors.base_index {
            Some(i) => self.advisors.list[i].name.clone(),
            None => DEFAULT_STRATEGY.to_string(),
        }
    }

    /// 座位的稱呼（相對於人類）。
    pub fn seat_name(&self, seat: Seat) -> &'static str {
        seat_name(self.me(), seat)
    }
}
