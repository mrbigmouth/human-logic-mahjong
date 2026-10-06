//! 簡單的參考機器人：用於引擎的模糊測試，也作為比較強弱的基準。

use mj_calc::{naive_remaining, shanten, ukeire};
use mj_core::TileCounts;
use mj_engine::{Action, Player, PlayerView};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

/// 隨機機器人：從合法動作中均勻隨機挑一個。鳴牌、槓很多，和牌很少，適合測試鳴牌流程。
pub struct RandomBot {
    rng: ChaCha8Rng,
}

impl RandomBot {
    /// 以 seed 建立（結果可重現）。
    pub fn new(seed: u64) -> Self {
        Self {
            rng: ChaCha8Rng::seed_from_u64(seed),
        }
    }
}

impl Player for RandomBot {
    fn act(&mut self, _view: &PlayerView, legal: &[Action]) -> Action {
        legal[self.rng.random_range(0..legal.len())].clone()
    }
}

/// 貪婪機器人：能和就和、聽牌就立直，否則打出讓向聽數最低、進張最多的牌（純牌效）。
/// 吃碰槓以機率 `call_rate` 隨機進行。和牌、鳴牌、槓、立直都很多，適合拿來對引擎做模糊測試。
pub struct GreedyBot {
    rng: ChaCha8Rng,
    /// 有鳴牌選項時實際鳴牌的機率
    pub call_rate: f64,
}

impl GreedyBot {
    /// 以 seed 與鳴牌機率建立。
    pub fn new(seed: u64, call_rate: f64) -> Self {
        Self {
            rng: ChaCha8Rng::seed_from_u64(seed),
            call_rate,
        }
    }

    /// 看得到的牌：自己的手牌、各家牌河（被鳴走的不重複算）、副露、寶牌指示牌。
    /// 用來估算每種牌還剩幾張。
    fn visible(view: &PlayerView) -> TileCounts {
        let mut v = view.hand;
        for river in &view.rivers {
            for d in river.iter().filter(|d| !d.called) {
                v.add(d.pai.tile);
            }
        }
        for melds in &view.melds {
            for m in melds {
                m.tiles.iter().for_each(|p| v.add(p.tile));
            }
        }
        for &t in &view.dora_indicators {
            v.add(t);
        }
        v
    }

    /// 打出這個動作的牌之後的 (向聽數, −進張數)；越小越好。不是打牌動作時回傳 `None`。
    fn discard_key(view: &PlayerView, action: &Action) -> Option<(i8, i32)> {
        let pai = match action {
            Action::Discard { pai, .. } | Action::Riichi { pai, .. } => *pai,
            _ => return None,
        };
        let mut h = view.hand;
        h.remove(pai.tile);
        let remaining = naive_remaining(&Self::visible(view));
        let u = ukeire(&h, &remaining);
        Some((shanten(&h), -(u.total as i32)))
    }
}

impl Player for GreedyBot {
    fn act(&mut self, view: &PlayerView, legal: &[Action]) -> Action {
        // 1. 能自摸或榮和就和。
        for win in [Action::Tsumo, Action::Ron] {
            if legal.contains(&win) {
                return win;
            }
        }
        // 2. 有鳴牌選項時，以 call_rate 的機率隨機選一個。
        let calls: Vec<&Action> = legal
            .iter()
            .filter(|a| {
                matches!(
                    a,
                    Action::Chi { .. }
                        | Action::Pon { .. }
                        | Action::Daiminkan
                        | Action::Ankan { .. }
                        | Action::Kakan { .. }
                )
            })
            .collect();
        if !calls.is_empty() && self.rng.random_bool(self.call_rate) {
            return calls[self.rng.random_range(0..calls.len())].clone();
        }
        // 3. 回應他家打牌時，不鳴牌就跳過。
        if legal.contains(&Action::Pass) {
            return Action::Pass;
        }
        // 4. 自己的回合：能立直就選最好的立直打法，否則選牌效最好的打牌。
        let best = |want_riichi: bool| {
            legal
                .iter()
                .filter(|a| matches!(a, Action::Riichi { .. }) == want_riichi)
                .filter_map(|a| Self::discard_key(view, a).map(|k| (k, a)))
                .min_by_key(|(k, _)| *k)
                .map(|(_, a)| a.clone())
        };
        best(true)
            .or_else(|| best(false))
            .unwrap_or_else(|| legal[0].clone())
    }
}
