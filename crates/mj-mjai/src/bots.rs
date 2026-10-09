//! 內建的簡單 mjai 機器人：只用 mjai 事件追蹤自己的手牌，用來驗證介接（不需要外部程式或模型）。
//!
//! - [`Kind::Tsumogiri`]：永遠摸切，其他時候一律不動作；
//! - [`Kind::Shanten`]：打出讓向聽數最低、進張最多的牌（不鳴牌、不立直、不和牌）。
//!
//! 兩者都只做一定合法的動作，所以介接若有任何錯誤（事件轉換、回應解析），對局會停下來並回報。
//! 自己不會鳴牌，但旁觀牌譜時（`MjaiPlayer::shadow`）玩家鳴牌後也會被問要打哪張，這時避開食替的牌。

use crate::Bot;
use mj_calc::{discard_options, naive_remaining};
use mj_core::{Pai, Tile, TileCounts};
use mj_paifu::mjai::{pai_str, parse_pai};
use serde_json::{Value, json};

/// 機器人種類。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// 永遠摸切
    Tsumogiri,
    /// 向聽數最低、進張最多
    Shanten,
}

/// 內建的 mjai 機器人。
#[derive(Debug, Clone)]
pub struct MjaiBot {
    kind: Kind,
    me: u8,
    hand: Vec<Pai>,
    /// 自己已立直（旁觀牌譜時，玩家立直後只能摸切）
    riichi: bool,
}

impl MjaiBot {
    pub fn new(kind: Kind) -> Self {
        MjaiBot {
            kind,
            me: 0,
            hand: Vec::new(),
            riichi: false,
        }
    }

    fn remove(&mut self, p: Pai) -> Result<(), String> {
        let i = self
            .hand
            .iter()
            .position(|&x| x == p)
            .ok_or_else(|| format!("手牌中沒有 {}", pai_str(p)))?;
        self.hand.remove(i);
        Ok(())
    }

    /// 依事件更新自己的手牌。
    fn observe(&mut self, e: &Value) -> Result<(), String> {
        let actor = e["actor"].as_u64().map(|a| a as u8);
        let mine = actor == Some(self.me);
        let pai = |k: &str| parse_pai(e[k].as_str().unwrap_or("?"));
        match e["type"].as_str().unwrap_or("") {
            "start_game" => self.me = e["id"].as_u64().unwrap_or(0) as u8,
            "start_kyoku" => {
                self.riichi = false;
                self.hand = e["tehais"][usize::from(self.me)]
                    .as_array()
                    .ok_or("start_kyoku 缺少 tehais")?
                    .iter()
                    .map(|x| parse_pai(x.as_str().unwrap_or("?")))
                    .collect::<Result<_, _>>()?;
            }
            "tsumo" if mine => self.hand.push(pai("pai")?),
            "dahai" if mine => self.remove(pai("pai")?)?,
            "chi" | "pon" | "daiminkan" | "ankan" if mine => {
                for x in e["consumed"].as_array().ok_or("缺少 consumed")? {
                    self.remove(parse_pai(x.as_str().unwrap_or("?"))?)?;
                }
            }
            "kakan" if mine => self.remove(pai("pai")?)?,
            "reach" if mine => self.riichi = true,
            _ => {}
        }
        Ok(())
    }
}

impl Bot for MjaiBot {
    fn react(&mut self, events: &[Value]) -> Result<Value, String> {
        for e in events {
            self.observe(e)?;
        }
        // 只有「自己剛摸牌」或「自己剛吃碰」時才打牌，其他時候不動作。
        let Some(last) = events.last() else {
            return Ok(json!({ "type": "none" }));
        };
        let ty = last["type"].as_str().unwrap_or("");
        if !matches!(ty, "tsumo" | "chi" | "pon")
            || last["actor"].as_u64() != Some(u64::from(self.me))
        {
            return Ok(json!({ "type": "none" }));
        }
        let drawn = (ty == "tsumo")
            .then(|| parse_pai(last["pai"].as_str().unwrap_or("?")))
            .transpose()?;
        let forbidden = if ty == "tsumo" {
            Vec::new()
        } else {
            kuikae(last)?
        };
        let allowed: Vec<Pai> = self
            .hand
            .iter()
            .copied()
            .filter(|p| !forbidden.contains(&p.tile))
            .collect();
        let pai = match (self.kind, drawn) {
            (_, Some(d)) if self.riichi => d,
            (Kind::Tsumogiri, Some(d)) => d,
            (Kind::Tsumogiri, None) => *allowed.first().ok_or("沒有可以打的牌")?,
            (Kind::Shanten, _) => {
                let counts = TileCounts::from_tiles(self.hand.iter().map(|p| p.tile));
                let best = discard_options(&counts, &naive_remaining(&counts))
                    .into_iter()
                    .map(|o| o.discard)
                    .find(|t| !forbidden.contains(t))
                    .or_else(|| allowed.first().map(|p| p.tile))
                    .ok_or("沒有可以打的牌")?;
                // 同種牌優先打剛摸到的那張（摸切），其次是非赤五。
                match drawn {
                    Some(d) if d.tile == best => d,
                    _ => *allowed
                        .iter()
                        .filter(|p| p.tile == best)
                        .min_by_key(|p| p.red)
                        .expect("best discard is in hand"),
                }
            }
        };
        Ok(json!({
            "type": "dahai",
            "actor": self.me,
            "pai": pai_str(pai),
            "tsumogiri": Some(pai) == drawn,
        }))
    }
}

/// 吃碰後不能打的牌（食替）：鳴的那種牌；吃的時候還有順子另一端的牌（例如 34 吃 2 → 不能打 5）。
fn kuikae(call: &Value) -> Result<Vec<Tile>, String> {
    let called = parse_pai(call["pai"].as_str().unwrap_or("?"))?.tile;
    let mut out = vec![called];
    if call["type"] == "chi" {
        let mut consumed: Vec<Tile> = call["consumed"]
            .as_array()
            .ok_or("缺少 consumed")?
            .iter()
            .map(|x| parse_pai(x.as_str().unwrap_or("?")).map(|p| p.tile))
            .collect::<Result<_, _>>()?;
        consumed.sort();
        let (n, lo, hi) = (called.number(), consumed[0], consumed[1]);
        let other = if called < lo {
            Tile::from_suit_number(called.suit(), n + 3)
        } else if called > hi && n > 3 {
            Tile::from_suit_number(called.suit(), n - 3)
        } else {
            None
        };
        out.extend(other);
    }
    Ok(out)
}
