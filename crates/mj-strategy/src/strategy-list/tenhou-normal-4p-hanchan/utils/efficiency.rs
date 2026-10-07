//! 牌效輔助：把候選牌依「打出後離和牌多近」排序。

use crate::prelude::*;

/// 依牌效排序候選牌：向聽數低者優先，其次進張多者；再來保留寶牌（先打非寶牌），
/// 最後先打字牌與老頭牌（較難用）。回傳（牌、打出後的手牌評估），最好的在前面。
pub fn rank_by_efficiency(ctx: &Ctx, candidates: &[Pai]) -> Vec<(Pai, AfterEval)> {
    let mut v: Vec<(Pai, AfterEval)> = candidates
        .iter()
        .map(|&p| (p, ctx.after_discard(p)))
        .collect();
    v.sort_by(|(pa, a), (pb, b)| {
        a.shanten
            .cmp(&b.shanten)
            .then(b.ukeire.total_cmp(&a.ukeire))
            .then(dora_of(ctx, *pa).cmp(&dora_of(ctx, *pb)))
            .then(pb.tile.is_yaochu().cmp(&pa.tile.is_yaochu()))
    });
    v
}

/// 這張牌值幾張寶牌（含赤五）。
pub fn dora_of(ctx: &Ctx, pai: Pai) -> u8 {
    tools::dora::pai_dora_value(ctx.view, pai)
}
