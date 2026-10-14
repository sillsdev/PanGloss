//! Ordered phonology measured for one explicit underlying shape under a lexical entry's real context.
//!
//! The named entry supplies only grammatical context (syntactic features, rule-feature set, stratum).
//! The shape to run comes from the request, so no root is invented and no part of speech is guessed.

use std::time::Duration;

use pg_grammar_model::model::{Grammar, LexEntryDef, LexEntryId, PRuleId, PhonRuleDef, StratumId};
use pg_rules::shape_feat::segment_with_features;
use pg_rules::stratum::{synthesize_phonological_stratum_traced, StepBudget};
use pg_rules::trace::{NoopSink, TraceHandle};
use pg_snapshot::FwClass;

use crate::morpher::Morpher;

/// Stable identity of the lexical entry whose context the shape runs under.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PhonologyContext {
    /// An HC XML entry, identified by `LexicalEntry@id`.
    HcXmlEntry { entry_id: String },
    /// A FieldWorks entry and the exact MSA that belongs to it.
    FwdataEntry {
        entry_guid: String,
        msa_guid: String,
        msa_class: FwClass,
    },
}

/// One measurement request: the context, the underlying text, and finite limits.
#[derive(Clone, Debug)]
pub struct PhonologyRequest {
    pub context: PhonologyContext,
    pub underlying_shape: String,
    /// Inner search work allowed across every stratum the fold visits.
    pub work_cap: usize,
    /// Wall-clock allowance for the whole measurement.
    pub timeout: Duration,
    /// Fold attempts allowed; `usize::MAX` leaves only the work cap and timeout.
    pub attempt_cap: usize,
}

/// Why no measurement was started.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PhonologyUnavailableReason {
    EmptyLexicon,
    UnknownEntry,
    AmbiguousContext,
    MsaNotOwnedByEntry,
    InvalidShape,
    InvalidStratum,
}

/// Which finite limit stopped a started measurement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PhonologyStopReason {
    WorkCap,
    Timeout,
    AttemptCap,
}

/// The typed result; only `Complete` carries a surface.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PhonologyResult {
    Unavailable { reason: PhonologyUnavailableReason },
    Complete { surface: String },
    Incomplete { reason: PhonologyStopReason },
}

/// Completion state a consumer records next to its own disposition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PhonologyCompletion {
    Complete,
    Incomplete,
    NotStarted,
}

/// A measurement with the owner budget facts that bound it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PhonologyMeasurement {
    pub result: PhonologyResult,
    /// Authored ids of the ordered phonological rules the fold ran, stratum by stratum; empty when not started.
    pub ordered_rule_ids: Vec<String>,
    pub work_steps: usize,
    pub capped: bool,
    pub timed_out: bool,
}

impl PhonologyMeasurement {
    pub fn completion(&self) -> PhonologyCompletion {
        match self.result {
            PhonologyResult::Unavailable { .. } => PhonologyCompletion::NotStarted,
            PhonologyResult::Complete { .. } => PhonologyCompletion::Complete,
            PhonologyResult::Incomplete { .. } => PhonologyCompletion::Incomplete,
        }
    }

    fn unavailable(reason: PhonologyUnavailableReason) -> Self {
        Self {
            result: PhonologyResult::Unavailable { reason },
            ordered_rule_ids: Vec::new(),
            work_steps: 0,
            capped: false,
            timed_out: false,
        }
    }
}

/// Resolve the context, segment the shape, and run only the ordered phonology under the limits.
pub fn measure_phonology(
    morpher: &Morpher<'_>,
    request: &PhonologyRequest,
) -> PhonologyMeasurement {
    let g = morpher.grammar();
    let entry = match resolve_entry(g, &request.context) {
        Ok(entry) => entry,
        Err(reason) => return PhonologyMeasurement::unavailable(reason),
    };
    let def = &g.entries[entry.0 as usize];
    let root = g.morphemes[def.morpheme.0 as usize].stratum;
    let Some(root_table) = g
        .strata
        .get(root.0 as usize)
        .and_then(|sd| g.char_tables.get(sd.table.0 as usize))
    else {
        return PhonologyMeasurement::unavailable(PhonologyUnavailableReason::InvalidStratum);
    };
    if request.underlying_shape.is_empty() {
        return PhonologyMeasurement::unavailable(PhonologyUnavailableReason::InvalidShape);
    }
    let Ok(shape) = segment_with_features(g, root_table, &request.underlying_shape) else {
        return PhonologyMeasurement::unavailable(PhonologyUnavailableReason::InvalidShape);
    };

    let mut word = morpher.build_root_seed(entry, 0, pg_featstruct::FeatureStruct::EMPTY);
    word.shape = shape;
    let budget = StepBudget::with_limits(request.attempt_cap, request.work_cap)
        .with_timeout(Some(request.timeout))
        .with_synthesis_counting();
    let mut ordered_rule_ids = Vec::new();
    let mut stop = None;
    for s in root.0 as usize..g.strata.len() {
        let stratum = StratumId(s as u8);
        ordered_rule_ids.extend(g.strata[s].prules.iter().map(|&pid| rule_xml_id(g, pid)));
        let outcome = synthesize_phonological_stratum_traced(
            g,
            stratum,
            word.clone(),
            morpher.rule_cache(),
            &budget,
            None,
            &NoopSink,
            TraceHandle::DUMMY,
        );
        // A timeout outranks a cap; an attempt cap alone leaves a partial output, a work stop leaves none.
        let reason = if outcome.timed_out {
            Some(PhonologyStopReason::Timeout)
        } else if outcome.capped {
            Some(if outcome.output.is_some() {
                PhonologyStopReason::AttemptCap
            } else {
                PhonologyStopReason::WorkCap
            })
        } else {
            None
        };
        match (reason, outcome.output) {
            (None, Some(next)) => word = next,
            (reason, _) => {
                stop = Some(reason.unwrap_or(PhonologyStopReason::WorkCap));
                break;
            }
        }
    }
    let result = match stop {
        Some(reason) => PhonologyResult::Incomplete { reason },
        None => PhonologyResult::Complete {
            surface: morpher.generated_surface_of(&word),
        },
    };
    PhonologyMeasurement {
        result,
        ordered_rule_ids,
        work_steps: budget.work_steps(),
        capped: budget.capped(),
        timed_out: budget.timed_out(),
    }
}

fn rule_xml_id(g: &Grammar, pid: PRuleId) -> String {
    match &g.prules[pid.0 as usize] {
        PhonRuleDef::Rewrite(r) => r.xml_id.clone(),
        PhonRuleDef::Metathesis(r) => r.xml_id.clone(),
    }
}

/// Exactly one compiled entry for the context, with the MSA verified against that entry's morpheme.
fn resolve_entry(
    g: &Grammar,
    context: &PhonologyContext,
) -> Result<LexEntryId, PhonologyUnavailableReason> {
    use PhonologyUnavailableReason as Why;
    if g.entries.is_empty() {
        return Err(Why::EmptyLexicon);
    }
    let with_id = |keep: &dyn Fn(&LexEntryDef) -> bool| -> Vec<LexEntryId> {
        g.entries
            .iter()
            .enumerate()
            .filter(|(_, e)| keep(e))
            .map(|(i, _)| LexEntryId(i as u32))
            .collect()
    };
    let exactly_one = |found: Vec<LexEntryId>, none: Why| match found.as_slice() {
        [] => Err(none),
        [one] => Ok(*one),
        _ => Err(Why::AmbiguousContext),
    };
    match context {
        PhonologyContext::HcXmlEntry { entry_id } => {
            exactly_one(with_id(&|e| e.authored_id == *entry_id), Why::UnknownEntry)
        }
        PhonologyContext::FwdataEntry {
            entry_guid,
            msa_guid,
            msa_class,
        } => {
            let named = with_id(&|e| e.source_guid.as_deref() == Some(entry_guid.as_str()));
            if named.is_empty() {
                return Err(Why::UnknownEntry);
            }
            let owned: Vec<LexEntryId> = named
                .into_iter()
                .filter(|le| {
                    let info = &g.morphemes[g.entries[le.0 as usize].morpheme.0 as usize];
                    info.source_msa_guid.as_deref() == Some(msa_guid.as_str())
                        && info.source_msa_class == Some(*msa_class)
                        && info
                            .owner
                            .as_ref()
                            .is_none_or(|owner| owner.entry_guid == *entry_guid)
                })
                .collect();
            exactly_one(owned, Why::MsaNotOwnedByEntry)
        }
    }
}
