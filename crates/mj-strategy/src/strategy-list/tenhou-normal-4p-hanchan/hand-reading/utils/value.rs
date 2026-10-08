//! 自己手牌的價值：如果這手牌和了，大約值多少點（榮和、不含本場）。

use crate::prelude::*;

/// 門前沒有其他役時，靠立直（＋自摸、裏寶牌的平均加成）大約能有的番數。
pub const RIICHI_HAN: u8 = 2;

/// 打出 `discarded` 之後的手牌 `hand`（3k+1 張）和了時大約值多少點。
/// - 已聽牌：實際計算每個聽牌張的點數（門前時假設立直），依剩餘張數平均；
/// - 還沒聽牌：依「役的番數＋手上的寶牌」粗估，沒有役的副露手值 0（和不了）。
pub fn my_value(ctx: &Ctx, hand: &TileCounts, shanten: i8, discarded: Pai) -> f64 {
    let v = ctx.view;
    let closed = tools::table::i_am_closed(v);
    let dealer = v.dealer() == v.me;
    if shanten == 0
        && let Some(tv) =
            tools::value::tenpai_value(v, hand, closed && !v.riichi[usize::from(v.me)])
    {
        return f64::from(tv.avg_ron);
    }
    let yaku_han = if closed {
        RIICHI_HAN
    } else {
        let me = usize::from(v.me);
        let sets: Vec<Vec<Tile>> = v.melds[me]
            .iter()
            .map(|m| m.tiles.iter().map(|p| p.tile).collect())
            .collect();
        match tools::yaku_path::open_yaku_path(v, &sets, hand) {
            OpenYakuPath::None => return 0.0,
            OpenYakuPath::Flush => 2,
            _ => 1,
        }
    };
    let dora = dora_in(ctx, hand, u8::from(discarded.red));
    f64::from(tools::value::points_for_han(yaku_han + dora, dealer))
}

/// 手牌（門前 `hand`＋副露）中的寶牌張數（含赤五）。`red_removed` = 從門前手牌打出的赤五張數。
pub fn dora_in(ctx: &Ctx, hand: &TileCounts, red_removed: u8) -> u8 {
    let v = ctx.view;
    let me = usize::from(v.me);
    // `tiles()` 每張牌各出現一次（同種牌有幾張就出現幾次）。
    let concealed: u8 = hand.tiles().map(|t| ctx.dora_value(t)).sum();
    let red_in_hand = v.hand_pais.iter().filter(|p| p.red).count() as u8;
    let melded: u8 = v.melds[me]
        .iter()
        .flat_map(|m| m.tiles.iter())
        .map(|&p| tools::dora::pai_dora_value(v, p))
        .sum();
    concealed + red_in_hand.saturating_sub(red_removed) + melded
}
