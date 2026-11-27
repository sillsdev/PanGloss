//! `batch --analyses --query-fields`: reading count, stable morph signatures and a typed failure per row.

use pg_grammar::model::Grammar;
use pg_parse::parse_morph::{ParseMorph, ProjectedMorphs};
use pg_parse::WordAnalysis;
use pg_stats::WorkCap;
use serde::Serialize;
use serde_json::Value;

/// What the row writer needs besides the outcome: the work cap the run used.
#[derive(Clone, Copy)]
pub(crate) struct QueryFields {
    work_cap: Option<u64>,
}

impl QueryFields {
    pub(crate) fn new(work_cap: &WorkCap) -> Self {
        Self {
            work_cap: match work_cap {
                WorkCap::Finite(cap) => Some(cap.get()),
                WorkCap::Unbounded => None,
            },
        }
    }
}

/// One morph: the v1 members plus its entry guid and `signature`.
#[derive(Serialize)]
pub(crate) struct QueryMorph {
    #[serde(flatten)]
    morph: ParseMorph,
    entry: Option<String>,
    signature: String,
}

/// One reading in the `--query-fields` shape.
#[derive(Serialize)]
pub(crate) struct QueryReading {
    morphs: Vec<QueryMorph>,
    #[serde(rename = "morphSpans", skip_serializing_if = "Option::is_none")]
    morph_spans: Option<Value>,
    signature: String,
}

/// The row-level members `--query-fields` adds.
#[derive(Serialize)]
pub(crate) struct RowFields {
    #[serde(rename = "readingCount")]
    reading_count: usize,
    failure: Option<Failure>,
}

/// Why a word has no reading, as a typed status.
#[derive(Serialize)]
pub(crate) struct Failure {
    status: FailureStatus,
    #[serde(rename = "workSteps")]
    work_steps: Option<usize>,
    #[serde(rename = "workCap")]
    work_cap: Option<u64>,
}

#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
pub(crate) enum FailureStatus {
    /// The search finished inside its budget and found no reading.
    NoParse,
    WorkCap,
    CaptureCap,
    MemoryCap,
    Timeout,
    InvalidShape,
}

pub(crate) fn status_of(outcome: &pg_parse::ParseOutcome) -> FailureStatus {
    if outcome.invalid_shape {
        FailureStatus::InvalidShape
    } else if outcome.timed_out {
        FailureStatus::Timeout
    } else if outcome.word_mem_capped {
        FailureStatus::MemoryCap
    } else if outcome.capture_capped {
        FailureStatus::CaptureCap
    } else if outcome.capped {
        FailureStatus::WorkCap
    } else {
        FailureStatus::NoParse
    }
}

/// `entry:msa[:inflType]`, with `-` for a morph that has no lexical entry; built only from source guids.
fn signature(entry: Option<&str>, morph: &ParseMorph) -> String {
    let mut signature = format!(
        "{}:{}",
        entry.unwrap_or("-"),
        morph.msa.as_deref().unwrap_or("-")
    );
    if let Some(infl_type) = &morph.infl_type {
        signature.push(':');
        signature.push_str(infl_type);
    }
    signature
}

pub(crate) fn reading(
    analysis: &WordAnalysis,
    projected: ProjectedMorphs,
    grammar: &Grammar,
    morph_spans: Option<Value>,
) -> QueryReading {
    let guessed = analysis.guessed_pattern.map(|pattern| pattern.morpheme_id);
    let mut morphs = Vec::with_capacity(projected.analysis.morphs.len());
    for (morph, morpheme) in projected
        .analysis
        .morphs
        .into_iter()
        .zip(projected.morphemes)
    {
        let morpheme = if morph.guessed_string.is_some() {
            guessed.unwrap_or(morpheme)
        } else {
            morpheme
        };
        let entry = grammar
            .morphemes
            .get(morpheme as usize)
            .and_then(|info| info.owner.as_ref())
            .map(|owner| owner.entry_guid.clone());
        let signature = signature(entry.as_deref(), &morph);
        morphs.push(QueryMorph {
            morph,
            entry,
            signature,
        });
    }
    let signature = morphs
        .iter()
        .map(|morph| morph.signature.as_str())
        .collect::<Vec<_>>()
        .join("+");
    QueryReading {
        morphs,
        morph_spans,
        signature,
    }
}

pub(crate) fn row_fields(
    fields: QueryFields,
    outcome: &pg_parse::ParseOutcome,
    analyses: &[crate::ReadingOut],
    unavailable: &[String],
) -> RowFields {
    let reading_count = analyses.len() + unavailable.len();
    let failure = (reading_count == 0).then(|| {
        let status = status_of(outcome);
        Failure {
            status,
            // A timeout's step count depends on the clock.
            work_steps: (status != FailureStatus::Timeout).then_some(outcome.work_steps),
            work_cap: fields.work_cap,
        }
    });
    RowFields {
        reading_count,
        failure,
    }
}
