//! 天鳳 JSON 格式（tenhou.net/6 牌譜檢視器使用的格式，雀魂等的牌譜轉換工具也多輸出這種格式）。
//!
//! 每局是一個陣列：`[[局序, 本場, 供託], [四家點數], [寶牌指示牌], [裏寶牌指示牌],
//! 配牌0, 摸牌0, 打牌0, 配牌1, 摸牌1, 打牌1, …, 結果]`。
//!
//! - 牌：11–19 萬、21–29 筒、31–39 索、41–47 字（東南西北白發中），51／52／53 = 赤五萬／筒／索。
//! - 打牌欄：數字 = 手切；60 = 摸切；`"r23"` = 立直宣言牌（`r60` = 摸切立直）；
//!   `"333333a33"` = 暗槓；`"25k522525"` = 加槓（`k` 後面是加上的那一張）；0 = 大明槓後的佔位。
//! - 摸牌欄：數字 = 摸牌；`"c191718"` = 吃（`c` 後面是鳴進的牌）；`"41p4141"` = 碰；
//!   `"454545m45"` = 大明槓。字母的位置代表來源：碰在 0／2／4、大明槓在 0／2／6 分別是上家／對家／下家。
//!
//! 這種格式把每家的摸牌與打牌分開記錄，沒有全體的時間順序，所以要自己模擬輪轉（`sequence`）：
//! 打牌後若有一家的下一個摸牌欄是「鳴這一張」，就輪到他；否則輪到下家摸牌。

use crate::{GameRecord, HandRecord, Step};
use mj_core::{Pai, Tile};
use mj_engine::{Action, Seat};
use mj_rules::riichi::RiichiConfig;
use serde_json::Value;

/// 解析天鳳 JSON 牌譜。
pub fn parse(text: &str, id: &str) -> Result<GameRecord, String> {
    let root: Value = serde_json::from_str(text).map_err(|e| format!("JSON 格式錯誤：{e}"))?;
    let disp = root["rule"]["disp"].as_str().unwrap_or("南喰赤");
    if disp.contains('三') {
        return Err("三人麻將不支援".into());
    }
    let has_red = ["aka51", "aka52", "aka53", "aka"]
        .iter()
        .any(|k| root["rule"][k].as_i64().unwrap_or(0) > 0)
        || disp.contains('赤');
    let rules = RiichiConfig {
        hanchan: !disp.contains('東'),
        red_fives: if has_red { 3 } else { 0 },
        kuitan: disp.contains('喰'),
        ..RiichiConfig::TENHOU
    };
    let names: [String; 4] = std::array::from_fn(|s| {
        root["name"][s]
            .as_str()
            .map(str::to_string)
            .unwrap_or_default()
    });
    let hands = root["log"]
        .as_array()
        .ok_or("缺少 log 欄位")?
        .iter()
        .enumerate()
        .map(|(i, h)| hand(h, rules.red_fives).map_err(|e| format!("第 {i} 局：{e}")))
        .collect::<Result<Vec<_>, _>>()?;
    if hands.is_empty() {
        return Err("牌譜中沒有任何一局".into());
    }
    let final_scores = root["sc"].as_array().and_then(|sc| {
        (sc.len() >= 8).then(|| std::array::from_fn(|s| sc[2 * s].as_f64().unwrap_or(0.0) as i32))
    });
    Ok(GameRecord {
        id: root["ref"].as_str().unwrap_or(id).to_string(),
        players: names,
        rules,
        hands,
        final_scores,
    })
}

/// 牌的代碼 → 牌。
fn pai(code: i64, red_fives: u8) -> Result<Pai, String> {
    let (index, red) = match code {
        11..=19 => (code - 11, false),
        21..=29 => (code - 21 + 9, false),
        31..=39 => (code - 31 + 18, false),
        41..=47 => (code - 41 + 27, false),
        51..=53 => ((code - 51) * 9 + 4, red_fives > 0),
        _ => return Err(format!("無法辨識的牌代碼 {code}")),
    };
    let tile = Tile::new(index as u8).ok_or_else(|| format!("牌代碼 {code}"))?;
    Ok(Pai { tile, red })
}

fn pais(v: &Value, red: u8) -> Result<Vec<Pai>, String> {
    v.as_array()
        .ok_or("應為陣列")?
        .iter()
        .map(|x| pai(x.as_i64().ok_or("牌應為數字")?, red))
        .collect()
}

/// 鳴牌字串（如 `"41p4141"`）的內容。
#[derive(Debug, Clone, PartialEq, Eq)]
struct CallString {
    /// 字母：c 吃、p 碰、m 大明槓、a 暗槓、k 加槓
    letter: char,
    /// 字母在字串中的位置
    pos: usize,
    /// 字母後面的那一張（鳴進或加上的牌）
    key: Pai,
    /// 其餘的牌
    others: Vec<Pai>,
}

fn call_string(s: &str, red: u8) -> Result<CallString, String> {
    let pos = s
        .find(|c: char| c.is_ascii_alphabetic())
        .ok_or_else(|| format!("無法辨識的鳴牌 {s:?}"))?;
    let letter = s[pos..].chars().next().unwrap_or('?');
    let digits: String = s.chars().filter(char::is_ascii_digit).collect();
    if digits.len() % 2 != 0 {
        return Err(format!("無法辨識的鳴牌 {s:?}"));
    }
    let codes: Vec<i64> = (0..digits.len() / 2)
        .map(|i| digits[2 * i..2 * i + 2].parse().unwrap_or(0))
        .collect();
    // 字母後面那一張在 codes 中的位置 = 字母前面的牌數。
    let k = pos / 2;
    let key = pai(*codes.get(k).ok_or("鳴牌字串太短")?, red)?;
    let others = codes
        .iter()
        .enumerate()
        .filter(|&(i, _)| i != k)
        .map(|(_, &c)| pai(c, red))
        .collect::<Result<_, _>>()?;
    Ok(CallString {
        letter,
        pos,
        key,
        others,
    })
}

impl CallString {
    /// 被鳴牌者相對於鳴牌者的位置：1 = 下家、2 = 對家、3 = 上家。
    fn source_offset(&self) -> u8 {
        match (self.letter, self.pos) {
            ('m', 6) | (_, 4) => 1,
            (_, 2) => 2,
            _ => 3,
        }
    }

    /// 摸牌欄的鳴牌（吃、碰、大明槓）對應的動作。
    fn call_action(&self) -> Result<Action, String> {
        let two = || -> Result<[Pai; 2], String> {
            self.others
                .get(..2)
                .and_then(|s| s.try_into().ok())
                .ok_or_else(|| "鳴牌的牌數不對".to_string())
        };
        match self.letter {
            'c' => Ok(Action::Chi { consumed: two()? }),
            'p' => Ok(Action::Pon { consumed: two()? }),
            'm' => Ok(Action::Daiminkan),
            c => Err(format!("摸牌欄不應出現 {c}")),
        }
    }
}

/// 摸牌欄或打牌欄的一格。
enum Entry {
    Tile(i64),
    Call(String),
}

fn entries(v: &Value) -> Result<Vec<Entry>, String> {
    v.as_array()
        .ok_or("摸牌／打牌欄應為陣列")?
        .iter()
        .map(|x| match x {
            Value::Number(n) => n.as_i64().map(Entry::Tile).ok_or("牌應為整數".into()),
            Value::String(s) => Ok(Entry::Call(s.clone())),
            _ => Err("無法辨識的摸牌／打牌".to_string()),
        })
        .collect()
}

/// 一局的結果中與動作有關的部分。
struct Result_ {
    /// 和牌：(和牌者, 放銃者)；自摸時兩者相同
    wins: Vec<(Seat, Seat)>,
    kyuushu: bool,
    sanchahou: bool,
}

fn result(v: &Value) -> Result<Result_, String> {
    let a = v.as_array().ok_or("結果應為陣列")?;
    let kind = a.first().and_then(Value::as_str).ok_or("缺少結果種類")?;
    let mut wins = Vec::new();
    if kind == "和了" {
        // 之後每兩格一組：[四家增減], [和牌者, 放銃者, 包牌者, 說明, 役…]
        for info in a.iter().skip(2).step_by(2) {
            let who = info[0].as_u64().ok_or("和牌者")? as Seat;
            let from = info[1].as_u64().ok_or("放銃者")? as Seat;
            wins.push((who, from));
        }
    }
    Ok(Result_ {
        wins,
        kyuushu: kind == "九種九牌",
        sanchahou: kind == "三家和了",
    })
}

fn hand(h: &Value, red: u8) -> Result<HandRecord, String> {
    let a = h.as_array().ok_or("每局應為陣列")?;
    if a.len() < 17 {
        return Err("每局的欄位不足".into());
    }
    let head = a[0].as_array().ok_or("局資訊")?;
    let n = |i: usize| head.get(i).and_then(Value::as_i64).unwrap_or(0);
    let scores = a[1].as_array().ok_or("點數")?;
    let haipai: [Vec<Pai>; 4] = (0..4)
        .map(|s| pais(&a[4 + 3 * s], red))
        .collect::<Result<Vec<_>, _>>()?
        .try_into()
        .expect("four hands");
    let takes = (0..4)
        .map(|s| entries(&a[5 + 3 * s]))
        .collect::<Result<Vec<_>, _>>()?;
    let discards = (0..4)
        .map(|s| entries(&a[6 + 3 * s]))
        .collect::<Result<Vec<_>, _>>()?;
    let dealer = (n(0) % 4) as Seat;
    let res = result(&a[16])?;
    Ok(HandRecord {
        round_wind: (n(0) / 4) as u8,
        kyoku: (n(0) % 4) as u8,
        honba: n(1) as u8,
        riichi_sticks: n(2) as u8,
        scores: std::array::from_fn(|s| scores.get(s).and_then(Value::as_i64).unwrap_or(0) as i32),
        haipai,
        dora_indicators: pais(&a[2], red)?,
        ura_indicators: pais(&a[3], red)?,
        steps: sequence(dealer, &takes, &discards, &res, red)?,
        end_scores: None,
    })
}

/// 依輪轉規則把四家分開的摸牌、打牌欄排成時間順序。
fn sequence(
    dealer: Seat,
    takes: &[Vec<Entry>],
    discards: &[Vec<Entry>],
    res: &Result_,
    red: u8,
) -> Result<Vec<Step>, String> {
    let mut steps = Vec::new();
    let mut ti = [0usize; 4];
    let mut di = [0usize; 4];
    let mut cur = dealer;
    // 輪到 cur 時是否要先摸牌（吃碰後不摸牌）
    let mut draw = true;
    let mut last_discard: Option<(Seat, Pai)> = None;
    loop {
        let c = usize::from(cur);
        let mut drawn = None;
        if draw {
            let Some(e) = takes[c].get(ti[c]) else {
                // 該摸牌卻沒有紀錄：這一局在上一個動作結束（榮和、流局）。
                for &(who, from) in &res.wins {
                    if who != from {
                        steps.push(Step::Act {
                            seat: who,
                            action: Action::Ron,
                        });
                    }
                }
                if res.sanchahou {
                    let (d, _) = last_discard.ok_or("三家和之前沒有打牌")?;
                    for i in 1..4 {
                        steps.push(Step::Act {
                            seat: (d + i) % 4,
                            action: Action::Ron,
                        });
                    }
                }
                break;
            };
            ti[c] += 1;
            let Entry::Tile(code) = e else {
                return Err(format!("座位 {cur} 應摸牌，卻是鳴牌"));
            };
            let p = pai(*code, red)?;
            steps.push(Step::Draw { seat: cur, pai: p });
            drawn = Some(p);
        }
        let Some(e) = discards[c].get(di[c]) else {
            // 摸牌後沒有打牌：自摸或九種九牌。
            let action = if res.wins.iter().any(|&(w, f)| w == cur && f == cur) {
                Action::Tsumo
            } else if res.kyuushu {
                Action::KyuushuKyuuhai
            } else {
                return Err(format!("座位 {cur} 摸牌後沒有動作"));
            };
            steps.push(Step::Act { seat: cur, action });
            break;
        };
        di[c] += 1;
        let (code, riichi) = match e {
            Entry::Tile(code) => (*code, false),
            Entry::Call(s) if s.starts_with('r') => {
                let code = s[1..]
                    .parse()
                    .map_err(|_| format!("無法辨識的立直 {s:?}"))?;
                (code, true)
            }
            Entry::Call(s) => {
                let cs = call_string(s, red)?;
                let action = match cs.letter {
                    'a' => Action::Ankan { tile: cs.key.tile },
                    'k' => Action::Kakan { pai: cs.key },
                    l => return Err(format!("打牌欄不應出現 {l}")),
                };
                steps.push(Step::Act { seat: cur, action });
                // 開槓後摸嶺上牌，仍是同一家。
                draw = true;
                continue;
            }
        };
        let (pai, tsumogiri) = if code == 60 {
            (drawn.ok_or(format!("座位 {cur} 沒有摸牌卻摸切"))?, true)
        } else {
            (pai(code, red)?, false)
        };
        let action = if riichi {
            Action::Riichi { pai, tsumogiri }
        } else {
            Action::Discard { pai, tsumogiri }
        };
        steps.push(Step::Act { seat: cur, action });
        last_discard = Some((cur, pai));

        // 是否有人鳴這張牌：他的下一個摸牌欄是鳴牌，來源是打牌者，而且鳴的是這張牌。
        let caller = (1..4).map(|i| (cur + i) % 4).find_map(|s| {
            let Some(Entry::Call(cs)) = takes[usize::from(s)].get(ti[usize::from(s)]) else {
                return None;
            };
            let cs = call_string(cs, red).ok()?;
            ((s + cs.source_offset()) % 4 == cur && cs.key == pai).then_some((s, cs))
        });
        match caller {
            Some((s, cs)) => {
                ti[usize::from(s)] += 1;
                steps.push(Step::Act {
                    seat: s,
                    action: cs.call_action()?,
                });
                cur = s;
                if cs.letter == 'm' {
                    // 大明槓：打牌欄有一格 0 佔位，接著摸嶺上牌。
                    di[usize::from(s)] += 1;
                    draw = true;
                } else {
                    draw = false;
                }
            }
            None => {
                cur = (cur + 1) % 4;
                draw = true;
            }
        }
    }
    Ok(steps)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 鳴牌字串：鳴進的牌與來源位置。
    #[test]
    fn parses_call_strings() {
        let c = call_string("41p4141", 3).unwrap();
        assert_eq!((c.letter, c.source_offset()), ('p', 2));
        let c = call_string("4747p47", 3).unwrap();
        assert_eq!(c.source_offset(), 1);
        let c = call_string("c275226", 3).unwrap();
        assert_eq!(c.key, "7p".parse().unwrap());
        assert_eq!(c.others, vec!["0p".parse().unwrap(), "6p".parse().unwrap()]);
        assert_eq!(c.source_offset(), 3);
        let c = call_string("454545m45", 3).unwrap();
        assert_eq!(c.source_offset(), 1);
        let c = call_string("25k522525", 3).unwrap();
        assert_eq!((c.letter, c.key), ('k', "0p".parse().unwrap()));
    }
}
