//! 給 UI 的牌桌狀態：把人類座位收到的（遮蔽過的）事件，逐一轉成「這個事件之後的牌桌」。
//! 前端只要依序畫出每一幀，不必自己推算手牌、牌河與點數。
//!
//! 牌一律以字串表示（`5m`、`0p` = 赤五筒、`7z` = 中），與專案其他地方的記法相同。

use mj_core::{Meld, MeldKind, Pai, Tile};
use mj_engine::{DrawReason, Event, HandEnd, PlayerView, Seat};
use mj_rules::riichi::Limit;
use serde::Serialize;

/// 一幀：一個事件與它發生之後的牌桌。
#[derive(Debug, Clone, Serialize)]
pub struct Frame {
    pub event: EventView,
    pub table: TableView,
    /// 這一幀是自動代打的動作（立直後只能摸切時，不必詢問人類）
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub auto: bool,
}

/// 人類視角的牌桌。
#[derive(Debug, Clone, Default, Serialize)]
pub struct TableView {
    /// 人類的座位（座位 0 是起家）
    pub me: Seat,
    /// 局名，例如「東1局 0本場」
    pub round: String,
    pub round_wind: u8,
    pub kyoku: u8,
    pub honba: u8,
    /// 場上的供託（立直棒）數
    pub riichi_sticks: u8,
    /// 四家點數（依座位）
    pub scores: [i32; 4],
    /// 寶牌指示牌
    pub dora_indicators: Vec<String>,
    /// 牌山剩餘張數（不含王牌）
    pub wall_left: u8,
    /// 四家（依座位）
    pub seats: [SeatView; 4],
    /// 目前輪到誰打牌（剛摸牌或剛鳴牌的座位）
    pub turn: Option<Seat>,
    /// 最後一張打出的牌：(座位, 在牌河中的位置)
    pub last_discard: Option<(Seat, usize)>,
}

/// 一家的狀態。
#[derive(Debug, Clone, Default, Serialize)]
pub struct SeatView {
    /// 門前手牌（已排序，不含剛摸的牌）。他家的手牌看不到時是空的，和牌時公開。
    pub hand: Vec<String>,
    /// 門前手牌張數（含剛摸的牌）
    pub hand_count: u8,
    /// 剛摸到的牌（只有自己的看得到）
    pub drawn: Option<String>,
    /// 他家剛摸了牌（還沒打）
    pub has_drawn: bool,
    /// 副露
    pub melds: Vec<MeldView>,
    /// 牌河
    pub river: Vec<RiverTile>,
    /// 已立直
    pub riichi: bool,
    /// 手牌已公開（和牌）
    pub revealed: bool,
    /// 檢視牌譜時公開所有人的手牌：`hand`、`drawn` 是實際的牌（不是和牌）
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub open: bool,
    /// 門風：0 = 東 .. 3 = 北
    pub wind: u8,
}

/// 一組副露。
#[derive(Debug, Clone, Serialize)]
pub struct MeldView {
    /// chi、pon、daiminkan、ankan、kakan
    pub kind: &'static str,
    /// 全部的牌（加槓為碰的三張加上第四張）
    pub tiles: Vec<String>,
    /// 從他家取得的那張牌
    pub called: Option<String>,
    /// 被鳴牌者的座位
    pub from: Option<Seat>,
}

/// 牌河中的一張牌。
#[derive(Debug, Clone, Serialize)]
pub struct RiverTile {
    pub pai: String,
    /// 摸切
    pub tsumogiri: bool,
    /// 立直宣言牌（橫放）
    pub riichi: bool,
    /// 被鳴走了
    pub called: bool,
}

/// UI 用的事件（每個都附一句中文說明 `text`）。
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventView {
    StartGame {
        text: String,
    },
    StartKyoku {
        text: String,
    },
    Draw {
        seat: Seat,
        /// 自己摸的牌（他家的看不到）
        pai: Option<String>,
        rinshan: bool,
        text: String,
    },
    Discard {
        seat: Seat,
        pai: String,
        tsumogiri: bool,
        riichi: bool,
        text: String,
    },
    Call {
        seat: Seat,
        meld: MeldView,
        text: String,
    },
    RiichiAccepted {
        seat: Seat,
        text: String,
    },
    Dora {
        indicator: String,
        text: String,
    },
    Win {
        winner: Seat,
        /// 放銃者；自摸時等於 `winner`
        from: Seat,
        hand: Vec<String>,
        win_tile: String,
        /// 役（名稱與番數；役滿為 13）
        yaku: Vec<(String, u8)>,
        dora: u8,
        ura: u8,
        aka: u8,
        han: u8,
        fu: u8,
        /// 滿貫、跳滿……（未達滿貫時為空字串）
        limit: String,
        ura_indicators: Vec<String>,
        deltas: [i32; 4],
        text: String,
    },
    Ryuukyoku {
        reason: String,
        tenpai: [bool; 4],
        nagashi: Vec<Seat>,
        deltas: [i32; 4],
        text: String,
    },
    EndKyoku {
        scores: [i32; 4],
        text: String,
    },
    EndGame {
        scores: [i32; 4],
        text: String,
    },
}

/// 座位相對於人類的稱呼。
pub fn seat_name(me: Seat, seat: Seat) -> &'static str {
    ["你", "下家", "對家", "上家"][usize::from((seat + 4 - me) % 4)]
}

fn wind_name(w: u8) -> &'static str {
    ["東", "南", "西", "北"][usize::from(w % 4)]
}

fn pai_str(p: Pai) -> String {
    p.to_string()
}

fn tile_str(t: Tile) -> String {
    t.to_string()
}

fn meld_kind(k: MeldKind) -> &'static str {
    match k {
        MeldKind::Chi => "chi",
        MeldKind::Pon => "pon",
        MeldKind::Daiminkan => "daiminkan",
        MeldKind::Ankan => "ankan",
        MeldKind::Kakan => "kakan",
    }
}

fn meld_kind_name(k: MeldKind) -> &'static str {
    match k {
        MeldKind::Chi => "吃",
        MeldKind::Pon => "碰",
        MeldKind::Daiminkan => "明槓",
        MeldKind::Ankan => "暗槓",
        MeldKind::Kakan => "加槓",
    }
}

fn meld_view(m: &Meld) -> MeldView {
    MeldView {
        kind: meld_kind(m.kind),
        tiles: m.tiles.iter().map(|&p| pai_str(p)).collect(),
        called: m.called.map(pai_str),
        from: m.from,
    }
}

/// 滿貫以上的名稱。
pub fn limit_name(l: Limit) -> String {
    match l {
        Limit::None => String::new(),
        Limit::Mangan => "滿貫".into(),
        Limit::Haneman => "跳滿".into(),
        Limit::Baiman => "倍滿".into(),
        Limit::Sanbaiman => "三倍滿".into(),
        Limit::Yakuman(1) => "役滿".into(),
        Limit::Yakuman(n) => format!("{n} 倍役滿"),
    }
}

/// 流局原因的名稱。
pub fn draw_reason_name(r: DrawReason) -> &'static str {
    match r {
        DrawReason::Exhaustive => "荒牌流局",
        DrawReason::KyuushuKyuuhai => "九種九牌",
        DrawReason::SuufonRenda => "四風連打",
        DrawReason::SuuchaRiichi => "四家立直",
        DrawReason::Suukaikan => "四槓散了",
        DrawReason::Sanchahou => "三家和",
    }
}

/// 從一個個事件累積出牌桌（人類視角）。
#[derive(Debug, Clone)]
pub struct TableBuilder {
    t: TableView,
    /// 自己的門前手牌（不含剛摸的牌）
    hand: Vec<Pai>,
    drawn: Option<Pai>,
    /// 各家副露（引擎的型別，加槓時要找回原本的碰）
    melds: [Vec<Meld>; 4],
}

impl TableBuilder {
    pub fn new(me: Seat) -> Self {
        TableBuilder {
            t: TableView {
                me,
                ..TableView::default()
            },
            hand: Vec::new(),
            drawn: None,
            melds: Default::default(),
        }
    }

    /// 目前的牌桌。
    pub fn table(&self) -> &TableView {
        &self.t
    }

    /// 自己的門前手牌（含剛摸的牌，已排序）。
    pub fn my_hand(&self) -> Vec<Pai> {
        let mut h = self.hand.clone();
        h.extend(self.drawn);
        h.sort();
        h
    }

    fn me(&self) -> Seat {
        self.t.me
    }

    fn name(&self, seat: Seat) -> &'static str {
        seat_name(self.me(), seat)
    }

    /// 把剛摸的牌併入手牌。
    fn merge_drawn(&mut self) {
        if let Some(d) = self.drawn.take() {
            self.hand.push(d);
        }
    }

    /// 從自己的手牌拿掉一張（優先完全相同的牌，其次同種牌）。
    fn remove_mine(&mut self, p: Pai) {
        let i = self
            .hand
            .iter()
            .position(|&h| h == p)
            .or_else(|| self.hand.iter().position(|h| h.tile == p.tile));
        if let Some(i) = i {
            self.hand.remove(i);
        }
    }

    /// 把內部狀態寫回 `TableView`。
    fn sync(&mut self) {
        let me = usize::from(self.me());
        self.hand.sort();
        for s in 0..4 {
            self.t.seats[s].melds = self.melds[s].iter().map(meld_view).collect();
        }
        let seat = &mut self.t.seats[me];
        if !seat.revealed {
            seat.hand = self.hand.iter().map(|&p| pai_str(p)).collect();
        }
        seat.drawn = self.drawn.map(pai_str);
        seat.has_drawn = self.drawn.is_some();
        seat.hand_count = (self.hand.len() + usize::from(self.drawn.is_some())) as u8;
    }

    /// 套用一個事件，回傳這一幀。
    pub fn apply(&mut self, ev: &Event) -> Frame {
        let me = self.me();
        let event = match ev {
            Event::StartGame { .. } => EventView::StartGame {
                text: format!("對局開始，你是{}家", wind_name(me)),
            },
            Event::StartKyoku {
                round_wind,
                kyoku,
                honba,
                riichi_sticks,
                scores,
                dora_indicator,
                hands,
            } => {
                let round = format!("{}{}局 {}本場", wind_name(*round_wind), kyoku + 1, honba);
                self.t = TableView {
                    me,
                    round: round.clone(),
                    round_wind: *round_wind,
                    kyoku: *kyoku,
                    honba: *honba,
                    riichi_sticks: *riichi_sticks,
                    scores: *scores,
                    dora_indicators: vec![tile_str(*dora_indicator)],
                    wall_left: 70,
                    seats: Default::default(),
                    turn: None,
                    last_discard: None,
                };
                for s in 0..4u8 {
                    let v = &mut self.t.seats[usize::from(s)];
                    v.hand_count = 13;
                    v.wind = (s + 4 - kyoku) % 4;
                }
                self.hand = hands[usize::from(me)].clone();
                self.drawn = None;
                self.melds = Default::default();
                EventView::StartKyoku {
                    text: format!("{round}開始"),
                }
            }
            Event::Draw { seat, pai, rinshan } => {
                self.t.wall_left = self.t.wall_left.saturating_sub(1);
                self.t.turn = Some(*seat);
                if *seat == me {
                    self.merge_drawn();
                    self.drawn = *pai;
                } else {
                    let v = &mut self.t.seats[usize::from(*seat)];
                    v.hand_count += 1;
                    v.has_drawn = true;
                }
                EventView::Draw {
                    seat: *seat,
                    pai: pai.map(pai_str),
                    rinshan: *rinshan,
                    text: format!(
                        "{}摸{}{}",
                        self.name(*seat),
                        if *rinshan { "嶺上牌" } else { "牌" },
                        pai.map(|p| format!(" {p}")).unwrap_or_default()
                    ),
                }
            }
            Event::Discard {
                seat,
                pai,
                tsumogiri,
                riichi,
            } => {
                if *seat == me {
                    if *tsumogiri && self.drawn == Some(*pai) {
                        self.drawn = None;
                    } else {
                        self.merge_drawn();
                        self.remove_mine(*pai);
                    }
                } else {
                    let v = &mut self.t.seats[usize::from(*seat)];
                    v.hand_count = v.hand_count.saturating_sub(1);
                    v.has_drawn = false;
                }
                let river = &mut self.t.seats[usize::from(*seat)].river;
                river.push(RiverTile {
                    pai: pai_str(*pai),
                    tsumogiri: *tsumogiri,
                    riichi: *riichi,
                    called: false,
                });
                self.t.last_discard = Some((*seat, river.len() - 1));
                self.t.turn = None;
                EventView::Discard {
                    seat: *seat,
                    pai: pai_str(*pai),
                    tsumogiri: *tsumogiri,
                    riichi: *riichi,
                    text: format!(
                        "{}{}{} {pai}",
                        self.name(*seat),
                        if *riichi { "立直，" } else { "" },
                        if *tsumogiri { "摸切" } else { "手切" }
                    ),
                }
            }
            Event::Call { seat, meld } => {
                let s = usize::from(*seat);
                match meld.kind {
                    MeldKind::Kakan => {
                        // 加槓：原本的碰換成槓；加上去的牌 = 槓的四張扣掉碰的三張。
                        let mut added = meld.tiles.clone();
                        if let Some(i) = self.melds[s]
                            .iter()
                            .position(|m| m.kind == MeldKind::Pon && m.base() == meld.base())
                        {
                            for p in &self.melds[s][i].tiles {
                                if let Some(j) = added.iter().position(|a| a == p) {
                                    added.remove(j);
                                }
                            }
                            self.melds[s][i] = meld.clone();
                        } else {
                            self.melds[s].push(meld.clone());
                        }
                        let added = added.first().copied();
                        if *seat == me {
                            self.merge_drawn();
                            self.remove_mine(added.unwrap_or(meld.tiles[0]));
                        } else {
                            let v = &mut self.t.seats[s];
                            v.hand_count = v.hand_count.saturating_sub(1);
                            v.has_drawn = false;
                        }
                    }
                    MeldKind::Ankan => {
                        self.melds[s].push(meld.clone());
                        if *seat == me {
                            self.merge_drawn();
                            for &p in &meld.tiles {
                                self.remove_mine(p);
                            }
                        } else {
                            let v = &mut self.t.seats[s];
                            v.hand_count = v.hand_count.saturating_sub(4);
                            v.has_drawn = false;
                        }
                    }
                    _ => {
                        self.melds[s].push(meld.clone());
                        // 被鳴走的牌在牌河中標記。
                        if let Some(from) = meld.from
                            && let Some(last) = self.t.seats[usize::from(from)].river.last_mut()
                        {
                            last.called = true;
                        }
                        let consumed = meld.tiles.len() - 1;
                        if *seat == me {
                            let mut skipped = false;
                            for &p in &meld.tiles {
                                if !skipped && Some(p) == meld.called {
                                    skipped = true;
                                    continue;
                                }
                                self.remove_mine(p);
                            }
                        } else {
                            let v = &mut self.t.seats[s];
                            v.hand_count = v.hand_count.saturating_sub(consumed as u8);
                        }
                        self.t.turn = Some(*seat);
                        self.t.last_discard = None;
                    }
                }
                let tiles: Vec<String> = meld.tiles.iter().map(|p| p.to_string()).collect();
                EventView::Call {
                    seat: *seat,
                    meld: meld_view(meld),
                    text: format!(
                        "{}{} {}",
                        self.name(*seat),
                        meld_kind_name(meld.kind),
                        tiles.join("")
                    ),
                }
            }
            Event::RiichiAccepted { seat } => {
                let s = usize::from(*seat);
                self.t.seats[s].riichi = true;
                self.t.scores[s] -= 1000;
                self.t.riichi_sticks += 1;
                EventView::RiichiAccepted {
                    seat: *seat,
                    text: format!("{}立直成立", self.name(*seat)),
                }
            }
            Event::Dora { indicator } => {
                self.t.dora_indicators.push(tile_str(*indicator));
                EventView::Dora {
                    indicator: tile_str(*indicator),
                    text: format!("新的寶牌指示牌 {indicator}"),
                }
            }
            Event::HandEnd { end, deltas } => {
                for (score, d) in self.t.scores.iter_mut().zip(deltas) {
                    *score += d;
                }
                match end {
                    HandEnd::Win {
                        winner,
                        from,
                        hand,
                        win_tile,
                        score,
                        ura_indicators,
                    } => {
                        self.t.riichi_sticks = 0;
                        let v = &mut self.t.seats[usize::from(*winner)];
                        v.revealed = true;
                        v.hand = hand.iter().map(|&p| pai_str(p)).collect();
                        v.drawn = None;
                        let limit = limit_name(score.limit);
                        let points = deltas[usize::from(*winner)];
                        let how = if winner == from {
                            "自摸".to_string()
                        } else {
                            format!("榮和（{}放銃）", self.name(*from))
                        };
                        let value = if score.han == 0 {
                            limit.clone()
                        } else if limit.is_empty() {
                            format!("{} 符 {} 番", score.fu, score.han)
                        } else {
                            format!("{} 番 {limit}", score.han)
                        };
                        EventView::Win {
                            winner: *winner,
                            from: *from,
                            hand: hand.iter().map(|&p| pai_str(p)).collect(),
                            win_tile: pai_str(*win_tile),
                            yaku: score
                                .yaku
                                .iter()
                                .map(|(y, h)| (y.name_ja().to_string(), *h))
                                .collect(),
                            dora: score.dora,
                            ura: score.ura,
                            aka: score.aka,
                            han: score.han,
                            fu: score.fu,
                            limit,
                            ura_indicators: ura_indicators.iter().map(|&t| tile_str(t)).collect(),
                            deltas: *deltas,
                            text: format!(
                                "{}{how} {win_tile}：{value}，{points:+}",
                                self.name(*winner)
                            ),
                        }
                    }
                    HandEnd::Draw {
                        reason,
                        tenpai,
                        nagashi,
                    } => {
                        let names: Vec<&str> = (0..4u8)
                            .filter(|&s| tenpai[usize::from(s)])
                            .map(|s| self.name(s))
                            .collect();
                        EventView::Ryuukyoku {
                            reason: draw_reason_name(*reason).into(),
                            tenpai: *tenpai,
                            nagashi: nagashi.clone(),
                            deltas: *deltas,
                            text: format!(
                                "{}（聽牌：{}）",
                                draw_reason_name(*reason),
                                if names.is_empty() {
                                    "無".to_string()
                                } else {
                                    names.join("、")
                                }
                            ),
                        }
                    }
                }
            }
            Event::EndKyoku { scores } => {
                self.t.scores = *scores;
                EventView::EndKyoku {
                    scores: *scores,
                    text: "本局結束".into(),
                }
            }
            Event::EndGame { scores } => {
                self.t.scores = *scores;
                EventView::EndGame {
                    scores: *scores,
                    text: "對局結束".into(),
                }
            }
        };
        self.sync();
        Frame {
            event,
            table: self.t.clone(),
            auto: false,
        }
    }
}

/// 由完整事件紀錄產生某個座位視角的幀（每個事件一幀，幀的編號 = 事件的位置）。
/// `open` = 公開四家的手牌（牌譜檢視用）：四家各自的視角同時推算，再把他家的手牌填進來。
pub fn log_frames(log: &[Event], seat: Seat, open: bool) -> Vec<Frame> {
    let mut builders: Vec<TableBuilder> = (0..4).map(TableBuilder::new).collect();
    log.iter()
        .map(|ev| {
            if !open {
                return builders[usize::from(seat)].apply(&ev.masked_for(seat));
            }
            let frames: Vec<Frame> = builders
                .iter_mut()
                .enumerate()
                .map(|(s, b)| b.apply(&ev.masked_for(s as Seat)))
                .collect();
            let mut frame = frames[usize::from(seat)].clone();
            for (s, f) in frames.iter().enumerate() {
                let v = &mut frame.table.seats[s];
                if s != usize::from(seat) && !v.revealed {
                    let own = &f.table.seats[s];
                    v.hand = own.hand.clone();
                    v.drawn = own.drawn.clone();
                    v.open = true;
                }
            }
            frame
        })
        .collect()
}

/// 牌桌與引擎視角不一致的地方（沒有時為 `None`）。用來確認由事件推算的牌桌是對的。
pub fn table_mismatch(t: &TableView, view: &PlayerView) -> Option<String> {
    let me = usize::from(t.me);
    let mut hand: Vec<String> = t.seats[me].hand.clone();
    hand.extend(t.seats[me].drawn.clone());
    hand.sort();
    let mut expected: Vec<String> = view.hand_pais.iter().map(|&p| pai_str(p)).collect();
    expected.sort();
    if hand != expected {
        return Some(format!("手牌不一致：{hand:?} ≠ {expected:?}"));
    }
    if t.scores != view.scores {
        return Some(format!("點數不一致：{:?} ≠ {:?}", t.scores, view.scores));
    }
    if t.wall_left != view.tiles_left_in_wall {
        return Some(format!(
            "牌山張數不一致：{} ≠ {}",
            t.wall_left, view.tiles_left_in_wall
        ));
    }
    if t.riichi_sticks != view.riichi_sticks {
        return Some("供託不一致".into());
    }
    for s in 0..4 {
        let seat = &t.seats[s];
        let river: Vec<String> = view.rivers[s].iter().map(|d| pai_str(d.pai)).collect();
        let shown: Vec<String> = seat.river.iter().map(|r| r.pai.clone()).collect();
        if shown != river {
            return Some(format!("座位 {s} 的牌河不一致"));
        }
        if seat.melds.len() != view.melds[s].len() {
            return Some(format!("座位 {s} 的副露不一致"));
        }
        if seat.riichi != view.riichi[s] {
            return Some(format!("座位 {s} 的立直不一致"));
        }
        // 門前張數 + 副露張數（槓算 3 張）= 13 或 14。
        let melded: usize = view.melds[s].iter().map(|m| m.tiles.len().min(3)).sum();
        let total = usize::from(seat.hand_count) + melded;
        if total != 13 && total != 14 {
            return Some(format!("座位 {s} 的手牌張數 {total} 不對"));
        }
    }
    None
}
