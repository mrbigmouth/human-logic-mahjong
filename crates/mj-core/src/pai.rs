use crate::{ParseTileError, Tile, parse_pais};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// 牌（pai）：實體牌＝牌種＋是否為赤五。記法 `0m`／`0p`／`0s` 代表赤五萬／赤五筒／赤五索。
/// 赤五本身算一張寶牌（赤寶牌），所以打牌、吃碰時要分清楚是哪一張 5。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Pai {
    /// 牌種
    pub tile: Tile,
    /// 是否為赤五
    pub red: bool,
}

impl Pai {
    /// 一般（非赤）牌。
    pub const fn plain(tile: Tile) -> Self {
        Pai { tile, red: false }
    }
}

impl From<Tile> for Pai {
    fn from(tile: Tile) -> Self {
        Pai::plain(tile)
    }
}

/// 赤五輸出為 `0m`／`0p`／`0s`，其餘同 [`Tile`]。
impl fmt::Display for Pai {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.red {
            let s = self.tile.to_string();
            write!(f, "0{}", &s[1..])
        } else {
            self.tile.fmt(f)
        }
    }
}

impl FromStr for Pai {
    type Err = ParseTileError;

    /// 解析單張牌，例如 `0p`（赤五筒）。
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match parse_pais(s)?.as_slice() {
            [p] => Ok(*p),
            _ => Err(ParseTileError(s.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 赤五的記法；字牌沒有赤牌（`0z` 不合法）。
    #[test]
    fn red_five_notation() {
        let p: Pai = "0p".parse().unwrap();
        assert!(p.red);
        assert_eq!(p.tile, "5p".parse().unwrap());
        assert_eq!(p.to_string(), "0p");
        assert!("0z".parse::<Pai>().is_err());
    }
}
