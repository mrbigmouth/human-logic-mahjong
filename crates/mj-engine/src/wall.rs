//! 牌山（wall，山）：洗牌、摸牌、嶺上牌、寶牌指示牌。

use mj_core::{Pai, Tile};
use rand::SeedableRng;
use rand::seq::SliceRandom;
use rand_chacha::ChaCha8Rng;

/// 全部牌數：34 種 × 4 張。
pub const WALL_SIZE: usize = 136;
/// 王牌（wanpai，dead wall）張數：嶺上牌 4 張＋寶牌指示牌 5 張＋裏寶牌指示牌 5 張。
const DEAD_WALL: usize = 14;
/// 可摸的牌山張數（含配牌）。
const LIVE_WALL: usize = WALL_SIZE - DEAD_WALL;

/// 由對局 seed 推導第 `index` 手牌的牌山 seed（splitmix64 雜湊）。
/// 牌山只取決於「第幾手」，與打法無關，所以 duplicate（複式）對戰中即使各桌連莊次數不同，
/// 第 N 手仍會拿到相同的牌山。
pub fn hand_seed(match_seed: u64, index: u32) -> u64 {
    let mut z = match_seed ^ u64::from(index).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// 一手牌的 136 張牌。`tiles[..122]` 是可摸的牌山，從前面依序摸（含配牌）；
/// `tiles[122..]` 是王牌：嶺上牌 4 張、寶牌指示牌 5 張、裏寶牌指示牌 5 張。
#[derive(Debug, Clone)]
pub struct Wall {
    tiles: Vec<Pai>,
    /// 下一張要摸的位置
    next: usize,
    /// 已開槓數（已摸的嶺上牌數）
    kans: usize,
    /// 已翻開的寶牌指示牌數（開局 1 張，每槓 +1）
    revealed: usize,
}

impl Wall {
    /// 用 seed 洗出的牌山（同一個 seed 一定得到同一個牌山）。
    pub fn shuffled(seed: u64, red_fives: u8) -> Self {
        let mut tiles = full_set(red_fives);
        tiles.shuffle(&mut ChaCha8Rng::seed_from_u64(seed));
        Self::from_tiles(tiles)
    }

    /// 指定好的牌山（測試、重播牌譜用）。不是 136 張時 panic。
    pub fn from_tiles(tiles: Vec<Pai>) -> Self {
        assert_eq!(tiles.len(), WALL_SIZE, "a wall has {WALL_SIZE} tiles");
        Wall {
            tiles,
            next: 0,
            kans: 0,
            revealed: 1,
        }
    }

    /// 牌山剩餘可摸張數。每開一槓，王牌要從牌山尾端補一張，所以可摸的牌少一張。
    pub fn remaining(&self) -> usize {
        LIVE_WALL - self.kans - self.next
    }

    /// 摸一張；牌山摸完（荒牌）時回傳 `None`。
    pub fn draw(&mut self) -> Option<Pai> {
        (self.remaining() > 0).then(|| {
            self.next += 1;
            self.tiles[self.next - 1]
        })
    }

    /// 摸嶺上牌（rinshan）：開槓後補的牌。一手最多 4 槓。
    pub fn draw_rinshan(&mut self) -> Pai {
        assert!(self.kans < 4, "at most four kans per hand");
        self.kans += 1;
        self.tiles[LIVE_WALL + self.kans - 1]
    }

    /// 翻開下一張寶牌指示牌（槓寶牌，kan dora）。
    pub fn reveal_dora(&mut self) {
        self.revealed = (self.revealed + 1).min(5);
    }

    /// 已開槓數。
    pub fn kans(&self) -> usize {
        self.kans
    }

    /// 已翻開的寶牌指示牌。
    pub fn dora_indicators(&self) -> Vec<Tile> {
        (0..self.revealed)
            .map(|i| self.tiles[LIVE_WALL + 4 + i].tile)
            .collect()
    }

    /// 裏寶牌指示牌（張數與已翻開的寶牌指示牌相同；只有立直和牌時才用）。
    pub fn ura_indicators(&self) -> Vec<Tile> {
        (0..self.revealed)
            .map(|i| self.tiles[LIVE_WALL + 9 + i].tile)
            .collect()
    }
}

/// 全部 136 張牌；前 `red_fives` 種花色（依序萬、筒、索）各有一張赤五。
pub fn full_set(red_fives: u8) -> Vec<Pai> {
    let mut tiles = Vec::with_capacity(WALL_SIZE);
    for t in Tile::all() {
        for copy in 0..4 {
            let red = copy == 0
                && !t.is_honor()
                && t.number() == 5
                && (t.index() / 9) < usize::from(red_fives);
            tiles.push(Pai { tile: t, red });
        }
    }
    tiles
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 相同 seed 得到相同牌山，不同 seed 不同；赤五共 3 張。
    #[test]
    fn seeded_walls_are_reproducible() {
        let a = Wall::shuffled(42, 3);
        let b = Wall::shuffled(42, 3);
        assert_eq!(a.tiles, b.tiles);
        assert_ne!(a.tiles, Wall::shuffled(43, 3).tiles);
        assert_eq!(a.tiles.iter().filter(|p| p.red).count(), 3);
    }

    /// 開槓後可摸張數減少，寶牌指示牌增加。
    #[test]
    fn kans_shorten_the_live_wall() {
        let mut w = Wall::shuffled(1, 3);
        assert_eq!(w.remaining(), 122);
        w.draw();
        w.draw_rinshan();
        w.reveal_dora();
        assert_eq!(w.remaining(), 120);
        assert_eq!(w.dora_indicators().len(), 2);
    }

    /// 不同手的牌山 seed 不同、同一手相同。
    #[test]
    fn hand_seeds_differ() {
        assert_ne!(hand_seed(7, 0), hand_seed(7, 1));
        assert_eq!(hand_seed(7, 3), hand_seed(7, 3));
    }
}
