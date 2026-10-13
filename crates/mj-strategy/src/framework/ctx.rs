//! 決策情境 `Ctx`：每次呼叫 decision 函式時傳入，提供這次決策需要的一切。
//!
//! - `ctx.view`：玩家看得到的全部牌局資訊（手牌、牌河、副露、點數……）
//! - 這次決策的選項：`discard_candidates()`、`call_options()`、`kan_options()`……
//! - 常用查詢：`shanten()`、`after_discard()`、`after_call()`……（結果在同一次決策內快取）
//! - `note()`：寫下「為什麼這樣判斷」，復盤時顯示給玩家看
//! - `claim()`：記錄預測，供預測準確度評估

use super::choice::{CallChoice, DecisionKind, KanChoice, WinKind};
use super::claim::{Claim, ClaimRecord, TargetSituation};
use super::param::{Param, ParamSet};
use mj_core::{Pai, Tile, TileCounts};
use mj_engine::{Action, PlayerView, Seat};
use mj_tools::RemainingEstimate;
use mj_tools::after::{AfterEval, evaluate_after};
use mj_tools::table::Standing;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

/// 決策情境。
pub struct Ctx<'a> {
    /// 玩家看得到的全部資訊
    pub view: &'a PlayerView,
    /// 這次是哪一種決策
    pub kind: DecisionKind,
    pub(crate) discards: Vec<Pai>,
    pub(crate) calls: Vec<CallChoice>,
    pub(crate) kans: Vec<KanChoice>,
    pub(crate) win: Option<WinKind>,
    pub(crate) riichi_discard: Option<Pai>,
    pub(crate) tedashi: Option<(Pai, bool)>,
    pub(crate) event_index: usize,
    params: Arc<ParamSet>,
    remaining: RemainingEstimate,
    after: RefCell<HashMap<Action, AfterEval>>,
    notes: RefCell<Vec<String>>,
    claims: RefCell<Vec<ClaimRecord>>,
}

impl<'a> Ctx<'a> {
    /// 建立情境（框架內部使用；測試時也可以直接建立再用 `with_*` 設定選項）。
    pub fn new(view: &'a PlayerView, kind: DecisionKind) -> Self {
        Ctx {
            view,
            kind,
            discards: Vec::new(),
            calls: Vec::new(),
            kans: Vec::new(),
            win: None,
            riichi_discard: None,
            tedashi: None,
            event_index: 0,
            params: Arc::default(),
            remaining: mj_tools::visible::remaining(view),
            after: RefCell::default(),
            notes: RefCell::default(),
            claims: RefCell::default(),
        }
    }

    /// 設定參數覆寫值（框架依策略的設定傳入；測試時也可以用來試不同的值）。
    pub fn with_params(mut self, p: Arc<ParamSet>) -> Self {
        self.params = p;
        self
    }

    /// 以另一個牌桌（例如 `tools::river::view_after_discard` 做出的假想牌桌）建立同種類、同參數的情境，
    /// 用來「換個角度想」：例如把自己的讀牌預測用在自己身上，看別人眼中的我。
    /// 在新情境寫的理由與預測不會被記錄。
    pub fn imagine<'b>(&self, view: &'b PlayerView) -> Ctx<'b> {
        Ctx::new(view, self.kind).with_params(self.params.clone())
    }

    /// 可調參數的值：有覆寫用覆寫值，否則用程式中的預設值（見 `framework::param`）。
    pub fn param(&self, p: &Param) -> f64 {
        self.params.get(p)
    }

    /// 設定何切的候選牌（測試用）。
    pub fn with_discards(mut self, d: Vec<Pai>) -> Self {
        self.discards = d;
        self
    }

    /// 設定鳴牌選項（測試用）。
    pub fn with_calls(mut self, c: Vec<CallChoice>) -> Self {
        self.calls = c;
        self
    }

    /// 設定和牌方式（測試用）。
    pub fn with_win(mut self, w: WinKind) -> Self {
        self.win = Some(w);
        self
    }

    /// 設定立直時要打的牌（測試用）。
    pub fn with_riichi_discard(mut self, p: Pai) -> Self {
        self.riichi_discard = Some(p);
        self
    }

    /// 設定手切摸切決策的牌與是否立直（測試用）。
    pub fn with_tedashi(mut self, pai: Pai, riichi: bool) -> Self {
        self.tedashi = Some((pai, riichi));
        self
    }

    // ------------------------------------------------------------ 這次決策的選項

    /// 何切：可以打的牌（已排除食替禁止的牌；同種牌只列一次，赤五與普通 5 分開）。
    pub fn discard_candidates(&self) -> &[Pai] {
        &self.discards
    }

    /// 鳴牌：可以選的鳴法（一定包含「不鳴」）。
    pub fn call_options(&self) -> &[CallChoice] {
        &self.calls
    }

    /// 開槓：可以槓的選項。
    pub fn kan_options(&self) -> &[KanChoice] {
        &self.kans
    }

    /// 和牌決策：這次是自摸、榮和還是搶槓。
    pub fn win_kind(&self) -> Option<WinKind> {
        self.win
    }

    /// 立直決策：已經決定要打出的那張牌。
    pub fn riichi_discard(&self) -> Option<Pai> {
        self.riichi_discard
    }

    /// 手切摸切決策：要打出的牌。
    pub fn tedashi_pai(&self) -> Option<Pai> {
        self.tedashi.map(|(p, _)| p)
    }

    /// 手切摸切決策：這張牌是否為立直宣言牌。
    pub fn declaring_riichi(&self) -> bool {
        self.tedashi.is_some_and(|(_, r)| r)
    }

    /// 鳴牌、榮和決策：他家打出（或加槓）的那張牌，與打出者的座位。
    pub fn offered(&self) -> Option<(Seat, Pai)> {
        self.view.last_discard
    }

    // ------------------------------------------------------------ 常用查詢

    /// 自己的座位。
    pub fn me(&self) -> Seat {
        self.view.me
    }

    /// 自己的門前手牌（牌數表）。
    pub fn hand(&self) -> TileCounts {
        self.view.hand
    }

    /// 剩餘牌估計（4 − 看得到的張數）。
    pub fn remaining(&self) -> &RemainingEstimate {
        &self.remaining
    }

    /// 名次與點差。
    pub fn standing(&self) -> Standing {
        mj_tools::table::standing(self.view)
    }

    /// 目前的向聽數（摸牌後取打出最好一張後的值）。
    pub fn shanten(&self) -> i8 {
        mj_tools::hand::my_shanten(self.view)
    }

    /// 執行某個動作之後的手牌（向聽、進張），同一次決策內快取。
    pub fn after(&self, action: &Action) -> AfterEval {
        if let Some(e) = self.after.borrow().get(action) {
            return *e;
        }
        let e = evaluate_after(self.view, &self.remaining, action);
        self.after.borrow_mut().insert(action.clone(), e);
        e
    }

    /// 打出 `pai` 之後的手牌。
    pub fn after_discard(&self, pai: Pai) -> AfterEval {
        self.after(&Action::Discard {
            pai,
            tsumogiri: false,
        })
    }

    /// 這樣鳴牌（再打出最好的一張）之後的手牌；`Pass` = 不鳴時的手牌。
    pub fn after_call(&self, c: CallChoice) -> AfterEval {
        self.after(&call_action(c))
    }

    /// 某種牌對自己而言值幾張寶牌（不含赤五）。
    pub fn dora_value(&self, tile: Tile) -> u8 {
        mj_tools::dora::dora_value(self.view, tile)
    }

    // ------------------------------------------------------------ 紀錄

    /// 寫下判斷理由（復盤時顯示）。
    pub fn note(&self, text: impl Into<String>) {
        self.notes.borrow_mut().push(text.into());
    }

    /// 記錄一個預測。`source` = 預測函式的名稱，例如 `"predict::tenpai"`。
    pub fn claim(&self, source: &str, claim: Claim) {
        let t = usize::from(claim.target().unwrap_or(self.view.me));
        let target = TargetSituation {
            turn: self.view.rivers[t].len() as u8,
            open_melds: self.view.melds[t].iter().filter(|m| m.is_open()).count() as u8,
            riichi: self.view.riichi[t],
        };
        self.claims.borrow_mut().push(ClaimRecord {
            source: source.to_string(),
            claim,
            me: self.view.me,
            at_event: self.event_index,
            target,
        });
    }

    /// 取出本次決策寫下的理由與預測（框架內部使用）。
    pub(crate) fn take_records(&self) -> (Vec<String>, Vec<ClaimRecord>) {
        (self.notes.take(), self.claims.take())
    }

    /// 本次決策目前寫下的理由（測試用）。
    pub fn notes(&self) -> Vec<String> {
        self.notes.borrow().clone()
    }
}

/// 鳴牌選項對應的引擎動作。
pub fn call_action(c: CallChoice) -> Action {
    match c {
        CallChoice::Pass => Action::Pass,
        CallChoice::Chi(consumed) => Action::Chi { consumed },
        CallChoice::Pon(consumed) => Action::Pon { consumed },
        CallChoice::Daiminkan => Action::Daiminkan,
    }
}

/// 開槓選項對應的引擎動作。
pub fn kan_action(k: KanChoice) -> Action {
    match k {
        KanChoice::Ankan(tile) => Action::Ankan { tile },
        KanChoice::Kakan(pai) => Action::Kakan { pai },
    }
}
