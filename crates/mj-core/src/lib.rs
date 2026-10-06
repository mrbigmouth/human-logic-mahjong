//! Rule-agnostic primitives: tiles and tile multisets.
//!
//! Tiles are indexed 0..34 in the conventional order
//! `1m..9m, 1p..9p, 1s..9s, E S W N P(白) F(發) C(中)`.
//! Red fives are tracked separately by the rule layer, not here.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

pub const NUM_TILE_KINDS: usize = 34;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Suit {
    Man,
    Pin,
    Sou,
    Honor,
}

impl Suit {
    fn letter(self) -> char {
        match self {
            Suit::Man => 'm',
            Suit::Pin => 'p',
            Suit::Sou => 's',
            Suit::Honor => 'z',
        }
    }
}

/// A tile kind, 0..34.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Tile(u8);

impl Tile {
    pub fn new(index: u8) -> Option<Self> {
        (usize::from(index) < NUM_TILE_KINDS).then_some(Tile(index))
    }

    pub fn from_suit_number(suit: Suit, number: u8) -> Option<Self> {
        let (base, max) = match suit {
            Suit::Man => (0, 9),
            Suit::Pin => (9, 9),
            Suit::Sou => (18, 9),
            Suit::Honor => (27, 7),
        };
        (1..=max).contains(&number).then(|| Tile(base + number - 1))
    }

    pub fn index(self) -> usize {
        usize::from(self.0)
    }

    pub fn suit(self) -> Suit {
        match self.0 {
            0..=8 => Suit::Man,
            9..=17 => Suit::Pin,
            18..=26 => Suit::Sou,
            _ => Suit::Honor,
        }
    }

    /// 1..=9 for number tiles, 1..=7 for honors (E S W N P F C).
    pub fn number(self) -> u8 {
        self.0 % 9 + 1
    }

    pub fn is_honor(self) -> bool {
        self.suit() == Suit::Honor
    }

    pub fn is_terminal(self) -> bool {
        !self.is_honor() && matches!(self.number(), 1 | 9)
    }

    pub fn is_yaochu(self) -> bool {
        self.is_honor() || self.is_terminal()
    }

    pub fn all() -> impl Iterator<Item = Tile> {
        (0..NUM_TILE_KINDS as u8).map(Tile)
    }
}

impl fmt::Display for Tile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.number(), self.suit().letter())
    }
}

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

    /// Parses a single tile such as `5m` or `7z`.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let tiles = parse_tiles(s)?;
        match tiles.as_slice() {
            [t] => Ok(*t),
            _ => Err(ParseTileError(s.to_string())),
        }
    }
}

/// Parses compact notation such as `123m456p789s11z`. `0` is accepted as a red five.
pub fn parse_tiles(s: &str) -> Result<Vec<Tile>, ParseTileError> {
    let err = || ParseTileError(s.to_string());
    let mut out = Vec::new();
    let mut pending: Vec<u8> = Vec::new();
    for c in s.chars().filter(|c| !c.is_whitespace()) {
        if let Some(d) = c.to_digit(10) {
            pending.push(if d == 0 { 5 } else { d as u8 });
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
            out.push(Tile::from_suit_number(suit, n).ok_or_else(err)?);
        }
    }
    if pending.is_empty() {
        Ok(out)
    } else {
        Err(err())
    }
}

/// A multiset of tiles, stored as counts per kind. Serialized as a 34-element array.
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
    pub fn from_tiles(tiles: impl IntoIterator<Item = Tile>) -> Self {
        let mut c = Self::default();
        for t in tiles {
            c.add(t);
        }
        c
    }

    pub fn get(&self, t: Tile) -> u8 {
        self.0[t.index()]
    }

    pub fn add(&mut self, t: Tile) {
        self.0[t.index()] += 1;
    }

    /// Returns false (and leaves the counts unchanged) if the tile is absent.
    pub fn remove(&mut self, t: Tile) -> bool {
        let c = &mut self.0[t.index()];
        if *c == 0 {
            return false;
        }
        *c -= 1;
        true
    }

    pub fn total(&self) -> u32 {
        self.0.iter().map(|&c| u32::from(c)).sum()
    }

    pub fn tiles(&self) -> impl Iterator<Item = Tile> + '_ {
        Tile::all().flat_map(|t| std::iter::repeat_n(t, usize::from(self.get(t))))
    }
}

impl fmt::Display for TileCounts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut last: Option<Suit> = None;
        for t in self.tiles() {
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

    #[test]
    fn tile_roundtrip() {
        for t in Tile::all() {
            assert_eq!(t.to_string().parse::<Tile>().unwrap(), t);
        }
    }

    #[test]
    fn classification() {
        let t: Tile = "1m".parse().unwrap();
        assert!(t.is_terminal() && t.is_yaochu() && !t.is_honor());
        let t: Tile = "5p".parse().unwrap();
        assert!(!t.is_yaochu());
        let t: Tile = "7z".parse().unwrap();
        assert!(t.is_honor() && t.is_yaochu());
        assert!("8z".parse::<Tile>().is_err());
    }

    #[test]
    fn counts_display_and_parse() {
        let hand = TileCounts::from_tiles(parse_tiles("123m406p789s11z").unwrap());
        assert_eq!(hand.total(), 11);
        assert_eq!(hand.to_string(), "123m456p789s11z");
    }

    #[test]
    fn rejects_bad_notation() {
        assert!(parse_tiles("123").is_err());
        assert!(parse_tiles("m").is_err());
        assert!(parse_tiles("1x").is_err());
    }
}
