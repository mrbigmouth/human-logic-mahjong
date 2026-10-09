//! 預測：本局最後由我和牌的機率（`IWinThisHand`）。
//!
//! 想法：把剩下的巡目一巡一巡走過去——
//! - 還沒聽牌時，每巡摸到進張（前進一向聽）的機率 = 進張張數 ÷ 看不到的牌；
//! - 聽牌後，每巡和牌的機率 = 聽牌張數 ÷ 看不到的牌 ×（自摸 1 次＋他家打牌的放銃機會）；
//! - 每巡也有一定機率被別人先和掉（有人立直或有威脅的副露時更高）。
//!
//! 參數分成預測用（`win.*`，以牌譜校準）與押引用（`push.*`，以對戰調整）兩組，見 [`Model`]。

use super::super::super::memory::Memory;
use crate::prelude::*;

/// 一組「本局和牌機率」的模型參數。同一個一巡一巡往後推的計算有兩組參數：
/// - **預測**（[`Model::prediction`]，`win.*`）：以天鳳牌譜的對數損失校準，是準確的機率，用於常駐預測與說明；
/// - **押引**（[`Model::push`]，`push.*`）：押引公式（`decision/discard.rs`）用的版本，以 duplicate 對戰調整。
///
/// 兩者分開，是因為較準的機率不一定帶來較好的決策（M5、M8 都觀察到）：押引公式本身有沒計入的因素
/// （棄和後仍可能流局聽牌、名次……），校準過的機率直接放進去反而變弱（名次點 −2.87 ± 0.61）。
#[derive(Debug, Clone, Copy)]
pub struct Model {
    /// 聽牌前假設的聽牌張數
    pub assumed_waits: f64,
    /// 下一步之後每一步假設的進張張數（只有「下一步」用實際進張，否則進張廣的 2 向聽會被估得比窄的 1 向聽快）
    pub later_ukeire: f64,
    /// 他家打牌給我榮和的機會，相當於每巡幾次「摸牌」（沒有人立直時）
    pub ron_chances: f64,
    /// 自己已立直時的榮和機會（他家會防守）
    pub ron_chances_when_riichi: f64,
    /// 每巡被別人先和掉的基本機率
    pub others_win_per_turn: f64,
    /// 每多一家威脅（以他已聽牌的機率計），每巡被別人先和掉的機率再加這麼多
    pub others_win_per_threat: f64,
}

/// 預測用：聽牌前假設的聽牌張數（以牌譜校準時達到範圍上限附近）。
pub const ASSUMED_WAITS: Param = Param::new(
    "win.assumed_waits",
    10.0,
    2.0,
    16.0,
    "預測：聽牌前假設的聽牌張數",
);
/// 預測用：之後每一步假設的進張張數。
pub const LATER_UKEIRE: Param = Param::new(
    "win.later_ukeire",
    19.6,
    4.0,
    30.0,
    "預測：之後每一步假設的進張張數",
);
/// 預測用：每巡的榮和機會（沒有人立直時）。
pub const RON_CHANCES: Param =
    Param::new("win.ron_chances", 0.257, 0.0, 2.0, "預測：每巡的榮和機會");
/// 預測用：自己立直時每巡的榮和機會。
pub const RON_CHANCES_WHEN_RIICHI: Param = Param::new(
    "win.ron_chances_when_riichi",
    0.713,
    0.0,
    2.0,
    "預測：自己立直時每巡的榮和機會",
);
/// 預測用：每巡被別人先和掉的基本機率。
pub const OTHERS_WIN_PER_TURN: Param = Param::new(
    "win.others_win_per_turn",
    0.0634,
    0.0,
    0.2,
    "預測：每巡被別人先和掉的基本機率",
);
/// 預測用：每家威脅增加的被先和機率。
pub const OTHERS_WIN_PER_THREAT: Param = Param::new(
    "win.others_win_per_threat",
    0.191,
    0.0,
    0.4,
    "預測：每家威脅增加的被先和機率",
);

/// 押引用：聽牌前假設的聽牌張數。
pub const PUSH_ASSUMED_WAITS: Param = Param::new(
    "push.assumed_waits",
    5.0,
    2.0,
    16.0,
    "押引：聽牌前假設的聽牌張數",
);
/// 押引用：之後每一步假設的進張張數。
pub const PUSH_LATER_UKEIRE: Param = Param::new(
    "push.later_ukeire",
    12.0,
    4.0,
    30.0,
    "押引：之後每一步假設的進張張數",
);
/// 押引用：每巡的榮和機會。
pub const PUSH_RON_CHANCES: Param =
    Param::new("push.ron_chances", 0.4, 0.0, 2.0, "押引：每巡的榮和機會");
/// 押引用：自己立直時每巡的榮和機會。
pub const PUSH_RON_CHANCES_WHEN_RIICHI: Param = Param::new(
    "push.ron_chances_when_riichi",
    0.8,
    0.0,
    2.0,
    "押引：自己立直時每巡的榮和機會",
);
/// 押引用：每巡被別人先和掉的基本機率。
pub const PUSH_OTHERS_WIN_PER_TURN: Param = Param::new(
    "push.others_win_per_turn",
    0.07,
    0.0,
    0.2,
    "押引：每巡被別人先和掉的基本機率",
);
/// 押引用：每家威脅增加的被先和機率。
pub const PUSH_OTHERS_WIN_PER_THREAT: Param = Param::new(
    "push.others_win_per_threat",
    0.06,
    0.0,
    0.4,
    "押引：每家威脅增加的被先和機率",
);

impl Model {
    /// 預測用的參數（`win.*`）。
    pub fn prediction(ctx: &Ctx) -> Self {
        Model {
            assumed_waits: ctx.param(&ASSUMED_WAITS),
            later_ukeire: ctx.param(&LATER_UKEIRE),
            ron_chances: ctx.param(&RON_CHANCES),
            ron_chances_when_riichi: ctx.param(&RON_CHANCES_WHEN_RIICHI),
            others_win_per_turn: ctx.param(&OTHERS_WIN_PER_TURN),
            others_win_per_threat: ctx.param(&OTHERS_WIN_PER_THREAT),
        }
    }

    /// 押引用的參數（`push.*`）。
    pub fn push(ctx: &Ctx) -> Self {
        Model {
            assumed_waits: ctx.param(&PUSH_ASSUMED_WAITS),
            later_ukeire: ctx.param(&PUSH_LATER_UKEIRE),
            ron_chances: ctx.param(&PUSH_RON_CHANCES),
            ron_chances_when_riichi: ctx.param(&PUSH_RON_CHANCES_WHEN_RIICHI),
            others_win_per_turn: ctx.param(&PUSH_OTHERS_WIN_PER_TURN),
            others_win_per_threat: ctx.param(&PUSH_OTHERS_WIN_PER_THREAT),
        }
    }
}

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
    let m = Model::prediction(ctx);
    let waits = if shanten == 0 {
        f64::from(ukeire)
    } else {
        m.assumed_waits
    };
    let p = estimate(ctx, &m, shanten, f64::from(ukeire), waits, threats);
    ctx.claim("predict::win", Claim::IWinThisHand { p });
}

/// 我在本局和牌的機率（依模型參數 `m`）。
/// - `shanten`／`ukeire`：打牌後的向聽數與進張張數；
/// - `waits`：聽牌時的聽牌張數（沒聽牌時用 `m.assumed_waits`）；
/// - `threats`：對我有威脅的對手份量（每家以他已聽牌的機率計）。
pub fn estimate(ctx: &Ctx, m: &Model, shanten: i8, ukeire: f64, waits: f64, threats: f64) -> f64 {
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
        m.ron_chances_when_riichi
    } else {
        m.ron_chances
    };
    let hit = (waits / unseen * (1.0 + ron)).min(0.9);
    let first = (ukeire / unseen).min(0.9);
    let later = (m.later_ukeire / unseen).min(0.9);
    let others = m.others_win_per_turn + m.others_win_per_threat * threats;

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
