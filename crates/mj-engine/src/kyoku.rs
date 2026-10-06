//! 一局（kyoku）：從配牌到和牌或流局。
//!
//! 流程概要：
//! 1. 配牌：從莊家開始每人 13 張。
//! 2. 輪到的玩家摸牌（或在吃碰後不摸牌）→ 選擇自摸、暗槓／加槓、九種九牌、打牌或立直。
//! 3. 打出的牌依序詢問其他三家：榮和、碰／明槓、吃（只有下家）、跳過。
//!    優先順序：榮和 ＞ 碰／槓 ＞ 吃。
//! 4. 沒人榮和時，檢查立直成立、四家立直、四風連打、四槓散了；有人鳴牌就輪到他，否則輪到下家。
//! 5. 牌山摸完時荒牌流局，計算聽牌料（不聽罰符）或流局滿貫。
//!
//! 除錯建置（含 `fuzz` profile）下，每回合都會檢查牌數守恆與點數守恆。

use crate::event::{DrawReason, Event, HandEnd};
use crate::wall::Wall;
use crate::{Action, DiscardRecord, Player, PlayerView, Seat};
use mj_calc::{is_agari, is_kokushi, waits};
use mj_core::{Meld, MeldKind, Pai, Tile, TileCounts};
use mj_rules::riichi::{RiichiConfig, RiichiState, Score, WinContext, Yaku, score};
use std::fmt;

/// 引擎錯誤。
#[derive(Debug, Clone, PartialEq)]
pub enum EngineError {
    /// 玩家回傳了不在合法動作清單中的動作。
    IllegalAction {
        seat: Seat,
        action: Action,
        legal: Vec<Action>,
    },
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EngineError::IllegalAction {
                seat,
                action,
                legal,
            } => {
                write!(
                    f,
                    "seat {seat} chose illegal action {action:?}; legal: {legal:?}"
                )
            }
        }
    }
}

impl std::error::Error for EngineError {}

/// 開局時的狀態（由 `game` 提供）。
pub(crate) struct KyokuSetup {
    pub round_wind: u8,
    pub kyoku: u8,
    pub honba: u8,
    pub riichi_sticks: u8,
    pub scores: [i32; 4],
    /// 正規局數（半莊 8）
    pub total_kyoku: u8,
}

/// 一局的結果（交回 `game` 決定下一局）。
pub(crate) struct KyokuOutcome {
    /// 結算後的四家點數
    pub scores: [i32; 4],
    /// 結算後場上剩下的供託數
    pub riichi_sticks: u8,
    /// 和牌者（流局為空）
    pub winners: Vec<Seat>,
    /// 是否連莊（莊家和牌、莊家流局聽牌、途中流局）
    pub dealer_continues: bool,
}

/// 牌桌：負責發送事件與詢問玩家。事件會寫入完整紀錄，並把遮蔽過的版本通知各家。
pub(crate) struct Table<'p> {
    pub players: &'p mut [Box<dyn Player>; 4],
    pub log: Option<&'p mut Vec<Event>>,
}

impl Table<'_> {
    /// 發送一個事件。
    pub fn emit(&mut self, ev: Event) {
        for (s, p) in self.players.iter_mut().enumerate() {
            p.on_event(s as Seat, &ev.masked_for(s as Seat));
        }
        if let Some(log) = self.log.as_deref_mut() {
            log.push(ev);
        }
    }

    /// 詢問玩家要選哪個動作，並檢查回傳的動作是否合法。
    fn ask(&mut self, view: &PlayerView, legal: Vec<Action>) -> Result<Action, EngineError> {
        let seat = view.me;
        let action = self.players[usize::from(seat)].act(view, &legal);
        if legal.contains(&action) {
            Ok(action)
        } else {
            Err(EngineError::IllegalAction {
                seat,
                action,
                legal,
            })
        }
    }
}

/// 單一玩家在這一局的狀態（含不公開的手牌）。
#[derive(Default)]
struct PState {
    /// 門前手牌
    hand: Vec<Pai>,
    /// 副露
    melds: Vec<Meld>,
    /// 牌河
    river: Vec<DiscardRecord>,
    /// 立直狀態
    riichi: RiichiState,
    /// 一發是否仍有效
    ippatsu: bool,
    /// 第一巡：自己還沒打過牌，且還沒有任何人鳴牌（天和、地和、兩立直、九種九牌用）。
    first_turn: bool,
    /// 同巡振聽：放過了可以榮和的牌，到自己下次打牌前不能榮和。
    temp_furiten: bool,
    /// 立直振聽：立直後放過和牌，本局之後都不能榮和。
    riichi_furiten: bool,
    /// 目前 13 張手牌的聽牌張（每次自己打牌後更新）；空 = 未聽牌。
    waits: Vec<Tile>,
    /// 食替限制：鳴牌後這一手不能打的牌種。
    kuikae: Vec<Tile>,
    /// 流局滿貫資格：打出的牌全是幺九牌且沒被鳴走。
    nagashi: bool,
    /// 包牌者：讓這位玩家完成大三元（第三組三元牌）或大四喜（第四組風牌）的放牌者。
    pao: Option<Seat>,
}

impl PState {
    /// 門前手牌的牌數表。
    fn counts(&self) -> TileCounts {
        TileCounts::from_tiles(self.hand.iter().map(|p| p.tile))
    }

    /// 是否門前清（沒有吃、碰、明槓；暗槓不影響）。
    fn closed(&self) -> bool {
        self.melds.iter().all(|m| !m.is_open())
    }

    /// 從手牌拿出一張指定的牌（必須存在）。
    fn take(&mut self, pai: Pai) {
        let i = self
            .hand
            .iter()
            .position(|&p| p == pai)
            .expect("tile in hand");
        self.hand.remove(i);
    }

    /// 是否振聽（furiten）：同巡振聽、立直振聽，或捨牌振聽（自己的牌河裡有聽牌張，含被鳴走的）。
    fn furiten(&self) -> bool {
        self.temp_furiten
            || self.riichi_furiten
            || self.river.iter().any(|d| self.waits.contains(&d.pai.tile))
    }

    /// 手牌與副露中的赤五張數。
    fn red_count(&self) -> u8 {
        let melded = self.melds.iter().flat_map(|m| m.tiles.iter());
        self.hand.iter().chain(melded).filter(|p| p.red).count() as u8
    }
}

/// 下一個回合開始時要做什麼。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Next {
    /// 從牌山摸牌
    Draw,
    /// 摸嶺上牌（開槓後）
    Rinshan,
    /// 吃碰之後：不摸牌，直接打牌
    DiscardOnly,
}

/// 一個回合結束後：繼續（輪到誰、做什麼），或這一局結束。
enum Step {
    Continue(Seat, Next),
    End(KyokuOutcome),
}

/// 和牌張是怎麼來的（判定狀況役用）。
#[derive(Clone, Copy)]
struct WinWay {
    /// 自摸
    tsumo: bool,
    /// 嶺上開花
    rinshan: bool,
    /// 搶槓
    chankan: bool,
}

/// 一局的進行狀態。
pub(crate) struct Kyoku<'a, 'p> {
    setup: &'a KyokuSetup,
    cfg: &'a RiichiConfig,
    table: &'a mut Table<'p>,
    wall: Wall,
    p: [PState; 4],
    scores: [i32; 4],
    /// 場上的供託（立直棒）數
    sticks: u8,
    /// 本局還沒有任何人鳴牌（含暗槓）
    no_calls_yet: bool,
    /// 明槓、加槓的槓寶牌要等打牌後才翻開，這裡記錄待翻的張數
    pending_dora: u8,
    /// 每次開槓的人（判斷四槓散了）
    kan_owners: Vec<Seat>,
    /// 已從牌山摸出的張數（牌數守恆檢查用）
    drawn_total: usize,
}

/// 座位 `s` 往下家方向數 `k` 個的座位（1 = 下家、2 = 對家、3 = 上家）。
fn seat_offset(s: Seat, k: u8) -> Seat {
    (s + k) % 4
}

impl<'a, 'p> Kyoku<'a, 'p> {
    pub fn new(
        setup: &'a KyokuSetup,
        cfg: &'a RiichiConfig,
        wall: Wall,
        table: &'a mut Table<'p>,
    ) -> Self {
        Kyoku {
            setup,
            cfg,
            table,
            wall,
            p: Default::default(),
            scores: setup.scores,
            sticks: setup.riichi_sticks,
            no_calls_yet: true,
            pending_dora: 0,
            kan_owners: Vec::new(),
            drawn_total: 0,
        }
    }

    /// 莊家座位。
    fn dealer(&self) -> Seat {
        self.setup.kyoku % 4
    }

    /// 門風：0 = 東（莊家）.. 3 = 北。
    fn seat_wind(&self, s: Seat) -> u8 {
        (s + 4 - self.dealer()) % 4
    }

    /// 某座位的狀態。
    fn ps(&self, s: Seat) -> &PState {
        &self.p[usize::from(s)]
    }

    fn ps_mut(&mut self, s: Seat) -> &mut PState {
        &mut self.p[usize::from(s)]
    }

    /// 進行這一局直到結束。
    pub fn run(mut self) -> Result<KyokuOutcome, EngineError> {
        let dealer = self.dealer();
        // 配牌：從莊家開始每人 13 張。
        for i in 0..4 {
            let s = seat_offset(dealer, i);
            for _ in 0..13 {
                let pai = self.wall.draw().expect("wall holds the deal");
                self.drawn_total += 1;
                self.ps_mut(s).hand.push(pai);
            }
        }
        for ps in &mut self.p {
            ps.hand.sort();
            ps.first_turn = true;
            ps.nagashi = true;
            ps.waits = waits(&ps.counts());
        }
        self.table.emit(Event::StartKyoku {
            round_wind: self.setup.round_wind,
            kyoku: self.setup.kyoku,
            honba: self.setup.honba,
            riichi_sticks: self.setup.riichi_sticks,
            scores: self.scores,
            dora_indicator: self.wall.dora_indicators()[0],
            hands: std::array::from_fn(|s| self.p[s].hand.clone()),
        });

        // 莊家先摸牌，之後依 turn() 的結果輪轉。
        let (mut seat, mut next) = (dealer, Next::Draw);
        loop {
            self.debug_check_conservation();
            match self.turn(seat, next)? {
                Step::Continue(s, n) => (seat, next) = (s, n),
                Step::End(out) => return Ok(out),
            }
        }
    }

    /// 守恆檢查（只在除錯建置）：
    /// - 牌數：手牌＋副露＋牌河中沒被鳴走的牌 = 已摸出的張數；
    /// - 點數：四家點數＋供託 = 開局時的總和。
    fn debug_check_conservation(&self) {
        if cfg!(debug_assertions) {
            let held: usize = self
                .p
                .iter()
                .map(|ps| {
                    ps.hand.len()
                        + ps.melds.iter().map(|m| m.tiles.len()).sum::<usize>()
                        + ps.river.iter().filter(|d| !d.called).count()
                })
                .sum();
            assert_eq!(held, self.drawn_total, "tile conservation violated");
            let total: i32 = self.scores.iter().sum::<i32>() + 1000 * i32::from(self.sticks);
            let start: i32 =
                self.setup.scores.iter().sum::<i32>() + 1000 * i32::from(self.setup.riichi_sticks);
            assert_eq!(total, start, "point conservation violated");
        }
    }

    // ---------------------------------------------------------------- 玩家視角

    /// 建立某座位看得到的資訊。
    fn view(&self, me: Seat, drawn: Option<Pai>, last_discard: Option<(Seat, Pai)>) -> PlayerView {
        let ps = self.ps(me);
        let mut hand_pais = ps.hand.clone();
        hand_pais.sort();
        PlayerView {
            me,
            round_wind: self.setup.round_wind,
            kyoku: self.setup.kyoku,
            honba: self.setup.honba,
            riichi_sticks: self.sticks,
            scores: self.scores,
            dora_indicators: self.wall.dora_indicators(),
            hand: ps.counts(),
            hand_pais,
            drawn,
            melds: std::array::from_fn(|s| self.p[s].melds.clone()),
            rivers: std::array::from_fn(|s| self.p[s].river.clone()),
            riichi: std::array::from_fn(|s| self.p[s].riichi != RiichiState::None),
            tiles_left_in_wall: self.wall.remaining() as u8,
            total_kyoku: self.setup.total_kyoku,
            last_discard,
        }
    }

    // ---------------------------------------------------------------- 計分輔助

    /// 組出計分用的和牌狀況（立直、一發、海底、河底、嶺上、搶槓、天和地和、風位、寶牌）。
    fn win_context(&self, s: Seat, win: Tile, way: WinWay) -> WinContext {
        let ps = self.ps(s);
        let mut ctx = WinContext::new(win, way.tsumo);
        let last = self.wall.remaining() == 0;
        ctx.riichi = ps.riichi;
        ctx.ippatsu = ps.ippatsu;
        ctx.haitei = way.tsumo && last && !way.rinshan;
        ctx.houtei = !way.tsumo && last && !way.chankan;
        ctx.rinshan = way.rinshan;
        ctx.chankan = way.chankan;
        ctx.first_draw = way.tsumo && ps.first_turn && self.no_calls_yet;
        ctx.dealer = s == self.dealer();
        ctx.seat_wind = self.seat_wind(s);
        ctx.round_wind = self.setup.round_wind;
        ctx.dora_indicators = self.wall.dora_indicators();
        ctx.ura_indicators = self.wall.ura_indicators();
        ctx
    }

    /// 座位 `s` 以 `pai` 和牌的分數；自摸時這張牌已在手上，榮和時是別人的牌。
    /// 不是和牌型或無役時回傳 `None`。
    fn try_score(&self, s: Seat, pai: Pai, way: WinWay) -> Option<Score> {
        let ps = self.ps(s);
        let mut counts = ps.counts();
        if !way.tsumo {
            counts.add(pai.tile);
        }
        if !is_agari(&counts) {
            return None;
        }
        let mut ctx = self.win_context(s, pai.tile, way);
        ctx.red_fives = ps.red_count() + u8::from(!way.tsumo && pai.red);
        score(&counts, &ps.melds, &ctx, self.cfg)
    }

    // ---------------------------------------------------------------- 自己的回合

    /// 一個回合：摸牌（或不摸）→ 詢問玩家 → 執行動作。
    fn turn(&mut self, seat: Seat, next: Next) -> Result<Step, EngineError> {
        let mut drawn = None;
        let mut rinshan = false;
        match next {
            Next::Draw => match self.wall.draw() {
                Some(pai) => {
                    self.drawn_total += 1;
                    drawn = Some(pai);
                }
                // 牌山摸完：荒牌流局。
                None => return Ok(Step::End(self.exhaustive_draw())),
            },
            Next::Rinshan => {
                drawn = Some(self.wall.draw_rinshan());
                self.drawn_total += 1;
                rinshan = true;
            }
            Next::DiscardOnly => {}
        }
        if let Some(pai) = drawn {
            self.ps_mut(seat).hand.push(pai);
            self.table.emit(Event::Draw {
                seat,
                pai: Some(pai),
                rinshan,
            });
        }

        let legal = self.own_actions(seat, drawn, next == Next::DiscardOnly, rinshan);
        let view = self.view(seat, drawn, None);
        let action = self.table.ask(&view, legal)?;
        match action {
            Action::Tsumo => {
                let pai = drawn.expect("tsumo needs a drawn tile");
                let way = WinWay {
                    tsumo: true,
                    rinshan,
                    chankan: false,
                };
                Ok(Step::End(self.settle_wins(&[seat], seat, pai, way)))
            }
            Action::KyuushuKyuuhai => Ok(Step::End(self.abort(DrawReason::KyuushuKyuuhai))),
            Action::Ankan { tile } => self.ankan(seat, tile),
            Action::Kakan { pai } => self.kakan(seat, pai),
            Action::Discard { pai, tsumogiri } => self.discard(seat, pai, tsumogiri, false),
            Action::Riichi { pai, tsumogiri } => self.discard(seat, pai, tsumogiri, true),
            other => unreachable!("not an own-turn action: {other:?}"),
        }
    }

    /// 自己回合的合法動作：自摸、九種九牌、暗槓、加槓、打牌、立直。
    /// `after_call` = 剛吃碰完（只能打牌，且受食替限制）。
    fn own_actions(
        &self,
        seat: Seat,
        drawn: Option<Pai>,
        after_call: bool,
        rinshan: bool,
    ) -> Vec<Action> {
        let ps = self.ps(seat);
        let counts = ps.counts();
        let mut out = Vec::new();
        // 開槓需要牌山還有牌（要補嶺上牌），且全局最多 4 槓。
        let can_kan = !after_call && self.wall.remaining() > 0 && self.wall.kans() < 4;
        if let Some(pai) = drawn {
            if self
                .try_score(
                    seat,
                    pai,
                    WinWay {
                        tsumo: true,
                        rinshan,
                        chankan: false,
                    },
                )
                .is_some()
            {
                out.push(Action::Tsumo);
            }
            // 九種九牌：第一巡、無人鳴牌、手上有 9 種以上幺九牌。
            let yaochu_kinds = Tile::all()
                .filter(|t| t.is_yaochu() && counts.get(*t) > 0)
                .count();
            if ps.first_turn && self.no_calls_yet && yaochu_kinds >= 9 {
                out.push(Action::KyuushuKyuuhai);
            }
        }
        if can_kan {
            // 暗槓。立直後只能槓剛摸到的牌，且槓了以後聽牌張不能改變。
            for t in Tile::all().filter(|&t| counts.get(t) == 4) {
                let ok = match (ps.riichi, drawn) {
                    (RiichiState::None, _) => true,
                    (_, Some(d)) if d.tile == t => {
                        let mut after = counts;
                        (0..4).for_each(|_| {
                            after.remove(t);
                        });
                        let w = waits(&after);
                        !w.is_empty() && w == ps.waits
                    }
                    _ => false,
                };
                if ok {
                    out.push(Action::Ankan { tile: t });
                }
            }
            // 加槓：手上有已碰刻子的第四張（立直中不能加槓，因為立直必須門前）。
            if ps.riichi == RiichiState::None {
                for m in ps.melds.iter().filter(|m| m.kind == MeldKind::Pon) {
                    let mut seen = Vec::new();
                    for &p in ps.hand.iter().filter(|p| p.tile == m.base()) {
                        if !seen.contains(&p) {
                            seen.push(p);
                            out.push(Action::Kakan { pai: p });
                        }
                    }
                }
            }
        }

        // 立直後只能摸切。
        if ps.riichi != RiichiState::None {
            let pai = drawn.expect("riichi players always draw");
            out.push(Action::Discard {
                pai,
                tsumogiri: true,
            });
            return out;
        }

        // 立直條件：門前清、點數 1000 以上、牌山剩 4 張以上，且打出後聽牌。
        let discards = self.discard_options(seat, drawn);
        let can_riichi = !after_call
            && ps.closed()
            && self.scores[usize::from(seat)] >= 1000
            && self.wall.remaining() >= 4;
        for &(pai, tsumogiri) in &discards {
            out.push(Action::Discard { pai, tsumogiri });
            if can_riichi {
                let mut c = counts;
                c.remove(pai.tile);
                if !waits(&c).is_empty() {
                    out.push(Action::Riichi { pai, tsumogiri });
                }
            }
        }
        out
    }

    /// 可以打的牌（去重）。剛摸到的牌以「摸切」提供；若手上原本就有相同的牌，
    /// 也提供「手切」版本（打出同一種牌，但對手看到的資訊不同）。食替禁止的牌不列入。
    fn discard_options(&self, seat: Seat, drawn: Option<Pai>) -> Vec<(Pai, bool)> {
        let ps = self.ps(seat);
        let mut out: Vec<(Pai, bool)> = Vec::new();
        let drawn_extra = drawn.map(|d| ps.hand.iter().filter(|&&p| p == d).count() > 1);
        for &p in &ps.hand {
            if ps.kuikae.contains(&p.tile) {
                continue;
            }
            let opts: &[bool] = match (drawn, drawn_extra) {
                (Some(d), Some(true)) if d == p => &[true, false],
                (Some(d), _) if d == p => &[true],
                _ => &[false],
            };
            for &tg in opts {
                if !out.contains(&(p, tg)) {
                    out.push((p, tg));
                }
            }
        }
        out
    }

    /// 有人鳴牌（含暗槓）：所有人的一發與第一巡狀態都消失。
    fn break_ippatsu_and_first_turn(&mut self) {
        for ps in &mut self.p {
            ps.ippatsu = false;
            ps.first_turn = false;
        }
        self.no_calls_yet = false;
    }

    /// 暗槓：公開四張 → 國士無雙可以搶暗槓 → 立刻翻槓寶牌 → 摸嶺上牌。
    fn ankan(&mut self, seat: Seat, tile: Tile) -> Result<Step, EngineError> {
        let ps = self.ps_mut(seat);
        let tiles: Vec<Pai> = ps.hand.iter().copied().filter(|p| p.tile == tile).collect();
        ps.hand.retain(|p| p.tile != tile);
        let meld = Meld {
            kind: MeldKind::Ankan,
            tiles,
            called: None,
            from: None,
        };
        ps.melds.push(meld.clone());
        self.kan_owners.push(seat);
        self.break_ippatsu_and_first_turn();
        self.table.emit(Event::Call { seat, meld });
        // 只有國士無雙可以搶暗槓。
        let pai = Pai::plain(tile);
        if let Some(step) = self.offer_chankan(seat, pai, true)? {
            return Ok(step);
        }
        self.wall.reveal_dora();
        self.table.emit(Event::Dora {
            indicator: *self.wall.dora_indicators().last().unwrap(),
        });
        Ok(Step::Continue(seat, Next::Rinshan))
    }

    /// 加槓：先讓其他人搶槓 → 沒人搶就把碰改成加槓 → 槓寶牌等打牌後翻 → 摸嶺上牌。
    fn kakan(&mut self, seat: Seat, pai: Pai) -> Result<Step, EngineError> {
        if let Some(step) = self.offer_chankan(seat, pai, false)? {
            return Ok(step);
        }
        let ps = self.ps_mut(seat);
        ps.take(pai);
        let m = ps
            .melds
            .iter_mut()
            .find(|m| m.kind == MeldKind::Pon && m.base() == pai.tile)
            .expect("pon");
        m.kind = MeldKind::Kakan;
        m.tiles.push(pai);
        let meld = m.clone();
        self.kan_owners.push(seat);
        self.pending_dora += 1;
        self.break_ippatsu_and_first_turn();
        self.table.emit(Event::Call { seat, meld });
        Ok(Step::Continue(seat, Next::Rinshan))
    }

    /// 搶槓：詢問其他人要不要榮和這張槓的牌。有人和牌就回傳本局結束。
    /// `kokushi_only` = 暗槓時只允許國士無雙搶槓。
    fn offer_chankan(
        &mut self,
        kanner: Seat,
        pai: Pai,
        kokushi_only: bool,
    ) -> Result<Option<Step>, EngineError> {
        let way = WinWay {
            tsumo: false,
            rinshan: false,
            chankan: true,
        };
        let mut rons = Vec::new();
        for k in 1..4 {
            let s = seat_offset(kanner, k);
            let ps = self.ps(s);
            if !ps.waits.contains(&pai.tile) {
                continue;
            }
            let mut c = ps.counts();
            c.add(pai.tile);
            let shape_ok = !kokushi_only || is_kokushi(&c);
            if shape_ok && !ps.furiten() && self.try_score(s, pai, way).is_some() {
                let view = self.view(s, None, Some((kanner, pai)));
                if self.table.ask(&view, vec![Action::Ron, Action::Pass])? == Action::Ron {
                    rons.push(s);
                    continue;
                }
            }
            self.mark_missed(s);
        }
        Ok(self.resolve_rons(&rons, kanner, pai, way))
    }

    /// 玩家放過了自己的聽牌張：進入同巡振聽，到自己下次打牌為止；立直中則永久振聽。
    fn mark_missed(&mut self, s: Seat) {
        let ps = self.ps_mut(s);
        ps.temp_furiten = true;
        if ps.riichi != RiichiState::None {
            ps.riichi_furiten = true;
        }
    }

    /// 處理榮和：沒人和 → `None`；三家和 → 途中流局；不允許雙響時只算離放銃者最近的一家（截和）。
    fn resolve_rons(&mut self, rons: &[Seat], from: Seat, pai: Pai, way: WinWay) -> Option<Step> {
        match rons.len() {
            0 => None,
            3 if self.cfg.double_ron => Some(Step::End(self.abort(DrawReason::Sanchahou))),
            n if n >= 2 && !self.cfg.double_ron => {
                Some(Step::End(self.settle_wins(&rons[..1], from, pai, way)))
            }
            _ => Some(Step::End(self.settle_wins(rons, from, pai, way))),
        }
    }

    // ---------------------------------------------------------------- 打牌與鳴牌

    /// 打牌（`riichi` = 同時宣告立直），之後詢問其他三家的回應。
    fn discard(
        &mut self,
        seat: Seat,
        pai: Pai,
        tsumogiri: bool,
        riichi: bool,
    ) -> Result<Step, EngineError> {
        // 第一巡、無人鳴牌時立直 = 兩立直。
        let double = riichi && self.ps(seat).first_turn && self.no_calls_yet;
        let ps = self.ps_mut(seat);
        ps.take(pai);
        ps.hand.sort();
        ps.kuikae.clear();
        // 自己打牌後同巡振聽解除；已立直者的一發到這裡失效（宣言牌本身不算）。
        ps.temp_furiten = false;
        if ps.riichi != RiichiState::None {
            ps.ippatsu = false;
        }
        ps.first_turn = false;
        ps.nagashi &= pai.tile.is_yaochu();
        ps.waits = waits(&ps.counts());
        ps.river.push(DiscardRecord {
            pai,
            tsumogiri,
            riichi_declared: riichi,
            called: false,
        });
        self.table.emit(Event::Discard {
            seat,
            pai,
            tsumogiri,
            riichi,
        });
        // 明槓、加槓的槓寶牌在打牌後翻開（榮和這張牌時也算新寶牌）。
        while self.pending_dora > 0 {
            self.pending_dora -= 1;
            self.wall.reveal_dora();
            self.table.emit(Event::Dora {
                indicator: *self.wall.dora_indicators().last().unwrap(),
            });
        }

        // 依下家、對家、上家的順序詢問：任何人都可以榮和；牌山還有牌時才能吃碰槓。
        let way = WinWay {
            tsumo: false,
            rinshan: false,
            chankan: false,
        };
        let mut rons = Vec::new();
        let mut call: Option<(Seat, Action)> = None;
        let mut chi: Option<(Seat, Action)> = None;
        for k in 1..4 {
            let s = seat_offset(seat, k);
            let legal = self.response_actions(s, seat, pai);
            let action = if legal.len() > 1 {
                let view = self.view(s, None, Some((seat, pai)));
                self.table.ask(&view, legal)?
            } else {
                Action::Pass
            };
            // 是聽牌張卻沒有榮和（不論是否有役）→ 振聽。
            if action != Action::Ron && self.ps(s).waits.contains(&pai.tile) {
                self.mark_missed(s);
            }
            match action {
                Action::Ron => rons.push(s),
                Action::Pon { .. } | Action::Daiminkan => call = Some((s, action)),
                Action::Chi { .. } => chi = Some((s, action)),
                _ => {}
            }
        }
        if let Some(step) = self.resolve_rons(&rons, seat, pai, way) {
            return Ok(step);
        }

        // 宣言牌沒有被榮和：立直成立，支付 1000 點供託，一發開始。
        if riichi {
            let ps = self.ps_mut(seat);
            ps.riichi = if double {
                RiichiState::Double
            } else {
                RiichiState::Riichi
            };
            ps.ippatsu = true;
            self.scores[usize::from(seat)] -= 1000;
            self.sticks += 1;
            self.table.emit(Event::RiichiAccepted { seat });
            if self.p.iter().all(|p| p.riichi != RiichiState::None) {
                return Ok(Step::End(self.abort(DrawReason::SuuchaRiichi)));
            }
        }
        // 四風連打：第一巡無人鳴牌，四家第一張都打同一種風牌。
        if self.no_calls_yet && self.p.iter().all(|p| p.river.len() == 1) {
            let first = self.p[0].river[0].pai.tile;
            if first.is_wind() && self.p.iter().all(|p| p.river[0].pai.tile == first) {
                return Ok(Step::End(self.abort(DrawReason::SuufonRenda)));
            }
        }
        // 四槓散了：開了第四槓且不是同一人開的（同一人四槓還可能做四槓子）。
        if self.wall.kans() == 4 && self.kan_owners.iter().any(|&o| o != self.kan_owners[0]) {
            return Ok(Step::End(self.abort(DrawReason::Suukaikan)));
        }

        // 碰／槓優先於吃；沒人鳴牌就輪到下家摸牌。
        if let Some((s, action)) = call.or(chi) {
            return Ok(self.apply_call(s, seat, pai, action));
        }
        Ok(Step::Continue(seat_offset(seat, 1), Next::Draw))
    }

    /// 對他家打出的牌可以做的回應：榮和、碰、大明槓、吃（只有下家，且只能吃數牌）、跳過。
    /// 立直中只能榮和；牌山已空（河底）時不能鳴牌。
    fn response_actions(&self, s: Seat, from: Seat, pai: Pai) -> Vec<Action> {
        let ps = self.ps(s);
        let mut out = Vec::new();
        let way = WinWay {
            tsumo: false,
            rinshan: false,
            chankan: false,
        };
        if ps.waits.contains(&pai.tile) && !ps.furiten() && self.try_score(s, pai, way).is_some() {
            out.push(Action::Ron);
        }
        if self.wall.remaining() > 0 && ps.riichi == RiichiState::None {
            let same: Vec<Pai> = ps
                .hand
                .iter()
                .copied()
                .filter(|p| p.tile == pai.tile)
                .collect();
            // 碰：手上同種牌任選兩張（有赤五時，用不用赤五是不同的選項）。
            if same.len() >= 2 {
                let mut seen = Vec::new();
                for i in 0..same.len() {
                    for j in i + 1..same.len() {
                        let mut pair = [same[i], same[j]];
                        pair.sort();
                        if !seen.contains(&pair) && self.call_leaves_discard(s, &pair, &[pai.tile])
                        {
                            seen.push(pair);
                            out.push(Action::Pon { consumed: pair });
                        }
                    }
                }
            }
            if same.len() == 3 && self.wall.kans() < 4 {
                out.push(Action::Daiminkan);
            }
            // 吃：只有下家可以吃，列出所有能組成順子的手牌組合。
            if s == seat_offset(from, 1) && !pai.tile.is_honor() {
                for (a, b) in chi_shapes(pai.tile) {
                    for &pa in ps.hand.iter().filter(|p| p.tile == a) {
                        for &pb in ps.hand.iter().filter(|p| p.tile == b) {
                            let consumed = [pa, pb];
                            let action = Action::Chi { consumed };
                            if !out.contains(&action)
                                && self.call_leaves_discard(
                                    s,
                                    &consumed,
                                    &kuikae_for_chi(pai.tile, a, b),
                                )
                            {
                                out.push(action);
                            }
                        }
                    }
                }
            }
        }
        out.push(Action::Pass);
        out
    }

    /// 鳴牌後必須還有可以打的牌（不受食替限制的牌），否則這個鳴牌不合法。
    fn call_leaves_discard(&self, s: Seat, consumed: &[Pai], forbidden: &[Tile]) -> bool {
        let mut rest = self.ps(s).hand.clone();
        for p in consumed {
            let i = rest
                .iter()
                .position(|q| q == p)
                .expect("consumed tile in hand");
            rest.remove(i);
        }
        rest.iter().any(|p| !forbidden.contains(&p.tile))
    }

    /// 執行吃、碰、大明槓：牌河標記被鳴走、組成副露、設定食替限制、檢查包牌。
    fn apply_call(&mut self, s: Seat, from: Seat, pai: Pai, action: Action) -> Step {
        {
            let fp = self.ps_mut(from);
            fp.river.last_mut().expect("discard").called = true;
            fp.nagashi = false;
        }
        let ps = self.ps_mut(s);
        let (kind, mut tiles, kuikae) = match action {
            Action::Chi { consumed } => {
                let k = kuikae_for_chi(pai.tile, consumed[0].tile, consumed[1].tile);
                (MeldKind::Chi, consumed.to_vec(), k)
            }
            Action::Pon { consumed } => (MeldKind::Pon, consumed.to_vec(), vec![pai.tile]),
            Action::Daiminkan => {
                let t: Vec<Pai> = ps
                    .hand
                    .iter()
                    .copied()
                    .filter(|p| p.tile == pai.tile)
                    .collect();
                (MeldKind::Daiminkan, t, Vec::new())
            }
            _ => unreachable!(),
        };
        for p in &tiles {
            ps.take(*p);
        }
        tiles.push(pai);
        tiles.sort();
        ps.kuikae = kuikae;
        let meld = Meld {
            kind,
            tiles,
            called: Some(pai),
            from: Some(from),
        };
        ps.melds.push(meld.clone());
        // 包牌：這次鳴牌完成了第三組三元牌（大三元）或第四組風牌（大四喜）。
        if kind != MeldKind::Chi {
            let sets = |pred: fn(Tile) -> bool| {
                ps.melds
                    .iter()
                    .filter(|m| m.kind != MeldKind::Chi && pred(m.base()))
                    .count()
            };
            let completes = (pai.tile.is_dragon() && sets(Tile::is_dragon) == 3)
                || (pai.tile.is_wind() && sets(Tile::is_wind) == 4);
            if completes {
                ps.pao = Some(from);
            }
        }
        self.break_ippatsu_and_first_turn();
        self.table.emit(Event::Call { seat: s, meld });
        // 大明槓：槓寶牌在打牌後翻開，接著摸嶺上牌；吃碰則直接打牌。
        if kind == MeldKind::Daiminkan {
            self.kan_owners.push(s);
            self.pending_dora += 1;
            Step::Continue(s, Next::Rinshan)
        } else {
            Step::Continue(s, Next::DiscardOnly)
        }
    }

    // ---------------------------------------------------------------- 結束

    /// 和牌結算。`winners` 依離放銃者由近到遠排列（雙響時兩人）；自摸時 `from` = 和牌者。
    fn settle_wins(&mut self, winners: &[Seat], from: Seat, pai: Pai, way: WinWay) -> KyokuOutcome {
        let honba = self.setup.honba;
        for (i, &w) in winners.iter().enumerate() {
            let sc = self
                .try_score(w, pai, way)
                .expect("winning action was legal");
            let dealer = w == self.dealer();
            // 本場與供託只給離放銃者最近的和牌者。
            let h = if i == 0 { honba } else { 0 };
            let mut deltas = [0i32; 4];
            // 包牌只在和出大三元或大四喜時適用。
            let pao = self.ps(w).pao.filter(|_| {
                sc.yaku
                    .iter()
                    .any(|(y, _)| matches!(y, Yaku::Daisangen | Yaku::Daisuushii))
            });
            if way.tsumo {
                let (d, n) = sc.tsumo_points(dealer, h);
                // 包牌自摸：包牌者一人支付全額；否則三家分攤。
                if let Some(x) = pao {
                    let total = if dealer { 3 * d } else { d + 2 * n };
                    deltas[usize::from(x)] -= total;
                } else {
                    for s in (0..4).filter(|&s| s != w) {
                        deltas[usize::from(s)] -= if s == self.dealer() { d } else { n };
                    }
                }
            } else {
                let pts = sc.ron_points(dealer, h);
                // 包牌榮和（放銃者不是包牌者）：包牌者與放銃者各付一半。
                match pao {
                    Some(x) if x != from => {
                        let half = pts / 2;
                        deltas[usize::from(x)] -= half;
                        deltas[usize::from(from)] -= pts - half;
                    }
                    _ => deltas[usize::from(from)] -= pts,
                }
            }
            let paid: i32 = -deltas.iter().sum::<i32>();
            deltas[usize::from(w)] += paid;
            if i == 0 {
                deltas[usize::from(w)] += 1000 * i32::from(self.sticks);
                self.sticks = 0;
            }
            for (score, d) in self.scores.iter_mut().zip(deltas) {
                *score += d;
            }
            let mut hand = self.ps(w).hand.clone();
            if !way.tsumo {
                hand.push(pai);
            }
            hand.sort();
            let ura = if self.ps(w).riichi != RiichiState::None {
                self.wall.ura_indicators()
            } else {
                Vec::new()
            };
            self.table.emit(Event::HandEnd {
                end: HandEnd::Win {
                    winner: w,
                    from,
                    hand,
                    win_tile: pai,
                    score: sc,
                    ura_indicators: ura,
                },
                deltas,
            });
        }
        self.finish(winners.to_vec(), winners.contains(&self.dealer()))
    }

    /// 四家是否聽牌（看牌型，不管有沒有役）。
    fn tenpai(&self) -> [bool; 4] {
        std::array::from_fn(|s| !self.p[s].waits.is_empty())
    }

    /// 荒牌流局：有流局滿貫就付滿貫，否則依聽牌人數付不聽罰符（共 3000 點）。
    /// 莊家聽牌則連莊。
    fn exhaustive_draw(&mut self) -> KyokuOutcome {
        let tenpai = self.tenpai();
        let nagashi: Vec<Seat> = (0..4).filter(|&s| self.ps(s).nagashi).collect();
        let mut deltas = [0i32; 4];
        if !nagashi.is_empty() {
            // 依滿貫自摸的方式支付，不加本場。
            for &w in &nagashi {
                let dealer_win = w == self.dealer();
                for s in (0..4).filter(|&s| s != w) {
                    let pay = if dealer_win || s == self.dealer() {
                        4000
                    } else {
                        2000
                    };
                    deltas[usize::from(s)] -= pay;
                    deltas[usize::from(w)] += pay;
                }
            }
        } else {
            // 1 人聽：+3000／−1000；2 人聽：+1500／−1500；3 人聽：+1000／−3000；全聽或全不聽不付。
            let n = tenpai.iter().filter(|&&t| t).count() as i32;
            if (1..4).contains(&n) {
                for (d, &t) in deltas.iter_mut().zip(&tenpai) {
                    *d = if t { 3000 / n } else { -3000 / (4 - n) };
                }
            }
        }
        for (score, d) in self.scores.iter_mut().zip(deltas) {
            *score += d;
        }
        self.table.emit(Event::HandEnd {
            end: HandEnd::Draw {
                reason: DrawReason::Exhaustive,
                tenpai,
                nagashi,
            },
            deltas,
        });
        let dealer_tenpai = tenpai[usize::from(self.dealer())];
        self.finish(Vec::new(), dealer_tenpai)
    }

    /// 途中流局（九種九牌、四風連打、四家立直、四槓散了、三家和）：不付點，莊家連莊。
    fn abort(&mut self, reason: DrawReason) -> KyokuOutcome {
        self.table.emit(Event::HandEnd {
            end: HandEnd::Draw {
                reason,
                tenpai: self.tenpai(),
                nagashi: Vec::new(),
            },
            deltas: [0; 4],
        });
        self.finish(Vec::new(), true)
    }

    /// 結束本局，回傳結果。
    fn finish(&mut self, winners: Vec<Seat>, dealer_continues: bool) -> KyokuOutcome {
        self.debug_check_conservation();
        self.table.emit(Event::EndKyoku {
            scores: self.scores,
        });
        KyokuOutcome {
            scores: self.scores,
            riichi_sticks: self.sticks,
            winners,
            dealer_continues,
        }
    }
}

/// 吃牌 `t` 時，手上需要的兩張牌的所有組合，例如吃 5m：(3m,4m)、(4m,6m)、(6m,7m)。
fn chi_shapes(t: Tile) -> Vec<(Tile, Tile)> {
    let n = t.number();
    let i = t.index() as u8;
    let mut v = Vec::new();
    if n >= 3 {
        v.push((Tile::from_index(i - 2), Tile::from_index(i - 1)));
    }
    if (2..=8).contains(&n) {
        v.push((Tile::from_index(i - 1), Tile::from_index(i + 1)));
    }
    if n <= 7 {
        v.push((Tile::from_index(i + 1), Tile::from_index(i + 2)));
    }
    v
}

/// 吃牌後禁止打出的牌種（食替，kuikae）：吃進來的同種牌（現物食替），
/// 以及吃在兩端時順子另一側的筋牌（筋食替），例如用 23m 吃 1m 後不能打 1m、4m。
fn kuikae_for_chi(called: Tile, a: Tile, b: Tile) -> Vec<Tile> {
    let mut v = vec![called];
    let (lo, hi) = (a.min(b), a.max(b));
    if called < lo && hi.number() <= 8 {
        v.push(Tile::from_index(hi.index() as u8 + 1));
    } else if called > hi && lo.number() >= 2 {
        v.push(Tile::from_index(lo.index() as u8 - 1));
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: &str) -> Tile {
        s.parse().unwrap()
    }

    /// 1、9 只有一種吃法，5 有三種。
    #[test]
    fn chi_shapes_at_edges() {
        assert_eq!(chi_shapes(t("1m")), vec![(t("2m"), t("3m"))]);
        assert_eq!(chi_shapes(t("5p")).len(), 3);
        assert_eq!(chi_shapes(t("9s")), vec![(t("7s"), t("8s"))]);
    }

    /// 現物食替與筋食替。
    #[test]
    fn suji_kuikae() {
        // 用 23m 吃 1m：禁打 1m、4m。
        assert_eq!(
            kuikae_for_chi(t("1m"), t("2m"), t("3m")),
            vec![t("1m"), t("4m")]
        );
        // 用 45m 吃 6m：禁打 6m、3m。
        assert_eq!(
            kuikae_for_chi(t("6m"), t("4m"), t("5m")),
            vec![t("6m"), t("3m")]
        );
        // 嵌張吃（46m 吃 5m）：只禁打 5m。
        assert_eq!(kuikae_for_chi(t("5m"), t("4m"), t("6m")), vec![t("5m")]);
        // 用 89m 吃 7m：另一側沒有「10m」，只禁打 7m。
        assert_eq!(kuikae_for_chi(t("7m"), t("8m"), t("9m")), vec![t("7m")]);
    }
}
