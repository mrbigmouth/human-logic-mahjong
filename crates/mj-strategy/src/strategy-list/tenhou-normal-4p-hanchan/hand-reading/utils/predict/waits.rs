//! 預測：某家（假設已聽牌）聽不聽某種牌——打出去會不會放銃給他。
//!
//! 判斷順序：
//! 1. 現物 → 0。
//! 2. 字牌：依看得到的張數查表（看得到越多越安全；對他是役牌時，副露者常用役牌對碰）。
//! 3. 數牌：依牌的位置（1·9／2·8／3·7／4·5·6）查基準值，再依安全度的線索調整勝算（odds）：
//!    - 筋、片筋（兩面聽不到）；No Chance、One Chance（兩面搭子不可能或很難存在）；
//!    - 寶牌、寶牌旁邊的牌（大家都想留在手上，常變成聽牌張）；
//!    - 他捨牌中這個花色多 → 不像在做這門（副露者特別明顯，例如染手）；
//!    - 副露者：最後手切的牌旁邊（相差 1–2）的牌較危險（剛拆掉的搭子附近）；
//!    - 立直者：宣言牌的筋不像一般的筋那麼安全；
//!    - 巡目越後面越危險（剩下的牌少，他等的牌比較可能還在別人手上被打出）。
//! 4. 記憶中若此人用過筋牌騙，筋牌再危險一些。
//!
//! 數值以天鳳鳳凰卓牌譜的統計校準（見 tenpai.rs 的說明）。

use super::super::super::memory::Memory;
use super::super::odds;
use crate::prelude::*;

/// 立直者、數牌的基準聽牌機率：`[1·9, 2·8, 3·7, 4·5·6]`。
/// 基準情況：無筋、兩面搭子兩種都可能、不是寶牌、這個花色佔他捨牌的 1/4、第 10 巡。
pub const RIICHI_BASE: [f64; 4] = [0.086, 0.105, 0.109, 0.143];
/// 未立直者（副露或默聽）、數牌的基準聽牌機率（同上）。
pub const OPEN_BASE: [f64; 4] = [0.067, 0.089, 0.102, 0.109];

/// 數牌的勝算倍數（立直者, 未立直者）。
pub struct Factors {
    /// 片筋（4·5·6 只有一邊是現物）
    pub half_suji: f64,
    /// 筋（含両筋）
    pub suji: f64,
    /// No Chance：兩面搭子不可能
    pub no_chance: f64,
    /// One Chance：兩面搭子剩最後 1 張
    pub one_chance: f64,
    /// 這張牌已經看得到的張數，每多 1 張（相對於 1 張）
    pub per_visible: f64,
    /// 寶牌
    pub dora: f64,
    /// 寶牌旁邊（相差 1）
    pub dora_neighbor: f64,
    /// 他捨牌中這個花色的比例每多 10%（相對於 25%）
    pub per_suit_share: f64,
    /// 立直宣言牌的筋
    pub riichi_tile_suji: f64,
    /// 最後手切的牌旁邊（相差 1–2）
    pub near_last_tedashi: f64,
    /// 他每多打 1 張（相對於第 10 巡）
    pub per_turn: f64,
}

/// 立直者的勝算倍數。
pub const RIICHI: Factors = Factors {
    half_suji: 0.51,
    suji: 0.22,
    no_chance: 0.28,
    one_chance: 0.59,
    per_visible: 1.0,
    dora: 1.24,
    dora_neighbor: 1.36,
    per_suit_share: 1.0,
    riichi_tile_suji: 1.77,
    near_last_tedashi: 1.0,
    per_turn: 1.11,
};
/// 未立直者（副露或默聽）的勝算倍數。
pub const OPEN: Factors = Factors {
    half_suji: 0.68,
    suji: 0.41,
    no_chance: 0.34,
    one_chance: 0.58,
    per_visible: 0.78,
    dora: 1.46,
    dora_neighbor: 1.28,
    per_suit_share: 0.84,
    riichi_tile_suji: 1.0,
    near_last_tedashi: 1.47,
    per_turn: 1.09,
};

/// 字牌的聽牌機率：`[看得到 1 張, 2 張, 3 張]`（4 張時不可能）。
pub const RIICHI_HONOR: [f64; 3] = [0.065, 0.025, 0.006];
/// 未立直者、不是他役牌的字牌。
pub const OPEN_HONOR: [f64; 3] = [0.051, 0.029, 0.001];
/// 未立直者、他的役牌（副露者常以役牌對碰或單騎聽）。
pub const OPEN_YAKUHAI: [f64; 3] = [0.11, 0.055, 0.003];
/// 用過筋牌騙的人，筋牌的勝算再乘上這個值。
pub const SUJI_TRAP: Param = Param::new(
    "waits.suji_trap",
    2.5,
    1.0,
    5.0,
    "用過筋牌騙的人，筋牌的勝算倍數",
);

/// 威脅門檻：常駐預測只評估聽牌機率至少這麼高的人（和防守時提防的對象相近）。
pub const EACH_TURN_MIN_TENPAI: f64 = 0.3;

/// 常駐預測：對可能已聽牌的對手，判斷自己手上每種牌打出去會不會被榮和（只為了評估準確度）。
/// 記錄的機率 = 他已聽牌的機率 × 他聽這張的機率（也就是放銃機率）。
pub fn each_turn(ctx: &Ctx, mem: &mut Memory) {
    let v = ctx.view;
    for seat in tools::table::opponents(v) {
        let p_tenpai = super::tenpai::estimate(ctx, mem, seat);
        if p_tenpai < EACH_TURN_MIN_TENPAI {
            continue;
        }
        for tile in Tile::all().filter(|&t| v.hand.get(t) > 0) {
            let p = p_tenpai * estimate(ctx, mem, seat, tile);
            ctx.claim("predict::waits", Claim::Waits { seat, tile, p });
        }
    }
}

/// `seat` 此刻聽 `tile` 的機率（假設他已聽牌；只計算，不記錄預測）。
pub fn estimate(ctx: &Ctx, mem: &Memory, seat: Seat, tile: Tile) -> f64 {
    let v = ctx.view;
    let s = usize::from(seat);
    let riichi = v.riichi[s];
    if tools::safety::is_genbutsu(v, seat, tile) {
        return 0.0;
    }
    let seen = tools::visible::visible_count(v, tile);
    if tile.is_honor() {
        let table = if riichi {
            RIICHI_HONOR
        } else if tools::table::is_yakuhai_for(v, seat, tile) {
            OPEN_YAKUHAI
        } else {
            OPEN_HONOR
        };
        return match seen {
            0 | 1 => table[0],
            2 => table[1],
            3 => table[2],
            _ => 0.0,
        };
    }
    let (base, f) = if riichi {
        (RIICHI_BASE, &RIICHI)
    } else {
        (OPEN_BASE, &OPEN)
    };
    let position = match tile.number() {
        1 | 9 => 0,
        2 | 8 => 1,
        3 | 7 => 2,
        _ => 3,
    };
    let mut factor = 1.0;
    let suji = tools::safety::suji(v, seat, tile);
    match suji {
        tools::safety::Suji::Full => {
            factor *= f.suji;
            if mem.suji_trap_wins[s] > 0 {
                factor *= ctx.param(&SUJI_TRAP);
            }
            if riichi_tile_suji(v, seat, tile) {
                factor *= f.riichi_tile_suji;
            }
        }
        tools::safety::Suji::Half => factor *= f.half_suji,
        tools::safety::Suji::None => {}
    }
    if tools::safety::ryanmen_chances(v, tile) == 0 {
        factor *= f.no_chance;
    }
    if tools::safety::is_one_chance(v, tile) {
        factor *= f.one_chance;
    }
    factor *= f.per_visible.powi(i32::from(seen.max(1)) - 1);
    let dora = tools::dora::dora_tiles(v);
    if dora.contains(&tile) {
        factor *= f.dora;
    }
    if dora.iter().any(|&d| distance(d, tile) == Some(1)) {
        factor *= f.dora_neighbor;
    }
    let suits = tools::river::suit_counts(v, seat);
    let total = suits.iter().map(|&c| f64::from(c)).sum::<f64>().max(1.0);
    let share = f64::from(suits[tools::river::suit_index(tile.suit())]) / total;
    factor *= f.per_suit_share.powf((share - 0.25) * 10.0);
    if let Some((_, pai)) = tools::river::last_tedashi(v, seat)
        && matches!(distance(pai.tile, tile), Some(1 | 2))
    {
        factor *= f.near_last_tedashi;
    }
    let turn = tools::river::turn(v, seat).min(18) as f64;
    factor *= f.per_turn.powf(turn - 10.0);
    odds::scale(base[position], factor)
}

/// 同花色數牌的距離；不同花色或字牌為 `None`。
fn distance(a: Tile, b: Tile) -> Option<u8> {
    (a.suit() == b.suit() && !a.is_honor()).then(|| a.number().abs_diff(b.number()))
}

/// `tile` 是否為 `seat` 立直宣言牌的筋（相差 3）。
fn riichi_tile_suji(v: &PlayerView, seat: Seat, tile: Tile) -> bool {
    tools::safety::riichi_declaration_tile(v, seat).is_some_and(|r| distance(r, tile) == Some(3))
}
