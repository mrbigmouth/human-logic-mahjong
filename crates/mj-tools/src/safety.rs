//! 安全度的「事實」：現物、筋、No Chance。這些都是確定的規則推論，不含機率推測；
//! 機率性的判斷（某家聽什麼、放銃率多少）屬於策略的預測函式（utils/predict）。

use crate::table::riichi_seq;
use crate::visible::visible_counts;
use mj_core::{Tile, TileCounts};
use mj_engine::{PlayerView, Seat};

/// 對 `target` 的現物（絕對不會放銃的牌）：
/// - `target` 自己打過的牌（捨牌振聽）；
/// - `target` 立直後，任何人打出而 `target` 沒有榮和的牌（立直振聽）；
/// - `target` 最後一次打牌之後，其他人打出而 `target` 沒有榮和的牌（同巡振聽）。
pub fn genbutsu(view: &PlayerView, target: Seat) -> TileCounts {
    let mut safe = TileCounts::default();
    let river = &view.rivers[usize::from(target)];
    let mark = |safe: &mut TileCounts, t: Tile| {
        if safe.get(t) == 0 {
            safe.add(t);
        }
    };
    for d in river {
        mark(&mut safe, d.pai.tile);
    }
    let after_seq = match riichi_seq(view, target) {
        Some(s) => Some(s),
        None => river.last().map(|d| d.seq),
    };
    if let Some(seq) = after_seq {
        for (s, r) in view.rivers.iter().enumerate() {
            if s == usize::from(target) {
                continue;
            }
            for d in r.iter().filter(|d| d.seq > seq) {
                mark(&mut safe, d.pai.tile);
            }
        }
    }
    safe
}

/// 是否為對 `target` 的現物。
pub fn is_genbutsu(view: &PlayerView, target: Seat, tile: Tile) -> bool {
    genbutsu(view, target).get(tile) > 0
}

/// 是否為對 `target` 的筋牌：兩面聽這張牌所需的「另一邊」都已是現物。
/// 1、2、3 看 +3（例如 1 看 4）；7、8、9 看 −3；4、5、6 兩邊都要（両筋）。字牌不適用（回傳 false）。
pub fn is_suji(view: &PlayerView, target: Seat, tile: Tile) -> bool {
    if tile.is_honor() {
        return false;
    }
    let safe = genbutsu(view, target);
    let n = tile.number();
    let i = tile.index() as u8;
    let low_ok = n <= 3 || safe.get(Tile::from_index(i - 3)) > 0;
    let high_ok = n >= 7 || safe.get(Tile::from_index(i + 3)) > 0;
    low_ok && high_ok
}

/// No Chance（壁）：兩面聽這張牌所需的搭子，因為某張牌 4 張都已看見而不可能存在。
/// 例：4 張 2m 都看得到 → 沒有 23m 搭子，也沒有 12m…… 對 1m 而言兩面（23m）不可能。
/// 字牌回傳 false。
pub fn is_no_chance(view: &PlayerView, tile: Tile) -> bool {
    if tile.is_honor() {
        return false;
    }
    let seen = visible_counts(view);
    let n = tile.number();
    let i = tile.index() as u8;
    let exhausted = |k: u8| seen.get(Tile::from_index(k)) >= 4;
    // 這張牌的兩面聽來自 (t+1, t+2) 或 (t−2, t−1)；兩組都不可能才算 No Chance。
    let upper_impossible = n > 7 || exhausted(i + 1) || exhausted(i + 2);
    let lower_impossible = n < 3 || exhausted(i - 1) || exhausted(i - 2);
    upper_impossible && lower_impossible
}

/// 字牌已經看得到幾張；3 張以上時只剩單騎聽法（且最多 1 張）。
pub fn honor_visible(view: &PlayerView, tile: Tile) -> u8 {
    visible_counts(view).get(tile)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mj_core::{Pai, TileCounts};
    use mj_engine::DiscardRecord;

    fn t(s: &str) -> Tile {
        s.parse().unwrap()
    }

    fn view() -> PlayerView {
        PlayerView {
            me: 0,
            round_wind: 0,
            kyoku: 0,
            honba: 0,
            riichi_sticks: 0,
            scores: [25_000; 4],
            dora_indicators: vec![],
            hand: TileCounts::default(),
            hand_pais: vec![],
            drawn: None,
            melds: Default::default(),
            rivers: Default::default(),
            riichi: [false; 4],
            tiles_left_in_wall: 60,
            total_kyoku: 8,
            last_discard: None,
        }
    }

    fn discard(v: &mut PlayerView, seat: usize, tile: &str, seq: u16, riichi: bool) {
        v.rivers[seat].push(DiscardRecord {
            pai: Pai::plain(t(tile)),
            tsumogiri: false,
            riichi_declared: riichi,
            called: false,
            seq,
        });
    }

    /// 立直後他家打出的牌是現物；立直前他家打的不是。
    #[test]
    fn genbutsu_after_riichi() {
        let mut v = view();
        discard(&mut v, 0, "9p", 0, false);
        discard(&mut v, 1, "4m", 1, true);
        discard(&mut v, 2, "5s", 2, false);
        assert!(is_genbutsu(&v, 1, t("4m")));
        assert!(is_genbutsu(&v, 1, t("5s")));
        assert!(!is_genbutsu(&v, 1, t("9p")));
    }

    /// 打過 4m：1m、7m 是筋；4m 本身是現物但筋要看另一邊；5m 需要 2m 和 8m。
    #[test]
    fn suji_rules() {
        let mut v = view();
        discard(&mut v, 1, "4m", 0, false);
        assert!(is_suji(&v, 1, t("1m")));
        assert!(is_suji(&v, 1, t("7m")));
        assert!(!is_suji(&v, 1, t("5m")));
        discard(&mut v, 1, "2m", 1, false);
        discard(&mut v, 1, "8m", 2, false);
        assert!(is_suji(&v, 1, t("5m")));
        assert!(!is_suji(&v, 1, t("1z")));
    }

    /// 2m 四張都看得到：1m 的兩面（23m）不可能 → No Chance；3m 還可能有 45m。
    #[test]
    fn no_chance() {
        let mut v = view();
        v.hand = TileCounts::from_tiles([t("2m"); 4]);
        assert!(is_no_chance(&v, t("1m")));
        assert!(!is_no_chance(&v, t("3m")));
    }
}
