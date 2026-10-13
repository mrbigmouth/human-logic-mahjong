//! 策略手冊（Book）：記錄「什麼情況呼叫哪一個 decision 函式」。
//! 策略的 `main.rs` 在 `register` 中填寫這本手冊。

use super::choice::{CallChoice, KanChoice};
use super::ctx::Ctx;
use mj_core::Pai;
use mj_engine::{Event, Seat};

/// 回答「是／否」的決策函式（九種九牌、和牌、立直、手切摸切）。
pub type DecideBool<M> = fn(&Ctx, &mut M) -> bool;
/// 開槓決策：`None` = 不槓。
pub type DecideKan<M> = fn(&Ctx, &mut M) -> Option<KanChoice>;
/// 何切決策：從 `ctx.discard_candidates()` 中選一張。
pub type DecideDiscard<M> = fn(&Ctx, &mut M) -> Pai;
/// 鳴牌決策：從 `ctx.call_options()` 中選一個（含不鳴）。
pub type DecideCall<M> = fn(&Ctx, &mut M) -> CallChoice;
/// 觀察事件、更新對局記憶（每個事件都會呼叫，`me` = 自己的座位）。
pub type Observe<M> = fn(Seat, &Event, &mut M);

/// 策略手冊。`M` = 這個策略的對局記憶型別（`memory.rs` 的 `Memory`）。
pub struct Book<M> {
    pub(crate) kyuushu: Option<DecideBool<M>>,
    pub(crate) win: Option<DecideBool<M>>,
    pub(crate) kan: Option<DecideKan<M>>,
    pub(crate) discard: Option<DecideDiscard<M>>,
    pub(crate) riichi: Option<DecideBool<M>>,
    pub(crate) call: Option<DecideCall<M>>,
    pub(crate) tedashi: Option<DecideBool<M>>,
    pub(crate) observe: Option<Observe<M>>,
    pub(crate) each_turn: Option<PredictEachTurn<M>>,
    pub(crate) think: Option<ThinkEachTurn<M>>,
}

/// 常駐預測：每次輪到自己時、在所有決策之前呼叫。只用來做預測（`ctx.claim`），不影響動作。
pub type PredictEachTurn<M> = fn(&Ctx, &mut M);
/// 每回合思考：每次輪到自己時、在所有決策之前呼叫（一定會執行），用來把「這時候的判斷」記進記憶。
pub type ThinkEachTurn<M> = fn(&Ctx, &mut M);

impl<M> Default for Book<M> {
    fn default() -> Self {
        Book {
            kyuushu: None,
            win: None,
            kan: None,
            discard: None,
            riichi: None,
            call: None,
            tedashi: None,
            observe: None,
            each_turn: None,
            think: None,
        }
    }
}

impl<M> Book<M> {
    /// 九種九牌時呼叫：回傳 true = 流局重來。沒有登記時一律不流局。
    pub fn kyuushu(&mut self, f: DecideBool<M>) {
        self.kyuushu = Some(f);
    }

    /// 能和牌時呼叫：回傳 true = 和牌。沒有登記時一律和牌。
    pub fn win(&mut self, f: DecideBool<M>) {
        self.win = Some(f);
    }

    /// 能暗槓或加槓時呼叫：回傳要槓的選項，`None` = 不槓。沒有登記時一律不槓。
    pub fn kan(&mut self, f: DecideKan<M>) {
        self.kan = Some(f);
    }

    /// 何切：每次要打牌時呼叫（必須登記）。只有一張可以打（例如立直後）時不會呼叫。
    pub fn discard(&mut self, f: DecideDiscard<M>) {
        self.discard = Some(f);
    }

    /// 打出的牌可以立直時呼叫（在何切之後）：回傳 true = 立直。沒有登記時一律立直。
    pub fn riichi(&mut self, f: DecideBool<M>) {
        self.riichi = Some(f);
    }

    /// 他家打牌可以吃碰槓時呼叫：回傳選擇（含不鳴）。沒有登記時一律不鳴。
    pub fn call(&mut self, f: DecideCall<M>) {
        self.call = Some(f);
    }

    /// 打出的牌和剛摸到的牌同種時呼叫（在何切、立直之後）：回傳 true = 手切（從手中打出同一種牌），
    /// false = 摸切。`ctx.tedashi_pai()` 是要打的牌，`ctx.declaring_riichi()` 表示這張是否為立直宣言牌。
    /// 沒有登記時一律摸切。
    pub fn tedashi(&mut self, f: DecideBool<M>) {
        self.tedashi = Some(f);
    }

    /// 每個事件都呼叫，用來更新對局記憶（例如記下誰總是即立直）。
    pub fn observe(&mut self, f: Observe<M>) {
        self.observe = Some(f);
    }

    /// 每回合思考：每次輪到自己時呼叫（在常駐預測與所有決策之前，一定會執行）。
    /// 用來把當下的判斷記進記憶，例如「我現在覺得每家聽牌的機率」，局末公開手牌時再和真相比對。
    /// 這裡寫的理由與預測不會被記錄。
    pub fn think_each_turn(&mut self, f: ThinkEachTurn<M>) {
        self.think = Some(f);
    }

    /// 常駐預測：每次輪到自己時呼叫（在所有決策之前）。用來執行「決策不一定會用到、
    /// 但希望持續被評估」的預測，例如每回合判斷三家是否聽牌。
    pub fn predict_each_turn(&mut self, f: PredictEachTurn<M>) {
        self.each_turn = Some(f);
    }
}
