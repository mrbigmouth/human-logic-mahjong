//! 寶牌與赤五。

use mj_core::{Pai, Tile};
use mj_engine::PlayerView;
pub use mj_rules::riichi::dora_from_indicator;

/// 目前的寶牌（依指示牌順序，可能重複）。
pub fn dora_tiles(view: &PlayerView) -> Vec<Tile> {
    view.dora_indicators
        .iter()
        .map(|&i| dora_from_indicator(i))
        .collect()
}

/// 這種牌值幾張寶牌（多個指示牌指向同一種時累加），不含赤五。
pub fn dora_value(view: &PlayerView, tile: Tile) -> u8 {
    view.dora_indicators
        .iter()
        .filter(|&&i| dora_from_indicator(i) == tile)
        .count() as u8
}

/// 這張牌值幾張寶牌，含赤五。
pub fn pai_dora_value(view: &PlayerView, pai: Pai) -> u8 {
    dora_value(view, pai.tile) + u8::from(pai.red)
}

/// 自己的手牌與副露中共有幾張寶牌（含赤五）。
pub fn my_dora_count(view: &PlayerView) -> u8 {
    let melded = view.melds[usize::from(view.me)]
        .iter()
        .flat_map(|m| m.tiles.iter());
    view.hand_pais
        .iter()
        .chain(melded)
        .map(|&p| pai_dora_value(view, p))
        .sum()
}

/// 自己看不到的寶牌張數（寶牌每種 4 張減去看得到的，加上看不到的赤五；赤五共 3 張）。
/// 這些寶牌在別人手上或牌山中：越多，別人的手越可能變大。
pub fn unseen_dora(view: &PlayerView) -> u8 {
    let seen = crate::visible::visible_counts(view);
    let dora: u8 = dora_tiles(view)
        .iter()
        .map(|&d| 4 - seen.get(d).min(4))
        .sum();
    let rivers = view
        .rivers
        .iter()
        .flatten()
        .filter(|d| !d.called)
        .map(|d| d.pai);
    let melds = view
        .melds
        .iter()
        .flatten()
        .flat_map(|m| m.tiles.iter().copied());
    let reds = view
        .hand_pais
        .iter()
        .copied()
        .chain(rivers)
        .chain(melds)
        .filter(|p| p.red)
        .count() as u8;
    dora + 3u8.saturating_sub(reds)
}
