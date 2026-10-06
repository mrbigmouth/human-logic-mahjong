//! 用向聽數的「定義」驗證計算結果，而不是依賴手算的預期值：
//! - 13 張手牌聽牌（向聽 0）⇔ 存在某張牌摸進來就和牌；
//! - 否則 向聽(h) = 1 + 所有「摸一張 t、打一張 d」之後的最小向聽(h + t − d)。
//!
//! 只要和牌判定（`is_agari`，靠拆解，很容易確認正確）是對的，這兩條就能歸納出整個向聽計算是對的。

use mj_calc::{is_agari, shanten};
use mj_core::{Tile, TileCounts};
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

/// 洗好的 136 張牌（不分赤牌）。
fn wall(rng: &mut ChaCha8Rng) -> Vec<Tile> {
    let mut w: Vec<Tile> = Tile::all().flat_map(|t| [t; 4]).collect();
    w.shuffle(rng);
    w
}

/// 純隨機手牌大多是 3–5 向聽，所以另外產生「接近完成」的手牌來覆蓋聽牌、一向聽等情況：
/// 先隨機組出 4 面子＋雀頭，拿掉一張變 13 張，再隨機換掉 0–2 張。
fn near_complete(rng: &mut ChaCha8Rng) -> TileCounts {
    loop {
        let mut h = TileCounts::default();
        let mut ok = true;
        for _ in 0..4 {
            let t = rng.random_range(0..34u8);
            let tile = Tile::from_index(t);
            if rng.random_bool(0.6) && !tile.is_honor() && tile.number() <= 7 {
                (0..3).for_each(|k| h.add(Tile::from_index(t + k)));
            } else {
                (0..3).for_each(|_| h.add(tile));
            }
        }
        let p = Tile::from_index(rng.random_range(0..34u8));
        h.add(p);
        h.add(p);
        // 同一種牌超過 4 張就重來。
        if h.0.iter().any(|&c| c > 4) {
            continue;
        }
        // 拿掉一張變成 13 張，再隨機換掉 k 張。
        let tiles: Vec<Tile> = h.tiles().collect();
        h.remove(tiles[rng.random_range(0..tiles.len())]);
        for _ in 0..rng.random_range(0..3) {
            let tiles: Vec<Tile> = h.tiles().collect();
            h.remove(tiles[rng.random_range(0..tiles.len())]);
            let t = Tile::from_index(rng.random_range(0..34u8));
            if h.get(t) >= 4 {
                ok = false;
                break;
            }
            h.add(t);
        }
        if ok {
            return h;
        }
    }
}

/// 檢查一手 13 張牌是否符合向聽數的定義。
fn check(h: &TileCounts) {
    let s = shanten(h);
    assert!(s >= 0, "13-tile hand {h} cannot be complete");
    let mut best = i8::MAX;
    let mut wins = false;
    let mut h14 = *h;
    // 摸進每一種還可能摸到的牌 t……
    for t in Tile::all().filter(|&t| h.get(t) < 4) {
        h14.add(t);
        wins |= is_agari(&h14);
        assert_eq!(
            shanten(&h14) == -1,
            is_agari(&h14),
            "agari mismatch for {h14}"
        );
        // ……再打出每一種手上的牌 d，記錄最小向聽。
        for d in Tile::all().filter(|&d| h14.get(d) > 0) {
            let mut h13 = h14;
            h13.remove(d);
            best = best.min(shanten(&h13));
        }
        h14.remove(t);
    }
    assert_eq!(s == 0, wins, "tenpai mismatch for {h} (shanten {s})");
    if s > 0 {
        assert_eq!(s, best + 1, "Bellman mismatch for {h}");
    }
}

/// 純隨機手牌。
#[test]
fn random_hands_satisfy_definition() {
    let mut rng = ChaCha8Rng::seed_from_u64(1);
    for _ in 0..150 {
        let w = wall(&mut rng);
        check(&TileCounts::from_tiles(w[..13].iter().copied()));
    }
}

/// 接近完成的手牌（聽牌、一向聽較多）。
#[test]
fn near_complete_hands_satisfy_definition() {
    let mut rng = ChaCha8Rng::seed_from_u64(2);
    for _ in 0..400 {
        check(&near_complete(&mut rng));
    }
}
