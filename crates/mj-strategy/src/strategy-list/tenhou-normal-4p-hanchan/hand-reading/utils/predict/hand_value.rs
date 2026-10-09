//! 預測：某家如果這一局和牌，大約值多少點（打點讀み）。
//!
//! - 立直者：看不到手牌，用平均值；越晚立直越大（愚形或已經夠大才拖到後面立直、或是追趕）。
//! - 門前還沒立直的人：之後多半會立直；場上看不到的寶牌越多（可能在他手上），越可能變大。
//! - 副露者：看得到的寶牌（副露中的寶牌、赤五）越多越大；寶牌刻子（3 張以上）多半是滿貫；
//!   碰 2 組以上可能是對對和。
//! - 莊家 1.5 倍。
//!
//! 數值是天鳳鳳凰卓牌譜中「這一家最後和牌時」的統計（見 docs/03「讀牌」）。

use super::super::super::memory::Memory;
use crate::prelude::*;

/// 立直者和牌的平均點數（閒家）。
pub const RIICHI_POINTS: Param = Param::new(
    "hand_value.riichi_points",
    6300.0,
    3000.0,
    10000.0,
    "立直者和牌的平均點數（閒家）",
);
/// 立直者和牌是滿貫以上的機率：`[第 0–8 張立直, 第 9–11 張, 第 12 張以後]`。
pub const RIICHI_MANGAN: [f64; 3] = [0.32, 0.39, 0.48];
/// 門前沒立直的人和牌的平均點數（閒家；多半之後會立直）。
pub const CLOSED_POINTS: Param = Param::new(
    "hand_value.closed_points",
    6200.0,
    3000.0,
    10000.0,
    "門前沒立直的人和牌的平均點數（閒家）",
);
/// 門前沒立直的人和牌是滿貫以上的機率：`[看不到的寶牌 3 張以下, 4, 5, 6 張以上]`。
pub const CLOSED_MANGAN: [f64; 4] = [0.15, 0.24, 0.27, 0.30];
/// 副露者和牌的平均點數（閒家）：`[副露中的寶牌 0 張, 1, 2, 3, 4 張以上]`。
pub const OPEN_POINTS: [f64; 5] = [4300.0, 5300.0, 6700.0, 8900.0, 12000.0];
/// 副露者和牌是滿貫以上的機率（同上分組）。
pub const OPEN_MANGAN: [f64; 5] = [0.13, 0.13, 0.2, 0.55, 0.95];
/// 副露者碰（含槓）2 組以上、寶牌 2 張以下時的滿貫機率（對對和的可能）。
pub const OPEN_TOITOI_MANGAN: f64 = 0.26;
/// 莊家的點數倍率。
pub const DEALER: f64 = 1.5;

/// 常駐預測：其他三家如果和牌，是不是滿貫以上（只為了評估準確度）。
pub fn each_turn(ctx: &Ctx, mem: &mut Memory) {
    for seat in tools::table::opponents(ctx.view) {
        let (_, p) = estimate(ctx, mem, seat);
        ctx.claim("predict::hand_value", Claim::BigHand { seat, p });
    }
}

/// `seat` 如果和牌：（平均點數, 滿貫以上的機率）。
pub fn estimate(ctx: &Ctx, _mem: &Memory, seat: Seat) -> (f64, f64) {
    let v = ctx.view;
    let s = usize::from(seat);
    let (points, mangan) = if v.riichi[s] {
        let turn = v.rivers[s]
            .iter()
            .position(|d| d.riichi_declared)
            .unwrap_or(0);
        let group = match turn {
            0..=8 => 0,
            9..=11 => 1,
            _ => 2,
        };
        (ctx.param(&RIICHI_POINTS), RIICHI_MANGAN[group])
    } else if tools::river::open_melds(v, seat) > 0 {
        let dora = usize::from(tools::river::meld_dora(v, seat)).min(4);
        let pons = v.melds[s].iter().filter(|m| !m.is_sequence()).count();
        let mangan = if pons >= 2 && dora <= 2 {
            OPEN_TOITOI_MANGAN
        } else {
            OPEN_MANGAN[dora]
        };
        (OPEN_POINTS[dora], mangan)
    } else {
        let unseen = usize::from(tools::dora::unseen_dora(v));
        (
            ctx.param(&CLOSED_POINTS),
            CLOSED_MANGAN[unseen.clamp(3, 6) - 3],
        )
    };
    let dealer = if v.dealer() == seat { DEALER } else { 1.0 };
    (points * dealer, mangan)
}
