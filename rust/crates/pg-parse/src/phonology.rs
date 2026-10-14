//! Ordered phonology measured for one explicit underlying shape under a lexical entry's real context.
//!
//! The named entry supplies only grammatical context (syntactic features, rule-feature set, stratum).
//! The shape to run comes from the request, so no root is invented and no part of speech is guessed.

use std::time::Duration;

use pg_grammar_model::model::{Grammar, LexEntryId, PRuleId, PhonRuleDef, StratumId};
use pg_rules::shape_feat::segment_with_features;
use pg_rules::stratum::{synthesize_phonological_stratum_traced, StepBudget};
use pg_rules::trace::{NoopSink, TraceHandle};
use pg_snapshot::FwClass;

use crate::morpher::Morpher;
use crate::source_identity::{resolve_fw_entry_msa, SourceIdentityError};

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
    /// Prevents a caller from retaining an unbounded ordered-rule result.
    pub max_ordered_rule_ids: usize,
    /// Prevents one grammar-authored rule identity from dominating the result.
    pub max_rule_id_chars: usize,
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
    ResultLimit,
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
    /// The resolved stable source identity; absent when entry resolution failed.
    pub resolved_context: Option<PhonologyContext>,
    /// Declared rule order for each stratum entered by the fold, whether or not every rule ran.
    pub ordered_rule_ids: Vec<String>,
    /// Rules whose budgeted invocation began, in actual visit order.
    pub visited_rule_ids: Vec<String>,
    /// Visited rules that produced a shape.
    pub applied_rule_ids: Vec<String>,
    pub steps: usize,
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

    fn unavailable(
        reason: PhonologyUnavailableReason,
        resolved_context: Option<PhonologyContext>,
    ) -> Self {
        Self {
            result: PhonologyResult::Unavailable { reason },
            resolved_context,
            ordered_rule_ids: Vec::new(),
            visited_rule_ids: Vec::new(),
            applied_rule_ids: Vec::new(),
            steps: 0,
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
    let budget = StepBudget::with_limits(request.attempt_cap, request.work_cap)
        .with_timeout(Some(request.timeout))
        .with_synthesis_counting();
    let entry = match resolve_entry(g, &request.context) {
        Ok(entry) => entry,
        Err(reason) => return PhonologyMeasurement::unavailable(reason, None),
    };
    let Some(resolved_context) = resolved_context(g, entry, &request.context) else {
        return PhonologyMeasurement::unavailable(
            PhonologyUnavailableReason::MsaNotOwnedByEntry,
            None,
        );
    };
    let def = &g.entries[entry.0 as usize];
    let root = g.morphemes[def.morpheme.0 as usize].stratum;
    let Some(root_table) = g
        .strata
        .get(root.0 as usize)
        .and_then(|sd| g.char_tables.get(sd.table.0 as usize))
    else {
        return PhonologyMeasurement::unavailable(
            PhonologyUnavailableReason::InvalidStratum,
            Some(resolved_context),
        );
    };
    if request.underlying_shape.is_empty() {
        return PhonologyMeasurement::unavailable(
            PhonologyUnavailableReason::InvalidShape,
            Some(resolved_context),
        );
    }
    let Ok(shape) = segment_with_features(g, root_table, &request.underlying_shape) else {
        return PhonologyMeasurement::unavailable(
            PhonologyUnavailableReason::InvalidShape,
            Some(resolved_context),
        );
    };
    if !rule_order_fits(
        g,
        root.0 as usize,
        request.max_ordered_rule_ids,
        request.max_rule_id_chars,
    ) {
        return PhonologyMeasurement::unavailable(
            PhonologyUnavailableReason::ResultLimit,
            Some(resolved_context),
        );
    }

    let mut word = morpher.build_root_seed(entry, 0, pg_featstruct::FeatureStruct::EMPTY);
    word.shape = shape;
    let mut ordered_rule_ids = Vec::new();
    let mut visited_rule_ids = Vec::new();
    let mut applied_rule_ids = Vec::new();
    let mut stop = None;
    for s in root.0 as usize..g.strata.len() {
        let stratum = StratumId(s as u8);
        ordered_rule_ids.extend(
            g.strata[s]
                .prules
                .iter()
                .map(|&pid| rule_xml_id(g, pid).to_string()),
        );
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
        visited_rule_ids.extend(
            outcome
                .visited_prule_ids
                .iter()
                .map(|&pid| rule_xml_id(g, pid).to_string()),
        );
        applied_rule_ids.extend(
            outcome
                .applied_prule_ids
                .iter()
                .map(|&pid| rule_xml_id(g, pid).to_string()),
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
        resolved_context: Some(resolved_context),
        ordered_rule_ids,
        visited_rule_ids,
        applied_rule_ids,
        steps: budget.steps(),
        work_steps: budget.work_steps(),
        capped: budget.capped(),
        timed_out: budget.timed_out(),
    }
}

fn rule_xml_id(g: &Grammar, pid: PRuleId) -> &str {
    match &g.prules[pid.0 as usize] {
        PhonRuleDef::Rewrite(r) => &r.xml_id,
        PhonRuleDef::Metathesis(r) => &r.xml_id,
    }
}

fn rule_order_fits(g: &Grammar, start: usize, max_count: usize, max_id_chars: usize) -> bool {
    let mut count = 0usize;
    for stratum in &g.strata[start..] {
        for &pid in &stratum.prules {
            if count >= max_count || rule_xml_id(g, pid).chars().count() > max_id_chars {
                return false;
            }
            count += 1;
        }
    }
    true
}

fn resolved_context(
    g: &Grammar,
    entry: LexEntryId,
    requested: &PhonologyContext,
) -> Option<PhonologyContext> {
    let def = &g.entries[entry.0 as usize];
    match requested {
        PhonologyContext::HcXmlEntry { .. } => Some(PhonologyContext::HcXmlEntry {
            entry_id: def.authored_id.clone(),
        }),
        PhonologyContext::FwdataEntry { .. } => {
            let info = &g.morphemes[def.morpheme.0 as usize];
            Some(PhonologyContext::FwdataEntry {
                entry_guid: def.source_guid.clone()?,
                msa_guid: info.source_msa_guid.clone()?,
                msa_class: info.source_msa_class?,
            })
        }
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
    match context {
        PhonologyContext::HcXmlEntry { entry_id } => {
            let found: Vec<_> = g
                .entries
                .iter()
                .enumerate()
                .filter(|(_, entry)| entry.authored_id == *entry_id)
                .map(|(index, _)| LexEntryId(index as u32))
                .collect();
            match found.as_slice() {
                [] => Err(Why::UnknownEntry),
                [one] => Ok(*one),
                _ => Err(Why::AmbiguousContext),
            }
        }
        PhonologyContext::FwdataEntry {
            entry_guid,
            msa_guid,
            msa_class,
        } => {
            resolve_fw_entry_msa(g, entry_guid, msa_guid, *msa_class).map_err(|error| match error {
                SourceIdentityError::EmptyLexicon => Why::EmptyLexicon,
                SourceIdentityError::UnknownEntry
                | SourceIdentityError::AuthoredIdentityUnavailable => Why::UnknownEntry,
                SourceIdentityError::AmbiguousIdentity => Why::AmbiguousContext,
                SourceIdentityError::UnknownMsa
                | SourceIdentityError::MsaNotOwnedByEntry
                | SourceIdentityError::UnsupportedMsaClass => Why::MsaNotOwnedByEntry,
            })
        }
    }
}
