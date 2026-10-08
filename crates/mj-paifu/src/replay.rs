//! 用引擎重播牌譜：還原每局的牌山，四家照牌譜出牌，並與原牌譜的點數比對。

use crate::{GameRecord, HandRecord, Step};
use mj_core::Pai;
use mj_engine::{
    Action, Event, GameConfig, GameResult, LIVE_WALL, Player, PlayerView, Seat, Snapshot,
    capture_snapshot, full_set, run_game_with,
};
use std::cell::RefCell;
use std::rc::Rc;

/// 由配牌、摸牌順序與寶牌指示牌還原一局的牌山（引擎的排列方式，見 `mj_engine::Wall`）。
///
/// 配牌從莊家起每人連續 13 張，接著是依序摸的牌；開槓後的下一次摸牌放在嶺上牌的位置。
/// 沒有用到的位置（剩下的牌山、沒翻開的指示牌）用其餘的牌依序補滿。
pub fn build_wall(hand: &HandRecord, red_fives: u8) -> Result<Vec<Pai>, String> {
    let mut wall: Vec<Option<Pai>> = vec![None; full_set(red_fives).len()];
    let dealer = usize::from(hand.kyoku);
    for i in 0..4 {
        let h = &hand.haipai[(dealer + i) % 4];
        if h.len() != 13 {
            return Err(format!("座位 {} 的配牌有 {} 張", (dealer + i) % 4, h.len()));
        }
        for (j, &p) in h.iter().enumerate() {
            wall[i * 13 + j] = Some(p);
        }
    }
    let (mut live, mut kans, mut kan_pending) = (52, 0, false);
    for step in &hand.steps {
        match step {
            Step::Draw { pai, .. } if kan_pending => {
                if kans >= 4 {
                    return Err("超過四次嶺上摸牌".into());
                }
                wall[LIVE_WALL + kans] = Some(*pai);
                kans += 1;
                kan_pending = false;
            }
            Step::Draw { pai, .. } => {
                if live >= LIVE_WALL - kans {
                    return Err("摸牌次數超過牌山張數".into());
                }
                wall[live] = Some(*pai);
                live += 1;
            }
            Step::Act { action, .. } => {
                if matches!(
                    action,
                    Action::Ankan { .. } | Action::Kakan { .. } | Action::Daiminkan
                ) {
                    kan_pending = true;
                }
            }
        }
    }
    for (i, p) in hand.dora_indicators.iter().take(5).enumerate() {
        wall[LIVE_WALL + 4 + i] = Some(*p);
    }
    for (i, p) in hand.ura_indicators.iter().take(5).enumerate() {
        wall[LIVE_WALL + 9 + i] = Some(*p);
    }
    // 其餘的牌 = 全部的牌 − 已放入的牌。
    let mut rest = full_set(red_fives);
    for p in wall.iter().flatten() {
        let i = rest
            .iter()
            .position(|r| r == p)
            .ok_or_else(|| format!("牌 {p} 出現超過 4 張（或赤五規則不符）"))?;
        rest.swap_remove(i);
    }
    rest.sort();
    let mut rest = rest.into_iter();
    Ok(wall
        .into_iter()
        .map(|p| p.unwrap_or_else(|| rest.next().expect("counts match")))
        .collect())
}

/// 牌譜中一次詢問的紀錄：當時的視角、合法動作與牌譜中的選擇。
#[derive(Debug, Clone)]
pub struct DecisionPoint {
    /// 被詢問的座位
    pub seat: Seat,
    /// 這個座位第幾次被詢問（從 0 開始；可用於 `mj_engine::capture_snapshot`）
    pub ask_index: u32,
    /// 這個座位當時已收到的事件數（= 完整事件紀錄中的位置）
    pub event_index: usize,
    /// 當時的視角
    pub view: PlayerView,
    /// 合法動作
    pub legal: Vec<Action>,
    /// 牌譜中的選擇（沒有記載的回應為 `Pass`）
    pub chosen: Action,
}

/// 四家共用的牌譜進度。
struct Cursor {
    /// 全部局的動作（不含摸牌），依時間排序
    acts: Vec<(usize, Seat, Action)>,
    /// 下一個要執行的動作
    pos: usize,
    /// 已被回應詢問用掉的動作（雙響時兩個榮和都在 `pos` 之後）
    used: Vec<bool>,
    /// 第一個不一致的地方
    error: Option<String>,
    /// 是否記錄決策點
    record: bool,
    points: Vec<DecisionPoint>,
}

impl Cursor {
    fn is_response(a: &Action) -> bool {
        matches!(
            a,
            Action::Chi { .. } | Action::Pon { .. } | Action::Daiminkan | Action::Ron
        )
    }

    /// 決定這次詢問的動作；與牌譜不一致時記下錯誤並回傳 `None`。
    fn next(&mut self, view: &PlayerView, legal: &[Action]) -> Option<Action> {
        let seat = view.me;
        if view.last_discard.is_some() {
            // 回應他家的打牌（或加槓）：緊接在後的回應動作中有這一家的就執行，否則跳過。
            let mut i = self.pos;
            while let Some((_, s, a)) = self.acts.get(i) {
                if !Self::is_response(a) {
                    break;
                }
                if *s == seat && !self.used[i] {
                    self.used[i] = true;
                    return self.matching(seat, a.clone(), legal);
                }
                i += 1;
            }
            if legal.contains(&Action::Pass) {
                return Some(Action::Pass);
            }
            self.fail(format!("座位 {seat} 必須回應，但牌譜中沒有對應的動作"));
            return None;
        }
        // 自己的回合：先跳過已用掉的回應。
        while self.pos < self.acts.len() && self.used[self.pos] {
            self.pos += 1;
        }
        let Some((hand, s, a)) = self.acts.get(self.pos).cloned() else {
            self.fail(format!("牌譜已結束，但引擎還在詢問座位 {seat}"));
            return None;
        };
        if s != seat || Self::is_response(&a) {
            self.fail(format!(
                "第 {hand} 局：引擎詢問座位 {seat}，但牌譜的下一個動作是座位 {s} 的 {a:?}"
            ));
            return None;
        }
        self.used[self.pos] = true;
        self.pos += 1;
        self.matching(seat, a, legal)
    }

    /// 在合法動作中找出與牌譜相同的動作（吃碰的兩張牌不計順序）。
    fn matching(&mut self, seat: Seat, a: Action, legal: &[Action]) -> Option<Action> {
        let sorted = |c: &[Pai; 2]| {
            let mut c = *c;
            c.sort();
            c
        };
        let found = legal.iter().find(|l| match (l, &a) {
            (Action::Chi { consumed: x }, Action::Chi { consumed: y })
            | (Action::Pon { consumed: x }, Action::Pon { consumed: y }) => sorted(x) == sorted(y),
            (l, a) => l == &a,
        });
        if found.is_none() {
            self.fail(format!(
                "座位 {seat} 的動作 {a:?} 不合法；合法動作：{legal:?}"
            ));
        }
        found.cloned()
    }

    fn fail(&mut self, msg: String) {
        if self.error.is_none() {
            self.error = Some(msg);
        }
    }
}

/// 照牌譜出牌的玩家。可以附帶一個「旁觀者」：收到同樣的事件與詢問（例如策略在做預測），但動作照牌譜。
pub struct ScriptedPlayer<'a> {
    cursor: Rc<RefCell<Cursor>>,
    shadow: Option<&'a mut dyn Player>,
    events: usize,
    asks: u32,
}

impl Player for ScriptedPlayer<'_> {
    fn on_event(&mut self, me: Seat, event: &Event) {
        self.events += 1;
        if let Some(s) = self.shadow.as_mut() {
            s.on_event(me, event);
        }
    }

    fn act(&mut self, view: &PlayerView, legal: &[Action]) -> Action {
        if let Some(s) = self.shadow.as_mut() {
            s.act(view, legal);
        }
        let mut c = self.cursor.borrow_mut();
        let chosen = c.next(view, legal);
        if c.record {
            c.points.push(DecisionPoint {
                seat: view.me,
                ask_index: self.asks,
                event_index: self.events,
                view: view.clone(),
                legal: legal.to_vec(),
                chosen: chosen.clone().unwrap_or(Action::Pass),
            });
        }
        self.asks += 1;
        // 不一致時回傳一定不合法的動作，讓引擎停止（自己的回合不會有榮和，回應時不會有自摸）。
        chosen.unwrap_or_else(|| {
            if legal.contains(&Action::Ron) {
                Action::Tsumo
            } else {
                Action::Ron
            }
        })
    }
}

/// 重播的結果。
#[derive(Debug, Clone)]
pub struct Replayed {
    /// 重播使用的設定（含各局牌山，可再用於快照與反事實模擬）
    pub cfg: GameConfig,
    /// 引擎的對局結果（`log` 為完整事件紀錄）
    pub result: GameResult,
    /// 每次詢問的紀錄（`record_points` 為 true 時）
    pub points: Vec<DecisionPoint>,
}

/// 重播設定：各局牌山與手數。
pub fn config(rec: &GameRecord) -> Result<GameConfig, String> {
    let walls = rec
        .hands
        .iter()
        .enumerate()
        .map(|(i, h)| build_wall(h, rec.rules.red_fives).map_err(|e| format!("第 {i} 局：{e}")))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(GameConfig {
        rules: rec.rules.clone(),
        seed: 0,
        preset_walls: walls,
        max_hands: Some(rec.hands.len() as u32),
    })
}

/// 用引擎重播整場牌譜並驗證。`shadows` = 各座位的旁觀者（例如要做預測的策略）。
pub fn replay(
    rec: &GameRecord,
    shadows: [Option<&mut dyn Player>; 4],
    record_points: bool,
) -> Result<Replayed, String> {
    let cfg = config(rec)?;
    let cursor = Rc::new(RefCell::new(Cursor::new(rec, record_points)));
    let mut players = shadows.map(|shadow| ScriptedPlayer {
        cursor: cursor.clone(),
        shadow,
        events: 0,
        asks: 0,
    });
    let refs = players.each_mut().map(|p| p as &mut dyn Player);
    let run = run_game_with(&cfg, refs, true);
    drop(players);
    let c = Rc::try_unwrap(cursor)
        .ok()
        .expect("players dropped")
        .into_inner();
    if let Some(e) = c.error {
        return Err(e);
    }
    let result = run.map_err(|e| format!("引擎錯誤：{e}"))?;
    if let Some(i) = c.used.iter().position(|u| !u) {
        let (hand, seat, a) = &c.acts[i];
        return Err(format!(
            "第 {hand} 局：牌譜中座位 {seat} 的 {a:?} 沒有被執行"
        ));
    }
    verify(rec, &result)?;
    Ok(Replayed {
        cfg,
        result,
        points: c.points,
    })
}

/// 照牌譜重播到「即將第 `ask_index` 次詢問 `seat`」時，保存局面快照（復盤的反事實評估用）。
/// 旁觀者會收到快照點之前的所有事件與詢問，所以可以直接當作從快照繼續打的玩家。
/// 回傳（重播設定, 快照）；快照點不支援（例如搶槓）時為 `None`。
pub fn capture(
    rec: &GameRecord,
    shadows: [Option<&mut dyn Player>; 4],
    seat: Seat,
    ask_index: u32,
) -> Result<Option<(GameConfig, Snapshot)>, String> {
    let cfg = config(rec)?;
    let cursor = Rc::new(RefCell::new(Cursor::new(rec, false)));
    let mut players = shadows.map(|shadow| ScriptedPlayer {
        cursor: cursor.clone(),
        shadow,
        events: 0,
        asks: 0,
    });
    let refs = players.each_mut().map(|p| p as &mut dyn Player);
    let snap = capture_snapshot(&cfg, refs, seat, ask_index);
    drop(players);
    let c = Rc::try_unwrap(cursor)
        .ok()
        .expect("players dropped")
        .into_inner();
    if let Some(e) = c.error {
        return Err(e);
    }
    let snap = snap.map_err(|e| format!("引擎錯誤：{e}"))?;
    Ok(snap.map(|s| (cfg, s)))
}

impl Cursor {
    fn new(rec: &GameRecord, record: bool) -> Self {
        let acts: Vec<(usize, Seat, Action)> = rec
            .hands
            .iter()
            .enumerate()
            .flat_map(|(i, h)| {
                h.steps.iter().filter_map(move |s| match s {
                    Step::Act { seat, action } => Some((i, *seat, action.clone())),
                    Step::Draw { .. } => None,
                })
            })
            .collect();
        Cursor {
            used: vec![false; acts.len()],
            acts,
            pos: 0,
            error: None,
            record,
            points: Vec::new(),
        }
    }
}

/// 比對引擎的結果與原牌譜：每局開始的局數、本場、供託、點數，每局結束與終局的點數。
pub fn verify(rec: &GameRecord, result: &GameResult) -> Result<(), String> {
    let mut hand = 0;
    for e in &result.log {
        match e {
            Event::StartKyoku {
                round_wind,
                kyoku,
                honba,
                riichi_sticks,
                scores,
                ..
            } => {
                let h = rec
                    .hands
                    .get(hand)
                    .ok_or_else(|| format!("引擎打了超過牌譜的 {} 局", rec.hands.len()))?;
                let got = (*round_wind, *kyoku, *honba, *riichi_sticks, *scores);
                let want = (h.round_wind, h.kyoku, h.honba, h.riichi_sticks, h.scores);
                if got != want {
                    return Err(format!(
                        "第 {hand} 局開始時不一致：引擎 {got:?}，牌譜 {want:?}"
                    ));
                }
            }
            Event::EndKyoku { scores } => {
                if let Some(want) = rec.hands[hand].end_scores
                    && want != *scores
                {
                    return Err(format!(
                        "第 {hand} 局結束點數不一致：引擎 {scores:?}，牌譜 {want:?}"
                    ));
                }
                hand += 1;
            }
            Event::EndGame { scores } => {
                if let Some(want) = rec.final_scores
                    && want != *scores
                {
                    return Err(format!("終局點數不一致：引擎 {scores:?}，牌譜 {want:?}"));
                }
            }
            _ => {}
        }
    }
    if hand != rec.hands.len() {
        return Err(format!(
            "引擎只打了 {hand} 局，牌譜有 {} 局（終局判定不同？）",
            rec.hands.len()
        ));
    }
    Ok(())
}
