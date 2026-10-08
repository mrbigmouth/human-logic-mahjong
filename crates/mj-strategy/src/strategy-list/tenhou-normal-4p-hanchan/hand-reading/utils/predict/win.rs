//! 預測：本局最後由我和牌的機率（`IWinThisHand`）。
//!
//! 想法：把剩下的巡目一巡一巡走過去——
//! - 還沒聽牌時，每巡摸到進張（前進一向聽）的機率 = 進張張數 ÷ 看不到的牌；
//! - 聽牌後，每巡和牌的機率 = 聽牌張數 ÷ 看不到的牌 ×（自摸 1 次＋他家打牌的放銃機會）；
//! - 每巡也有一定機率被別人先和掉（有人立直或有威脅的副露時更高）。
//!
//! 參數都是具名常數，用 `mj predict-eval` 的 `predict::win` 校準。

use super::super::super::memory::Memory;
use crate::prelude::*;

/// 聽牌前不知道會聽幾張，假設聽牌後平均剩這麼多張聽牌張。
pub const ASSUMED_WAITS: f64 = 5.0;
/// 下一步之後的每一步（還不知道手牌會變成什麼樣）假設的進張張數。
/// 只有「下一步」用實際的進張；否則進張很廣的 2 向聽會被估得比進張窄的 1 向聽還快。
pub const LATER_UKEIRE: f64 = 12.0;
/// 他家打牌給我榮和的機會，相當於每巡幾次「摸牌」（沒有人立直時）。
pub const RON_CHANCES: f64 = 0.4;
/// 自己已立直時，他家會防守，榮和機會降低到這個值。
pub const RON_CHANCES_WHEN_RIICHI: f64 = 0.8;
/// 每巡被別人先和掉的基本機率。
pub const OTHERS_WIN_PER_TURN: f64 = 0.07;
/// 每多一家威脅（立直，或聽牌機率高的沒立直者，以聽牌機率計），每巡被別人先和掉的機率再加這麼多。
pub const OTHERS_WIN_PER_THREAT: f64 = 0.06;

/// 常駐預測：每次輪到自己時，以目前最好的打法估計自己和牌的機率（只為了評估準確度）。
pub fn each_turn(ctx: &Ctx, mem: &mut Memory) {
    let h = ctx.hand();
    let (shanten, ukeire) = if h.total() % 3 == 2 {
        // 摸牌後：取牌效最好的打法
        tools::hand::discard_options(ctx.view, &h)
            .first()
            .map_or((8, 0.0), |o| (o.after.shanten, o.after.total))
    } else {
        let u = tools::hand::ukeire(ctx.view, &h);
        (u.shanten, u.total)
    };
    let threats: f64 = super::super::defense::threats(ctx, mem)
        .iter()
        .map(|t| t.p_tenpai)
        .sum();
    let waits = if shanten == 0 {
        f64::from(ukeire)
    } else {
        ASSUMED_WAITS
    };
    let p = estimate(ctx, shanten, f64::from(ukeire), waits, threats);
    ctx.claim("predict::win", Claim::IWinThisHand { p });
}

/// 我在本局和牌的機率。
/// - `shanten`／`ukeire`：打牌後的向聽數與進張張數；
/// - `waits`：聽牌時的聽牌張數（沒聽牌時用 [`ASSUMED_WAITS`]）；
/// - `threats`：對我有威脅的對手份量（每家以他已聽牌的機率計）。
pub fn estimate(ctx: &Ctx, shanten: i8, ukeire: f64, waits: f64, threats: f64) -> f64 {
    let v = ctx.view;
    let me = usize::from(v.me);
    // 看不到的牌（別人手上＋牌山＋王牌）
    let unseen: f64 = (0..34)
        .map(|i| f64::from(ctx.remaining()[i]))
        .sum::<f64>()
        .max(1.0);
    // 我還能摸幾次
    let draws = usize::from(v.tiles_left_in_wall) / 4;
    let ron = if v.riichi[me] {
        RON_CHANCES_WHEN_RIICHI
    } else {
        RON_CHANCES
    };
    let hit = (waits / unseen * (1.0 + ron)).min(0.9);
    let first = (ukeire / unseen).min(0.9);
    let later = (LATER_UKEIRE / unseen).min(0.9);
    let others = OTHERS_WIN_PER_TURN + OTHERS_WIN_PER_THREAT * threats;

    // alive[k] = 還在進行、目前 k 向聽的機率
    let steps = shanten.max(0) as usize;
    let mut alive = vec![0.0; steps + 1];
    alive[steps] = 1.0;
    let mut won = 0.0;
    for _ in 0..draws {
        won += alive[0] * hit;
        alive[0] *= 1.0 - hit;
        for k in 1..=steps {
            let advance = if k == steps { first } else { later };
            let moved = alive[k] * advance;
            alive[k] -= moved;
            alive[k - 1] += moved;
        }
        for a in &mut alive {
            *a *= 1.0 - others;
        }
    }
    won
}
