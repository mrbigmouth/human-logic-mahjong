//! 動作後的手牌：做了某個動作之後，手牌的向聽數與進張。
//! 用來比較「打這張 vs 打那張」「鳴 vs 不鳴」各自會讓手牌變成什麼樣子。

use mj_calc::{RemainingEstimate, discard_options, shanten, ukeire};
use mj_core::{Tile, TileCounts};
use mj_engine::{Action, PlayerView};

/// 一個動作執行後的手牌評估。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AfterEval {
    /// 這個動作就是和牌（自摸、榮和）
    pub win: bool,
    /// 動作後（等待摸牌時）的向聽數；和牌為 −1
    pub shanten: i8,
    /// 動作後的進張數（剩餘張數合計）；聽牌時就是能和的牌還剩幾張
    pub ukeire: f32,
}

impl AfterEval {
    const WIN: AfterEval = AfterEval {
        win: true,
        shanten: -1,
        ukeire: 0.0,
    };

    fn of(hand: &TileCounts, remaining: &RemainingEstimate) -> Self {
        let u = ukeire(hand, remaining);
        AfterEval {
            win: false,
            shanten: u.shanten,
            ukeire: u.total,
        }
    }

    /// 還需要打一張牌的手牌（3k+2 張）：取最好的打法（向聽最低、進張最多）。
    fn best_discard(hand: &TileCounts, remaining: &RemainingEstimate, forbidden: &[Tile]) -> Self {
        discard_options(hand, remaining)
            .into_iter()
            .find(|o| !forbidden.contains(&o.discard))
            .map(|o| AfterEval {
                win: false,
                shanten: o.after.shanten,
                ukeire: o.after.total,
            })
            .unwrap_or(AfterEval {
                win: false,
                shanten: shanten(hand),
                ukeire: 0.0,
            })
    }
}

/// 計算動作後的手牌評估。
///
/// | 動作 | 動作後的手牌 |
/// |---|---|
/// | 打牌、立直 | 手牌 − 打出的牌 |
/// | 吃、碰 | 手牌 − 用掉的兩張，再打出最好的一張（避開被鳴的牌種，近似食替限制） |
/// | 大明槓、暗槓、加槓 | 手牌 − 槓出去的牌（之後會摸嶺上牌） |
/// | 跳過 | 手牌不變 |
/// | 九種九牌 | 不流局時最好的打法（用來比較「流局」與「繼續打」） |
/// | 自摸、榮和 | 和牌 |
pub fn evaluate_after(
    view: &PlayerView,
    remaining: &RemainingEstimate,
    action: &Action,
) -> AfterEval {
    let mut h = view.hand;
    match action {
        Action::Tsumo | Action::Ron => AfterEval::WIN,
        Action::Discard { pai, .. } | Action::Riichi { pai, .. } => {
            h.remove(pai.tile);
            AfterEval::of(&h, remaining)
        }
        Action::Chi { consumed } | Action::Pon { consumed } => {
            for p in consumed {
                h.remove(p.tile);
            }
            let called: Vec<Tile> = view.last_discard.map(|(_, p)| p.tile).into_iter().collect();
            AfterEval::best_discard(&h, remaining, &called)
        }
        Action::Daiminkan => {
            if let Some((_, p)) = view.last_discard {
                (0..3).for_each(|_| {
                    h.remove(p.tile);
                });
            }
            AfterEval::of(&h, remaining)
        }
        Action::Ankan { tile } => {
            (0..4).for_each(|_| {
                h.remove(*tile);
            });
            AfterEval::of(&h, remaining)
        }
        Action::Kakan { pai } => {
            h.remove(pai.tile);
            AfterEval::of(&h, remaining)
        }
        Action::KyuushuKyuuhai => AfterEval::best_discard(&h, remaining, &[]),
        Action::Pass => AfterEval::of(&h, remaining),
    }
}

/// 便利版本：用「看得到的牌」估計剩餘張數，計算 `action` 之後的手牌。
pub fn after(view: &PlayerView, action: &Action) -> AfterEval {
    evaluate_after(view, &crate::visible::remaining(view), action)
}
