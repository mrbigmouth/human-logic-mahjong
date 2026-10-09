//! 立直與否：打出 `ctx.riichi_discard()` 後聽牌，要不要立直。
//!
//! 思考順序：
//! 1. 聽牌張已經一張不剩 → 默聽（空聽立直沒有意義）。
//! 2. 沒有役（默聽只能自摸）→ 立直。
//! 3. 默聽已經有跳滿左右（9000 點以上）→ 默聽（立直加的點數不多，卻讓別人提防、自己也不能換牌）。
//! 4. 牌山快沒了 → 默聽（剩下的巡目少，立直的好處小）。
//! 5. 最終局、自己是第一名、默聽能和 → 默聽（只要和了就好，不必冒險）。
//! 6. 聽牌張太少 → 默聽，等改聽。
//! 7. 其餘 → 立直。

use super::super::memory::Memory;
use crate::prelude::*;

/// 默聽達到這個點數（閒家）就不立直；莊家 1.5 倍。（M8 以 duplicate 調整：8000 → 9000，滿貫也立直）
pub const DAMA_POINTS: Param = Param::new(
    "riichi.dama_points",
    9000.0,
    3900.0,
    12000.0,
    "默聽已有這麼多點就不立直（閒家）",
);
/// 牌山剩下不到這麼多張時，有役就默聽。（M8：8 → 5）
pub const LATE_WALL: Param = Param::new(
    "riichi.late_wall",
    5.0,
    0.0,
    30.0,
    "牌山剩不到這麼多張就默聽",
);
/// 聽牌張少於這麼多張（且有役）時默聽，等待改聽。
pub const MIN_WAITS_FOR_RIICHI: Param = Param::new(
    "riichi.min_waits",
    2.0,
    1.0,
    6.0,
    "聽牌張少於這麼多張就默聽等改聽",
);

/// 回傳 true = 立直。
pub fn decide(ctx: &Ctx, _mem: &mut Memory) -> bool {
    let v = ctx.view;
    let pai = ctx.riichi_discard().expect("立直決策一定有要打的牌");
    let mut hand = ctx.hand();
    hand.remove(pai.tile);
    let Some(dama) = tools::value::tenpai_value(v, &hand, false) else {
        ctx.note("打這張不聽牌，不立直");
        return false;
    };
    if dama.waits <= 0.0 {
        ctx.note("聽牌張已經一張不剩，默聽");
        return false;
    }
    if dama.ron_waits <= 0.0 {
        ctx.note(format!(
            "沒有役，默聽只能自摸 → 立直（聽牌張 {} 張）",
            dama.waits
        ));
        return true;
    }
    let dealer = v.dealer() == v.me;
    let mangan = if dealer {
        ctx.param(&DAMA_POINTS) * 1.5
    } else {
        ctx.param(&DAMA_POINTS)
    };
    if f64::from(dama.avg_ron) >= mangan {
        ctx.note(format!("默聽已有約 {:.0} 點，不立直", dama.avg_ron));
        return false;
    }
    if f64::from(v.tiles_left_in_wall) < ctx.param(&LATE_WALL) {
        ctx.note(format!(
            "牌山只剩 {} 張，有役（約 {:.0} 點）就默聽",
            v.tiles_left_in_wall, dama.avg_ron
        ));
        return false;
    }
    let st = ctx.standing();
    if st.is_all_last && st.rank == 0 {
        ctx.note(format!(
            "最終局第一名，默聽能和（約 {:.0} 點）就好",
            dama.avg_ron
        ));
        return false;
    }
    if f64::from(dama.waits) < ctx.param(&MIN_WAITS_FOR_RIICHI) {
        ctx.note(format!("聽牌張只剩 {} 張，有役先默聽等改聽", dama.waits));
        return false;
    }
    let riichi = tools::value::tenpai_value(v, &hand, true).map_or(0.0, |t| t.avg_ron);
    ctx.note(format!(
        "聽牌張 {} 張，默聽約 {:.0} 點、立直約 {:.0} 點 → 立直",
        dama.waits, dama.avg_ron, riichi
    ));
    true
}
