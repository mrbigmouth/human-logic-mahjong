//! 牌效輔助：把候選牌依「打出後離和牌多近」排序。

use crate::prelude::*;

/// 打出寶牌時，進張要打這個折扣來比較：進張差不多時保留寶牌（例如進張 30 張打寶牌、
/// 26 張留寶牌 → 30 × 0.85 = 25.5 < 26，留寶牌）。
pub const DORA_DISCOUNT: Param = Param::new(
    "efficiency.dora_discount",
    0.15,
    0.0,
    0.5,
    "打出寶牌時進張打的折扣",
);

/// 依牌效排序候選牌：向聽數低者優先，其次進張多者（打出寶牌時進張打折，見 [`DORA_DISCOUNT`]）；
/// 再來保留寶牌（先打非寶牌），最後先打字牌與老頭牌（較難用）。回傳（牌、打出後的手牌評估），最好的在前面。
pub fn rank_by_efficiency(ctx: &Ctx, candidates: &[Pai]) -> Vec<(Pai, AfterEval)> {
    let mut v: Vec<(Pai, AfterEval)> = candidates
        .iter()
        .map(|&p| (p, ctx.after_discard(p)))
        .collect();
    let adjusted = |p: Pai, e: &AfterEval| {
        e.ukeire * (1.0 - ctx.param(&DORA_DISCOUNT) as f32 * f32::from(dora_of(ctx, p))).max(0.0)
    };
    v.sort_by(|(pa, a), (pb, b)| {
        a.shanten
            .cmp(&b.shanten)
            .then(adjusted(*pb, b).total_cmp(&adjusted(*pa, a)))
            .then(dora_of(ctx, *pa).cmp(&dora_of(ctx, *pb)))
            .then(pb.tile.is_yaochu().cmp(&pa.tile.is_yaochu()))
    });
    v
}

/// 這張牌值幾張寶牌（含赤五）。
pub fn dora_of(ctx: &Ctx, pai: Pai) -> u8 {
    tools::dora::pai_dora_value(ctx.view, pai)
}
