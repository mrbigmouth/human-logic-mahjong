//! 精確的向聽數（shanten，向聴）計算。
//!
//! 向聽數＝距離聽牌還差幾步：0 = 聽牌（tenpai），−1 = 已經和牌（agari），1 = 一向聽……
//!
//! 一般型（4 面子＋1 雀頭）採用「距離」定義：手牌還需要「摸進」最少幾張牌，才能湊出
//! `n` 組面子（順子或刻子）＋1 組雀頭（且任何一種牌都不會需要第 5 張）。
//! 向聽數 = 距離 − 1。
//!
//! 做法：萬、筒、索、字四個花色各自獨立，用逐個數字推進的動態規劃（DP）算出
//! 「在這個花色裡湊出 m 組面子（＋或不＋雀頭）最少要摸幾張」，結果依花色快取；
//! 最後把四個花色的結果合併，取總和最小者。

use mj_core::TileCounts;
use std::cell::RefCell;
use std::collections::HashMap;

/// 代表「不可能」的大數：取最小值時永遠不會被選中，相加也不會溢位。
const INF: u8 = 64;

/// `dist[p][m]`：在單一花色內湊出 `m` 組面子＋`p` 組雀頭（p ∈ {0,1}）最少還要摸幾張。
type SuitDist = [[u8; 5]; 2];

/// 門前（未副露部分）還需要幾組面子，由張數推得：13/14 張 → 4 組，10/11 張 → 3 組（已有一組副露）……
pub fn sets_needed(hand: &TileCounts) -> usize {
    (hand.total() / 3) as usize
}

/// 一般型（4 面子＋雀頭）的向聽數。手牌張數可以是 3k+1（等待摸牌）或 3k+2（摸牌後）。
pub fn shanten_standard(hand: &TileCounts) -> i8 {
    let need = sets_needed(hand);
    // best[p][m]：目前已處理過的花色合計，湊出 m 組面子＋p 組雀頭的最少摸牌數。
    let mut best: SuitDist = [[INF; 5]; 2];
    best[0][0] = 0;
    // 萬、筒、索可以組順子；字牌不行。
    for (range, seq) in [(0..9, true), (9..18, true), (18..27, true), (27..34, false)] {
        let d = suit_dist(&hand.0[range], seq);
        let mut next: SuitDist = [[INF; 5]; 2];
        for p0 in 0..2 {
            for m0 in 0..=need {
                if best[p0][m0] >= INF {
                    continue;
                }
                // 雀頭全手只能有一組。
                for p1 in 0..2 - p0 {
                    for m1 in 0..=need - m0 {
                        let v = best[p0][m0] + d[p1][m1];
                        let slot = &mut next[p0 + p1][m0 + m1];
                        *slot = (*slot).min(v);
                    }
                }
            }
        }
        best = next;
    }
    best[1][need] as i8 - 1
}

/// 七對子（chiitoi，七対子）的向聽數：6 − 對子數，若牌種不足 7 種還要再補。
/// 只適用門前 13/14 張；有副露時回傳 `i8::MAX`（不可能）。
pub fn shanten_chiitoi(hand: &TileCounts) -> i8 {
    if hand.total() < 13 {
        return i8::MAX;
    }
    let pairs = hand.0.iter().filter(|&&c| c >= 2).count() as i8;
    let kinds = hand.0.iter().filter(|&&c| c >= 1).count() as i8;
    // 四張相同的牌只能算一對，所以牌種少於 7 時要另外換牌。
    6 - pairs + (7 - kinds).max(0)
}

/// 國士無雙（kokushi，国士無双）的向聽數：13 − 持有的幺九牌種數 − (有幺九對子 ? 1 : 0)。
/// 只適用門前 13/14 張；有副露時回傳 `i8::MAX`（不可能）。
pub fn shanten_kokushi(hand: &TileCounts) -> i8 {
    if hand.total() < 13 {
        return i8::MAX;
    }
    let yaochu = mj_core::Tile::all().filter(|t| t.is_yaochu());
    let (mut kinds, mut pair) = (0i8, false);
    for t in yaochu {
        let c = hand.get(t);
        kinds += i8::from(c > 0);
        pair |= c >= 2;
    }
    13 - kinds - i8::from(pair)
}

/// 向聽數：一般型、七對子、國士無雙三者取最小。
pub fn shanten(hand: &TileCounts) -> i8 {
    shanten_standard(hand)
        .min(shanten_chiitoi(hand))
        .min(shanten_kokushi(hand))
}

// 每個執行緒各自一份快取：key 是該花色 9（或 7）個數字的張數，以 5 進位編碼成整數。
// 索引 0 = 字牌（不可組順子），1 = 數牌。
thread_local! {
    static CACHE: RefCell<[HashMap<u32, SuitDist>; 2]> = RefCell::new([HashMap::new(), HashMap::new()]);
}

/// 單一花色的距離表（有快取）。`allow_seq` = 是否能組順子（數牌 true，字牌 false）。
fn suit_dist(counts: &[u8], allow_seq: bool) -> SuitDist {
    let key = counts.iter().fold(0u32, |k, &c| k * 5 + u32::from(c));
    let idx = usize::from(allow_seq);
    if let Some(d) = CACHE.with(|c| c.borrow()[idx].get(&key).copied()) {
        return d;
    }
    let d = compute_suit_dist(counts, allow_seq);
    CACHE.with(|c| c.borrow_mut()[idx].insert(key, d));
    d
}

/// 逐個數字（1→9）推進的 DP。
///
/// 狀態 (m, p, a, b)：已湊出 m 組面子、是否已有雀頭 p、從前一個數字開始的順子數 a、
/// 從前兩個數字開始的順子數 b（這些順子還會用到目前這個數字）。
/// 在每個數字上選擇：刻子 t（0/1 組）、雀頭 q（0/1 組）、從這裡開始的新順子 s 組；
/// 目標張數 = 3t + 2q + s + a + b 不能超過 4（沒有第 5 張），
/// 成本 = 目標張數比手上張數多出的部分（也就是要摸進的張數）。
// 用索引迴圈直接對應 DP 的狀態維度，比 iterator 串接好讀。
#[allow(clippy::needless_range_loop)]
fn compute_suit_dist(have: &[u8], allow_seq: bool) -> SuitDist {
    const S: usize = 5;
    // dp[m][p][a][b] = 到目前數字為止的最少摸牌數
    let mut dp = [[[[INF; S]; S]; 2]; S];
    dp[0][0][0][0] = 0;
    let n = have.len();
    for (r, &h) in have.iter().enumerate() {
        let mut next = [[[[INF; S]; S]; 2]; S];
        // 順子最多從 7 開始（789）；字牌不能組順子。
        let max_new_seq = if allow_seq && r + 2 < n { 4 } else { 0 };
        for m in 0..S {
            for p in 0..2 {
                for a in 0..S {
                    for b in 0..S {
                        let cur = dp[m][p][a][b];
                        if cur >= INF {
                            continue;
                        }
                        for t in 0..2 {
                            for q in 0..2 - p {
                                for s in 0..=max_new_seq {
                                    let target = 3 * t + 2 * q + s + a + b;
                                    let m2 = m + t + s;
                                    if target > 4 || m2 >= S {
                                        continue;
                                    }
                                    let cost = cur + (target as u8).saturating_sub(h);
                                    // 推進到下一個數字：新順子變成「前一個開始的」，原本的 a 變成「前兩個開始的」。
                                    let slot = &mut next[m2][p + q][s][a];
                                    *slot = (*slot).min(cost);
                                }
                            }
                        }
                    }
                }
            }
        }
        dp = next;
    }
    let mut out: SuitDist = [[INF; 5]; 2];
    for (m, row) in dp.iter().enumerate() {
        for p in 0..2 {
            // 走完最後一個數字時，不能有未完成的順子（a = b = 0）。
            out[p][m] = row[p][0][0];
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use mj_core::parse_tiles;

    fn h(s: &str) -> TileCounts {
        TileCounts::from_tiles(parse_tiles(s).unwrap())
    }

    /// 已和牌的手牌向聽數為 −1：一般型、七對子（字一色）、國士無雙、九蓮型。
    #[test]
    fn complete_hands() {
        assert_eq!(shanten_standard(&h("123m456p789s11122z")), -1);
        assert_eq!(shanten(&h("11223344556677z")), -1);
        assert_eq!(shanten(&h("19m19p19s12345677z")), -1);
        assert_eq!(shanten_standard(&h("11122233344455m")), -1);
    }

    /// 聽牌（0）與多向聽的例子。
    #[test]
    fn tenpai_and_beyond() {
        assert_eq!(shanten_standard(&h("123m456p789s1112z")), 0);
        assert_eq!(shanten_standard(&h("1112345678999m")), 0); // 九蓮寶燈型，聽 1–9m 九面
        assert_eq!(shanten_standard(&h("123m456p789s1357z")), 2);
        assert_eq!(shanten(&h("1133557799m113p")), 0); // 七對子聽牌（單騎 3p）
        assert_eq!(shanten_kokushi(&h("19m19p19s1234567z")), 0);
        assert_eq!(shanten_standard(&h("147m258p369s1234z")), 8);
        assert_eq!(shanten(&h("147m258p369s1234z")), 6);
    }

    /// 同一種牌最多 4 張，不能同時當刻子又當雀頭（需要 5 張）。
    #[test]
    fn four_copies_are_not_a_pair_plus_set_twice() {
        // 1111m 若要單靠 1m 湊成「刻子＋雀頭」需要第 5 張 1m，DP 不能允許這種情況。
        assert_eq!(shanten_standard(&h("1111m")), 1);
        // 已副露三組時，手上只剩 4（或 1、2）張：需要 1 組面子＋雀頭（或只需雀頭）。
        assert_eq!(shanten_standard(&h("1123m")), 0);
        assert_eq!(shanten_standard(&h("1m")), 0);
        assert_eq!(shanten_standard(&h("11m")), -1);
    }
}
