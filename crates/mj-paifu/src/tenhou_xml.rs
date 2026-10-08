//! 天鳳 mjlog（XML）格式。`https://tenhou.net/0/log/?<牌譜 ID>` 下載的原始牌譜。
//!
//! 主要標籤：
//! - `GO type=`：規則旗標（0x02 無赤、0x04 無食斷、0x08 半莊、0x10 三麻）；`UN n0..n3`：名稱（URL 編碼）。
//! - `INIT seed="局序,本場,供託,骰,骰,寶牌指示牌" ten="點數/100" oya= hai0..hai3=`：一局開始。
//! - `T/U/V/W<牌>`：座位 0–3 摸牌；`D/E/F/G<牌>`：座位 0–3 打牌（打出剛摸的那一張 = 摸切）。
//! - `N who= m=`：副露（`m` 是位元編碼，見 [`decode_meld`]）；`REACH who= step=1`：立直宣言。
//! - `DORA hai=`：槓寶牌；`AGARI`／`RYUUKYOKU`：和牌／流局，`sc` = 各家「原點數,增減」（單位 100 點），
//!   最後一局附 `owari`（終局點數與名次點）。
//!
//! 牌以 0–135 表示：`牌 / 4` = 牌種（與本專案的 [`Tile`] 編號相同），16／52／88 是赤五萬／筒／索。

use crate::{GameRecord, HandRecord, Step};
use mj_core::{Pai, Tile};
use mj_engine::{Action, Seat};
use mj_rules::riichi::RiichiConfig;
use std::collections::HashMap;

/// 解析 mjlog XML。
pub fn parse(text: &str, id: &str) -> Result<GameRecord, String> {
    let mut p = Parser::default();
    for (name, attrs) in tags(text) {
        p.tag(name, &attrs)
            .map_err(|e| format!("第 {} 局：{e}", p.hands.len()))?;
    }
    p.finish(id)
}

/// 136 編號 → 牌。赤五只有在規則有赤牌時才算。
fn pai(id: u32, red_fives: u8) -> Result<Pai, String> {
    let tile = Tile::new(u8::try_from(id / 4).map_err(|_| format!("牌編號 {id} 超出範圍"))?)
        .ok_or_else(|| format!("牌編號 {id} 超出範圍"))?;
    let red = red_fives > 0 && matches!(id, 16 | 52 | 88);
    Ok(Pai { tile, red })
}

/// 解析後的副露。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedMeld {
    /// 對應的動作
    pub action: Action,
    /// 從手中拿出的牌（136 編號；加槓為加上去的那一張）
    pub from_hand: Vec<u32>,
}

/// 解析副露的位元編碼 `m`。
///
/// - 吃（bit 2）：`m >> 10` = 順子起點×3 + 鳴進的是第幾張；三張的 136 餘數在 bit 3–8。
/// - 碰（bit 3）／加槓（bit 4）：`m >> 9` = 牌種×3 + 鳴進的是第幾張；沒用到的那一張（加槓時是加上的那一張）在 bit 5–6。
/// - 其餘為槓：`m >> 8` = 牌種×4 + 鳴進的是第幾張；`m & 3` = 0 為暗槓，否則為大明槓。
/// - bit 5（只有 bit 5）為三麻的拔北，不支援。
pub fn decode_meld(m: u32, red_fives: u8) -> Result<DecodedMeld, String> {
    let p = |id| pai(id, red_fives);
    if m & 0x4 != 0 {
        let base = m >> 10;
        let called = base % 3;
        let start = base / 3;
        let start = start / 7 * 9 + start % 7;
        let ids: Vec<u32> = (0..3)
            .map(|i| (start + i) * 4 + ((m >> (3 + 2 * i)) & 3))
            .collect();
        let own: Vec<u32> = (0..3)
            .filter(|&i| i != called)
            .map(|i| ids[i as usize])
            .collect();
        Ok(DecodedMeld {
            action: Action::Chi {
                consumed: [p(own[0])?, p(own[1])?],
            },
            from_hand: own,
        })
    } else if m & 0x18 != 0 {
        let unused = (m >> 5) & 3;
        let base = m >> 9;
        let called = base % 3;
        let tile = base / 3;
        let three: Vec<u32> = (0..4)
            .filter(|&i| i != unused)
            .map(|i| tile * 4 + i)
            .collect();
        if m & 0x8 != 0 {
            let own: Vec<u32> = (0..3)
                .filter(|&i| i != called)
                .map(|i| three[i as usize])
                .collect();
            Ok(DecodedMeld {
                action: Action::Pon {
                    consumed: [p(own[0])?, p(own[1])?],
                },
                from_hand: own,
            })
        } else {
            let added = tile * 4 + unused;
            Ok(DecodedMeld {
                action: Action::Kakan { pai: p(added)? },
                from_hand: vec![added],
            })
        }
    } else if m & 0x20 != 0 {
        Err("拔北（三人麻將）不支援".into())
    } else {
        let base = m >> 8;
        let tile = base / 4;
        let called = base % 4;
        if m & 3 == 0 {
            Ok(DecodedMeld {
                action: Action::Ankan {
                    tile: p(tile * 4)?.tile,
                },
                from_hand: (0..4).map(|i| tile * 4 + i).collect(),
            })
        } else {
            Ok(DecodedMeld {
                action: Action::Daiminkan,
                from_hand: (0..4)
                    .filter(|&i| i != called)
                    .map(|i| tile * 4 + i)
                    .collect(),
            })
        }
    }
}

/// 逐一取出 `<名稱 屬性="值" ...>` 標籤（不處理巢狀內容；mjlog 只有自我結束的標籤）。
fn tags(text: &str) -> impl Iterator<Item = (&str, HashMap<&str, &str>)> {
    text.split('<').skip(1).filter_map(|chunk| {
        let body = chunk.split('>').next()?.trim_end_matches('/').trim();
        if body.starts_with('/') || body.starts_with('?') || body.is_empty() {
            return None;
        }
        let (name, mut rest) = body.split_once(char::is_whitespace).unwrap_or((body, ""));
        let mut attrs = HashMap::new();
        while let Some((k, after)) = rest.split_once("=\"") {
            let Some((v, after)) = after.split_once('"') else {
                break;
            };
            attrs.insert(k.trim(), v);
            rest = after;
        }
        Some((name, attrs))
    })
}

/// 解析逗號分隔的數字。
fn nums(s: &str) -> Result<Vec<i64>, String> {
    s.split(',')
        .filter(|x| !x.is_empty())
        .map(|x| x.trim().parse().map_err(|_| format!("無法解析數字 {x:?}")))
        .collect()
}

/// URL 解碼（名稱以 %XX 編碼的 UTF-8）。
fn url_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && let Ok(v) = u8::from_str_radix(s.get(i + 1..i + 3).unwrap_or(""), 16)
        {
            out.push(v);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[derive(Default)]
struct Parser {
    rules: Option<RiichiConfig>,
    names: Option<[String; 4]>,
    hands: Vec<HandRecord>,
    /// 各座位最後摸到的牌（136 編號）；打出同一張 = 摸切
    last_draw: [Option<u32>; 4],
    /// 已宣告立直、下一張打牌是宣言牌的座位
    riichi_pending: Option<Seat>,
    /// 最後一次摸牌的座位（九種九牌用）
    last_drawer: Option<Seat>,
    /// 最後一次打牌的座位（三家和用）
    last_discarder: Option<Seat>,
    final_scores: Option<[i32; 4]>,
}

impl Parser {
    fn red(&self) -> u8 {
        self.rules.as_ref().map_or(3, |r| r.red_fives)
    }

    fn hand(&mut self) -> Result<&mut HandRecord, String> {
        self.hands
            .last_mut()
            .ok_or_else(|| "INIT 之前出現對局內容".into())
    }

    fn push(&mut self, step: Step) -> Result<(), String> {
        self.hand()?.steps.push(step);
        Ok(())
    }

    fn tag(&mut self, name: &str, a: &HashMap<&str, &str>) -> Result<(), String> {
        let get = |k: &str| {
            a.get(k)
                .copied()
                .ok_or_else(|| format!("{name} 缺少屬性 {k}"))
        };
        // 摸牌 T/U/V/W、打牌 D/E/F/G：名稱是一個字母＋數字。
        let mut chars = name.chars();
        if let Some(c) = chars.next()
            && !chars.as_str().is_empty()
            && chars.as_str().bytes().all(|b| b.is_ascii_digit())
            && let Some(k) = "TUVWDEFG".find(c)
        {
            let id: u32 = chars
                .as_str()
                .parse()
                .map_err(|_| format!("無法解析 {name}"))?;
            let seat = (k % 4) as Seat;
            let pai = pai(id, self.red())?;
            if k < 4 {
                self.last_draw[k] = Some(id);
                self.last_drawer = Some(seat);
                return self.push(Step::Draw { seat, pai });
            }
            let tsumogiri = self.last_draw[usize::from(seat)].take() == Some(id);
            let action = if self.riichi_pending.take() == Some(seat) {
                Action::Riichi { pai, tsumogiri }
            } else {
                Action::Discard { pai, tsumogiri }
            };
            self.last_discarder = Some(seat);
            return self.push(Step::Act { seat, action });
        }
        match name {
            "GO" => {
                let t: u32 = get("type")?.parse().map_err(|_| "GO type 無法解析")?;
                if t & 0x10 != 0 {
                    return Err("三人麻將不支援".into());
                }
                self.rules = Some(RiichiConfig {
                    hanchan: t & 0x08 != 0,
                    red_fives: if t & 0x02 != 0 { 0 } else { 3 },
                    kuitan: t & 0x04 == 0,
                    ..RiichiConfig::TENHOU
                });
            }
            "UN" if self.names.is_none() => {
                // 斷線重連時也會出現只有部分名稱的 UN，只採用第一個完整的。
                if let (Ok(a0), Ok(a1), Ok(a2), Ok(a3)) =
                    (get("n0"), get("n1"), get("n2"), get("n3"))
                {
                    self.names = Some([a0, a1, a2, a3].map(url_decode));
                }
            }
            "INIT" => {
                let seed = nums(get("seed")?)?;
                let ten = nums(get("ten")?)?;
                if seed.len() < 6 || ten.len() < 4 {
                    return Err("INIT 的 seed 或 ten 欄位不完整".into());
                }
                let red = self.red();
                let haipai = (0..4)
                    .map(|s| {
                        nums(get(&format!("hai{s}"))?)?
                            .into_iter()
                            .map(|id| pai(id as u32, red))
                            .collect::<Result<Vec<_>, _>>()
                    })
                    .collect::<Result<Vec<_>, String>>()?;
                let oya: u8 = get("oya")?.parse().map_err(|_| "oya 無法解析")?;
                if oya != (seed[0] % 4) as u8 {
                    return Err(format!("莊家 {oya} 與局序 {} 不一致", seed[0]));
                }
                self.hands.push(HandRecord {
                    round_wind: (seed[0] / 4) as u8,
                    kyoku: (seed[0] % 4) as u8,
                    honba: seed[1] as u8,
                    riichi_sticks: seed[2] as u8,
                    scores: std::array::from_fn(|s| ten[s] as i32 * 100),
                    haipai: haipai.try_into().expect("four hands"),
                    dora_indicators: vec![pai(seed[5] as u32, red)?],
                    ura_indicators: Vec::new(),
                    steps: Vec::new(),
                    end_scores: None,
                });
                self.last_draw = [None; 4];
                self.riichi_pending = None;
            }
            "N" => {
                let who: Seat = get("who")?.parse().map_err(|_| "N who 無法解析")?;
                let m: u32 = get("m")?.parse().map_err(|_| "N m 無法解析")?;
                let meld = decode_meld(m, self.red())?;
                if matches!(meld.action, Action::Chi { .. } | Action::Pon { .. }) {
                    // 吃碰後沒有摸牌，接下來打的牌一定是手切。
                    self.last_draw[usize::from(who)] = None;
                }
                self.push(Step::Act {
                    seat: who,
                    action: meld.action,
                })?;
            }
            "REACH" => {
                if get("step")? == "1" {
                    self.riichi_pending = Some(get("who")?.parse().map_err(|_| "REACH who")?);
                }
            }
            "DORA" => {
                let id: u32 = get("hai")?.parse().map_err(|_| "DORA hai 無法解析")?;
                let p = pai(id, self.red())?;
                self.hand()?.dora_indicators.push(p);
            }
            "AGARI" => {
                let who: Seat = get("who")?.parse().map_err(|_| "AGARI who")?;
                let from: Seat = get("fromWho")?.parse().map_err(|_| "AGARI fromWho")?;
                let action = if who == from {
                    Action::Tsumo
                } else {
                    Action::Ron
                };
                self.push(Step::Act { seat: who, action })?;
                if let Ok(ura) = get("doraHaiUra") {
                    let red = self.red();
                    let ura = nums(ura)?
                        .into_iter()
                        .map(|id| pai(id as u32, red))
                        .collect::<Result<Vec<_>, _>>()?;
                    self.hand()?.ura_indicators = ura;
                }
                self.end(a)?;
            }
            "RYUUKYOKU" => {
                match a.get("type").copied() {
                    Some("yao9") => {
                        let seat = self.last_drawer.ok_or("九種九牌之前沒有摸牌")?;
                        self.push(Step::Act {
                            seat,
                            action: Action::KyuushuKyuuhai,
                        })?;
                    }
                    Some("ron3") => {
                        let d = self.last_discarder.ok_or("三家和之前沒有打牌")?;
                        for i in 1..4 {
                            self.push(Step::Act {
                                seat: (d + i) % 4,
                                action: Action::Ron,
                            })?;
                        }
                    }
                    _ => {}
                }
                self.end(a)?;
            }
            _ => {}
        }
        Ok(())
    }

    /// 一局結束：記下結束後的點數，最後一局另有終局點數。
    fn end(&mut self, a: &HashMap<&str, &str>) -> Result<(), String> {
        if let Some(sc) = a.get("sc") {
            let v = nums(sc)?;
            if v.len() >= 8 {
                let end = std::array::from_fn(|s| ((v[2 * s] + v[2 * s + 1]) * 100) as i32);
                self.hand()?.end_scores = Some(end);
            }
        }
        if let Some(owari) = a.get("owari") {
            // 「點數/100,名次點」交錯；名次點是小數，只取點數。
            let v: Vec<&str> = owari.split(',').collect();
            if v.len() >= 8 {
                let mut scores = [0; 4];
                for (s, x) in scores.iter_mut().enumerate() {
                    *x = v[2 * s]
                        .trim()
                        .parse::<i32>()
                        .map_err(|_| "owari 無法解析")?
                        * 100;
                }
                self.final_scores = Some(scores);
            }
        }
        Ok(())
    }

    fn finish(self, id: &str) -> Result<GameRecord, String> {
        if self.hands.is_empty() {
            return Err("牌譜中沒有任何一局".into());
        }
        Ok(GameRecord {
            id: id.to_string(),
            players: self.names.unwrap_or_default(),
            rules: self.rules.unwrap_or(RiichiConfig::TENHOU),
            hands: self.hands,
            final_scores: self.final_scores,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tile(s: &str) -> Tile {
        s.parse().unwrap()
    }

    /// 副露解碼：以天鳳牌譜中實際出現的編碼驗證。
    #[test]
    fn decodes_melds() {
        // 碰：m=46122 → 牌種 46122>>9 = 90 → 30 = 4z（北）… 檢查種類即可
        let d = decode_meld(46122, 3).unwrap();
        assert!(matches!(d.action, Action::Pon { consumed } if consumed[0].tile == tile("4z")));
        // 吃：順子 4m5m6m（起點索引 3），鳴進第 0 張（4m），各張的 136 餘數都是 0（5m 為赤五 16）
        let m = ((3 * 3) << 10) | 0x4;
        let d = decode_meld(m, 3).unwrap();
        assert_eq!(
            d.action,
            Action::Chi {
                consumed: ["0m".parse().unwrap(), "6m".parse().unwrap()]
            }
        );
        // 4137 是碰（bit 3）
        assert!(matches!(
            decode_meld(4137, 3).unwrap().action,
            Action::Pon { .. }
        ));
        // 暗槓：牌種 9（1p）、m & 3 = 0
        let d = decode_meld((9 * 4) << 8, 3).unwrap();
        assert_eq!(d.action, Action::Ankan { tile: tile("1p") });
        // 大明槓：牌種 0（1m），鳴進第 2 張，來自對家
        let d = decode_meld((2 << 8) | 2, 3).unwrap();
        assert_eq!(d.action, Action::Daiminkan);
        assert_eq!(d.from_hand, vec![0, 1, 3]);
    }

    /// 名稱的 URL 解碼。
    #[test]
    fn decodes_names() {
        assert_eq!(url_decode("%78%74%74%66"), "xttf");
        assert_eq!(url_decode("%E7%AB%8B%E7%9B%B4"), "立直");
    }
}
