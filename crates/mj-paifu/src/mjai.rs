//! mjai 格式：外部 AI（Mortal 等）使用的事件格式，每行一個 JSON 事件。
//!
//! - 牌的記法：`1m`–`9m`、`1p`–`9p`、`1s`–`9s`，赤五 `5mr`／`5pr`／`5sr`，
//!   字牌 `E S W N`（東南西北）`P F C`（白發中），看不到的牌 `?`。
//! - [`events_to_mjai`]：引擎事件 → mjai 事件（玩家視角的遮蔽事件也可以轉，別人的牌變成 `?`）。
//! - [`parse`]：完整資訊的 mjai 牌譜（每行一個事件）→ [`GameRecord`]，之後就能重播、預測評估、復盤。
//!
//! 事件對應：
//!
//! | 引擎 | mjai |
//! |---|---|
//! | `StartGame` | `start_game` |
//! | `StartKyoku` | `start_kyoku`（`bakaze`、`kyoku` 從 1 起算、`honba`、`kyotaku`、`oya`、`scores`、`dora_marker`、`tehais`） |
//! | `Draw` | `tsumo` |
//! | `Discard`（立直宣言牌） | `reach` ＋ `dahai` |
//! | `Discard` | `dahai`（含 `tsumogiri`） |
//! | `Call` | `chi`／`pon`／`daiminkan`（`target`、`pai`、`consumed`）、`ankan`（`consumed`）、`kakan`（`pai`、`consumed`） |
//! | `RiichiAccepted` | `reach_accepted` |
//! | `Dora` | `dora` |
//! | `HandEnd`（和牌） | `hora`（`actor`、`target`、`pai`、`deltas`、`ura_markers`） |
//! | `HandEnd`（流局） | `ryukyoku`（`reason`：`fanpai` 荒牌、`yao9` 九種九牌、`kaze4` 四風連打、`reach4` 四家立直、`kan4` 四槓散了、`ron3` 三家和；`tenpais`、`deltas`） |
//! | `EndKyoku` | `end_kyoku`（附 `scores`） |
//! | `EndGame` | `end_game`（`scores`） |

use crate::{GameRecord, HandRecord, Step};
use mj_core::{Meld, MeldKind, Pai, Suit, Tile};
use mj_engine::{Action, DrawReason, Event, HandEnd, Seat};
use mj_rules::riichi::RiichiConfig;
use serde_json::{Value, json};

/// 牌 → mjai 記法。
pub fn pai_str(p: Pai) -> String {
    let t = p.tile;
    match t.suit() {
        Suit::Honor => ["E", "S", "W", "N", "P", "F", "C"][usize::from(t.number() - 1)].into(),
        s => {
            let letter = match s {
                Suit::Man => 'm',
                Suit::Pin => 'p',
                _ => 's',
            };
            format!("{}{letter}{}", t.number(), if p.red { "r" } else { "" })
        }
    }
}

/// 牌種 → mjai 記法（不含赤）。
pub fn tile_str(t: Tile) -> String {
    pai_str(Pai::plain(t))
}

/// mjai 記法 → 牌（`?` 等無法辨識的記法回傳錯誤）。
pub fn parse_pai(s: &str) -> Result<Pai, String> {
    let honor = ["E", "S", "W", "N", "P", "F", "C"]
        .iter()
        .position(|&h| h == s);
    if let Some(i) = honor {
        return Ok(Pai::plain(Tile::from_index(27 + i as u8)));
    }
    let (body, red) = match s.strip_suffix('r') {
        Some(b) => (b, true),
        None => (s, false),
    };
    let mut cs = body.chars();
    let (Some(n), Some(suit), None) = (cs.next(), cs.next(), cs.next()) else {
        return Err(format!("無法辨識的牌 {s:?}"));
    };
    let suit = match suit {
        'm' => Suit::Man,
        'p' => Suit::Pin,
        's' => Suit::Sou,
        _ => return Err(format!("無法辨識的牌 {s:?}")),
    };
    let n = n
        .to_digit(10)
        .ok_or_else(|| format!("無法辨識的牌 {s:?}"))? as u8;
    let tile = Tile::from_suit_number(suit, n).ok_or_else(|| format!("無法辨識的牌 {s:?}"))?;
    if red && n != 5 {
        return Err(format!("只有 5 有赤牌：{s:?}"));
    }
    Ok(Pai { tile, red })
}

fn pais_json(ps: &[Pai]) -> Value {
    Value::from(ps.iter().map(|&p| pai_str(p)).collect::<Vec<_>>())
}

fn reason_str(r: DrawReason) -> &'static str {
    match r {
        DrawReason::Exhaustive => "fanpai",
        DrawReason::KyuushuKyuuhai => "yao9",
        DrawReason::SuufonRenda => "kaze4",
        DrawReason::SuuchaRiichi => "reach4",
        DrawReason::Suukaikan => "kan4",
        DrawReason::Sanchahou => "ron3",
    }
}

/// 一個引擎事件 → mjai 事件（立直宣言牌會變成 `reach` 與 `dahai` 兩個事件）。
/// 不處理搶槓（需要記住前後事件），一般請用 [`Converter`]。
/// `id` = 接收者的座位（`start_game` 的 `id` 欄位；完整牌譜可以給 `None`）。
pub fn event_to_mjai(ev: &Event, id: Option<Seat>, names: &[String; 4]) -> Vec<Value> {
    match ev {
        Event::StartGame { .. } => {
            let mut v = json!({ "type": "start_game", "names": names });
            if let Some(id) = id {
                v["id"] = json!(id);
            }
            vec![v]
        }
        Event::StartKyoku {
            round_wind,
            kyoku,
            honba,
            riichi_sticks,
            scores,
            dora_indicator,
            hands,
        } => {
            let tehais: Vec<Value> = hands
                .iter()
                .map(|h| {
                    if h.is_empty() {
                        Value::from(vec!["?"; 13])
                    } else {
                        pais_json(h)
                    }
                })
                .collect();
            let bakaze = ["E", "S", "W", "N"][usize::from(round_wind % 4)];
            vec![json!({
                "type": "start_kyoku",
                "bakaze": bakaze,
                "kyoku": kyoku + 1,
                "honba": honba,
                "kyotaku": riichi_sticks,
                "oya": kyoku,
                "scores": scores,
                "dora_marker": tile_str(*dora_indicator),
                "tehais": tehais,
            })]
        }
        Event::Draw { seat, pai, .. } => vec![json!({
            "type": "tsumo",
            "actor": seat,
            "pai": pai.map_or("?".to_string(), pai_str),
        })],
        Event::Discard {
            seat,
            pai,
            tsumogiri,
            riichi,
        } => {
            let dahai = json!({
                "type": "dahai",
                "actor": seat,
                "pai": pai_str(*pai),
                "tsumogiri": tsumogiri,
            });
            if *riichi {
                vec![json!({ "type": "reach", "actor": seat }), dahai]
            } else {
                vec![dahai]
            }
        }
        Event::Call { seat, meld } => vec![call_to_mjai(*seat, meld)],
        Event::RiichiAccepted { seat } => vec![json!({ "type": "reach_accepted", "actor": seat })],
        Event::Dora { indicator } => {
            vec![json!({ "type": "dora", "dora_marker": tile_str(*indicator) })]
        }
        Event::HandEnd { end, deltas } => match end {
            HandEnd::Win {
                winner,
                from,
                win_tile,
                ura_indicators,
                ..
            } => vec![json!({
                "type": "hora",
                "actor": winner,
                "target": from,
                "pai": pai_str(*win_tile),
                "deltas": deltas,
                "ura_markers": ura_indicators.iter().map(|&t| tile_str(t)).collect::<Vec<_>>(),
            })],
            HandEnd::Draw { reason, tenpai, .. } => vec![json!({
                "type": "ryukyoku",
                "reason": reason_str(*reason),
                "tenpais": tenpai,
                "deltas": deltas,
            })],
        },
        Event::EndKyoku { scores } => vec![json!({ "type": "end_kyoku", "scores": scores })],
        Event::EndGame { scores } => vec![json!({ "type": "end_game", "scores": scores })],
    }
}

/// 副露 → mjai 事件。
fn call_to_mjai(seat: Seat, meld: &Meld) -> Value {
    match meld.kind {
        MeldKind::Ankan => json!({
            "type": "ankan",
            "actor": seat,
            "consumed": pais_json(&meld.tiles),
        }),
        MeldKind::Kakan => json!({
            "type": "kakan",
            "actor": seat,
            "pai": pai_str(meld.tiles[3]),
            "consumed": pais_json(&meld.tiles[..3]),
        }),
        kind => {
            let called = meld.called.expect("open meld has a called tile");
            let mut consumed = meld.tiles.clone();
            let i = consumed
                .iter()
                .position(|&p| p == called)
                .expect("called tile in meld");
            consumed.remove(i);
            let name = match kind {
                MeldKind::Chi => "chi",
                MeldKind::Pon => "pon",
                _ => "daiminkan",
            };
            json!({
                "type": name,
                "actor": seat,
                "target": meld.from,
                "pai": pai_str(called),
                "consumed": pais_json(&consumed),
            })
        }
    }
}

/// 有狀態的轉換器：記住各家的副露，處理搶槓。
///
/// 引擎在加槓（或國士無雙搶暗槓）被搶時不會發出副露事件（槓沒有成立），但 mjai 的對方需要先看到
/// `kakan`／`ankan` 才知道可以搶槓。所以和牌的前一個事件是放銃者摸牌（不是打牌）時，
/// 先補上那個槓的事件；把事件即時轉給外部 AI 時，在詢問搶槓前也要補（[`Converter::robbed_kan`]），
/// 之後引擎若真的完成這個槓，不再重複送出。
#[derive(Debug, Clone)]
pub struct Converter {
    id: Option<Seat>,
    names: [String; 4],
    melds: [Vec<Meld>; 4],
    /// 最後一個事件是誰的摸牌（之後若有人和牌，就是搶槓）
    drew: Option<Seat>,
    /// 已經先送出的槓（座位, 牌種）
    sent_kan: Option<(Seat, Tile)>,
}

impl Converter {
    /// `id` = 接收者的座位（`start_game` 的 `id`；完整牌譜給 `None`），`names` = 四家名稱。
    pub fn new(id: Option<Seat>, names: &[String; 4]) -> Self {
        Converter {
            id,
            names: names.clone(),
            melds: Default::default(),
            drew: None,
            sent_kan: None,
        }
    }

    /// 轉換一個事件。
    pub fn convert(&mut self, ev: &Event) -> Vec<Value> {
        let mut out = Vec::new();
        match ev {
            Event::StartKyoku { .. } => {
                self.melds = Default::default();
                self.sent_kan = None;
            }
            Event::Call { seat, meld } => {
                let s = usize::from(*seat);
                if meld.kind == MeldKind::Kakan {
                    self.melds[s].retain(|m| !(m.kind == MeldKind::Pon && m.base() == meld.base()));
                }
                self.melds[s].push(meld.clone());
                if matches!(meld.kind, MeldKind::Kakan | MeldKind::Ankan)
                    && self.sent_kan.take() == Some((*seat, meld.base()))
                {
                    self.drew = None;
                    return out;
                }
            }
            Event::HandEnd {
                end:
                    HandEnd::Win {
                        winner,
                        from,
                        win_tile,
                        ..
                    },
                ..
            } if winner != from && self.drew == Some(*from) && self.sent_kan.is_none() => {
                out.push(self.robbed_kan(*from, *win_tile));
            }
            _ => {}
        }
        self.drew = match ev {
            Event::Draw { seat, .. } => Some(*seat),
            Event::HandEnd { .. } => self.drew,
            _ => None,
        };
        out.extend(event_to_mjai(ev, self.id, &self.names));
        out
    }

    /// 最後一個事件是誰的摸牌（之後若被詢問榮和，就是搶槓）。
    pub fn last_draw(&self) -> Option<Seat> {
        self.drew
    }

    /// 被搶的槓的事件：`seat` 有這張牌的碰 → `kakan`，否則是暗槓（國士無雙搶暗槓）。
    /// 記住已送出，之後引擎完成這個槓時不再重複。
    pub fn robbed_kan(&mut self, seat: Seat, pai: Pai) -> Value {
        self.sent_kan = Some((seat, pai.tile));
        let pon = self.melds[usize::from(seat)]
            .iter()
            .find(|m| m.kind == MeldKind::Pon && m.base() == pai.tile);
        match pon {
            Some(m) => json!({
                "type": "kakan",
                "actor": seat,
                "pai": pai_str(pai),
                "consumed": pais_json(&m.tiles),
            }),
            None => json!({
                "type": "ankan",
                "actor": seat,
                "consumed": vec![tile_str(pai.tile); 4],
            }),
        }
    }
}

/// 引擎事件紀錄 → mjai 事件。完整紀錄（上帝視角）轉出來就是完整的 mjai 牌譜。
pub fn events_to_mjai(log: &[Event], id: Option<Seat>, names: &[String; 4]) -> Vec<Value> {
    let mut c = Converter::new(id, names);
    log.iter().flat_map(|e| c.convert(e)).collect()
}

fn field<'a>(v: &'a Value, k: &str) -> Result<&'a Value, String> {
    v.get(k)
        .ok_or_else(|| format!("{} 事件缺少 {k}", v["type"]))
}

fn seat_of(v: &Value, k: &str) -> Result<Seat, String> {
    field(v, k)?
        .as_u64()
        .filter(|&s| s < 4)
        .map(|s| s as Seat)
        .ok_or_else(|| format!("{k} 應為 0–3"))
}

fn pai_of(v: &Value, k: &str) -> Result<Pai, String> {
    parse_pai(
        field(v, k)?
            .as_str()
            .ok_or_else(|| format!("{k} 應為字串"))?,
    )
}

fn pais_of(v: &Value, k: &str) -> Result<Vec<Pai>, String> {
    field(v, k)?
        .as_array()
        .ok_or_else(|| format!("{k} 應為陣列"))?
        .iter()
        .map(|x| parse_pai(x.as_str().ok_or("牌應為字串")?))
        .collect()
}

fn scores_of(v: &Value, k: &str) -> Result<[i32; 4], String> {
    let a = field(v, k)?
        .as_array()
        .ok_or_else(|| format!("{k} 應為陣列"))?;
    if a.len() != 4 {
        return Err(format!("{k} 應有 4 個值"));
    }
    let mut out = [0; 4];
    for (o, x) in out.iter_mut().zip(a) {
        *o = x.as_i64().ok_or_else(|| format!("{k} 應為整數"))? as i32;
    }
    Ok(out)
}

/// 是否看起來是 mjai 牌譜（第一個非空行是帶 `type` 欄位的 JSON 物件）。
pub fn looks_like_mjai(text: &str) -> bool {
    text.lines()
        .find(|l| !l.trim().is_empty())
        .and_then(|l| serde_json::from_str::<Value>(l).ok())
        .is_some_and(|v| v.get("type").is_some())
}

/// 解析完整資訊的 mjai 牌譜（每行一個事件；需要所有人的配牌與摸牌）。
pub fn parse(text: &str, id: &str) -> Result<GameRecord, String> {
    let mut players: [String; 4] = Default::default();
    let mut hands: Vec<HandRecord> = Vec::new();
    let mut final_scores = None;
    let mut reach = [false; 4];
    let mut last_draw: Option<Seat> = None;
    for (n, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let v: Value =
            serde_json::from_str(line).map_err(|e| format!("第 {} 行不是 JSON：{e}", n + 1))?;
        let ty = v["type"].as_str().unwrap_or("");
        let at = |e: String| format!("第 {} 行（{ty}）：{e}", n + 1);
        let hand = hands.last_mut();
        let act = |seat: Seat, action: Action| Step::Act { seat, action };
        match (ty, hand) {
            ("start_game", _) => {
                if let Some(names) = v.get("names").and_then(Value::as_array) {
                    for (p, name) in players.iter_mut().zip(names) {
                        *p = name.as_str().unwrap_or("").to_string();
                    }
                }
            }
            ("start_kyoku", _) => {
                let bakaze = v["bakaze"].as_str().unwrap_or("E");
                let round_wind = ["E", "S", "W", "N"]
                    .iter()
                    .position(|&w| w == bakaze)
                    .ok_or_else(|| at(format!("無法辨識的場風 {bakaze}")))?
                    as u8;
                let tehais = field(&v, "tehais").map_err(at)?;
                let mut haipai: [Vec<Pai>; 4] = Default::default();
                for (s, h) in haipai.iter_mut().enumerate() {
                    *h = tehais
                        .get(s)
                        .ok_or_else(|| at("tehais 應有 4 家".into()))?
                        .as_array()
                        .ok_or_else(|| at("tehais 應為陣列".into()))?
                        .iter()
                        .map(|x| parse_pai(x.as_str().unwrap_or("?")))
                        .collect::<Result<_, _>>()
                        .map_err(|e| at(format!("需要完整資訊的牌譜（{e}）")))?;
                }
                hands.push(HandRecord {
                    round_wind,
                    kyoku: v["kyoku"].as_u64().unwrap_or(1).saturating_sub(1) as u8,
                    honba: v["honba"].as_u64().unwrap_or(0) as u8,
                    riichi_sticks: v["kyotaku"].as_u64().unwrap_or(0) as u8,
                    scores: scores_of(&v, "scores").map_err(at)?,
                    haipai,
                    dora_indicators: vec![pai_of(&v, "dora_marker").map_err(at)?],
                    ura_indicators: Vec::new(),
                    steps: Vec::new(),
                    end_scores: None,
                });
                reach = [false; 4];
                last_draw = None;
            }
            ("tsumo", Some(h)) => {
                let seat = seat_of(&v, "actor").map_err(at)?;
                h.steps.push(Step::Draw {
                    seat,
                    pai: pai_of(&v, "pai").map_err(at)?,
                });
                last_draw = Some(seat);
            }
            ("reach", Some(_)) => reach[usize::from(seat_of(&v, "actor").map_err(at)?)] = true,
            ("dahai", Some(h)) => {
                let seat = seat_of(&v, "actor").map_err(at)?;
                let pai = pai_of(&v, "pai").map_err(at)?;
                let tsumogiri = v["tsumogiri"].as_bool().unwrap_or(false);
                let action = if std::mem::take(&mut reach[usize::from(seat)]) {
                    Action::Riichi { pai, tsumogiri }
                } else {
                    Action::Discard { pai, tsumogiri }
                };
                h.steps.push(act(seat, action));
                last_draw = None;
            }
            ("chi" | "pon", Some(h)) => {
                let seat = seat_of(&v, "actor").map_err(at)?;
                let mut c = pais_of(&v, "consumed").map_err(at)?;
                if c.len() != 2 {
                    return Err(at("consumed 應有 2 張".into()));
                }
                c.sort();
                let consumed = [c[0], c[1]];
                let action = if ty == "chi" {
                    Action::Chi { consumed }
                } else {
                    Action::Pon { consumed }
                };
                h.steps.push(act(seat, action));
            }
            ("daiminkan", Some(h)) => {
                h.steps
                    .push(act(seat_of(&v, "actor").map_err(at)?, Action::Daiminkan));
            }
            ("ankan", Some(h)) => {
                let c = pais_of(&v, "consumed").map_err(at)?;
                let tile = c.first().ok_or_else(|| at("consumed 是空的".into()))?.tile;
                h.steps.push(act(
                    seat_of(&v, "actor").map_err(at)?,
                    Action::Ankan { tile },
                ));
                last_draw = None;
            }
            ("kakan", Some(h)) => {
                let pai = pai_of(&v, "pai").map_err(at)?;
                h.steps.push(act(
                    seat_of(&v, "actor").map_err(at)?,
                    Action::Kakan { pai },
                ));
                last_draw = None;
            }
            ("dora", Some(h)) => h
                .dora_indicators
                .push(pai_of(&v, "dora_marker").map_err(at)?),
            ("hora", Some(h)) => {
                let actor = seat_of(&v, "actor").map_err(at)?;
                let target = seat_of(&v, "target").map_err(at)?;
                let action = if actor == target {
                    Action::Tsumo
                } else {
                    Action::Ron
                };
                h.steps.push(act(actor, action));
                if let Ok(ura) = pais_of(&v, "ura_markers")
                    && !ura.is_empty()
                {
                    h.ura_indicators = ura;
                }
            }
            ("ryukyoku", Some(h)) => match v["reason"].as_str() {
                Some("yao9") => {
                    let seat = last_draw.ok_or_else(|| at("九種九牌之前沒有摸牌".into()))?;
                    h.steps.push(act(seat, Action::KyuushuKyuuhai));
                }
                Some("ron3") => {
                    // 三家和：最後打牌者以外的三家都宣告榮和。
                    let Some(Step::Act { seat: d, .. }) = h.steps.iter().rev().find(|s| {
                        matches!(
                            s,
                            Step::Act {
                                action: Action::Discard { .. } | Action::Riichi { .. },
                                ..
                            }
                        )
                    }) else {
                        return Err(at("三家和之前沒有打牌".into()));
                    };
                    let d = *d;
                    for i in 1..4 {
                        h.steps.push(act((d + i) % 4, Action::Ron));
                    }
                }
                _ => {}
            },
            ("end_kyoku", Some(h)) => {
                if let Ok(s) = scores_of(&v, "scores") {
                    h.end_scores = Some(s);
                }
            }
            ("end_game", _) => final_scores = scores_of(&v, "scores").ok(),
            ("reach_accepted" | "none", _) => {}
            (other, None) => return Err(at(format!("{other} 出現在 start_kyoku 之前"))),
            (other, Some(_)) => return Err(at(format!("無法辨識的事件 {other}"))),
        }
    }
    if hands.is_empty() {
        return Err("牌譜中沒有任何一局".into());
    }
    for h in &mut hands {
        restore_red_indicators(h);
    }
    Ok(GameRecord {
        id: id.to_string(),
        players,
        rules: RiichiConfig::TENHOU,
        hands,
        final_scores,
    })
}

/// 寶牌指示牌的赤五：有些來源（例如本專案引擎匯出的牌譜）的指示牌只記牌種、不分赤五。
/// 每種 5 只有一張赤牌，所以某種 5 的普通牌出現超過 3 張、又沒有赤牌時，多出來的那張一定是赤五指示牌。
fn restore_red_indicators(h: &mut HandRecord) {
    for suit in [Suit::Man, Suit::Pin, Suit::Sou] {
        let five = Tile::from_suit_number(suit, 5).expect("five");
        let known = h
            .haipai
            .iter()
            .flatten()
            .copied()
            .chain(h.steps.iter().filter_map(|s| match s {
                Step::Draw { pai, .. } => Some(*pai),
                _ => None,
            }))
            .chain(h.dora_indicators.iter().copied())
            .chain(h.ura_indicators.iter().copied());
        let (plain, red) = known.filter(|p| p.tile == five).fold((0, 0), |(a, b), p| {
            if p.red { (a, b + 1) } else { (a + 1, b) }
        });
        if plain > 3 && red == 0 {
            let fix = h
                .dora_indicators
                .iter_mut()
                .chain(h.ura_indicators.iter_mut())
                .find(|p| p.tile == five && !p.red);
            if let Some(p) = fix {
                p.red = true;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 所有牌（含赤五）的記法來回轉換一致。
    #[test]
    fn tile_notation_round_trip() {
        for t in Tile::all() {
            let p = Pai::plain(t);
            assert_eq!(parse_pai(&pai_str(p)).unwrap(), p);
        }
        for s in ["5mr", "5pr", "5sr"] {
            let p = parse_pai(s).unwrap();
            assert!(p.red);
            assert_eq!(pai_str(p), s);
        }
        assert_eq!(pai_str(Pai::plain(Tile::from_index(31))), "P");
        assert!(parse_pai("?").is_err());
        assert!(parse_pai("4mr").is_err());
    }
}
