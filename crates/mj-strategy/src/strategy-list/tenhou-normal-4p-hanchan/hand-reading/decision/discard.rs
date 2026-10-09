//! 何切：打哪一張。
//!
//! 思考順序：
//! 1. 沒有人對我有威脅 → 照牌效：向聽數最低 → 進張最多 → 保留寶牌 → 先打字牌、老頭牌。
//! 2. 有威脅（有人立直，或沒立直的人看起來已聽牌）→ 押引：每張牌算
//!
//!    「這張的價值」＝ 打了之後我和牌的機率 × 我的手牌價值 − 放銃機率 × 對手的打點
//!
//!    選價值最高的一張。這一條同時包含三種打法：
//!    - 對攻（押）：我的手夠快夠大，危險牌也打；
//!    - 回し打ち：保留和牌的可能，但改打比較安全的牌；
//!    - 棄和（降）：我的和牌希望不大，打最安全的牌。

use super::super::memory::Memory;
use super::super::utils::predict::win;
use super::super::utils::{defense, efficiency, value};
use crate::prelude::*;

/// 我和牌時，除了手牌的點數，還能阻止每一家威脅和牌（他的和牌本來可能自摸、讓我也付點數），
/// 每家威脅算這麼多點的額外好處。
pub const DENY_VALUE: Param = Param::new(
    "discard.deny_value",
    3000.0,
    0.0,
    8000.0,
    "和牌時阻止每家威脅和牌的價值（點）",
);
/// 供託（場上的立直棒）每根 1000 點，和牌者拿走。
pub const STICK_VALUE: f64 = 1000.0;
/// 押引時「和牌機率 × 價值」的權重。`predict::win` 以牌譜校準成準確的機率後，
/// 公式本身的偏差（例如沒有計入棄和後仍可能流局聽牌、名次的影響）要在決策這一側修正：
/// 這個權重用 duplicate 對戰調整，預測的參數則用牌譜的對數損失調整，兩者分開。
pub const WIN_WEIGHT: Param = Param::new(
    "discard.win_weight",
    1.0,
    0.2,
    2.0,
    "押引時和牌期望值的權重",
);

/// 回傳要打的牌（必須是 `ctx.discard_candidates()` 之一）。
pub fn decide(ctx: &Ctx, mem: &mut Memory) -> Pai {
    let candidates = ctx.discard_candidates();
    let ranked = efficiency::rank_by_efficiency(ctx, candidates);
    let (best, best_eval) = ranked[0];

    let threats = defense::threats(ctx, mem);
    if !threats.is_empty() {
        return push_or_fold(ctx, mem, &ranked, &threats);
    }

    if best_eval.shanten == 0 {
        ctx.note(format!("打 {best} 聽牌，聽牌張剩 {} 張", best_eval.ukeire));
    } else {
        ctx.note(format!(
            "打 {best}：{} 向聽、進張 {} 張（牌效最好）",
            best_eval.shanten, best_eval.ukeire
        ));
    }
    if let Some((second, e)) = ranked.get(1) {
        if e.shanten == best_eval.shanten && e.ukeire == best_eval.ukeire {
            let why = if efficiency::dora_of(ctx, *second) > efficiency::dora_of(ctx, best) {
                format!("保留寶牌 {second}")
            } else {
                format!("{second} 牌效相同，先打較難用的 {best}")
            };
            ctx.note(why);
        }
    }
    best
}

/// 一張候選牌的押引評估。
struct Option_ {
    pai: Pai,
    eval: AfterEval,
    /// 打了之後我和牌的機率
    p_win: f64,
    /// 我和牌的價值（手牌點數＋供託＋阻止威脅和牌，點）
    value: f64,
    /// 放銃機率
    deal_in: f64,
    /// 預期放銃損失（點）
    loss: f64,
    /// 和牌期望值的權重（[`WIN_WEIGHT`]）
    weight: f64,
}

impl Option_ {
    /// 這張牌的價值（點）。
    fn score(&self) -> f64 {
        self.weight * self.p_win * self.value - self.loss
    }
}

/// 有威脅時的押引判斷。`ranked` 依牌效排序（同分時取牌效較好的）。
fn push_or_fold(
    ctx: &Ctx,
    mem: &Memory,
    ranked: &[(Pai, AfterEval)],
    threats: &[defense::Threat],
) -> Pai {
    let who: Vec<String> = threats
        .iter()
        .map(|t| {
            let how = if ctx.view.riichi[usize::from(t.seat)] {
                "立直".to_string()
            } else if tools::river::open_melds(ctx.view, t.seat) > 0 {
                format!("副露、聽牌約 {:.0}%", t.p_tenpai * 100.0)
            } else {
                format!("門前、聽牌約 {:.0}%", t.p_tenpai * 100.0)
            };
            format!("座位 {}（{how}，打點約 {:.0}）", t.seat, t.value)
        })
        .collect();
    ctx.note(format!("有威脅：{}", who.join("、")));
    // 和牌的額外好處：拿走供託、阻止威脅和牌。
    // 威脅的份量：每家以他已聽牌的機率計（立直者算 1 家，聽牌機率 60% 的副露者算 0.6 家）。
    let threat_weight: f64 = threats.iter().map(|t| t.p_tenpai).sum();
    // 押引用的和牌機率模型（參數與預測用的分開，見 `utils/predict/win.rs`）。
    let push = win::Model::push(ctx);
    let bonus =
        STICK_VALUE * f64::from(ctx.view.riichi_sticks) + ctx.param(&DENY_VALUE) * threat_weight;

    let options: Vec<Option_> = ranked
        .iter()
        .map(|&(pai, eval)| {
            let mut hand = ctx.hand();
            hand.remove(pai.tile);
            let waits = if eval.shanten == 0 {
                f64::from(eval.ukeire)
            } else {
                push.assumed_waits
            };
            let p_win = win::estimate(
                ctx,
                &push,
                eval.shanten,
                f64::from(eval.ukeire),
                waits,
                threat_weight,
            );
            let points = value::my_value(ctx, &hand, eval.shanten, pai);
            // 和不了的手（例如副露沒有役）沒有額外好處。
            let value = if points > 0.0 { points + bonus } else { 0.0 };
            let (deal_in, loss) = defense::deal_in(ctx, mem, threats, pai);
            Option_ {
                pai,
                eval,
                p_win,
                value,
                deal_in,
                loss,
                weight: ctx.param(&WIN_WEIGHT),
            }
        })
        .collect();

    // 價值最高的一張；同分時保留牌效較好的（ranked 的順序）。
    let chosen = options
        .iter()
        .enumerate()
        .max_by(|(i, a), (j, b)| a.score().total_cmp(&b.score()).then(j.cmp(i)))
        .map(|(_, o)| o)
        .expect("at least one candidate");
    let efficient = &options[0];
    let safest = options
        .iter()
        .min_by(|a, b| a.deal_in.total_cmp(&b.deal_in))
        .expect("at least one candidate");

    let describe = |o: &Option_| {
        format!(
            "{}：{} 向聽、和牌約 {:.0}% × {:.0} 點，放銃約 {:.1}%（損失 {:.0}）→ {:+.0}",
            o.pai,
            o.eval.shanten,
            o.p_win * 100.0,
            o.value,
            o.deal_in * 100.0,
            o.loss,
            o.score()
        )
    };
    // 和牌希望幾乎沒有時，不論向聽數是否維持都是棄和。
    let kind = if chosen.pai == efficient.pai {
        "對攻（照牌效打）"
    } else if chosen.p_win < 0.02 {
        "棄和（打安全的牌）"
    } else if chosen.eval.shanten == efficient.eval.shanten {
        "回し打ち（維持向聽，改打較安全的牌）"
    } else if chosen.pai == safest.pai {
        "棄和（打安全的牌）"
    } else {
        "退一步（向聽倒退，換取安全）"
    };
    ctx.note(format!("{kind}：{}", describe(chosen)));
    if chosen.pai != efficient.pai {
        ctx.note(format!("牌效最好的 {}", describe(efficient)));
    }
    if chosen.pai != safest.pai && safest.pai != efficient.pai {
        ctx.note(format!("最安全的 {}", describe(safest)));
    }
    chosen.pai
}
