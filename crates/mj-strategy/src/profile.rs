//! Profiles: which considerations and modifiers a strategy uses, with weights,
//! response curves and parameters, stored as TOML so they can be versioned and tuned.
//!
//! ```toml
//! name = "speed"
//! [[consideration]]
//! id = "efficiency.ukeire"
//! weight = 1.0
//! curve = [[0, 0], [40, 1]]
//! params = { lookahead = 2 }
//! [[modifier]]
//! id = "situation.all_last_behind"
//! params = { value_scale = 1.5 }
//! ```

use crate::decide::{Decider, Term};
use crate::{Consideration, Curve, Modifier};
use serde::Deserialize;
use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;

pub type Params = toml::Table;

#[derive(Debug, Clone, Deserialize)]
pub struct Profile {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default, rename = "consideration")]
    pub considerations: Vec<TermSpec>,
    #[serde(default, rename = "modifier")]
    pub modifiers: Vec<ModifierSpec>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TermSpec {
    pub id: String,
    #[serde(default = "one")]
    pub weight: f64,
    #[serde(default)]
    pub curve: Curve,
    #[serde(default)]
    pub params: Params,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModifierSpec {
    pub id: String,
    #[serde(default)]
    pub params: Params,
}

fn one() -> f64 {
    1.0
}

#[derive(Debug, Clone, PartialEq)]
pub enum ProfileError {
    Parse(String),
    UnknownConsideration(String),
    UnknownModifier(String),
    UnsortedCurve(String),
    BadParams { id: String, message: String },
}

impl fmt::Display for ProfileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(m) => write!(f, "profile parse error: {m}"),
            Self::UnknownConsideration(id) => write!(f, "unknown consideration: {id}"),
            Self::UnknownModifier(id) => write!(f, "unknown modifier: {id}"),
            Self::UnsortedCurve(id) => write!(f, "curve points of {id} must be sorted by x"),
            Self::BadParams { id, message } => write!(f, "bad params for {id}: {message}"),
        }
    }
}

impl std::error::Error for ProfileError {}

impl FromStr for Profile {
    type Err = ProfileError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        toml::from_str(s).map_err(|e| ProfileError::Parse(e.to_string()))
    }
}

/// Factory results carry a message; the registry attaches the id.
pub type FactoryResult<T> = Result<Box<T>, String>;
type ConsiderationFactory = fn(&Params) -> FactoryResult<dyn Consideration>;
type ModifierFactory = fn(&Params) -> FactoryResult<dyn Modifier>;

/// Maps profile ids to constructors. Built-in modules register here; so do user modules.
#[derive(Default)]
pub struct Registry {
    considerations: HashMap<&'static str, ConsiderationFactory>,
    modifiers: HashMap<&'static str, ModifierFactory>,
}

impl Registry {
    pub fn add_consideration(&mut self, id: &'static str, f: ConsiderationFactory) {
        self.considerations.insert(id, f);
    }

    pub fn add_modifier(&mut self, id: &'static str, f: ModifierFactory) {
        self.modifiers.insert(id, f);
    }
}

impl Profile {
    pub fn build(&self, registry: &Registry) -> Result<Decider, ProfileError> {
        let terms = self
            .considerations
            .iter()
            .map(|spec| {
                let factory = registry
                    .considerations
                    .get(spec.id.as_str())
                    .ok_or_else(|| ProfileError::UnknownConsideration(spec.id.clone()))?;
                if !spec.curve.is_sorted() {
                    return Err(ProfileError::UnsortedCurve(spec.id.clone()));
                }
                Ok(Term {
                    consideration: factory(&spec.params).map_err(|m| bad_params(&spec.id, m))?,
                    weight: spec.weight,
                    curve: spec.curve.clone(),
                })
            })
            .collect::<Result<_, _>>()?;
        let modifiers = self
            .modifiers
            .iter()
            .map(|spec| {
                let factory = registry
                    .modifiers
                    .get(spec.id.as_str())
                    .ok_or_else(|| ProfileError::UnknownModifier(spec.id.clone()))?;
                factory(&spec.params).map_err(|m| bad_params(&spec.id, m))
            })
            .collect::<Result<_, _>>()?;
        Ok(Decider {
            name: self.name.clone(),
            terms,
            modifiers,
        })
    }
}

fn bad_params(id: &str, message: String) -> ProfileError {
    ProfileError::BadParams {
        id: id.to_string(),
        message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_example_profile() {
        let src = include_str!("../../../profiles/example.toml");
        let p: Profile = src.parse().unwrap();
        assert_eq!(p.name, "example");
        assert!(!p.considerations.is_empty());
        assert!(p.considerations.iter().all(|c| c.curve.is_sorted()));
    }

    #[test]
    fn unknown_id_is_an_error() {
        let p: Profile = "name = \"x\"\n[[consideration]]\nid = \"nope\""
            .parse()
            .unwrap();
        assert_eq!(
            p.build(&Registry::default()).err(),
            Some(ProfileError::UnknownConsideration("nope".into()))
        );
    }
}
