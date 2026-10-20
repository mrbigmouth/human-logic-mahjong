//! 顧問（M14.5）：提示與復盤時一起列出選擇的策略或 mjai AI（最多三個）。
//!
//! 顧問只旁觀：收到同樣的事件與詢問，記下每次詢問的回答（[`Answer`]），不影響對局。
//! 策略顧問的回答有決策紀錄（理由）；mjai AI（Mortal）的回答有各動作的機率（[`Policy`]），沒有理由。
//! 回答再用 [`pick`] 對應到某個決策的選項，與主要策略的選項並列比較。

use mj_core::Pai;
use mj_engine::{Action, Event, Player, PlayerView, Seat};
use mj_mjai::mortal::{Move, policy};
use mj_mjai::{MjaiPlayer, PooledBot};
use mj_strategy::framework::explain::action_text;
use mj_strategy::framework::{
    DecisionKind, DecisionRecord, Strategy, StrategyPlayer, option_of_action,
};
use serde::Serialize;
use std::sync::Arc;

/// 顧問的種類。
#[derive(Clone)]
pub enum AdvisorKind {
    Strategy(Arc<dyn Strategy>),
    /// mjai AI 的程式與參數
    Mjai(Vec<String>),
}

/// 一個顧問。
#[derive(Clone)]
pub struct Advisor {
    /// 名稱（策略名稱，或 `mjai:<程式 參數…>`）
    pub name: String,
    pub kind: AdvisorKind,
}

impl std::fmt::Debug for Advisor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Advisor").field("name", &self.name).finish()
    }
}

/// 一個動作的機率（mjai AI 的回答）。
#[derive(Debug, Clone, Serialize)]
pub struct Prob {
    /// 例如「打 5m」「立直」「碰（用 5z5z）」
    pub label: String,
    pub prob: f64,
    /// 對應的合法動作（立直宣言本身沒有對應的動作）
    #[serde(skip)]
    pub action: Option<Action>,
}

/// mjai AI 在一次詢問中各動作的機率（Q 值的 softmax，見 `mj_mjai::mortal`）。
#[derive(Debug, Clone, Default, Serialize)]
pub struct Policy {
    /// 由高到低
    pub probs: Vec<Prob>,
    /// 選了立直時：立直後打哪張的機率
    pub riichi: Vec<Prob>,
}

/// 顧問在一次詢問中的回答。
#[derive(Debug, Clone, Default)]
pub struct Answer {
    /// 選的合法動作（mjai AI 回應不合法時為 `None`）
    pub action: Option<Action>,
    /// 策略顧問：這次詢問的決策紀錄
    pub records: Vec<DecisionRecord>,
    /// mjai AI：各動作的機率（沒有 Q 值時為 `None`）
    pub policy: Option<Policy>,
    pub error: Option<String>,
}

/// 旁觀中的顧問。每次被詢問都在 [`Watcher::answers`] 加一筆（索引 = 第幾次詢問）。
pub struct Watcher {
    inner: Inner,
    pub answers: Vec<Answer>,
}

enum Inner {
    Strategy(Box<dyn StrategyPlayer>),
    Mjai(Box<MjaiPlayer<PooledBot>>),
}

impl Watcher {
    /// `names` = 四家名稱（mjai 的 `start_game` 用）。
    pub fn new(advisor: &Advisor, names: &[String; 4]) -> Result<Watcher, String> {
        let inner = match &advisor.kind {
            AdvisorKind::Strategy(s) => Inner::Strategy(s.player(true, None)),
            AdvisorKind::Mjai(cmd) => Inner::Mjai(Box::new(
                MjaiPlayer::new(PooledBot::checkout(cmd)?, names.clone()).shadow(),
            )),
        };
        Ok(Watcher {
            inner,
            answers: Vec::new(),
        })
    }

    /// 最近一次詢問的回答。
    pub fn last(&self) -> Option<&Answer> {
        self.answers.last()
    }
}

impl Player for Watcher {
    fn on_event(&mut self, me: Seat, event: &Event) {
        match &mut self.inner {
            Inner::Strategy(p) => p.on_event(me, event),
            Inner::Mjai(p) => p.on_event(me, event),
        }
    }

    fn act(&mut self, view: &PlayerView, legal: &[Action]) -> Action {
        let answer = match &mut self.inner {
            Inner::Strategy(p) => {
                let a = p.act(view, legal);
                Answer {
                    action: Some(a),
                    records: p.take_records(),
                    ..Answer::default()
                }
            }
            Inner::Mjai(p) => {
                p.act(view, legal);
                let c = p
                    .take_choices()
                    .pop()
                    .unwrap_or_else(|| mj_mjai::ShadowChoice {
                        action: None,
                        reply: serde_json::Value::Null,
                        error: Some("AI 沒有回答".into()),
                        peek: None,
                    });
                Answer {
                    policy: policy_of(&c, view, legal),
                    action: c.action,
                    records: Vec::new(),
                    error: c.error,
                }
            }
        };
        let a = answer.action.clone().unwrap_or_else(|| legal[0].clone());
        self.answers.push(answer);
        a
    }
}

fn probs(moves: Vec<(Move, f64)>) -> Vec<Prob> {
    moves
        .into_iter()
        .map(|(m, prob)| match m {
            Move::Riichi => Prob {
                label: "立直".into(),
                prob,
                action: None,
            },
            Move::Act(a) => Prob {
                label: action_text(&a),
                prob,
                action: Some(a),
            },
        })
        .collect()
}

fn policy_of(c: &mj_mjai::ShadowChoice, view: &PlayerView, legal: &[Action]) -> Option<Policy> {
    let main = policy(&c.reply, view, legal, false)?;
    let riichi = c
        .peek
        .as_ref()
        .and_then(|p| policy(p, view, legal, true))
        .unwrap_or_default();
    Some(Policy {
        probs: probs(main),
        riichi: probs(riichi),
    })
}

/// 顧問的回答對應到某個決策（種類 `kind`、選項 `options`）的哪個選項。
///
/// 策略顧問有同種類、同選項的決策紀錄時直接用它的選擇；否則把它最後的動作對應到選項
/// （`riichi_pai` = 立直與手切摸切決策時，主要策略要打的牌）。走了另一條路（例如它自摸了）時為 `None`。
pub fn pick(
    kind: DecisionKind,
    options: &[String],
    legal: &[Action],
    answer: &Answer,
    riichi_pai: Option<Pai>,
) -> Option<usize> {
    if let Some(r) = answer
        .records
        .iter()
        .find(|r| r.kind == kind && r.options == options)
    {
        return Some(r.chosen);
    }
    option_of_action(kind, legal, answer.action.as_ref()?, riichi_pai)
}
