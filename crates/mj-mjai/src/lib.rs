//! mjai 協定橋接（M8）：讓外部程式形式的 AI（例如 Mortal、akochan）加入對局，
//! 作為對手或評判基準。只透過協定溝通、不連結外部 AI 的程式碼，
//! 因此它們的授權（如 AGPL）不會影響本專案。
//!
//! - 事件與牌的轉換在 `mj_paifu::mjai`（同時用於 mjai 牌譜的匯入）。
//! - [`MjaiPlayer`]：實作引擎的 `Player`，把這個座位看得到的事件轉成 mjai 送給 [`Bot`]，
//!   再把回應轉成合法動作。回應不合法時記下錯誤並讓對局停止（不會默默換成別的動作）。
//! - [`Bot`]：一個說 mjai 的 AI。[`ProcessBot`] 是外部程式（子行程；[`PooledBot`] 在對局之間重複使用行程），
//!   [`bots`] 是內建的簡單機器人（驗證介接用，不需要外部程式）。
//!
//! 通訊格式（與 mjai.app 相同）：每次送出一行「JSON 陣列」，內容是上次回應之後發生的所有事件；
//! AI 回應一行 JSON 物件，例如 `{"type":"dahai","actor":0,"pai":"5m","tsumogiri":false}`、
//! `{"type":"none"}`。立直時先回應 `{"type":"reach","actor":0}`，收到自己的 `reach` 事件後再回應要打的牌。

pub mod bots;
pub mod mortal;

use mj_core::Pai;
use mj_engine::{Action, Event, Player, PlayerView, Seat};
use mj_paifu::mjai::{Converter, parse_pai};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::Mutex;

/// 一個說 mjai 協定的 AI：收到一批事件，回應一個動作。
pub trait Bot: Send {
    /// `events` = 上次回應之後發生的所有事件（最後一個通常是要它回應的那個）。
    fn react(&mut self, events: &[Value]) -> Result<Value, String>;
}

/// 外部程式形式的 AI：啟動子行程，經由標準輸入／輸出以一行一個 JSON 溝通。
pub struct ProcessBot {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl ProcessBot {
    /// 啟動外部程式（`program args...`）。
    pub fn spawn(program: &str, args: &[String]) -> Result<Self, String> {
        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| format!("無法啟動 {program}：{e}"))?;
        let stdin = child.stdin.take().expect("piped stdin");
        let stdout = BufReader::new(child.stdout.take().expect("piped stdout"));
        Ok(ProcessBot {
            child,
            stdin,
            stdout,
        })
    }
}

impl Bot for ProcessBot {
    fn react(&mut self, events: &[Value]) -> Result<Value, String> {
        let line = Value::from(events.to_vec()).to_string();
        writeln!(self.stdin, "{line}")
            .and_then(|_| self.stdin.flush())
            .map_err(|e| format!("寫入外部 AI 失敗：{e}"))?;
        let mut reply = String::new();
        let n = self
            .stdout
            .read_line(&mut reply)
            .map_err(|e| format!("讀取外部 AI 失敗：{e}"))?;
        if n == 0 {
            return Err("外部 AI 已結束（沒有回應）".into());
        }
        serde_json::from_str(reply.trim())
            .map_err(|e| format!("外部 AI 的回應不是 JSON：{e}：{}", reply.trim()))
    }
}

impl Drop for ProcessBot {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// 閒置中的外部程式（依指令區分），給 [`PooledBot`] 重複使用。
static POOL: Mutex<Vec<(Vec<String>, ProcessBot)>> = Mutex::new(Vec::new());

/// 可重複使用的 [`ProcessBot`]：用完（drop）後放回共用的池子，下一場對局直接沿用，
/// 不必每場重新啟動外部程式（例如每次載入模型）。同時存在的行程數 = 同時進行的對局所需的座位數。
///
/// 每場對局都以 `start_game` 開始，所以外部程式必須在收到 `start_game` 時重設狀態（mjai 的慣例）。
/// 通訊失敗過的行程不會放回池子。
pub struct PooledBot {
    cmd: Vec<String>,
    bot: Option<ProcessBot>,
    healthy: bool,
}

impl PooledBot {
    /// 取出一個閒置的行程，沒有就啟動新的。`cmd` = 程式與參數。
    pub fn checkout(cmd: &[String]) -> Result<Self, String> {
        let idle = {
            let mut pool = POOL.lock().unwrap_or_else(|e| e.into_inner());
            pool.iter()
                .position(|(c, _)| c == cmd)
                .map(|i| pool.swap_remove(i).1)
        };
        let bot = match idle {
            Some(b) => b,
            None => ProcessBot::spawn(&cmd[0], &cmd[1..])?,
        };
        Ok(PooledBot {
            cmd: cmd.to_vec(),
            bot: Some(bot),
            healthy: true,
        })
    }
}

impl Bot for PooledBot {
    fn react(&mut self, events: &[Value]) -> Result<Value, String> {
        let r = self.bot.as_mut().expect("bot present").react(events);
        if r.is_err() {
            self.healthy = false;
        }
        r
    }
}

impl Drop for PooledBot {
    fn drop(&mut self) {
        if let Some(bot) = self.bot.take()
            && self.healthy
        {
            let mut pool = POOL.lock().unwrap_or_else(|e| e.into_inner());
            pool.push((std::mem::take(&mut self.cmd), bot));
        }
    }
}

/// 讓說 mjai 的 AI 坐上引擎的牌桌。
pub struct MjaiPlayer<B: Bot> {
    bot: B,
    names: [String; 4],
    converter: Option<Converter>,
    /// 還沒送給 AI 的事件
    pending: Vec<Value>,
    /// 已經把自己的 `reach` 事件送過（立直的兩段式回應）；之後引擎的立直宣言牌事件不再重複送
    reach_sent: bool,
    /// 第一個錯誤（AI 回應不合法、通訊失敗）
    error: Option<String>,
    /// 旁觀模式（見 [`MjaiPlayer::shadow`]）
    shadow: bool,
    /// 旁觀模式下每次詢問的選擇（依詢問順序）
    choices: Vec<ShadowChoice>,
    /// 旁觀模式下最近一次立直時 [`PEEK`] 的回應
    last_peek: Option<Value>,
}

/// 旁觀模式下，AI 在一次詢問中的選擇。
#[derive(Debug, Clone)]
pub struct ShadowChoice {
    /// 對應到的合法動作（回應不合法、或立直時無法得知要打的牌時為 `None`）
    pub action: Option<Action>,
    /// AI 的原始回應（立直時是 `reach` 之後的打牌）
    pub reply: Value,
    /// 回應無法對應到合法動作的原因
    pub error: Option<String>,
    /// 回應是立直時，[`PEEK`] 詢問「立直後打哪張」的回應
    pub peek: Option<Value>,
}

/// mjai 的擴充請求（本專案的 `tools/mortal/mortal_bot.py` 支援）：「假設接著發生這些事件，你會怎麼做？」
/// 回答後 AI 的狀態不變。旁觀模式用它取得立直時要打的牌（立直並沒有真的發生，不能送出真的 `reach` 事件）。
/// 不支援的 AI 回應 `none` 或任何非打牌的回應時，那次立直的打牌視為未知。
pub const PEEK: &str = "mj_peek";

impl<B: Bot> MjaiPlayer<B> {
    /// `names` = 四家名稱（`start_game` 的 `names`）。
    pub fn new(bot: B, names: [String; 4]) -> Self {
        MjaiPlayer {
            bot,
            names,
            converter: None,
            pending: Vec::new(),
            reach_sent: false,
            error: None,
            shadow: false,
            choices: Vec::new(),
            last_peek: None,
        }
    }

    /// 旁觀模式：收到同樣的事件與詢問，記下 AI 的選擇（[`MjaiPlayer::take_choices`]），但不影響對局
    /// （例如掛在 `mj_paifu::replay` 的旁觀者位置，比較 AI 與牌譜的選擇）。回應不合法時只記錄、不停止。
    ///
    /// 立直時不能用兩段式（實際上可能沒有立直，送出 `reach` 事件會讓 AI 的局面與實際不符），
    /// 改用 [`PEEK`] 詢問立直後要打的牌。
    pub fn shadow(mut self) -> Self {
        self.shadow = true;
        self
    }

    /// 取出旁觀模式記下的選擇（第 i 個 = 這個座位第 i 次被詢問）。
    pub fn take_choices(&mut self) -> Vec<ShadowChoice> {
        std::mem::take(&mut self.choices)
    }

    /// 取出第一個錯誤（對局因此中止時，引擎的錯誤只會說「動作不合法」，原因在這裡）。
    pub fn take_error(&mut self) -> Option<String> {
        self.error.take()
    }

    fn fail(&mut self, msg: String) -> Action {
        if self.error.is_none() {
            // 引擎只會回報「動作不合法」，原因先印出來。
            eprintln!("mjai：{msg}");
            self.error = Some(msg);
        }
        // 回傳一定不合法的動作，讓引擎停止對局（自己的回合不會有榮和，回應時不會有自摸）。
        Action::KyuushuKyuuhai
    }

    fn ask(&mut self) -> Result<Value, String> {
        let events = std::mem::take(&mut self.pending);
        self.bot.react(&events)
    }

    /// 把 AI 的回應轉成合法動作。
    fn reply_to_action(
        &mut self,
        view: &PlayerView,
        legal: &[Action],
        r: &Value,
    ) -> Result<Action, String> {
        let me = view.me;
        let ty = r["type"].as_str().unwrap_or("");
        let pai = |k: &str| -> Result<Pai, String> {
            parse_pai(r[k].as_str().ok_or_else(|| format!("回應缺少 {k}：{r}"))?)
        };
        let find = |ok: &dyn Fn(&Action) -> bool| legal.iter().find(|a| ok(a)).cloned();
        let action = match ty {
            "none" => find(&|a| *a == Action::Pass),
            "dahai" => {
                let p = pai("pai")?;
                let t = r["tsumogiri"].as_bool();
                find(&|a| matches!(a, Action::Discard { pai, tsumogiri } if *pai == p && t.is_none_or(|t| t == *tsumogiri)))
                    .or_else(|| find(&|a| matches!(a, Action::Discard { pai, .. } if *pai == p)))
            }
            "reach" => {
                if !legal.iter().any(|a| matches!(a, Action::Riichi { .. })) {
                    return Err(format!("現在不能立直：{r}"));
                }
                let reach = json!({ "type": "reach", "actor": me });
                let d = if self.shadow {
                    let d = self
                        .bot
                        .react(&[json!({ "type": PEEK, "events": [reach] })])?;
                    if d["type"] != "dahai" {
                        return Err(format!("無法得知立直要打的牌（{PEEK} 的回應：{d}）"));
                    }
                    self.last_peek = Some(d.clone());
                    d
                } else {
                    // 兩段式：送出自己的 reach 事件，AI 再回應要打的牌。
                    self.pending.push(reach);
                    self.reach_sent = true;
                    self.ask()?
                };
                if d["type"] != "dahai" {
                    return Err(format!("立直後應回應打牌，卻是 {d}"));
                }
                let p = parse_pai(d["pai"].as_str().unwrap_or("?"))?;
                let t = d["tsumogiri"].as_bool();
                find(&|a| matches!(a, Action::Riichi { pai, tsumogiri } if *pai == p && t.is_none_or(|t| t == *tsumogiri)))
                    .or_else(|| find(&|a| matches!(a, Action::Riichi { pai, .. } if *pai == p)))
            }
            "chi" | "pon" => {
                let c: Vec<Pai> = r["consumed"]
                    .as_array()
                    .ok_or_else(|| format!("回應缺少 consumed：{r}"))?
                    .iter()
                    .map(|x| parse_pai(x.as_str().unwrap_or("?")))
                    .collect::<Result<_, _>>()?;
                let mut c = c;
                c.sort();
                find(&|a| match a {
                    Action::Chi { consumed } if ty == "chi" => {
                        let mut x = consumed.to_vec();
                        x.sort();
                        x == c
                    }
                    Action::Pon { consumed } if ty == "pon" => {
                        let mut x = consumed.to_vec();
                        x.sort();
                        x == c
                    }
                    _ => false,
                })
            }
            "daiminkan" => find(&|a| *a == Action::Daiminkan),
            "ankan" => {
                let c = r["consumed"][0].as_str().unwrap_or("?");
                let t = parse_pai(c)?.tile;
                find(&|a| *a == Action::Ankan { tile: t })
            }
            "kakan" => {
                let p = pai("pai")?;
                find(&|a| matches!(a, Action::Kakan { pai } if pai.tile == p.tile))
            }
            "hora" => find(&|a| matches!(a, Action::Tsumo | Action::Ron)),
            "ryukyoku" => find(&|a| *a == Action::KyuushuKyuuhai),
            _ => None,
        };
        action.ok_or_else(|| format!("回應不合法：{r}；合法動作：{legal:?}"))
    }
}

impl<B: Bot> Player for MjaiPlayer<B> {
    fn on_event(&mut self, me: Seat, event: &Event) {
        let names = &self.names;
        let conv = self
            .converter
            .get_or_insert_with(|| Converter::new(Some(me), names));
        let mut out = conv.convert(event);
        // 自己的立直：AI 在兩段式回應時已經收到自己的 reach 事件，不再重複。
        if let Event::Discard {
            seat, riichi: true, ..
        } = event
            && *seat == me
            && std::mem::take(&mut self.reach_sent)
        {
            out.retain(|v| v["type"] != "reach");
        }
        self.pending.extend(out);
    }

    fn act(&mut self, view: &PlayerView, legal: &[Action]) -> Action {
        if self.shadow {
            return self.shadow_act(view, legal);
        }
        if self.error.is_some() {
            return self.fail(String::new());
        }
        self.add_robbed_kan(view, legal);
        let reply = match self.ask() {
            Ok(r) => r,
            Err(e) => return self.fail(e),
        };
        match self.reply_to_action(view, legal, &reply) {
            Ok(a) => a,
            Err(e) => self.fail(e),
        }
    }
}

impl<B: Bot> MjaiPlayer<B> {
    fn shadow_act(&mut self, view: &PlayerView, legal: &[Action]) -> Action {
        self.add_robbed_kan(view, legal);
        self.last_peek = None;
        let mut choice = match self.ask() {
            Err(e) => ShadowChoice {
                action: None,
                reply: Value::Null,
                error: Some(e),
                peek: None,
            },
            Ok(r) => match self.reply_to_action(view, legal, &r) {
                Ok(a) => ShadowChoice {
                    action: Some(a),
                    reply: r,
                    error: None,
                    peek: None,
                },
                Err(e) => ShadowChoice {
                    action: None,
                    reply: r,
                    error: Some(e),
                    peek: None,
                },
            },
        };
        choice.peek = self.last_peek.take();
        let action = choice.action.clone();
        self.choices.push(choice);
        // 旁觀者的動作不會被採用。
        action.unwrap_or_else(|| legal[0].clone())
    }

    /// 搶槓：引擎在槓成立前就詢問，先補上那個槓的事件。
    fn add_robbed_kan(&mut self, view: &PlayerView, legal: &[Action]) {
        if let (Some((seat, pai)), Some(conv)) = (view.last_discard, self.converter.as_mut())
            && legal.contains(&Action::Ron)
            && conv.last_draw() == Some(seat)
        {
            let kan = conv.robbed_kan(seat, pai);
            self.pending.push(kan);
        }
    }
}

/// 在標準輸入／輸出上執行一個 [`Bot`]（`mj mjai-bot` 使用）：每讀一行事件陣列，回應一行動作。
pub fn serve(bot: &mut dyn Bot, input: impl BufRead, mut output: impl Write) -> Result<(), String> {
    for line in input.lines() {
        let line = line.map_err(|e| e.to_string())?;
        if line.trim().is_empty() {
            continue;
        }
        let v: Value = serde_json::from_str(&line).map_err(|e| format!("輸入不是 JSON：{e}"))?;
        let events: Vec<Value> = match v {
            Value::Array(a) => a,
            one => vec![one],
        };
        let reply = bot.react(&events)?;
        writeln!(output, "{reply}")
            .and_then(|_| output.flush())
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mj_core::TileCounts;

    /// 依序回應預先排好的動作，並記下收到的事件。
    struct Queue(Vec<Value>, Vec<Vec<Value>>);

    impl Bot for Queue {
        fn react(&mut self, events: &[Value]) -> Result<Value, String> {
            self.1.push(events.to_vec());
            Ok(self.0.remove(0))
        }
    }

    fn p(s: &str) -> Pai {
        s.parse().unwrap()
    }

    fn view() -> PlayerView {
        PlayerView {
            me: 1,
            round_wind: 0,
            kyoku: 0,
            honba: 0,
            riichi_sticks: 0,
            scores: [25_000; 4],
            dora_indicators: vec![],
            hand: TileCounts::default(),
            hand_pais: vec![],
            drawn: None,
            melds: Default::default(),
            rivers: Default::default(),
            riichi: [false; 4],
            tiles_left_in_wall: 60,
            total_kyoku: 8,
            last_discard: None,
        }
    }

    fn player(replies: Vec<Value>) -> MjaiPlayer<Queue> {
        MjaiPlayer::new(Queue(replies, Vec::new()), Default::default())
    }

    /// 各種回應對應到合法動作（吃碰不計兩張牌的順序；赤五要相符）。
    #[test]
    fn replies_map_to_legal_actions() {
        let legal = vec![
            Action::Pass,
            Action::Chi {
                consumed: [p("3m"), p("0m")],
            },
            Action::Pon {
                consumed: [p("4m"), p("4m")],
            },
            Action::Ron,
        ];
        let cases = [
            (json!({"type": "none"}), Action::Pass),
            (
                json!({"type": "chi", "consumed": ["5mr", "3m"]}),
                legal[1].clone(),
            ),
            (
                json!({"type": "pon", "consumed": ["4m", "4m"]}),
                legal[2].clone(),
            ),
            (json!({"type": "hora"}), Action::Ron),
        ];
        for (reply, want) in cases {
            let mut pl = player(vec![reply.clone()]);
            assert_eq!(pl.act(&view(), &legal), want, "{reply}");
            assert!(pl.take_error().is_none());
        }
        // 赤五不符：吃的是普通 5m，不合法。
        let mut pl = player(vec![json!({"type": "chi", "consumed": ["3m", "5m"]})]);
        pl.act(&view(), &legal);
        assert!(pl.take_error().is_some());
    }

    /// 旁觀模式：記錄選擇而不停止對局；立直用 PEEK 詢問要打的牌，不送出真的 reach 事件；不合法的回應只記錄。
    #[test]
    fn shadow_records_choices_and_peeks_riichi() {
        let legal = vec![
            Action::Discard {
                pai: p("9p"),
                tsumogiri: true,
            },
            Action::Riichi {
                pai: p("9p"),
                tsumogiri: true,
            },
        ];
        let mut pl = player(vec![
            json!({"type": "reach", "actor": 1}),
            json!({"type": "dahai", "actor": 1, "pai": "9p", "tsumogiri": true}),
            json!({"type": "dahai", "actor": 1, "pai": "1z", "tsumogiri": false}),
        ])
        .shadow();
        pl.act(&view(), &legal);
        assert_eq!(
            pl.bot.1[1],
            vec![json!({"type": PEEK, "events": [{"type": "reach", "actor": 1}]})]
        );
        assert!(pl.pending.is_empty() && !pl.reach_sent);
        // 不合法的回應：記錄原因，對局不停止。
        pl.act(&view(), &legal);
        assert!(pl.take_error().is_none());
        let c = pl.take_choices();
        assert_eq!(c.len(), 2);
        assert_eq!(c[0].action, Some(legal[1].clone()));
        assert_eq!(c[1].action, None);
        assert!(c[1].error.is_some());
    }

    /// 立直是兩段式：回應 reach → 收到自己的 reach 事件 → 回應打牌；之後引擎的立直宣言牌不再重複送 reach。
    #[test]
    fn riichi_is_two_step() {
        let legal = vec![
            Action::Discard {
                pai: p("9p"),
                tsumogiri: true,
            },
            Action::Riichi {
                pai: p("9p"),
                tsumogiri: true,
            },
            Action::Ankan { tile: p("1z").tile },
        ];
        let mut pl = player(vec![
            json!({"type": "reach", "actor": 1}),
            json!({"type": "dahai", "actor": 1, "pai": "9p", "tsumogiri": true}),
            json!({"type": "none"}),
        ]);
        assert_eq!(pl.act(&view(), &legal), legal[1]);
        let sent = &pl.bot.1;
        assert_eq!(sent[1], vec![json!({"type": "reach", "actor": 1})]);
        pl.on_event(
            1,
            &Event::Discard {
                seat: 1,
                pai: p("9p"),
                tsumogiri: true,
                riichi: true,
            },
        );
        assert_eq!(pl.pending.len(), 1);
        assert_eq!(pl.pending[0]["type"], "dahai");
        // 暗槓：consumed 的第一張決定牌種。
        let mut pl = player(vec![
            json!({"type": "ankan", "consumed": ["E", "E", "E", "E"]}),
        ]);
        assert_eq!(pl.act(&view(), &legal), legal[2]);
    }
}
