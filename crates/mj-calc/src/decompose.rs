//! 和牌拆解：列出一手完成的門前牌所有「面子＋雀頭」的拆法。
//!
//! 計分時需要全部拆法，因為同一手牌的不同拆法，成立的役和符數可能不同。
//! 例如 111222333m 可以拆成三組刻子（三暗刻），也可以拆成三組 123m 順子（一盃口、平和）。
//! 另外也提供七對子、國士無雙的判定，以及聽牌張（waits）的計算。

use mj_core::{Tile, TileCounts};

/// 面子（mentsu）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Block {
    /// 順子（shuntsu）：從這張牌開始的三張連號牌，例如 Sequence(3m) = 345m。
    Sequence(Tile),
    /// 刻子（koutsu）：三張相同的牌。
    Triplet(Tile),
}

/// 一種拆法：雀頭＋面子。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Decomposition {
    /// 雀頭（jantou，將）
    pub pair: Tile,
    /// 面子，已排序，所以相同拆法比較時會相等。
    pub blocks: Vec<Block>,
}

/// 一般型的所有拆法。輸入是門前部分，張數必須是 3k+2（有副露時張數較少）。
/// 不是和牌型時回傳空陣列。
pub fn decompose(hand: &TileCounts) -> Vec<Decomposition> {
    let mut out = Vec::new();
    if hand.total() % 3 != 2 {
        return out;
    }
    let mut c = hand.0;
    // 依序假設每一種有 2 張以上的牌當雀頭，剩下的牌再拆成面子。
    for p in 0..c.len() {
        if c[p] < 2 {
            continue;
        }
        c[p] -= 2;
        let mut blocks = Vec::new();
        split_sets(&mut c, 0, &mut blocks, &mut |b| {
            let mut blocks = b.to_vec();
            blocks.sort();
            out.push(Decomposition {
                pair: Tile::from_index(p as u8),
                blocks,
            });
        });
        c[p] += 2;
    }
    // 不同的搜尋路徑可能得到相同拆法，排序後去重。
    out.sort_by(|a, b| (a.pair, &a.blocks).cmp(&(b.pair, &b.blocks)));
    out.dedup();
    out
}

/// 遞迴拆面子：每次處理編號最小、還有剩的牌。這張牌一定是某個刻子的一部分，
/// 或某個順子的起點（因為比它小的牌都已用完），所以只需嘗試這兩種。
/// 拆完時呼叫 `emit` 回報一種拆法。
fn split_sets(
    c: &mut [u8; 34],
    from: usize,
    blocks: &mut Vec<Block>,
    emit: &mut impl FnMut(&[Block]),
) {
    let Some(i) = (from..34).find(|&i| c[i] > 0) else {
        emit(blocks);
        return;
    };
    let tile = Tile::from_index(i as u8);
    // 嘗試當刻子。
    if c[i] >= 3 {
        c[i] -= 3;
        blocks.push(Block::Triplet(tile));
        split_sets(c, i, blocks, emit);
        blocks.pop();
        c[i] += 3;
    }
    // 嘗試當順子起點（字牌不能組順子；最多從 7 開始）。
    if !tile.is_honor() && tile.number() <= 7 && c[i + 1] > 0 && c[i + 2] > 0 {
        c[i] -= 1;
        c[i + 1] -= 1;
        c[i + 2] -= 1;
        blocks.push(Block::Sequence(tile));
        split_sets(c, i, blocks, emit);
        blocks.pop();
        c[i] += 1;
        c[i + 1] += 1;
        c[i + 2] += 1;
    }
}

/// 是否為七對子（chiitoi）：七組「不同」的對子（四張相同不能算兩對）。
pub fn is_chiitoi(hand: &TileCounts) -> bool {
    hand.total() == 14 && hand.0.iter().filter(|&&c| c == 2).count() == 7
}

/// 是否為國士無雙（kokushi）：13 種幺九牌各一張，其中一種成對。
pub fn is_kokushi(hand: &TileCounts) -> bool {
    hand.total() == 14 && Tile::all().all(|t| (hand.get(t) > 0) == t.is_yaochu())
}

/// 是否為和牌型（agari）：一般型、七對子、國士無雙任一。只看牌型，不檢查有沒有役。
pub fn is_agari(hand: &TileCounts) -> bool {
    !decompose(hand).is_empty() || is_chiitoi(hand) || is_kokushi(hand)
}

/// 聽牌張（waits，待ち）：3k+1 張的手牌摸到哪些牌會成為和牌型。
/// 只看牌型、不檢查役；手上已經有 4 張的牌不可能再摸到，所以排除。
/// 振聽判定與流局聽牌判定都使用這個結果。
pub fn waits(hand: &TileCounts) -> Vec<Tile> {
    let mut h = *hand;
    Tile::all()
        .filter(|&t| {
            if h.get(t) >= 4 {
                return false;
            }
            h.add(t);
            let win = is_agari(&h);
            h.remove(t);
            win
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use mj_core::parse_tiles;

    fn h(s: &str) -> TileCounts {
        TileCounts::from_tiles(parse_tiles(s).unwrap())
    }

    /// 同一手牌有多種拆法。
    #[test]
    fn multiple_interpretations() {
        // 111222333m：三組刻子或三組 123m 順子。
        let d = decompose(&h("111222333m456p55s"));
        assert_eq!(d.len(), 2);
        // 22334455667788m：雀頭與順子有多種組合。
        let d = decompose(&h("22334455667788m"));
        assert!(d.len() >= 2);
    }

    /// 七對子（四張相同不算兩對）、國士無雙。
    #[test]
    fn special_forms() {
        assert!(is_chiitoi(&h("1133557799m1133p")));
        assert!(!is_chiitoi(&h("1111557799m1133p")));
        assert!(is_kokushi(&h("19m19p19s12345677z")));
        assert!(is_agari(&h("19m19p19s12345677z")));
        assert!(!is_agari(&h("123m456p789s1112z")));
    }

    /// 九蓮寶燈型聽 1–9m 九面；未聽牌時沒有聽牌張；單騎 1 張時聽同一張。
    #[test]
    fn waits_of_tenpai_hands() {
        let w: Vec<String> = waits(&h("1112345678999m"))
            .iter()
            .map(|t| t.to_string())
            .collect();
        assert_eq!(w, ["1m", "2m", "3m", "4m", "5m", "6m", "7m", "8m", "9m"]);
        assert!(waits(&h("123m456p789s1357z")).is_empty());
        assert_eq!(waits(&h("1m")).len(), 1);
    }

    /// 有副露時門前張數較少（2 張、5 張）也能拆。
    #[test]
    fn melded_sizes() {
        assert_eq!(decompose(&h("55z")).len(), 1);
        assert_eq!(decompose(&h("12355m")).len(), 1);
    }
}
