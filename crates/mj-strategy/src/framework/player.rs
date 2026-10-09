//! 依策略手冊打牌的玩家（框架核心，不需要策略編輯者修改）。
//!
//! 引擎每次只問「要做哪個動作」；這裡把它拆成依人類思考順序的幾個決策：
//!
//! - 自己的回合：九種九牌 → 和牌與否 → 開槓 → 何切 → 立直與否
//! - 他家打牌時：和牌與否 → 鳴牌
//!
//! 每個決策都可以記錄下來（選項、選擇、理由、預測），供評估與復盤使用。

use super::book::Book;
use super::choice::{CallChoice, DecisionKind, KanChoice, WinKind};
use super::claim::ClaimRecord;
use super::ctx::{Ctx, call_action, kan_action};
use super::param::{Param, ParamSet};
use mj_core::Pai;
use mj_engine::{Action, Event, Player, PlayerView, Seat};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// 決策當時的情境（分組統計用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Situation {
    /// 已立直的對手人數
    pub riichi_opponents: u8,
    /// 自己的向聽數（摸牌後取打出最好一張後的值）
    pub shanten: i8,
    /// 自己打過幾張（巡目）
    pub turn: u8,
    /// 目前名次，0 = 第一名
    pub rank: u8,
}

impl Situation {
    /// 由玩家視角計算。
    pub fn of(view: &PlayerView) -> Self {
        Situation {
            riichi_opponents: mj_tools::table::riichi_opponents(view).len() as u8,
            shanten: mj_tools::hand::my_shanten(view),
            turn: view.rivers[usize::from(view.me)].len() as u8,
            rank: mj_tools::table::standing(view).rank,
        }
    }
}

/// 一次決策的紀錄。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionRecord {
    /// 對局中的第幾局（從 0 開始，連莊也算新的一局）
    pub hand_index: u32,
    /// 這個玩家在本場對局中第幾次被引擎詢問（同一次詢問可能包含好幾個決策）
    pub act_index: u32,
    /// 決策時已經發生了幾個事件
    pub event_index: usize,
    /// 決策種類
    pub kind: DecisionKind,
    /// 所有選項（文字）
    pub options: Vec<String>,
    /// 選了第幾個選項
    pub chosen: usize,
    /// 決策函式寫下的理由
    pub notes: Vec<String>,
    /// 決策過程中做的預測
    pub claims: Vec<ClaimRecord>,
    /// 決策函式回傳了不在選項中的結果（已改用預設選項）
    pub invalid: bool,
    /// 這個決策是被強制指定的（反事實評估用）
    pub forced: bool,
    /// 決策當時的情境
    pub situation: Situation,
}

/// 強制指定某次決策的選項（反事實評估：「如果當時選另一個會怎樣」）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Force {
    pub act_index: u32,
    pub kind: DecisionKind,
    pub option: usize,
}

/// 依策略手冊打牌的玩家。
#[derive(Clone)]
pub struct BookPlayer<M> {
    book: Arc<Book<M>>,
    mem: M,
    record: bool,
    force: Option<Force>,
    records: Vec<DecisionRecord>,
    /// 全部預測紀錄（決策中做的與常駐預測），供預測評估使用
    claims: Vec<ClaimRecord>,
    events_seen: usize,
    hand_index: u32,
    act_index: u32,
    /// 可調參數的覆寫值
    params: Arc<ParamSet>,
}

impl<M: Default> BookPlayer<M> {
    pub fn new(book: Arc<Book<M>>, record: bool, force: Option<Force>) -> Self {
        BookPlayer {
            book,
            mem: M::default(),
            record,
            force,
            records: Vec::new(),
            claims: Vec::new(),
            events_seen: 0,
            hand_index: 0,
            act_index: 0,
            params: Arc::default(),
        }
    }

    /// 使用這組參數覆寫值。
    pub fn with_params(mut self, params: Arc<ParamSet>) -> Self {
        self.params = params;
        self
    }

    /// 建立一次決策的情境（帶上參數覆寫值）。
    fn ctx<'v>(&self, view: &'v PlayerView, kind: DecisionKind) -> Ctx<'v> {
        Ctx::new(view, kind).with_params(self.params.clone())
    }

    /// 取出目前累積的決策紀錄。
    pub fn take_records(&mut self) -> Vec<DecisionRecord> {
        std::mem::take(&mut self.records)
    }

    /// 取出目前累積的全部預測紀錄（決策中做的與常駐預測）。
    pub fn take_claims(&mut self) -> Vec<ClaimRecord> {
        std::mem::take(&mut self.claims)
    }

    /// 執行常駐預測（只在需要記錄時執行，因為它不影響動作）。
    fn run_each_turn(&mut self, view: &PlayerView) {
        let Some(f) = self.book.each_turn else {
            return;
        };
        if !self.record {
            return;
        }
        let mut ctx = self.ctx(view, DecisionKind::Predict);
        ctx.event_index = self.events_seen;
        f(&ctx, &mut self.mem);
        let (_, claims) = ctx.take_records();
        self.claims.extend(claims);
    }

    /// 執行一個決策：呼叫決策函式（或使用強制指定的選項），檢查結果在選項中，並記錄。
    fn step<T: PartialEq + Clone>(
        &mut self,
        mut ctx: Ctx,
        options: &[T],
        labels: Vec<String>,
        f: Option<fn(&Ctx, &mut M) -> T>,
        default: T,
    ) -> T {
        ctx.event_index = self.events_seen;
        let kind = ctx.kind;
        let forced = self.force.filter(|f| {
            f.act_index == self.act_index && f.kind == kind && f.option < options.len()
        });
        let (idx, invalid) = match forced {
            Some(f) => (f.option, false),
            None => {
                let choice = match f {
                    Some(f) => f(&ctx, &mut self.mem),
                    None => default.clone(),
                };
                match options.iter().position(|o| *o == choice) {
                    Some(i) => (i, false),
                    None => (
                        options.iter().position(|o| *o == default).unwrap_or(0),
                        true,
                    ),
                }
            }
        };
        if self.record {
            let (notes, claims) = ctx.take_records();
            self.claims.extend(claims.iter().cloned());
            self.records.push(DecisionRecord {
                hand_index: self.hand_index.saturating_sub(1),
                act_index: self.act_index,
                event_index: self.events_seen,
                kind,
                options: labels,
                chosen: idx,
                notes,
                claims,
                invalid,
                forced: forced.is_some(),
                situation: Situation::of(ctx.view),
            });
        }
        options[idx].clone()
    }

    /// 是／否決策（選項順序固定為 [否, 是]）。
    fn yes_no(
        &mut self,
        ctx: Ctx,
        no: &str,
        yes: &str,
        f: Option<fn(&Ctx, &mut M) -> bool>,
        default: bool,
    ) -> bool {
        self.step(ctx, &[false, true], vec![no.into(), yes.into()], f, default)
    }

    /// 他家打牌（或加槓）時的回應。
    fn respond(&mut self, view: &PlayerView, legal: &[Action]) -> Action {
        if legal.contains(&Action::Ron) {
            let ctx = self.ctx(view, DecisionKind::Win).with_win(
                if legal.len() == 2 && is_kan_robbery(view, legal) {
                    WinKind::Chankan
                } else {
                    WinKind::Ron
                },
            );
            if self.yes_no(ctx, "不和", "榮和", self.book.win, true) {
                return Action::Ron;
            }
        }
        let calls = call_options(legal);
        if calls.len() == 1 {
            return Action::Pass;
        }
        let labels = calls.iter().map(|c| c.to_string()).collect();
        let mut ctx = self.ctx(view, DecisionKind::Call);
        ctx.calls = calls.clone();
        let c = self.step(ctx, &calls, labels, self.book.call, CallChoice::Pass);
        call_action(c)
    }

    /// 自己的回合。
    fn own_turn(&mut self, view: &PlayerView, legal: &[Action]) -> Action {
        self.run_each_turn(view);
        if legal.contains(&Action::KyuushuKyuuhai) {
            let ctx = self.ctx(view, DecisionKind::Kyuushu);
            if self.yes_no(ctx, "繼續打", "流局", self.book.kyuushu, false) {
                return Action::KyuushuKyuuhai;
            }
        }
        if legal.contains(&Action::Tsumo) {
            let ctx = self.ctx(view, DecisionKind::Win).with_win(WinKind::Tsumo);
            if self.yes_no(ctx, "不和", "自摸", self.book.win, true) {
                return Action::Tsumo;
            }
        }
        let kans = kan_choices(legal);
        if !kans.is_empty() {
            let options = kan_options(legal);
            let labels = options
                .iter()
                .map(|o| o.map_or("不槓".to_string(), |k| k.to_string()))
                .collect();
            let mut ctx = self.ctx(view, DecisionKind::Kan);
            ctx.kans = kans;
            if let Some(k) = self.step(ctx, &options, labels, self.book.kan, None) {
                return kan_action(k);
            }
        }

        let candidates = discard_candidates(legal);
        let pai = if candidates.len() == 1 {
            candidates[0]
        } else {
            let labels = candidates.iter().map(|p| format!("打 {p}")).collect();
            let mut ctx = self.ctx(view, DecisionKind::Discard);
            ctx.discards = candidates.clone();
            let f = self.book.discard.expect("策略必須登記何切（discard）決策");
            self.step(ctx, &candidates, labels, Some(f), candidates[0])
        };
        // 打剛摸到的牌時用摸切，否則手切。
        let tsumogiri = view.drawn == Some(pai)
            && legal.contains(&Action::Discard {
                pai,
                tsumogiri: true,
            });
        let tsumogiri = tsumogiri
            || !legal.contains(&Action::Discard {
                pai,
                tsumogiri: false,
            });
        if legal.contains(&Action::Riichi { pai, tsumogiri }) {
            let ctx = self
                .ctx(view, DecisionKind::Riichi)
                .with_riichi_discard(pai);
            if self.yes_no(ctx, "默聽", "立直", self.book.riichi, true) {
                return Action::Riichi { pai, tsumogiri };
            }
        }
        Action::Discard { pai, tsumogiri }
    }
}

/// 鳴牌決策的選項（第一個是不鳴）。
fn call_options(legal: &[Action]) -> Vec<CallChoice> {
    let mut calls = vec![CallChoice::Pass];
    for a in legal {
        match a {
            Action::Chi { consumed } => calls.push(CallChoice::Chi(*consumed)),
            Action::Pon { consumed } => calls.push(CallChoice::Pon(*consumed)),
            Action::Daiminkan => calls.push(CallChoice::Daiminkan),
            _ => {}
        }
    }
    calls
}

/// 可以開的槓。
fn kan_choices(legal: &[Action]) -> Vec<KanChoice> {
    legal
        .iter()
        .filter_map(|a| match a {
            Action::Ankan { tile } => Some(KanChoice::Ankan(*tile)),
            Action::Kakan { pai } => Some(KanChoice::Kakan(*pai)),
            _ => None,
        })
        .collect()
}

/// 開槓決策的選項（第一個是不槓）。
fn kan_options(legal: &[Action]) -> Vec<Option<KanChoice>> {
    let mut options: Vec<Option<KanChoice>> = vec![None];
    options.extend(kan_choices(legal).into_iter().map(Some));
    options
}

/// 何切的候選牌（同一張牌的手切與摸切算一個）。
fn discard_candidates(legal: &[Action]) -> Vec<Pai> {
    let mut candidates: Vec<Pai> = Vec::new();
    for a in legal {
        if let Action::Discard { pai, .. } = a
            && !candidates.contains(pai)
        {
            candidates.push(*pai);
        }
    }
    candidates
}

/// 把一個實際動作（例如牌譜中玩家的選擇）對應到 `kind` 這種決策的選項編號；選項的列法與 [`BookPlayer`] 相同。
///
/// - `legal`：那次詢問的合法動作；
/// - `riichi_pai`：立直決策時，策略決定要打的牌（立直決策是「打這張時要不要立直」）。
///
/// 動作走的是另一條路（例如評估何切，但玩家自摸了；評估立直，但玩家打的是別張）時回傳 `None`：
/// 這個決策無法比較，差異會出現在更早的決策上。
pub fn option_of_action(
    kind: DecisionKind,
    legal: &[Action],
    action: &Action,
    riichi_pai: Option<Pai>,
) -> Option<usize> {
    let yes_no = |yes: bool| Some(usize::from(yes));
    match kind {
        DecisionKind::Kyuushu => yes_no(*action == Action::KyuushuKyuuhai),
        DecisionKind::Win => match action {
            Action::KyuushuKyuuhai => None,
            a => yes_no(matches!(a, Action::Tsumo | Action::Ron)),
        },
        DecisionKind::Kan => match action {
            Action::Ankan { tile } => kan_options(legal)
                .iter()
                .position(|o| *o == Some(KanChoice::Ankan(*tile))),
            Action::Kakan { pai } => kan_options(legal)
                .iter()
                .position(|o| *o == Some(KanChoice::Kakan(*pai))),
            Action::Discard { .. } | Action::Riichi { .. } => Some(0),
            _ => None,
        },
        DecisionKind::Discard => match action {
            Action::Discard { pai, .. } | Action::Riichi { pai, .. } => {
                discard_candidates(legal).iter().position(|p| p == pai)
            }
            _ => None,
        },
        DecisionKind::Riichi => match action {
            Action::Riichi { pai, .. } if Some(*pai) == riichi_pai => Some(1),
            Action::Discard { pai, .. } if Some(*pai) == riichi_pai => Some(0),
            _ => None,
        },
        DecisionKind::Call => {
            let sorted = |c: &[Pai; 2]| {
                let mut c = *c;
                c.sort();
                c
            };
            let choice = match action {
                Action::Pass => CallChoice::Pass,
                Action::Chi { consumed } => CallChoice::Chi(sorted(consumed)),
                Action::Pon { consumed } => CallChoice::Pon(sorted(consumed)),
                Action::Daiminkan => CallChoice::Daiminkan,
                _ => return None,
            };
            call_options(legal).iter().position(|c| match (c, &choice) {
                (CallChoice::Chi(x), CallChoice::Chi(y))
                | (CallChoice::Pon(x), CallChoice::Pon(y)) => sorted(x) == *y,
                (a, b) => a == b,
            })
        }
        DecisionKind::Predict => None,
    }
}

/// 只能榮和或跳過、而且被打出的牌是加槓（自己沒有吃碰選項）時，視為搶槓。
fn is_kan_robbery(view: &PlayerView, legal: &[Action]) -> bool {
    let Some((seat, pai)) = view.last_discard else {
        return false;
    };
    legal.len() == 2
        && view.melds[usize::from(seat)]
            .iter()
            .any(|m| m.kind == mj_core::MeldKind::Pon && m.base() == pai.tile)
}

impl<M: Default + Send> Player for BookPlayer<M> {
    fn on_event(&mut self, me: Seat, event: &Event) {
        self.events_seen += 1;
        if let Event::StartKyoku { .. } = event {
            self.hand_index += 1;
        }
        if let Some(f) = self.book.observe {
            f(me, event, &mut self.mem);
        }
    }

    fn act(&mut self, view: &PlayerView, legal: &[Action]) -> Action {
        let action = if legal.contains(&Action::Pass) {
            self.respond(view, legal)
        } else {
            self.own_turn(view, legal)
        };
        self.act_index += 1;
        action
    }
}

/// 可以上桌的策略（由 `strategy-list/<規則>/<策略>/` 的每個策略目錄自動產生）。
pub trait Strategy: Send + Sync {
    /// 完整名稱「規則／策略」，例如 `tenhou-normal-4p-hanchan/push-fold`
    fn name(&self) -> &str;
    /// 規則名稱（`strategy-list` 下第一層目錄），例如 `tenhou-normal-4p-hanchan`
    fn rule(&self) -> &str {
        self.name().split('/').next().unwrap_or_default()
    }
    /// 策略說明文件（策略目錄的 `README.md` 全文，Markdown）
    fn readme(&self) -> &str;
    /// 標題（README 第一行的 `# 標題`）
    fn title(&self) -> &str {
        readme_title(self.readme())
    }
    /// 一段話的摘要（README 標題後的第一段），供選單、`mj strategies` 顯示
    fn summary(&self) -> &str {
        readme_summary(self.readme())
    }
    /// 建立一個玩家。`record` = 是否保留決策紀錄；`force` = 強制指定某次決策的選項。
    fn player(&self, record: bool, force: Option<Force>) -> Box<dyn StrategyPlayer>;
    /// 這個策略的所有可調參數（建置腳本從原始碼中找出的 `Param` 常數）。
    fn params(&self) -> &[Param];
    /// 目前的參數覆寫值（空 = 全部用預設值）。
    fn overrides(&self) -> &ParamSet;
    /// 同一個策略、換一組參數覆寫值（覆寫的名稱必須是這個策略的參數）。
    fn with_params(&self, overrides: ParamSet) -> Result<Box<dyn Strategy>, String>;
}

/// 策略玩家：可以上桌，也可以取出決策紀錄。
pub trait StrategyPlayer: Player + Send {
    /// 取出決策紀錄。
    fn take_records(&mut self) -> Vec<DecisionRecord>;
    /// 取出全部預測紀錄（決策中做的與常駐預測）。
    fn take_claims(&mut self) -> Vec<ClaimRecord>;
    /// 複製目前的狀態（對局記憶、詢問次數……）。反事實評估在快照點複製玩家後繼續打。
    fn clone_box(&self) -> Box<dyn StrategyPlayer>;
    /// 設定（或取消）強制指定的決策選項。
    fn set_force(&mut self, force: Option<Force>);
    /// 設定是否記錄決策。
    fn set_record(&mut self, record: bool);
}

impl<M: Default + Clone + Send + 'static> StrategyPlayer for BookPlayer<M> {
    fn take_records(&mut self) -> Vec<DecisionRecord> {
        BookPlayer::take_records(self)
    }

    fn take_claims(&mut self) -> Vec<ClaimRecord> {
        BookPlayer::take_claims(self)
    }

    fn clone_box(&self) -> Box<dyn StrategyPlayer> {
        Box::new(self.clone())
    }

    fn set_force(&mut self, force: Option<Force>) {
        self.force = force;
    }

    fn set_record(&mut self, record: bool) {
        self.record = record;
    }
}

/// README 的標題：第一個 `# ` 開頭的行。
fn readme_title(readme: &str) -> &str {
    readme
        .lines()
        .find_map(|l| l.strip_prefix("# "))
        .unwrap_or_default()
        .trim()
}

/// README 的摘要：標題之後的第一段（到空行為止）。
fn readme_summary(readme: &str) -> &str {
    let after_title = match readme.find("# ") {
        Some(i) => readme[i..].split_once('\n').map_or("", |(_, rest)| rest),
        None => readme,
    }
    .trim_start();
    after_title.split("\n\n").next().unwrap_or_default().trim()
}

/// 由策略手冊建立的策略。
pub struct BookStrategy<M> {
    name: String,
    readme: &'static str,
    book: Arc<Book<M>>,
    params: Arc<Vec<Param>>,
    overrides: Arc<ParamSet>,
}

impl<M: Default + Clone + Send + 'static> Strategy for BookStrategy<M> {
    fn name(&self) -> &str {
        &self.name
    }

    fn readme(&self) -> &str {
        self.readme
    }

    fn player(&self, record: bool, force: Option<Force>) -> Box<dyn StrategyPlayer> {
        Box::new(
            BookPlayer::new(self.book.clone(), record, force).with_params(self.overrides.clone()),
        )
    }

    fn params(&self) -> &[Param] {
        &self.params
    }

    fn overrides(&self) -> &ParamSet {
        &self.overrides
    }

    fn with_params(&self, overrides: ParamSet) -> Result<Box<dyn Strategy>, String> {
        overrides.check(&self.params)?;
        Ok(Box::new(BookStrategy {
            name: self.name.clone(),
            readme: self.readme,
            book: self.book.clone(),
            params: self.params.clone(),
            overrides: Arc::new(overrides),
        }))
    }
}

/// 用策略的 `register` 函式建立策略（由自動產生的程式碼呼叫）。
/// `name` = 「規則／策略」，`readme` = 策略目錄的 `README.md`，`params` = 策略中的可調參數。
pub fn book_strategy<M: Default + Clone + Send + 'static>(
    name: &str,
    readme: &'static str,
    register: fn(&mut Book<M>),
    params: Vec<Param>,
) -> Box<dyn Strategy> {
    let mut book = Book::default();
    register(&mut book);
    assert!(
        book.discard.is_some(),
        "策略 {name} 沒有登記何切（discard）決策"
    );
    for (i, p) in params.iter().enumerate() {
        assert!(
            params[..i].iter().all(|q| q.name != p.name),
            "策略 {name} 有兩個參數都叫 {}",
            p.name
        );
    }
    Box::new(BookStrategy {
        name: name.to_string(),
        readme,
        book: Arc::new(book),
        params: Arc::new(params),
        overrides: Arc::default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use mj_core::{TileCounts, parse_pais};

    fn view(hand: &str) -> PlayerView {
        let pais = parse_pais(hand).unwrap();
        PlayerView {
            me: 1,
            round_wind: 0,
            kyoku: 0,
            honba: 0,
            riichi_sticks: 0,
            scores: [25_000; 4],
            dora_indicators: vec![],
            hand: TileCounts::from_tiles(pais.iter().map(|p| p.tile)),
            drawn: pais.last().copied(),
            hand_pais: pais,
            melds: Default::default(),
            rivers: Default::default(),
            riichi: [false; 4],
            tiles_left_in_wall: 60,
            total_kyoku: 8,
            last_discard: None,
        }
    }

    fn p(s: &str) -> Pai {
        s.parse().unwrap()
    }

    /// 測試用手冊：何切永遠打 5z，不立直。
    fn book() -> Arc<Book<u32>> {
        let mut b: Book<u32> = Book::default();
        b.discard(|ctx, calls| {
            *calls += 1;
            ctx.note("固定打 5z");
            "5z".parse().unwrap()
        });
        b.riichi(|_, _| false);
        Arc::new(b)
    }

    fn legal(hand: &str) -> Vec<Action> {
        let mut seen = Vec::new();
        parse_pais(hand)
            .unwrap()
            .into_iter()
            .filter(|x| {
                let new = !seen.contains(x);
                seen.push(*x);
                new
            })
            .flat_map(|pai| {
                [
                    Action::Discard {
                        pai,
                        tsumogiri: false,
                    },
                    Action::Riichi {
                        pai,
                        tsumogiri: false,
                    },
                ]
            })
            .collect()
    }

    /// 何切 → 立直依序執行並記錄；立直決策回傳 false 所以默聽。
    #[test]
    fn discard_then_riichi_are_recorded() {
        let mut pl = BookPlayer::new(book(), true, None);
        let a = pl.act(&view("123m456p789s11225z"), &legal("123m456p789s11225z"));
        assert_eq!(
            a,
            Action::Discard {
                pai: p("5z"),
                tsumogiri: false
            }
        );
        let r = pl.take_records();
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].kind, DecisionKind::Discard);
        assert_eq!(r[0].notes, vec!["固定打 5z".to_string()]);
        assert_eq!(r[1].kind, DecisionKind::Riichi);
        assert_eq!(r[1].options[r[1].chosen], "默聽");
    }

    /// 決策函式回傳不在選項中的牌：改用預設選項並標記 invalid。
    #[test]
    fn invalid_choice_falls_back() {
        let mut pl = BookPlayer::new(book(), true, None);
        let hand = "123m456p789s11226z";
        let a = pl.act(&view(hand), &legal(hand));
        assert!(matches!(a, Action::Discard { .. } | Action::Riichi { .. }));
        assert!(pl.take_records()[0].invalid);
    }

    /// 強制指定選項：不呼叫決策函式，直接使用指定的選項。
    #[test]
    fn forced_option_overrides_decision() {
        let force = Force {
            act_index: 0,
            kind: DecisionKind::Riichi,
            option: 1,
        };
        let mut pl = BookPlayer::new(book(), true, Some(force));
        let a = pl.act(&view("123m456p789s11225z"), &legal("123m456p789s11225z"));
        assert_eq!(
            a,
            Action::Riichi {
                pai: p("5z"),
                tsumogiri: false
            }
        );
        assert!(pl.take_records()[1].forced);
        assert_eq!(pl.mem, 1);
    }

    /// 沒有登記和牌決策時預設和牌；沒有登記鳴牌決策時預設不鳴。
    #[test]
    fn defaults_for_unregistered_decisions() {
        let mut pl = BookPlayer::new(book(), false, None);
        let v = view("123m456p789s1122z");
        assert_eq!(pl.act(&v, &[Action::Ron, Action::Pass]), Action::Ron);
        let pon = Action::Pon {
            consumed: [p("1z"), p("1z")],
        };
        assert_eq!(pl.act(&v, &[pon, Action::Pass]), Action::Pass);
    }

    /// 常駐預測：每次自己的回合都執行、記錄預測，但不影響動作；不記錄時不執行。
    #[test]
    fn each_turn_predictions_are_recorded() {
        let mut b: Book<u32> = Book::default();
        b.discard(|_, _| "5z".parse().unwrap());
        b.riichi(|_, _| false);
        b.predict_each_turn(|ctx, n| {
            *n += 1;
            ctx.claim("test", crate::framework::Claim::Tenpai { seat: 2, p: 0.5 });
        });
        let b = Arc::new(b);
        let hand = "123m456p789s11225z";
        let mut pl = BookPlayer::new(b.clone(), true, None);
        let a = pl.act(&view(hand), &legal(hand));
        assert_eq!(
            a,
            Action::Discard {
                pai: p("5z"),
                tsumogiri: false
            }
        );
        assert_eq!(pl.mem, 1);
        let claims = pl.take_claims();
        assert_eq!(claims.len(), 1);
        assert_eq!(claims[0].source, "test");
        assert!(
            pl.take_records()
                .iter()
                .all(|r| r.kind != DecisionKind::Predict)
        );

        let mut quiet = BookPlayer::new(b, false, None);
        quiet.act(&view(hand), &legal(hand));
        assert_eq!(quiet.mem, 0);
    }

    #[test]
    fn readme_title_and_summary() {
        let readme = "# 押引\n\n第一段\n第二行\n\n## 特色\n- x\n";
        assert_eq!(readme_title(readme), "押引");
        assert_eq!(readme_summary(readme), "第一段\n第二行");
        assert_eq!(readme_summary("# 只有標題\n"), "");
    }

    /// 實際動作對應到決策選項：何切看牌、立直看是不是同一張牌、鳴牌不計兩張牌的順序、走另一條路時無法比較。
    #[test]
    fn option_of_action_maps_player_actions() {
        let d = |t: &str| Action::Discard {
            pai: p(t),
            tsumogiri: false,
        };
        let legal = vec![d("1m"), d("5z"), Action::Tsumo];
        assert_eq!(
            option_of_action(DecisionKind::Discard, &legal, &d("5z"), None),
            Some(1)
        );
        assert_eq!(
            option_of_action(DecisionKind::Discard, &legal, &Action::Tsumo, None),
            None
        );
        assert_eq!(
            option_of_action(DecisionKind::Win, &legal, &Action::Tsumo, None),
            Some(1)
        );
        assert_eq!(
            option_of_action(DecisionKind::Win, &legal, &d("1m"), None),
            Some(0)
        );
        let riichi = Action::Riichi {
            pai: p("1m"),
            tsumogiri: false,
        };
        assert_eq!(
            option_of_action(DecisionKind::Riichi, &legal, &riichi, Some(p("1m"))),
            Some(1)
        );
        assert_eq!(
            option_of_action(DecisionKind::Riichi, &legal, &d("1m"), Some(p("1m"))),
            Some(0)
        );
        // 策略要打 1m 立直，玩家打的是 5z：立直決策無法比較。
        assert_eq!(
            option_of_action(DecisionKind::Riichi, &legal, &d("5z"), Some(p("1m"))),
            None
        );
        let calls = vec![
            Action::Pass,
            Action::Chi {
                consumed: [p("2m"), p("3m")],
            },
            Action::Pon {
                consumed: [p("4m"), p("4m")],
            },
        ];
        let chi_reversed = Action::Chi {
            consumed: [p("3m"), p("2m")],
        };
        assert_eq!(
            option_of_action(DecisionKind::Call, &calls, &chi_reversed, None),
            Some(1)
        );
        assert_eq!(
            option_of_action(DecisionKind::Call, &calls, &Action::Pass, None),
            Some(0)
        );
    }
}
