//! 與規則無關的基本型別：牌、牌數表。
//!
//! 牌種依慣例編號 0..34：
//! `1m..9m`（萬子 0–8）、`1p..9p`（筒子/餅 9–17）、`1s..9s`（索子/條 18–26）、
//! `1z..7z`（字牌 27–33：東 南 西 北 白 發 中）。
//! [`Tile`] 只代表牌種；[`Pai`]（牌）是實體牌，多了「是否為赤五」的旗標。
//!
//! 程式中的名稱多採用日語羅馬拼音，中文對照見 `docs/05-glossary.md`。

mod meld;
mod pai;

pub use meld::{Meld, MeldKind};
pub use pai::Pai;

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// 牌種數：數牌 27 種＋字牌 7 種。
pub const NUM_TILE_KINDS: usize = 34;

/// 花色。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Suit {
    /// 萬子（m）
    Man,
    /// 筒子／餅（p）
    Pin,
    /// 索子／條（s）
    Sou,
    /// 字牌（z）：風牌與三元牌
    Honor,
}

impl Suit {
    /// 記法中的花色字母。
    fn letter(self) -> char {
        match self {
            Suit::Man => 'm',
            Suit::Pin => 'p',
            Suit::Sou => 's',
            Suit::Honor => 'z',
        }
    }
}

/// 牌種，編號 0..34（不區分赤牌）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Tile(u8);

impl Tile {
    /// 由編號建立；超出範圍回傳 `None`。
    pub fn new(index: u8) -> Option<Self> {
        (usize::from(index) < NUM_TILE_KINDS).then_some(Tile(index))
    }

    /// 由花色與數字建立，例如 (筒子, 5) = 5p。字牌的數字 1–7 依序為東南西北白發中。
    pub fn from_suit_number(suit: Suit, number: u8) -> Option<Self> {
        let (base, max) = match suit {
            Suit::Man => (0, 9),
            Suit::Pin => (9, 9),
            Suit::Sou => (18, 9),
            Suit::Honor => (27, 7),
        };
        (1..=max).contains(&number).then(|| Tile(base + number - 1))
    }

    /// 編號 0..34，可直接當作 [`TileCounts`] 的索引。
    pub fn index(self) -> usize {
        usize::from(self.0)
    }

    /// 花色。
    pub fn suit(self) -> Suit {
        match self.0 {
            0..=8 => Suit::Man,
            9..=17 => Suit::Pin,
            18..=26 => Suit::Sou,
            _ => Suit::Honor,
        }
    }

    /// 數牌回傳 1..=9；字牌回傳 1..=7（東 南 西 北 白 發 中）。
    pub fn number(self) -> u8 {
        self.0 % 9 + 1
    }

    /// 是否為字牌。
    pub fn is_honor(self) -> bool {
        self.suit() == Suit::Honor
    }

    /// 是否為老頭牌（數牌的 1 或 9）。
    pub fn is_terminal(self) -> bool {
        !self.is_honor() && matches!(self.number(), 1 | 9)
    }

    /// 是否為幺九牌（yaochu＝老頭牌＋字牌，共 13 種）。
    pub fn is_yaochu(self) -> bool {
        self.is_honor() || self.is_terminal()
    }

    /// 是否為風牌（東、南、西、北）。
    pub fn is_wind(self) -> bool {
        (27..31).contains(&self.0)
    }

    /// 是否為三元牌（白、發、中）。
    pub fn is_dragon(self) -> bool {
        self.0 >= 31
    }

    /// 由編號建立；`index >= 34` 時 panic。可用於常數。
    pub const fn from_index(index: u8) -> Self {
        assert!((index as usize) < NUM_TILE_KINDS);
        Tile(index)
    }

    /// 風位編號（0 = 東 .. 3 = 北）對應的風牌。
    pub const fn wind(w: u8) -> Self {
        Tile::from_index(27 + w % 4)
    }

    /// 依編號順序列出全部 34 種牌。
    pub fn all() -> impl Iterator<Item = Tile> {
        (0..NUM_TILE_KINDS as u8).map(Tile)
    }
}

/// 輸出記法，例如 `5m`、`7z`。
impl fmt::Display for Tile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.number(), self.suit().letter())
    }
}

/// 牌的記法有誤。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseTileError(pub String);

impl fmt::Display for ParseTileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid tile notation: {}", self.0)
    }
}

impl std::error::Error for ParseTileError {}

impl FromStr for Tile {
    type Err = ParseTileError;

    /// 解析單張牌，例如 `5m` 或 `7z`。
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let tiles = parse_tiles(s)?;
        match tiles.as_slice() {
            [t] => Ok(*t),
            _ => Err(ParseTileError(s.to_string())),
        }
    }
}

/// 解析緊湊記法，例如 `123m456p789s11z`（一二三萬、四五六筒、七八九索、東東）。
/// `0` 代表赤五，但此函式會丟掉赤牌資訊；要保留請用 [`parse_pais`]。
pub fn parse_tiles(s: &str) -> Result<Vec<Tile>, ParseTileError> {
    Ok(parse_pais(s)?.into_iter().map(|p| p.tile).collect())
}

/// 解析緊湊記法並保留赤五（`0m`、`0p`、`0s`）。
pub fn parse_pais(s: &str) -> Result<Vec<Pai>, ParseTileError> {
    let err = || ParseTileError(s.to_string());
    let mut out = Vec::new();
    // 數字先暫存，遇到花色字母時才確定是哪種牌。
    let mut pending: Vec<u8> = Vec::new();
    for c in s.chars().filter(|c| !c.is_whitespace()) {
        if let Some(d) = c.to_digit(10) {
            pending.push(d as u8);
            continue;
        }
        let suit = match c {
            'm' => Suit::Man,
            'p' => Suit::Pin,
            's' => Suit::Sou,
            'z' => Suit::Honor,
            _ => return Err(err()),
        };
        if pending.is_empty() {
            return Err(err());
        }
        for n in pending.drain(..) {
            let red = n == 0;
            // 字牌沒有赤牌。
            if red && suit == Suit::Honor {
                return Err(err());
            }
            let tile = Tile::from_suit_number(suit, if red { 5 } else { n }).ok_or_else(err)?;
            out.push(Pai { tile, red });
        }
    }
    // 結尾還有數字沒接花色字母，例如 "123"。
    if pending.is_empty() {
        Ok(out)
    } else {
        Err(err())
    }
}

/// 牌數表：一組牌（多重集合），記錄每種牌各有幾張。序列化為長度 34 的陣列。
/// 向聽、拆解等計算都使用這個表示法，速度快且不需配置記憶體。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(into = "Vec<u8>", try_from = "Vec<u8>")]
pub struct TileCounts(pub [u8; NUM_TILE_KINDS]);

impl From<TileCounts> for Vec<u8> {
    fn from(c: TileCounts) -> Self {
        c.0.to_vec()
    }
}

impl TryFrom<Vec<u8>> for TileCounts {
    type Error = String;

    fn try_from(v: Vec<u8>) -> Result<Self, Self::Error> {
        let len = v.len();
        v.try_into()
            .map(TileCounts)
            .map_err(|_| format!("expected {NUM_TILE_KINDS} tile counts, got {len}"))
    }
}

impl Default for TileCounts {
    fn default() -> Self {
        TileCounts([0; NUM_TILE_KINDS])
    }
}

impl TileCounts {
    /// 由一串牌建立。
    pub fn from_tiles(tiles: impl IntoIterator<Item = Tile>) -> Self {
        let mut c = Self::default();
        for t in tiles {
            c.add(t);
        }
        c
    }

    /// 某種牌的張數。
    pub fn get(&self, t: Tile) -> u8 {
        self.0[t.index()]
    }

    /// 加入一張。
    pub fn add(&mut self, t: Tile) {
        self.0[t.index()] += 1;
    }

    /// 移除一張；沒有這張牌時回傳 false，表格不變。
    pub fn remove(&mut self, t: Tile) -> bool {
        let c = &mut self.0[t.index()];
        if *c == 0 {
            return false;
        }
        *c -= 1;
        true
    }

    /// 總張數。
    pub fn total(&self) -> u32 {
        self.0.iter().map(|&c| u32::from(c)).sum()
    }

    /// 依編號順序逐張列出（同種牌重複出現）。
    pub fn tiles(&self) -> impl Iterator<Item = Tile> + '_ {
        Tile::all().flat_map(|t| std::iter::repeat_n(t, usize::from(self.get(t))))
    }
}

/// 輸出緊湊記法，例如 `123m456p11z`。
impl fmt::Display for TileCounts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut last: Option<Suit> = None;
        for t in self.tiles() {
            // 換花色時先補上前一個花色的字母。
            if let Some(s) = last.filter(|&s| s != t.suit()) {
                write!(f, "{}", s.letter())?;
            }
            write!(f, "{}", t.number())?;
            last = Some(t.suit());
        }
        if let Some(s) = last {
            write!(f, "{}", s.letter())?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 每種牌輸出成記法再解析回來要一致。
    #[test]
    fn tile_roundtrip() {
        for t in Tile::all() {
            assert_eq!(t.to_string().parse::<Tile>().unwrap(), t);
        }
    }

    /// 老頭牌、幺九牌、字牌的分類。
    #[test]
    fn classification() {
        let t: Tile = "1m".parse().unwrap();
        assert!(t.is_terminal() && t.is_yaochu() && !t.is_honor());
        let t: Tile = "5p".parse().unwrap();
        assert!(!t.is_yaochu());
        let t: Tile = "7z".parse().unwrap();
        assert!(t.is_honor() && t.is_yaochu());
        // 字牌只有 1z–7z。
        assert!("8z".parse::<Tile>().is_err());
    }

    /// 0p（赤五筒）解析後以 5p 輸出。
    #[test]
    fn counts_display_and_parse() {
        let hand = TileCounts::from_tiles(parse_tiles("123m406p789s11z").unwrap());
        assert_eq!(hand.total(), 11);
        assert_eq!(hand.to_string(), "123m456p789s11z");
    }

    /// 缺花色字母、缺數字、未知字母都要報錯。
    #[test]
    fn rejects_bad_notation() {
        assert!(parse_tiles("123").is_err());
        assert!(parse_tiles("m").is_err());
        assert!(parse_tiles("1x").is_err());
    }
}
