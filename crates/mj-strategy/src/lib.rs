//! Explainable utility AI.
//!
//! Every legal action is scored by a set of [`Consideration`]s. Each one returns a raw
//! value plus a machine-readable [`Reason`]; the raw value is shaped by a [`Curve`],
//! multiplied by a weight, and summed. Weights change with the game situation through
//! [`Modifier`]s. The full breakdown is kept in a [`Trace`], which is what the review
//! layer diffs to explain *why* one choice beats another.

pub mod curve;
pub mod decide;
pub mod profile;
pub mod situation;

pub use curve::Curve;
pub use decide::{ActionTrace, Decider, Decision, TermTrace, Trace};
pub use profile::{Profile, ProfileError, Registry};
pub use situation::Situation;

use mj_engine::{Action, PlayerView};
use serde::Serialize;

/// The strategy layers, in information-flow order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum Layer {
    /// L1 讀牌: opponents' hands, tenpai probability, tile danger.
    Belief,
    /// L2 牌效: shanten, ukeire, win speed.
    Efficiency,
    /// L3 牌型・打點: yaku routes and hand value.
    Value,
    /// L4 防守・押引: deal-in risk versus reward.
    Defense,
    /// L5 局勢: placement, scores, hands remaining (mostly acts through modifiers).
    Situation,
    /// L6 印象操作: what opponents read from our river (deception / camouflage).
    Image,
}

impl Layer {
    pub const ALL: [Layer; 6] = [
        Layer::Belief,
        Layer::Efficiency,
        Layer::Value,
        Layer::Defense,
        Layer::Situation,
        Layer::Image,
    ];

    pub fn index(self) -> usize {
        self as usize
    }
}

/// A machine-readable explanation. `code` selects a message template (localized by the
/// review layer), `args` fills it, e.g. `("ukeire.drop", [("before", 24.0), ("after", 16.0)])`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Reason {
    pub code: &'static str,
    pub args: Vec<(&'static str, f64)>,
}

impl Reason {
    pub fn new(code: &'static str) -> Self {
        Self {
            code,
            args: Vec::new(),
        }
    }

    pub fn arg(mut self, name: &'static str, value: f64) -> Self {
        self.args.push((name, value));
        self
    }
}

/// Output of one consideration for one action.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Eval {
    pub value: f64,
    pub reason: Reason,
}

/// Opponent model produced by the belief layer once per decision (filled in from M5).
#[derive(Debug, Clone, Default)]
pub struct Belief {}

/// Shared, per-decision inputs for all considerations.
pub struct Ctx<'a> {
    pub view: &'a PlayerView,
    pub situation: Situation,
    pub belief: Belief,
}

impl<'a> Ctx<'a> {
    pub fn new(view: &'a PlayerView) -> Self {
        Self {
            view,
            situation: Situation::from_view(view),
            belief: Belief::default(),
        }
    }
}

/// One line of human reasoning, turned into code. Implement this to add a strategy.
pub trait Consideration: Send + Sync {
    /// Stable id used in profiles and traces, e.g. `"efficiency.ukeire"`.
    fn id(&self) -> &'static str;
    fn layer(&self) -> Layer;
    fn evaluate(&self, ctx: &Ctx, action: &Action) -> Eval;
}

/// Weights in effect for one decision.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Weights {
    /// Multiplier per [`Layer`], indexed by [`Layer::index`].
    pub layer: [f64; 6],
}

impl Default for Weights {
    fn default() -> Self {
        Self { layer: [1.0; 6] }
    }
}

impl Weights {
    pub fn scale(&mut self, layer: Layer, factor: f64) {
        self.layer[layer.index()] *= factor;
    }
}

/// Adjusts weights from the situation, e.g. "all last and behind → value ×1.5".
/// Returns a reason when it fired, so the trace can say why the weights changed.
pub trait Modifier: Send + Sync {
    fn id(&self) -> &'static str;
    fn adjust(&self, ctx: &Ctx, weights: &mut Weights) -> Option<Reason>;
}
