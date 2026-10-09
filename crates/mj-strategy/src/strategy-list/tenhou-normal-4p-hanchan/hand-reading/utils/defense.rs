//! 防守輔助：誰對我有威脅、每張牌的放銃機率與預期損失。

use super::super::memory::Memory;
use super::predict::{hand_value, tenpai, waits};
use crate::prelude::*;

/// 是否也提防沒立直的人（副露者、默聽）。M6 的聽牌預測以天鳳牌譜校準過，可以開啟。
pub const DEFEND_AGAINST_UNDECLARED: bool = true;
/// 沒立直的人：聽牌機率達到這個值，就當作威脅（人類會開始提防「這家好像聽了」）。
pub const UNDECLARED_THREAT_TENPAI: Param = Param::new(
    "defense.undeclared_threat_tenpai",
    0.5,
    0.2,
    1.0,
    "沒立直的人聽牌機率達到多少才當作威脅",
);

/// 一家對我的威脅。
#[derive(Debug, Clone, Copy)]
pub struct Threat {
    pub seat: Seat,
    /// 他已聽牌的機率
    pub p_tenpai: f64,
    /// 放銃給他時預期要付的點數
    pub value: f64,
}

/// 對我有威脅的對手：已立直者，以及聽牌機率高的副露者、默聽者。
pub fn threats(ctx: &Ctx, mem: &Memory) -> Vec<Threat> {
    let v = ctx.view;
    tools::table::opponents(v)
        .into_iter()
        .filter_map(|seat| {
            let p_tenpai = tenpai::estimate(ctx, mem, seat);
            let threat = v.riichi[usize::from(seat)]
                || (DEFEND_AGAINST_UNDECLARED && p_tenpai >= ctx.param(&UNDECLARED_THREAT_TENPAI));
            threat.then(|| Threat {
                seat,
                p_tenpai,
                value: hand_value::estimate(ctx, mem, seat).0,
            })
        })
        .collect()
}

/// 打出 `pai` 的風險：（放銃機率, 預期損失點數）。
/// 每家的放銃機率 = 他已聽牌的機率 × 他聽這張的機率。
pub fn deal_in(ctx: &Ctx, mem: &Memory, threats: &[Threat], pai: Pai) -> (f64, f64) {
    let mut safe = 1.0;
    let mut loss = 0.0;
    for t in threats {
        let p = t.p_tenpai * waits::estimate(ctx, mem, t.seat, pai.tile);
        safe *= 1.0 - p;
        loss += p * t.value;
    }
    (1.0 - safe, loss)
}
