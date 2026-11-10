//! Rule admission: a transducer from a surface word to the affix rules that could take part in an analysis.

mod build;
mod piece;
mod plan;
#[cfg(test)]
mod tests;

use crate::lower::{pattern_slots, PatternLowerScope, Slot};
use crate::replace::{PhonologyLoweringError, SegAlphabet};
use foma::apply::apply_init;
use foma::io::fsm_write_binary;
use foma::options::FomaOptions;
use foma::structures::fsm_sort_arcs;
use foma::types::ApplyHandle;
use pg_grammar::model::{Grammar, MRuleId, Pattern, PatternNode};
use std::collections::HashMap;
use std::sync::Mutex;

pub use plan::plan_admission;

/// Whether a token can match the one-node pattern; anything unclear answers yes.
pub fn class_test(g: &Grammar) -> impl Fn(&PatternNode, char) -> bool + '_ {
    move |node, ch| {
        let Some(table) = g.char_tables.first() else {
            return true;
        };
        let alphabet = SegAlphabet::new(table);
        let pattern = Pattern {
            nodes: vec![node.clone()],
        };
        let scope = PatternLowerScope::RewriteRuleCompile;
        let slots = pattern_slots(g, table, &pattern, &mut 0, scope);
        match slots.as_deref() {
            Some([Slot::Fixed(id)]) => alphabet.token(*id) == ch,
            Some([Slot::Union(members)]) => {
                members.is_empty() || members.iter().any(|id| alphabet.token(*id) == ch)
            }
            _ => true,
        }
    }
}

/// Why no admission net can be built for a grammar; the caller then uses the plain parse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdmissionRefusal {
    CharacterTables(usize),
    /// More than one stratum has phonological rules, or a rule after the phonological stratum edits its input.
    StratumLayout(String),
    /// A compounding rule cuts or deletes stem material.
    CompoundingEditsStem(String),
    Cascade(PhonologyLoweringError),
    Construction(String),
    /// Every rule is admitted for every word, so a net would add nothing; carries the affix rule count.
    NoUsablePiece(usize),
    Containment {
        structure: String,
        size: usize,
    },
}

impl AdmissionRefusal {
    /// A short stable reason for reports.
    pub fn reason(&self) -> String {
        match self {
            Self::CharacterTables(n) => format!("character-tables:{n}"),
            Self::StratumLayout(why) => format!("stratum-layout:{why}"),
            Self::CompoundingEditsStem(rule) => format!("compounding-edits-stem:{rule}"),
            Self::Cascade(error) => format!("cascade:{error:?}"),
            Self::Construction(why) => format!("construction:{why}"),
            Self::NoUsablePiece(n) => format!("no-usable-piece:{n}"),
            Self::Containment { structure, size } => format!("containment:{structure}:{size}"),
        }
    }
}

/// One rendered literal: a regular expression over tokens, owned by one affix rule.
#[derive(Debug, Clone)]
pub struct Piece {
    pub rule: MRuleId,
    pub regex: String,
    pub grammar_item: String,
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct PlanCounts {
    pub rules: usize,
    pub affix_rules: usize,
    pub free: usize,
    pub pieces: usize,
    pub cuts: usize,
    pub zones: usize,
    pub edges: usize,
    pub literals: usize,
    pub unusable_literals: usize,
    pub deletion_left: Option<usize>,
    pub deletion_right: Option<usize>,
    pub root_floor: usize,
    pub outer_rules: usize,
    pub outer_pieces: usize,
}

/// The pure result of grammar analysis: which rules are free and the piece of every other rule.
#[derive(Debug, Clone)]
pub struct AdmissionPlan {
    pub free: Vec<bool>,
    pub pieces: Vec<Piece>,
    /// Pieces of rules in strata after the phonological one; their literals reach the surface unchanged.
    pub outer_pieces: Vec<Piece>,
    pub counts: PlanCounts,
}

/// States and arcs of one foma network.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
pub struct Size {
    pub states: usize,
    pub arcs: usize,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct RulePartSize {
    pub rule: String,
    pub construction: String,
    pub size: Size,
    pub blocks: usize,
    pub block_tuples: usize,
    pub block_widened: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Composition {
    pub structure: String,
    pub before: Size,
    pub after: Size,
    pub ms: u64,
}

/// Sizes observed while building one admission net, gathered where each structure is made.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct AdmissionEvidence {
    pub plan: PlanCounts,
    pub acceptor: Size,
    pub outer_acceptor: Size,
    /// Each phonological rule's own relation, in cascade order.
    pub rule_parts: Vec<RulePartSize>,
    pub compositions: Vec<Composition>,
    pub final_net: Size,
    pub final_bytes: usize,
}

/// The admitted-rule lookup for one grammar.
pub struct RuleAdmission {
    handle: Mutex<Box<ApplyHandle>>,
    tags: HashMap<String, usize>,
    plan: AdmissionPlan,
    free: Vec<bool>,
    evidence: AdmissionEvidence,
    warnings: Vec<String>,
}

/// Builds the lookup, or says why the grammar is outside the model.
pub fn build_rule_admission(
    opts: &FomaOptions,
    g: &Grammar,
) -> Result<RuleAdmission, AdmissionRefusal> {
    check_strata(g)?;
    let plan = plan_admission(g, &class_test(g))?;
    let mut evidence = AdmissionEvidence {
        plan: plan.counts.clone(),
        ..AdmissionEvidence::default()
    };
    let mut meter = build::Meter::default();
    let built = build::build_net(opts, g, &plan, &mut evidence, &mut meter)?;
    let mut net = built.net;
    let mut counter = build::CountingWriter(0);
    fsm_write_binary(&net, &mut counter)
        .map_err(|e| AdmissionRefusal::Construction(e.to_string()))?;
    evidence.final_bytes = counter.0;
    fsm_sort_arcs(&mut net, 2);
    Ok(RuleAdmission {
        handle: Mutex::new(apply_init(&net)),
        tags: built.tags,
        free: built.free,
        plan,
        evidence,
        warnings: meter.warnings,
    })
}

/// Rules after the phonological stratum are covered by the plan when they are intact; more than one phonological stratum is out of the model.
fn check_strata(g: &Grammar) -> Result<(), AdmissionRefusal> {
    if g.char_tables.len() != 1 {
        return Err(AdmissionRefusal::CharacterTables(g.char_tables.len()));
    }
    let phonological: Vec<usize> = (0..g.strata.len())
        .filter(|i| !g.strata[*i].prules.is_empty())
        .collect();
    if phonological.len() > 1 {
        return Err(AdmissionRefusal::StratumLayout(
            "more than one stratum has phonological rules".into(),
        ));
    }
    Ok(())
}

impl RuleAdmission {
    /// Admitted rules for `word`, or `None` when more than `path_allowance` analyses of the net would be needed.
    pub fn admit(&self, word: &str, path_allowance: usize) -> Option<Vec<bool>> {
        self.admit_counted(word, path_allowance).0
    }

    /// `admit` plus the raw number of net paths read, capped one past the allowance.
    pub fn admit_counted(&self, word: &str, path_allowance: usize) -> (Option<Vec<bool>>, usize) {
        let mut admitted = self.free.clone();
        let mut raw = 0;
        let mut handle = self.handle.lock().unwrap_or_else(|e| e.into_inner());
        for tag in handle.up(&pg_grammar::nfd::nfd(word)) {
            raw += 1;
            if raw > path_allowance {
                return (None, raw);
            }
            if let Some(rule) = self.tags.get(&tag) {
                admitted[*rule] = true;
            }
        }
        (Some(admitted), raw)
    }

    pub fn evidence(&self) -> &AdmissionEvidence {
        &self.evidence
    }

    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    pub fn plan(&self) -> &AdmissionPlan {
        &self.plan
    }
}
