//! 和牌計分：番（han）、符（fu）、點數。
//!
//! 同一手牌可能有多種「解釋」：每一種拆法（見 `mj_calc::decompose`）×和牌張可能完成的
//! 每一組面子（聽法不同，符數和平和等役也不同），再加上七對子、國士無雙。
//! 每種解釋都算一次，取點數最高者——這也是實際規則的要求（高點法）。
//!
//! 點數計算流程：
//! 1. 判定役滿；有役滿就只算役滿。
//! 2. 否則列出所有役 → 加上寶牌、裏寶牌、赤寶牌 → 總番數。
//! 3. 計算符數（20 符底符＋門前榮和、自摸、雀頭、面子、聽法，進位到 10）。
//! 4. 基本點 = 符 × 2^(番+2)，達 2000 以上為滿貫；5 番以上依番數決定滿貫／跳滿／倍滿……
//! 5. 支付：榮和由放銃者付（閒家 ×4、莊家 ×6）；自摸由三家分攤。

use super::RiichiConfig;
use super::yaku::Yaku;
use mj_calc::{Block, decompose, is_chiitoi, is_kokushi};
use mj_core::{Meld, MeldKind, Tile, TileCounts};
use serde::{Deserialize, Serialize};

/// 立直狀態。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum RiichiState {
    /// 未立直
    #[default]
    None,
    /// 立直
    Riichi,
    /// 兩立直（第一巡立直）
    Double,
}

/// 和牌時的狀況：從牌面看不出來、但會影響役與點數的資訊。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WinContext {
    /// 和牌張（自摸摸到的牌／榮和的那張牌）
    pub win_tile: Tile,
    /// true = 自摸（tsumo），false = 榮和（ron）
    pub tsumo: bool,
    /// 立直狀態
    pub riichi: RiichiState,
    /// 一發（ippatsu）是否仍有效
    pub ippatsu: bool,
    /// 海底（haitei）：摸牌山最後一張自摸
    pub haitei: bool,
    /// 河底（houtei）：榮和最後一張打出的牌
    pub houtei: bool,
    /// 嶺上（rinshan）：開槓後補的牌自摸
    pub rinshan: bool,
    /// 搶槓（chankan）：榮和他家加槓的牌
    pub chankan: bool,
    /// 第一次摸牌、且之前無人鳴牌就自摸（莊家＝天和，閒家＝地和）
    pub first_draw: bool,
    /// 和牌者是否為莊家（oya）
    pub dealer: bool,
    /// 門風：0 = 東 .. 3 = 北
    pub seat_wind: u8,
    /// 場風：0 = 東場、1 = 南場……
    pub round_wind: u8,
    /// 寶牌指示牌（dora）
    pub dora_indicators: Vec<Tile>,
    /// 裏寶牌指示牌（ura dora），只有立直時才計算
    pub ura_indicators: Vec<Tile>,
    /// 手牌（含副露）中的赤五張數
    pub red_fives: u8,
}

impl WinContext {
    /// 預設狀況：東場、南家（閒家）、無寶牌、無立直；其餘欄位視需要再修改。
    pub fn new(win_tile: Tile, tsumo: bool) -> Self {
        Self {
            win_tile,
            tsumo,
            riichi: RiichiState::None,
            ippatsu: false,
            haitei: false,
            houtei: false,
            rinshan: false,
            chankan: false,
            first_draw: false,
            dealer: false,
            seat_wind: 1,
            round_wind: 0,
            dora_indicators: Vec::new(),
            ura_indicators: Vec::new(),
            red_fives: 0,
        }
    }
}

/// 滿貫以上的等級。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Limit {
    /// 未達滿貫，依符與番計算
    None,
    /// 滿貫（mangan）：5 番，或基本點達 2000
    Mangan,
    /// 跳滿（haneman）：6–7 番
    Haneman,
    /// 倍滿（baiman）：8–10 番
    Baiman,
    /// 三倍滿（sanbaiman）：11–12 番
    Sanbaiman,
    /// 役滿（yakuman），數字為倍數（累計役滿算 1 倍）
    Yakuman(u8),
}

/// 計分結果。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Score {
    /// 成立的役與各自的番數（役滿每個記 13）
    pub yaku: Vec<(Yaku, u8)>,
    /// 寶牌（dora）張數
    pub dora: u8,
    /// 裏寶牌（ura dora）張數
    pub ura: u8,
    /// 赤寶牌（aka dora，赤五）張數
    pub aka: u8,
    /// 總番數，含寶牌（役滿時為 0）
    pub han: u8,
    /// 符數
    pub fu: u8,
    /// 滿貫以上的等級
    pub limit: Limit,
    /// 基本點：乘上莊家／閒家、自摸／榮和的倍數之前的點數
    pub base: i32,
}

/// 無條件進位到百位。
fn round100(x: i32) -> i32 {
    (x + 99) / 100 * 100
}

impl Score {
    /// 榮和時放銃者支付的點數（閒家和牌 = 基本點 ×4，莊家 ×6），每本場加 300。
    pub fn ron_points(&self, dealer: bool, honba: u8) -> i32 {
        round100(self.base * if dealer { 6 } else { 4 }) + 300 * i32::from(honba)
    }

    /// 自摸時的支付：`(莊家付, 每個閒家付)`。
    /// 閒家自摸：莊家付基本點 ×2、閒家付 ×1；莊家自摸：三家都付 ×2（兩個值相同）。每本場每人加 100。
    pub fn tsumo_points(&self, dealer: bool, honba: u8) -> (i32, i32) {
        let h = 100 * i32::from(honba);
        if dealer {
            let each = round100(self.base * 2) + h;
            (each, each)
        } else {
            (round100(self.base * 2) + h, round100(self.base) + h)
        }
    }

    /// 是否為役滿。
    pub fn is_yakuman(&self) -> bool {
        matches!(self.limit, Limit::Yakuman(_))
    }
}

/// 風位編號對應的風牌（0 = 東 .. 3 = 北）。
fn wind(w: u8) -> Tile {
    Tile::wind(w)
}

/// 由寶牌指示牌求寶牌：指示牌的「下一張」。
/// 數牌 9 的下一張是 1；風牌 東→南→西→北→東；三元牌 白→發→中→白。
pub fn dora_from_indicator(indicator: Tile) -> Tile {
    let i = indicator.index() as u8;
    let next = match i {
        0..=26 => i - i % 9 + (i % 9 + 1) % 9,
        27..=30 => 27 + (i - 27 + 1) % 4,
        _ => 31 + (i - 31 + 1) % 3,
    };
    Tile::from_index(next)
}

/// 面子種類（計分用，副露與門前一起處理）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SetKind {
    /// 順子
    Seq,
    /// 刻子
    Trip,
    /// 槓子
    Kan,
}

/// 一組面子（計分用）。
#[derive(Debug, Clone, Copy)]
struct Set {
    kind: SetKind,
    /// 順子的第一張，或刻子／槓子的牌
    tile: Tile,
    /// 是否算「明」的（影響符數與暗刻數）：鳴牌而來，或靠榮和完成的刻子（對碰榮和算明刻）。
    open: bool,
}

impl Set {
    /// 這組面子的每一張牌。
    fn tiles(&self) -> impl Iterator<Item = Tile> {
        let t = self.tile;
        let (n, step) = match self.kind {
            SetKind::Seq => (3, 1),
            SetKind::Trip => (3, 0),
            SetKind::Kan => (4, 0),
        };
        (0..n).map(move |k| Tile::from_index(t.index() as u8 + k * step))
    }

    /// 是否含幺九牌（全帶么九判定用）。
    fn has_yaochu(&self) -> bool {
        self.tiles().any(|t| t.is_yaochu())
    }

    /// 是否為刻子或槓子。
    fn is_pon_like(&self) -> bool {
        self.kind != SetKind::Seq
    }
}

/// 聽法（和牌張完成的是哪一種搭子）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Wait {
    /// 兩面（ryanmen），例：45m 和 3m/6m
    Ryanmen,
    /// 嵌張（kanchan），例：46m 和 5m（+2 符）
    Kanchan,
    /// 邊張（penchan），例：12m 和 3m、89m 和 7m（+2 符）
    Penchan,
    /// 對碰（shanpon），兩組對子其中一組成刻
    Shanpon,
    /// 單騎（tanki），只聽雀頭（+2 符）
    Tanki,
}

/// 一種解釋：全部面子（門前＋副露）、雀頭、聽法。
struct Interp {
    sets: Vec<Set>,
    pair: Tile,
    wait: Wait,
}

/// 副露轉成計分用的面子（暗槓算暗，其餘算明）。
fn meld_set(m: &Meld) -> Set {
    let kind = match m.kind {
        MeldKind::Chi => SetKind::Seq,
        MeldKind::Pon => SetKind::Trip,
        _ => SetKind::Kan,
    };
    Set {
        kind,
        tile: m.base(),
        open: m.is_open(),
    }
}

/// 列出一般型的所有解釋：每種拆法 × 和牌張可能落在的每一組面子（或雀頭）。
fn interpretations(concealed: &TileCounts, melds: &[Meld], ctx: &WinContext) -> Vec<Interp> {
    let win = ctx.win_tile;
    let meld_sets: Vec<Set> = melds.iter().map(meld_set).collect();
    let mut out = Vec::new();
    for d in decompose(concealed) {
        let base: Vec<Set> = d
            .blocks
            .iter()
            .map(|b| match *b {
                Block::Sequence(t) => Set {
                    kind: SetKind::Seq,
                    tile: t,
                    open: false,
                },
                Block::Triplet(t) => Set {
                    kind: SetKind::Trip,
                    tile: t,
                    open: false,
                },
            })
            .chain(meld_sets.iter().copied())
            .collect();
        let n_concealed = d.blocks.len();
        // 和牌張是雀頭 → 單騎。
        if d.pair == win {
            out.push(Interp {
                sets: base.clone(),
                pair: d.pair,
                wait: Wait::Tanki,
            });
        }
        // 和牌張落在某組門前面子裡；相同的面子只需算一次。
        let mut seen: Vec<Block> = Vec::new();
        for (i, b) in d.blocks.iter().enumerate().take(n_concealed) {
            if seen.contains(b) {
                continue;
            }
            let wait = match *b {
                Block::Triplet(t) if t == win => Wait::Shanpon,
                Block::Sequence(s) if (s.index()..s.index() + 3).contains(&win.index()) => {
                    // 和牌張在順子中的位置：中間 = 嵌張；
                    // 789 的 7 或 123 的 3 = 邊張；其餘 = 兩面。
                    let pos = win.index() - s.index();
                    match pos {
                        1 => Wait::Kanchan,
                        0 if s.number() == 7 => Wait::Penchan,
                        2 if s.number() == 1 => Wait::Penchan,
                        _ => Wait::Ryanmen,
                    }
                }
                _ => continue,
            };
            seen.push(*b);
            let mut sets = base.clone();
            // 對碰榮和：靠別人的牌完成的刻子算明刻（符數減半、不算暗刻）。
            if wait == Wait::Shanpon && !ctx.tsumo {
                sets[i].open = true;
            }
            out.push(Interp {
                sets,
                pair: d.pair,
                wait,
            });
        }
    }
    out
}

/// 雀頭是否為役牌（三元牌、自風、場風）——有的話不成立平和，且雀頭有符。
fn is_yakuhai_pair(t: Tile, ctx: &WinContext) -> bool {
    t.is_dragon() || t == wind(ctx.seat_wind) || t == wind(ctx.round_wind)
}

/// 門前＋副露的全部牌。
fn all_tiles(concealed: &TileCounts, melds: &[Meld]) -> TileCounts {
    let mut all = *concealed;
    for m in melds {
        for p in &m.tiles {
            all.add(p.tile);
        }
    }
    all
}

/// 染手判定用：回傳（若只有一種數牌花色則為該花色編號，是否有字牌）。
/// 只有一種花色且無字牌 = 清一色；只有一種花色且有字牌 = 混一色。
fn suit_profile(all: &TileCounts) -> (Option<usize>, bool) {
    let suits: Vec<usize> = (0..3)
        .filter(|&s| all.0[s * 9..s * 9 + 9].iter().any(|&c| c > 0))
        .collect();
    let honors = all.0[27..].iter().any(|&c| c > 0);
    (
        if suits.len() == 1 {
            Some(suits[0])
        } else {
            None
        },
        honors,
    )
}

/// 綠一色可用的牌：2s 3s 4s 6s 8s 發（6z）。
const GREEN: [u8; 6] = [19, 20, 21, 23, 25, 32];

/// 只看牌的組成或狀況就能判定的役滿：字一色、清老頭、綠一色、天和、地和。
fn common_yakuman(all: &TileCounts, ctx: &WinContext, out: &mut Vec<Yaku>) {
    if all.tiles().all(|t| t.is_honor()) {
        out.push(Yaku::Tsuuiisou);
    }
    if all.tiles().all(|t| t.is_terminal()) {
        out.push(Yaku::Chinroutou);
    }
    if all.tiles().all(|t| GREEN.contains(&(t.index() as u8))) {
        out.push(Yaku::Ryuuiisou);
    }
    if ctx.first_draw && ctx.tsumo {
        out.push(if ctx.dealer {
            Yaku::Tenhou
        } else {
            Yaku::Chiihou
        });
    }
}

/// 狀況役：立直、兩立直、一發、門前清自摸和、海底、河底、嶺上、搶槓。
fn situational(ctx: &WinContext, closed: bool, out: &mut Vec<Yaku>) {
    match ctx.riichi {
        RiichiState::Riichi => out.push(Yaku::Riichi),
        RiichiState::Double => out.push(Yaku::DoubleRiichi),
        RiichiState::None => {}
    }
    if ctx.riichi != RiichiState::None && ctx.ippatsu {
        out.push(Yaku::Ippatsu);
    }
    if closed && ctx.tsumo {
        out.push(Yaku::MenzenTsumo);
    }
    if ctx.haitei {
        out.push(Yaku::Haitei);
    }
    if ctx.houtei {
        out.push(Yaku::Houtei);
    }
    if ctx.rinshan {
        out.push(Yaku::Rinshan);
    }
    if ctx.chankan {
        out.push(Yaku::Chankan);
    }
}

/// 只看牌的組成的一般役：斷么九（副露時看是否有食斷）、清一色、混一色、混老頭。
fn flush_and_tanyao(all: &TileCounts, closed: bool, cfg: &RiichiConfig, out: &mut Vec<Yaku>) {
    if all.tiles().all(|t| !t.is_yaochu()) && (closed || cfg.kuitan) {
        out.push(Yaku::Tanyao);
    }
    match suit_profile(all) {
        (Some(_), false) => out.push(Yaku::Chinitsu),
        (Some(_), true) => out.push(Yaku::Honitsu),
        _ => {}
    }
    // 全字牌（字一色）、全老頭牌（清老頭）是役滿，不會走到這裡，所以這裡只剩混老頭。
    if all.tiles().all(|t| t.is_yaochu()) {
        out.push(Yaku::Honroutou);
    }
}

/// 一種解釋下成立的役與符數（尚未加寶牌、尚未判斷門前限定役）。
struct Candidate {
    yaku: Vec<Yaku>,
    fu: u8,
}

/// 計算一般型（4 面子＋雀頭）某一種解釋的役與符。
fn standard_candidate(
    it: &Interp,
    all: &TileCounts,
    closed: bool,
    ctx: &WinContext,
    cfg: &RiichiConfig,
) -> Candidate {
    let sets = &it.sets;
    let mut ym = Vec::new();
    common_yakuman(all, ctx, &mut ym);
    // 暗刻數（含暗槓）：四暗刻、三暗刻用。
    let concealed_trips = sets.iter().filter(|s| s.is_pon_like() && !s.open).count();
    if concealed_trips == 4 {
        ym.push(Yaku::Suuankou);
    }
    let dragon_trips = sets
        .iter()
        .filter(|s| s.is_pon_like() && s.tile.is_dragon())
        .count();
    let wind_trips = sets
        .iter()
        .filter(|s| s.is_pon_like() && s.tile.is_wind())
        .count();
    if dragon_trips == 3 {
        ym.push(Yaku::Daisangen);
    }
    if wind_trips == 4 {
        ym.push(Yaku::Daisuushii);
    } else if wind_trips == 3 && it.pair.is_wind() {
        ym.push(Yaku::Shousuushii);
    }
    let kans = sets.iter().filter(|s| s.kind == SetKind::Kan).count();
    if kans == 4 {
        ym.push(Yaku::Suukantsu);
    }
    if closed && is_chuuren(all) {
        ym.push(Yaku::Chuuren);
    }
    // 有役滿就只算役滿，不計一般役。
    if !ym.is_empty() {
        return Candidate { yaku: ym, fu: 0 };
    }

    let mut y = Vec::new();
    situational(ctx, closed, &mut y);
    flush_and_tanyao(all, closed, cfg, &mut y);

    let seqs: Vec<Tile> = sets
        .iter()
        .filter(|s| s.kind == SetKind::Seq)
        .map(|s| s.tile)
        .collect();
    // 平和：門前、四組順子、雀頭不是役牌、兩面聽。
    let pinfu =
        closed && seqs.len() == 4 && !is_yakuhai_pair(it.pair, ctx) && it.wait == Wait::Ryanmen;
    if pinfu {
        y.push(Yaku::Pinfu);
    }
    // 一盃口／二盃口（門前限定）：數有幾對完全相同的順子。
    if closed {
        let mut sorted = seqs.clone();
        sorted.sort();
        let mut identical_pairs = 0;
        let mut i = 0;
        while i + 1 < sorted.len() {
            if sorted[i] == sorted[i + 1] {
                identical_pairs += 1;
                i += 2;
            } else {
                i += 1;
            }
        }
        match identical_pairs {
            1 => y.push(Yaku::Iipeikou),
            2 => y.push(Yaku::Ryanpeikou),
            _ => {}
        }
    }
    // 役牌：三元牌刻子、自風刻、場風刻（連風牌如東場東家的東算兩個役）。
    for s in sets.iter().filter(|s| s.is_pon_like()) {
        match s.tile.index() {
            31 => y.push(Yaku::Haku),
            32 => y.push(Yaku::Hatsu),
            33 => y.push(Yaku::Chun),
            _ => {}
        }
        if s.tile == wind(ctx.seat_wind) {
            y.push(Yaku::SeatWind);
        }
        if s.tile == wind(ctx.round_wind) {
            y.push(Yaku::RoundWind);
        }
    }
    // 全帶么九：每組面子與雀頭都含幺九牌，且至少一組順子（沒有順子就是混老頭）。
    // 有字牌 = 混全帶么九（chanta），沒有 = 純全帶么九（junchan）。
    if !seqs.is_empty() && sets.iter().all(|s| s.has_yaochu()) && it.pair.is_yaochu() {
        let honors = sets.iter().any(|s| s.tile.is_honor()) || it.pair.is_honor();
        y.push(if honors { Yaku::Chanta } else { Yaku::Junchan });
    }
    // 一氣通貫：同花色的 123、456、789 順子。
    for suit in 0..3u8 {
        let has = |n: u8| seqs.contains(&Tile::from_index(suit * 9 + n));
        if has(0) && has(3) && has(6) {
            y.push(Yaku::Ittsu);
        }
    }
    // 三色同順：萬筒索同數字的順子。
    for n in 0..7u8 {
        if (0..3).all(|s| seqs.contains(&Tile::from_index(s * 9 + n))) {
            y.push(Yaku::SanshokuDoujun);
            break;
        }
    }
    // 三色同刻：萬筒索同數字的刻子。
    for n in 0..9u8 {
        let trip = |t: u8| {
            sets.iter()
                .any(|s| s.is_pon_like() && s.tile.index() as u8 == t)
        };
        if (0..3).all(|s| trip(s * 9 + n)) {
            y.push(Yaku::SanshokuDoukou);
        }
    }
    if kans == 3 {
        y.push(Yaku::Sankantsu);
    }
    if seqs.is_empty() {
        y.push(Yaku::Toitoi);
    }
    if concealed_trips == 3 {
        y.push(Yaku::Sanankou);
    }
    if dragon_trips == 2 && it.pair.is_dragon() {
        y.push(Yaku::Shousangen);
    }
    // 二盃口的牌型同時也是七對子；兩種解釋都會算，最後取點數高者（二盃口）。

    Candidate {
        yaku: y,
        fu: fu(it, closed, pinfu, ctx),
    }
}

/// 符數計算（一般型）。
///
/// - 底符 20；門前榮和 +10；自摸 +2（平和自摸固定 20 符）
/// - 雀頭：三元牌、自風、場風各 +2（連風牌 +4）
/// - 面子：明刻 2、暗刻 4、明槓 8、暗槓 16；幺九牌再加倍
/// - 聽法：嵌張、邊張、單騎 +2
/// - 副露且無任何加符（食平和型）固定 30 符；其餘進位到 10 符
fn fu(it: &Interp, closed: bool, pinfu: bool, ctx: &WinContext) -> u8 {
    if pinfu && ctx.tsumo {
        return 20;
    }
    let mut fu: u32 = 20;
    if closed && !ctx.tsumo {
        fu += 10;
    }
    if ctx.tsumo {
        fu += 2;
    }
    let p = it.pair;
    if p.is_dragon() {
        fu += 2;
    }
    if p == wind(ctx.seat_wind) {
        fu += 2;
    }
    if p == wind(ctx.round_wind) {
        fu += 2;
    }
    for s in &it.sets {
        let mut f = match s.kind {
            SetKind::Seq => continue,
            SetKind::Trip => 2,
            SetKind::Kan => 8,
        };
        if s.tile.is_yaochu() {
            f *= 2;
        }
        if !s.open {
            f *= 2;
        }
        fu += f;
    }
    if matches!(it.wait, Wait::Kanchan | Wait::Penchan | Wait::Tanki) {
        fu += 2;
    }
    // 副露且沒有任何加符（食平和型）固定 30 符。
    if !closed && fu == 20 {
        return 30;
    }
    fu.div_ceil(10) as u8 * 10
}

/// 九蓮寶燈：清一色（無字牌）14 張，且包含 1112345678999。
fn is_chuuren(all: &TileCounts) -> bool {
    let Some(suit) = suit_profile(all)
        .0
        .filter(|_| !all.0[27..].iter().any(|&c| c > 0))
    else {
        return false;
    };
    let c = &all.0[suit * 9..suit * 9 + 9];
    const BASE: [u8; 9] = [3, 1, 1, 1, 1, 1, 1, 1, 3];
    all.total() == 14 && c.iter().zip(BASE).all(|(&h, b)| h >= b)
}

/// 特殊型的解釋：國士無雙、七對子（只限門前、無副露）。
fn special_candidates(
    concealed: &TileCounts,
    melds: &[Meld],
    ctx: &WinContext,
    cfg: &RiichiConfig,
) -> Vec<Candidate> {
    let mut out = Vec::new();
    if !melds.is_empty() {
        return out;
    }
    if is_kokushi(concealed) {
        let mut ym = vec![Yaku::Kokushi];
        if ctx.first_draw && ctx.tsumo {
            ym.push(if ctx.dealer {
                Yaku::Tenhou
            } else {
                Yaku::Chiihou
            });
        }
        out.push(Candidate { yaku: ym, fu: 0 });
    }
    if is_chiitoi(concealed) {
        // 七對子型也可能是字一色、天和、地和。
        let mut ym = Vec::new();
        common_yakuman(concealed, ctx, &mut ym);
        if !ym.is_empty() {
            out.push(Candidate { yaku: ym, fu: 0 });
        } else {
            let mut y = vec![Yaku::Chiitoitsu];
            situational(ctx, true, &mut y);
            flush_and_tanyao(concealed, true, cfg, &mut y);
            out.push(Candidate { yaku: y, fu: 25 });
        }
    }
    out
}

/// 寶牌張數：每個指示牌各自計算（同一指示牌重複出現就算兩次）。
fn count_dora(all: &TileCounts, indicators: &[Tile]) -> u8 {
    indicators
        .iter()
        .map(|&i| all.get(dora_from_indicator(i)))
        .sum()
}

/// 由役與符算出最終分數；沒有役時回傳 `None`（寶牌不算役）。
fn finish(
    c: Candidate,
    all: &TileCounts,
    closed: bool,
    ctx: &WinContext,
    cfg: &RiichiConfig,
) -> Option<Score> {
    if c.yaku.is_empty() {
        return None;
    }
    if c.yaku[0].is_yakuman() {
        let n = if cfg.composite_yakuman {
            c.yaku.len() as u8
        } else {
            1
        };
        return Some(Score {
            yaku: c.yaku.iter().map(|&y| (y, 13)).collect(),
            dora: 0,
            ura: 0,
            aka: 0,
            han: 0,
            fu: 0,
            limit: Limit::Yakuman(n),
            base: 8000 * i32::from(n),
        });
    }
    // 依門前／副露取得番數；門前限定役在副露時被濾掉。
    let yaku: Vec<(Yaku, u8)> = c
        .yaku
        .iter()
        .filter_map(|&y| y.han(closed).map(|h| (y, h)))
        .collect();
    if yaku.is_empty() {
        return None;
    }
    let dora = count_dora(all, &ctx.dora_indicators);
    let ura = if ctx.riichi != RiichiState::None {
        count_dora(all, &ctx.ura_indicators)
    } else {
        0
    };
    let aka = ctx.red_fives;
    let han = yaku.iter().map(|&(_, h)| h).sum::<u8>() + dora + ura + aka;
    let fu = c.fu;
    // 基本點 = 符 × 2^(番+2)；未滿 5 番但基本點達 2000 也算滿貫。
    let (limit, base) = match han {
        13.. if cfg.kazoe_yakuman => (Limit::Yakuman(1), 8000),
        11.. => (Limit::Sanbaiman, 6000),
        8..=10 => (Limit::Baiman, 4000),
        6..=7 => (Limit::Haneman, 3000),
        5 => (Limit::Mangan, 2000),
        _ => {
            let b = i32::from(fu) << (2 + han);
            if b >= 2000 || (cfg.kiriage && b == 1920) {
                (Limit::Mangan, 2000)
            } else {
                (Limit::None, b)
            }
        }
    };
    Some(Score {
        yaku,
        dora,
        ura,
        aka,
        han,
        fu,
        limit,
        base,
    })
}

/// 計算一手和牌的分數。`concealed` 是門前部分，包含和牌張；`melds` 是副露。
/// 不是和牌型或沒有役時回傳 `None`。所有解釋中取基本點最高者（同分取番數、符數高者）。
pub fn score(
    concealed: &TileCounts,
    melds: &[Meld],
    ctx: &WinContext,
    cfg: &RiichiConfig,
) -> Option<Score> {
    let closed = melds.iter().all(|m| !m.is_open());
    let all = all_tiles(concealed, melds);
    let mut cands: Vec<Candidate> = interpretations(concealed, melds, ctx)
        .iter()
        .map(|it| standard_candidate(it, &all, closed, ctx, cfg))
        .collect();
    cands.extend(special_candidates(concealed, melds, ctx, cfg));
    cands
        .into_iter()
        .filter_map(|c| finish(c, &all, closed, ctx, cfg))
        .max_by_key(|s| (s.base, s.han, s.fu))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mj_core::{Pai, parse_tiles};

    fn h(s: &str) -> TileCounts {
        TileCounts::from_tiles(parse_tiles(s).unwrap())
    }

    fn t(s: &str) -> Tile {
        s.parse().unwrap()
    }

    /// 測試用的和牌狀況（南家、東場）。
    fn ctx(win: &str, tsumo: bool) -> WinContext {
        WinContext::new(t(win), tsumo)
    }

    /// 建立碰的副露。
    fn pon(s: &str, from: u8) -> Meld {
        let p: Pai = s.parse().unwrap();
        Meld {
            kind: MeldKind::Pon,
            tiles: vec![p; 3],
            called: Some(p),
            from: Some(from),
        }
    }

    /// 建立吃的副露。
    fn chi(s: &str, called: &str) -> Meld {
        let tiles = mj_core::parse_pais(s).unwrap();
        Meld {
            kind: MeldKind::Chi,
            tiles,
            called: Some(called.parse().unwrap()),
            from: Some(0),
        }
    }

    fn names(s: &Score) -> Vec<Yaku> {
        s.yaku.iter().map(|&(y, _)| y).collect()
    }

    const CFG: RiichiConfig = RiichiConfig::TENHOU;

    /// 立直＋門前清自摸＋平和：3 番 20 符，閒家自摸 700/1300。
    #[test]
    fn riichi_pinfu_tsumo_3han_20fu() {
        let mut c = ctx("1m", true);
        c.riichi = RiichiState::Riichi;
        let s = score(&h("123456m789p234s55s"), &[], &c, &CFG).unwrap();
        assert_eq!((s.han, s.fu), (3, 20));
        assert_eq!(s.tsumo_points(false, 0), (1300, 700));
    }

    /// 平和＋斷么九榮和：2 番 30 符 = 2000；莊家 1 本場 = 2900 + 300。
    #[test]
    fn pinfu_tanyao_ron() {
        let s = score(&h("234m345p456s678s22p"), &[], &ctx("2m", false), &CFG).unwrap();
        assert_eq!((s.han, s.fu), (2, 30));
        assert_eq!(s.ron_points(false, 0), 2000);
        assert_eq!(s.ron_points(true, 1), 2900 + 300);
    }

    /// 碰中、單騎榮和：20 + 么九暗刻 8 + 明刻中 4 + 單騎 2 = 34 → 40 符，1 番 = 1300。
    #[test]
    fn open_chun_tanki_40fu() {
        let s = score(
            &h("111m234p567s99s"),
            &[pon("7z", 0)],
            &ctx("9s", false),
            &CFG,
        )
        .unwrap();
        assert_eq!(names(&s), [Yaku::Chun]);
        assert_eq!((s.han, s.fu), (1, 40));
        assert_eq!(s.ron_points(false, 0), 1300);
    }

    /// 立直＋七對子：3 番 25 符 = 3200。
    #[test]
    fn riichi_chiitoi_ron() {
        let mut c = ctx("3p", false);
        c.riichi = RiichiState::Riichi;
        let s = score(&h("1133557799m1133p"), &[], &c, &CFG).unwrap();
        assert_eq!((s.han, s.fu), (3, 25));
        assert_eq!(s.ron_points(false, 0), 3200);
    }

    /// 111222333m：三暗刻解釋（3 番 40 符）高於平和一盃口解釋（3 番 20 符），取前者。
    #[test]
    fn best_interpretation_sanankou_over_pinfu() {
        let s = score(&h("111222333m456p55s"), &[], &ctx("1m", true), &CFG).unwrap();
        assert!(names(&s).contains(&Yaku::Sanankou));
        assert_eq!((s.han, s.fu), (3, 40));
        assert_eq!(s.tsumo_points(false, 0), (2600, 1300));
    }

    /// 副露後沒有役（無役）不能和牌。
    #[test]
    fn no_yaku_open_hand() {
        let melds = [chi("123m", "1m")];
        assert!(score(&h("456p789s234s55m"), &melds, &ctx("5m", false), &CFG).is_none());
    }

    /// 大三元榮和 32000；莊家國士無雙自摸 16000 all。
    #[test]
    fn yakuman() {
        let s = score(&h("555666777z123m44p"), &[], &ctx("4p", false), &CFG).unwrap();
        assert_eq!(names(&s), [Yaku::Daisangen]);
        assert_eq!(s.ron_points(false, 0), 32000);
        let mut c = ctx("1z", true);
        c.dealer = true;
        c.seat_wind = 0;
        let s = score(&h("19m19p19s12345677z"), &[], &c, &CFG).unwrap();
        assert_eq!(s.tsumo_points(true, 0), (16000, 16000));
    }

    /// 寶牌、赤寶牌加番後達到跳滿。
    #[test]
    fn dora_and_limits() {
        let mut c = ctx("2m", false);
        c.dora_indicators = vec![t("1m"), t("1p")]; // 寶牌為 2m、2p
        c.red_fives = 1;
        // 平和、斷么九（2）＋寶牌 2m×1、2p×2（3）＋赤寶牌 1 = 6 番 → 跳滿。
        let s = score(&h("234m345p456s678s22p"), &[], &c, &CFG).unwrap();
        assert_eq!((s.dora, s.han), (3, 6));
        assert_eq!(s.limit, Limit::Haneman);
        assert_eq!(s.ron_points(false, 0), 12000);
    }

    /// 指示牌循環：9m→1m、北→東、中→白、白→發。
    #[test]
    fn dora_indicator_wraps() {
        assert_eq!(dora_from_indicator(t("9m")), t("1m"));
        assert_eq!(dora_from_indicator(t("4z")), t("1z"));
        assert_eq!(dora_from_indicator(t("7z")), t("5z"));
        assert_eq!(dora_from_indicator(t("5z")), t("6z"));
    }
}
