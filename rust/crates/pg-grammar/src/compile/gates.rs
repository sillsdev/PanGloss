//! What the compiler did with each authored morphological gate of an allomorph.

use std::collections::btree_map::Entry;
use std::collections::BTreeMap;

/// An authored gate on an allomorph, as `pg-facts` publishes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AllomorphGateKind {
    InflectionClass,
    RequiredFeatures,
    StemName,
}

impl AllomorphGateKind {
    /// The stored `allomorph_gate.gate_kind` value.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InflectionClass => "inflection_class",
            Self::RequiredFeatures => "required_features",
            Self::StemName => "stem_name",
        }
    }
}

/// The compiler's effect on one gate value. Declared weakest first, so `Ord` picks the strongest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AllomorphGateEffect {
    Ignored,
    Unresolved,
    Applied,
}

impl AllomorphGateEffect {
    /// The stored `allomorph_gate.parser_effect` value.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ignored => "ignored",
            Self::Unresolved => "unresolved",
            Self::Applied => "applied",
        }
    }
}

/// One gate value of one allomorph and the effect the compiler gave it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AllomorphGateOutcome {
    pub allomorph_guid: String,
    pub gate_kind: AllomorphGateKind,
    pub ordinal: u32,
    /// The inflection class or stem name the gate names; `None` for required features.
    pub target_guid: Option<String>,
    pub effect: AllomorphGateEffect,
    pub reason_code: Option<&'static str>,
}

/// Collects outcomes across every rule reading an allomorph; the strongest effect wins, ties keep the first.
#[derive(Debug, Default)]
pub(crate) struct GateOutcomes {
    by_gate: BTreeMap<(String, AllomorphGateKind, u32), AllomorphGateOutcome>,
}

impl GateOutcomes {
    pub(crate) fn record(&mut self, outcome: AllomorphGateOutcome) {
        let key = (
            outcome.allomorph_guid.clone(),
            outcome.gate_kind,
            outcome.ordinal,
        );
        match self.by_gate.entry(key) {
            Entry::Vacant(slot) => {
                slot.insert(outcome);
            }
            Entry::Occupied(mut slot) => {
                if outcome.effect > slot.get().effect {
                    slot.insert(outcome);
                }
            }
        }
    }

    pub(crate) fn into_outcomes(self) -> Vec<AllomorphGateOutcome> {
        self.by_gate.into_values().collect()
    }
}
