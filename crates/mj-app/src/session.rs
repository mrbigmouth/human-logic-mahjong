//! 對局 session：人類一個座位、其餘座位是 AI。對局在背景執行緒進行，輪到人類時暫停，
//! 等 UI 送來動作（[`Session::act`]）。UI 以 [`Session::poll`] 取得新的事件幀與目前的詢問。

use crate::paifu::{LogMeta, with_meta};
use crate::review::ReviewJob;
pub use crate::review::{ReviewConfig, ReviewState};
use crate::table::{Frame, TableBuilder, seat_name, table_mismatch};
use mj_core::Pai;
use mj_engine::{Action, Event, GameConfig, Player, PlayerView, Seat, run_game};
use mj_mjai::{MjaiPlayer, PooledBot};
use mj_rules::riichi::RiichiConfig;
use mj_sim::bots::{GreedyBot, RandomBot};
use mj_strategy::find_strategy_spec;
use mj_strategy::framework::{Strategy, StrategyPlayer};
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
    /// 提示與復盤用的策略
    pub strategy: String,
    /// 打完這麼多手就結束（單局練習用；`None` = 打完整場半莊）
    pub max_hands: Option<u32>,
}

impl Default for SessionConfig {
    fn default() -> Self {
        SessionConfig {
            seed: None,
            seat: 0,
            opponents: std::array::from_fn(|_| DEFAULT_STRATEGY.to_string()),
            strategy: DEFAULT_STRATEGY.to_string(),
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
    pub options: Vec<String>,
    pub chosen: usize,
    /// 決策函式寫下的理由
    pub notes: Vec<String>,
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

/// 即時提示：策略在這次詢問的選擇與理由。
#[derive(Debug, Clone, Serialize)]
pub struct Hint {
    /// 策略選的動作（在 `Ask::actions` 中的編號）
    pub action: Option<usize>,
    pub label: String,
    pub decisions: Vec<HintDecision>,
    /// 要打牌時，每種打法的牌效（由好到壞）
    pub efficiency: Vec<Efficiency>,
}

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
    /// 旁觀的策略（提示用）
    hint: Box<dyn StrategyPlayer>,
    /// session 已被丟棄：之後自動打完（摸切或跳過）
    abandoned: bool,
    /// 剛自動摸切：下一個自己的打牌幀標記為自動
    auto_pending: bool,
}

impl HumanSeat {
    fn hint(&mut self, view: &PlayerView, legal: &[Action]) -> Option<Hint> {
        let chosen = self.hint.act(view, legal);
        let decisions = self
            .hint
            .take_records()
            .into_iter()
            .map(|r| HintDecision {
                kind: r.kind.name().to_string(),
                options: r.options,
                chosen: r.chosen,
                notes: r.notes,
            })
            .collect();
        let index = legal.iter().position(|a| *a == chosen);
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
        Some(Hint {
            action: index,
            label: action_view(0, &chosen).label,
            decisions,
            efficiency,
        })
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

/// 建立一個 AI 座位。
fn make_ai(spec: &str, seed: u64, names: &[String; 4]) -> Result<Box<dyn Player + Send>, String> {
    Ok(match spec.trim() {
        "greedy" => Box::new(GreedyBot::new(seed, 0.3)),
        "random" => Box::new(RandomBot::new(seed)),
        mjai if mjai.starts_with("mjai:") => {
            let cmd: Vec<String> = mjai["mjai:".len()..]
                .split_whitespace()
                .map(String::from)
                .collect();
            if cmd.is_empty() {
                return Err("mjai: 後面要接外部程式".into());
            }
            Box::new(MjaiPlayer::new(PooledBot::checkout(&cmd)?, names.clone()))
        }
        name => find_strategy_spec(name)?.player(false, None),
    })
}

/// 一場進行中的對局。丟棄時，若還在等人類的動作，對局會自動打完並結束。
pub struct Session {
    pub config: SessionConfig,
    /// 實際使用的 seed
    pub seed: u64,
    /// 四家名稱（依座位）
    pub names: [String; 4],
    strategy: Arc<dyn Strategy>,
    shared: Arc<Shared>,
    actions: Sender<Action>,
    review: ReviewJob,
}

impl Session {
    /// 開始一場對局（立即在背景執行緒開打）。
    pub fn start(config: SessionConfig) -> Result<Session, String> {
        if config.seat > 3 {
            return Err("座位必須是 0–3".into());
        }
        let strategy: Arc<dyn Strategy> = Arc::from(find_strategy_spec(&config.strategy)?);
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
        let mut players: Vec<Box<dyn Player + Send>> = Vec::with_capacity(4);
        for s in 0..4u8 {
            if s == me {
                players.push(Box::new(HumanSeat {
                    shared: shared.clone(),
                    actions: rx.take().expect("one human seat"),
                    builder: TableBuilder::new(me),
                    hint: strategy.player(true, None),
                    abandoned: false,
                    auto_pending: false,
                }));
            } else {
                players.push(make_ai(
                    &names[usize::from(s)],
                    seed.wrapping_mul(31).wrapping_add(u64::from(s)),
                    &names,
                )?);
            }
        }
        let mut cfg = GameConfig::new(RiichiConfig::TENHOU, seed);
        cfg.max_hands = config.max_hands;
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
            strategy,
            shared,
            actions: tx,
            review: ReviewJob::default(),
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
        self.review
            .start(self.strategy.clone(), record, self.me(), rc)
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
                strategy: Some(self.config.strategy.clone()),
            },
        );
        let lines: Vec<String> = events.iter().map(|v| v.to_string()).collect();
        Some(lines.join("\n") + "\n")
    }

    /// 座位的稱呼（相對於人類）。
    pub fn seat_name(&self, seat: Seat) -> &'static str {
        seat_name(self.me(), seat)
    }
}
