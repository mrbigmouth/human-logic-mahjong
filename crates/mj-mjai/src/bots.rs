//! 內建的簡單 mjai 機器人：只用 mjai 事件追蹤自己的手牌，用來驗證介接（不需要外部程式或模型）。
//!
//! - [`Kind::Tsumogiri`]：永遠摸切，其他時候一律不動作；
//! - [`Kind::Shanten`]：打出讓向聽數最低、進張最多的牌（不鳴牌、不立直、不和牌）。
//!
//! 兩者都只做一定合法的動作，所以介接若有任何錯誤（事件轉換、回應解析），對局會停下來並回報。

use crate::Bot;
use mj_calc::{discard_options, naive_remaining};
use mj_core::{Pai, TileCounts};
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
}

impl MjaiBot {
    pub fn new(kind: Kind) -> Self {
        MjaiBot {
            kind,
            me: 0,
            hand: Vec::new(),
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
        // 只有「自己剛摸牌」時才打牌，其他時候不動作。
        let Some(last) = events.last() else {
            return Ok(json!({ "type": "none" }));
        };
        if last["type"] != "tsumo" || last["actor"].as_u64() != Some(u64::from(self.me)) {
            return Ok(json!({ "type": "none" }));
        }
        let drawn = parse_pai(last["pai"].as_str().unwrap_or("?"))?;
        let pai = match self.kind {
            Kind::Tsumogiri => drawn,
            Kind::Shanten => {
                let counts = TileCounts::from_tiles(self.hand.iter().map(|p| p.tile));
                let best = discard_options(&counts, &naive_remaining(&counts))
                    .first()
                    .map(|o| o.discard)
                    .unwrap_or(drawn.tile);
                // 同種牌優先打剛摸到的那張（摸切），其次是非赤五。
                if drawn.tile == best {
                    drawn
                } else {
                    *self
                        .hand
                        .iter()
                        .filter(|p| p.tile == best)
                        .min_by_key(|p| p.red)
                        .expect("best discard is in hand")
                }
            }
        };
        Ok(json!({
            "type": "dahai",
            "actor": self.me,
            "pai": pai_str(pai),
            "tsumogiri": pai == drawn,
        }))
    }
}
