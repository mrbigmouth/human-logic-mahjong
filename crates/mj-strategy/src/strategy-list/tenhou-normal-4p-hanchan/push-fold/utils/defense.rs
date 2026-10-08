//! 防守輔助：誰對我有威脅、每張牌的放銃機率與預期損失。

use super::super::memory::Memory;
use super::predict::{tenpai, waits};
use crate::prelude::*;

/// 是否也提防副露者。目前的副露者聽牌預測不夠準（M6 改進），開啟反而變弱
///（duplicate 對 M4 版本：只防立直 −0.041 ± 0.033，也防副露（聽牌機率 ≥ 0.5）+0.003 ± 0.035），所以先關閉。
pub const DEFEND_AGAINST_OPEN_HANDS: bool = false;
/// 提防副露者時：他的聽牌機率達到這個值，就當作威脅（人類會開始提防「這家好像聽了」）。
pub const OPEN_THREAT_TENPAI: f64 = 0.5;
/// 立直者和牌的平均點數（閒家；莊家 1.5 倍）。含裏寶牌、一發、自摸的平均加成。
pub const RIICHI_VALUE: f64 = 6000.0;
/// 副露者和牌的基本點數（閒家），每張看得到的寶牌（副露中的寶牌、赤五）再加 [`OPEN_DORA_VALUE`]。
pub const OPEN_VALUE: f64 = 3000.0;
/// 副露中每張寶牌增加的點數。
pub const OPEN_DORA_VALUE: f64 = 2000.0;

/// 一家對我的威脅。
#[derive(Debug, Clone, Copy)]
pub struct Threat {
    pub seat: Seat,
    /// 他已聽牌的機率
    pub p_tenpai: f64,
    /// 放銃給他時預期要付的點數
    pub value: f64,
}

/// 對我有威脅的對手：已立直者，以及聽牌機率高的副露者。
/// 門前沒立直的人看不出聽牌與否，不列入（人類也只能當作沒聽牌）。
pub fn threats(ctx: &Ctx, mem: &Memory) -> Vec<Threat> {
    let v = ctx.view;
    tools::table::opponents(v)
        .into_iter()
        .filter_map(|seat| {
            let s = usize::from(seat);
            let open = v.melds[s].iter().any(|m| m.is_open());
            let p_tenpai = tenpai::estimate(ctx, mem, seat);
            let threat = v.riichi[s]
                || (DEFEND_AGAINST_OPEN_HANDS && open && p_tenpai >= OPEN_THREAT_TENPAI);
            threat.then(|| Threat {
                seat,
                p_tenpai,
                value: threat_value(ctx, seat),
            })
        })
        .collect()
}

/// 放銃給 `seat` 時大約要付多少點。
pub fn threat_value(ctx: &Ctx, seat: Seat) -> f64 {
    let v = ctx.view;
    let s = usize::from(seat);
    let base = if v.riichi[s] {
        RIICHI_VALUE
    } else {
        let dora: u32 = v.melds[s]
            .iter()
            .flat_map(|m| m.tiles.iter())
            .map(|&p| u32::from(tools::dora::pai_dora_value(v, p)))
            .sum();
        OPEN_VALUE + OPEN_DORA_VALUE * f64::from(dora)
    };
    if v.dealer() == seat { base * 1.5 } else { base }
}

/// 打出 `pai` 的風險：（放銃機率, 預期損失點數）。
/// 每家的放銃機率 = 他已聽牌的機率 × 他聽這張的機率。
/// 對立直者的「聽這張」判斷會記錄成預測（`predict::waits`），供準確度評估。
pub fn deal_in(ctx: &Ctx, mem: &Memory, threats: &[Threat], pai: Pai) -> (f64, f64) {
    let mut safe = 1.0;
    let mut loss = 0.0;
    for t in threats {
        let p_wait = if ctx.view.riichi[usize::from(t.seat)] {
            waits::predict(ctx, mem, t.seat, pai.tile)
        } else {
            waits::estimate(ctx, mem, t.seat, pai.tile)
        };
        let p = t.p_tenpai * p_wait;
        safe *= 1.0 - p;
        loss += p * t.value;
    }
    (1.0 - safe, loss)
}
