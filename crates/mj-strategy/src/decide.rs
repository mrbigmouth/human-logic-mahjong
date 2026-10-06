//! 決策器（Decider）：對所有合法動作打分、排序，並保留完整的分數拆解（Trace）。

use crate::{Consideration, Ctx, Curve, Layer, Modifier, Reason, Weights};
use mj_engine::Action;
use serde::Serialize;

/// 策略側寫中設定好的一個考量（含權重與響應曲線）。
pub(crate) struct Term {
    pub consideration: Box<dyn Consideration>,
    /// 側寫中設定的基礎權重
    pub weight: f64,
    /// 響應曲線
    pub curve: Curve,
}

/// 一個考量對一個動作分數的貢獻。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TermTrace {
    /// 考量的識別碼
    pub id: &'static str,
    /// 所屬策略層
    pub layer: Layer,
    /// 原始值
    pub raw: f64,
    /// 經響應曲線轉換後的值
    pub shaped: f64,
    /// 實際權重 = 側寫中的基礎權重 × 該層的局勢倍率
    pub weight: f64,
    /// 貢獻分數 = shaped × weight
    pub contribution: f64,
    /// 理由
    pub reason: Reason,
}

/// 一個動作的總分與拆解。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ActionTrace {
    pub action: Action,
    /// 總分（各考量貢獻的總和）
    pub score: f64,
    /// 各考量的貢獻
    pub terms: Vec<TermTrace>,
}

/// 本次決策的權重從何而來。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Trace {
    /// 最終的各層倍率
    pub weights: Weights,
    /// 有生效的局勢修正器與理由
    pub modifiers_fired: Vec<(&'static str, Reason)>,
}

/// 決策結果：所有合法動作由好到壞排序，各附完整的分數拆解。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Decision {
    pub ranked: Vec<ActionTrace>,
    pub trace: Trace,
}

impl Decision {
    /// 最佳動作。
    pub fn best(&self) -> Option<&Action> {
        self.ranked.first().map(|a| &a.action)
    }

    /// 找出某個動作的分數拆解（復盤時用來和最佳動作比較）。
    pub fn find(&self, action: &Action) -> Option<&ActionTrace> {
        self.ranked.iter().find(|a| &a.action == action)
    }
}

/// 可執行的策略，由策略側寫（[`crate::Profile`]）建立。
pub struct Decider {
    /// 側寫名稱
    pub name: String,
    pub(crate) terms: Vec<Term>,
    pub(crate) modifiers: Vec<Box<dyn Modifier>>,
}

impl Decider {
    /// 對合法動作打分並排序。
    pub fn decide(&self, ctx: &Ctx, legal: &[Action]) -> Decision {
        // 1. 局勢修正器依序調整各層倍率，並記錄有生效者。
        let mut weights = Weights::default();
        let modifiers_fired = self
            .modifiers
            .iter()
            .filter_map(|m| m.adjust(ctx, &mut weights).map(|r| (m.id(), r)))
            .collect();

        // 2. 每個動作 × 每個考量：評估 → 曲線轉換 → 乘權重 → 加總。
        let mut ranked: Vec<ActionTrace> = legal
            .iter()
            .map(|action| {
                let terms: Vec<TermTrace> = self
                    .terms
                    .iter()
                    .map(|t| {
                        let c = &t.consideration;
                        let eval = c.evaluate(ctx, action);
                        let shaped = t.curve.apply(eval.value);
                        let weight = t.weight * weights.layer[c.layer().index()];
                        TermTrace {
                            id: c.id(),
                            layer: c.layer(),
                            raw: eval.value,
                            shaped,
                            weight,
                            contribution: shaped * weight,
                            reason: eval.reason,
                        }
                    })
                    .collect();
                let score = terms.iter().map(|t| t.contribution).sum();
                ActionTrace {
                    action: action.clone(),
                    score,
                    terms,
                }
            })
            .collect();

        // 3. 依分數排序。穩定排序讓同分時維持引擎給的順序，重播結果才會一致。
        ranked.sort_by(|a, b| b.score.total_cmp(&a.score));
        Decision {
            ranked,
            trace: Trace {
                weights,
                modifiers_fired,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::situation::tests::view;
    use crate::{Eval, Registry};
    use mj_core::Pai;

    /// 測試用考量：喜歡打字牌（牌效層）。
    struct PreferHonorDiscard;
    impl Consideration for PreferHonorDiscard {
        fn id(&self) -> &'static str {
            "test.prefer_honor"
        }
        fn layer(&self) -> Layer {
            Layer::Efficiency
        }
        fn evaluate(&self, _: &Ctx, action: &Action) -> Eval {
            let honor = matches!(action, Action::Discard { pai, .. } if pai.tile.is_honor());
            Eval {
                value: if honor { 1.0 } else { 0.0 },
                reason: Reason::new("test.honor"),
            }
        }
    }

    /// 測試用考量：喜歡跳過（防守層）。
    struct PreferPass;
    impl Consideration for PreferPass {
        fn id(&self) -> &'static str {
            "test.prefer_pass"
        }
        fn layer(&self) -> Layer {
            Layer::Defense
        }
        fn evaluate(&self, _: &Ctx, action: &Action) -> Eval {
            let v = if *action == Action::Pass { 1.0 } else { 0.0 };
            Eval {
                value: v,
                reason: Reason::new("test.pass"),
            }
        }
    }

    /// 測試用修正器：最終局防守層 ×3。
    struct AllLastDefense;
    impl Modifier for AllLastDefense {
        fn id(&self) -> &'static str {
            "test.all_last_defense"
        }
        fn adjust(&self, ctx: &Ctx, w: &mut Weights) -> Option<Reason> {
            ctx.situation.is_all_last.then(|| {
                w.scale(Layer::Defense, 3.0);
                Reason::new("test.all_last")
            })
        }
    }

    fn registry() -> Registry {
        let mut r = Registry::default();
        r.add_consideration("test.prefer_honor", |_| Ok(Box::new(PreferHonorDiscard)));
        r.add_consideration("test.prefer_pass", |_| Ok(Box::new(PreferPass)));
        r.add_modifier("test.all_last_defense", |_| Ok(Box::new(AllLastDefense)));
        r
    }

    const PROFILE: &str = r#"
        name = "test"
        [[consideration]]
        id = "test.prefer_honor"
        weight = 2.0
        [[consideration]]
        id = "test.prefer_pass"
        weight = 1.0
        [[modifier]]
        id = "test.all_last_defense"
    "#;

    fn legal() -> Vec<Action> {
        let d = |s: &str| Action::Discard {
            pai: s.parse::<Pai>().unwrap(),
            tsumogiri: false,
        };
        vec![d("5m"), d("1z"), Action::Pass]
    }

    /// 東一局：字牌（1z）的分數 2.0 最高，且分數拆解正確。
    #[test]
    fn ranks_with_breakdown() {
        let decider = PROFILE
            .parse::<crate::Profile>()
            .unwrap()
            .build(&registry())
            .unwrap();
        let v = view(0, [25_000; 4], 0, 0);
        let d = decider.decide(&Ctx::new(&v), &legal());
        assert_eq!(d.best(), Some(&legal()[1]));
        assert_eq!(d.ranked[0].score, 2.0);
        assert_eq!(d.ranked[0].terms[0].contribution, 2.0);
        assert!(d.trace.modifiers_fired.is_empty());
    }

    /// 南四局（最終局）：修正器把防守權重 ×3，最佳選擇變成跳過，並記錄在決策紀錄中。
    #[test]
    fn modifier_shifts_choice_and_is_traced() {
        let decider = PROFILE
            .parse::<crate::Profile>()
            .unwrap()
            .build(&registry())
            .unwrap();
        let v = view(0, [25_000; 4], 1, 3);
        let d = decider.decide(&Ctx::new(&v), &legal());
        assert_eq!(d.best(), Some(&Action::Pass));
        assert_eq!(d.trace.modifiers_fired[0].0, "test.all_last_defense");
        assert_eq!(d.find(&Action::Pass).unwrap().terms[1].weight, 3.0);
    }
}
