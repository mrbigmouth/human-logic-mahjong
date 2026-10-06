use crate::{Consideration, Ctx, Curve, Layer, Modifier, Reason, Weights};
use mj_engine::Action;
use serde::Serialize;

/// A consideration as configured by a profile.
pub(crate) struct Term {
    pub consideration: Box<dyn Consideration>,
    pub weight: f64,
    pub curve: Curve,
}

/// Contribution of one consideration to one action's score.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TermTrace {
    pub id: &'static str,
    pub layer: Layer,
    pub raw: f64,
    pub shaped: f64,
    /// Effective weight = profile weight × layer multiplier.
    pub weight: f64,
    pub contribution: f64,
    pub reason: Reason,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ActionTrace {
    pub action: Action,
    pub score: f64,
    pub terms: Vec<TermTrace>,
}

/// Why the weights look the way they do for this decision.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Trace {
    pub weights: Weights,
    pub modifiers_fired: Vec<(&'static str, Reason)>,
}

/// Result of a decision: every legal action ranked best-first, with full breakdowns.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Decision {
    pub ranked: Vec<ActionTrace>,
    pub trace: Trace,
}

impl Decision {
    pub fn best(&self) -> Option<&Action> {
        self.ranked.first().map(|a| &a.action)
    }

    pub fn find(&self, action: &Action) -> Option<&ActionTrace> {
        self.ranked.iter().find(|a| &a.action == action)
    }
}

/// A runnable strategy, built from a [`crate::Profile`].
pub struct Decider {
    pub name: String,
    pub(crate) terms: Vec<Term>,
    pub(crate) modifiers: Vec<Box<dyn Modifier>>,
}

impl Decider {
    pub fn decide(&self, ctx: &Ctx, legal: &[Action]) -> Decision {
        let mut weights = Weights::default();
        let modifiers_fired = self
            .modifiers
            .iter()
            .filter_map(|m| m.adjust(ctx, &mut weights).map(|r| (m.id(), r)))
            .collect();

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

        // Stable sort keeps the engine's legal-action order on ties (deterministic replays).
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
    use mj_core::Tile;

    struct PreferHonorDiscard;
    impl Consideration for PreferHonorDiscard {
        fn id(&self) -> &'static str {
            "test.prefer_honor"
        }
        fn layer(&self) -> Layer {
            Layer::Efficiency
        }
        fn evaluate(&self, _: &Ctx, action: &Action) -> Eval {
            let honor = matches!(action, Action::Discard { tile, .. } if tile.is_honor());
            Eval {
                value: if honor { 1.0 } else { 0.0 },
                reason: Reason::new("test.honor"),
            }
        }
    }

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
            tile: s.parse::<Tile>().unwrap(),
            tsumogiri: false,
        };
        vec![d("5m"), d("1z"), Action::Pass]
    }

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
