//! `pangloss.investigation-handoff/v1` — the evidence for one case, bound to the run that produced
//! the delta.
//!
//! ## What this operation is actually for
//!
//! `compare` says case 47 lost an analysis and is forbidden from saying why. A reviewer therefore
//! has nowhere to go. `investigate` hands back the raw material for that one case so a human or an
//! AI can draw the conclusion — never so PanGloss can.
//!
//! It is deliberately not a tracer. FieldWorks has its own HermitCrab and its own trace UI, and
//! competing with it would be wasted work. The one thing FLEx structurally *cannot* do is bind
//! evidence to a specific PanGloss report, model fingerprint, and case: C# HermitCrab traces the
//! FLEx model as it is right now, with no idea whether that corresponds to the baseline or the
//! candidate you compared. That binding is cheap here and impossible there, and it is the job.
//!
//! ## The trap this artifact exists to avoid
//!
//! When FieldWorks investigates a PanGloss-detected delta using C# HermitCrab, it is tracing a
//! **different engine** than the one that found it. Usually harmless — the two are contractually
//! required to agree on complete cases. But in exactly the case where the delta was *caused* by a
//! PanGloss/C# divergence, the C# trace shows the analysis being produced perfectly normally. The
//! investigator then hunts a grammar bug that does not exist, and the real defect is the one thing
//! the trace has hidden. So every piece of evidence here records which engine produced it, and
//! `EngineCaveat` says this out loud in the artifact rather than in documentation nobody reads.
//!
//! ## Honest source identity
//!
//! Stable FieldWorks IDs survive import for lexical entries only (`LexEntryDef.authored_id`).
//! Rules, strata, and templates are compiler-assigned dense indices with no GUID retained, so a
//! handoff cannot name them in FieldWorks terms. Following ADR 0001's honest-capability-boundary
//! pattern, each reference is explicitly tagged `SourceIdKind` — a handoff that silently
//! presented a dense index as if it were a FieldWorks identity would be worse than one that admits
//! the gap.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::identity::AnalysisIdentity;
use crate::report::AssessmentReport;

pub const HANDOFF_SCHEMA: &str = "pangloss.investigation-handoff";
pub const HANDOFF_SCHEMA_VERSION: u32 = 1;

/// Whether a reference names something the author would recognize.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SourceIdKind {
    /// A GUID or `id` attribute retained through import. A caller can look this up in FieldWorks.
    SourceId,
    /// A compiler-assigned dense ordinal, stable only within this compiled model — never a source identity.
    CompilerAssigned,
}

/// One grammar construct the evidence refers to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConstructRef {
    /// `lexicalEntry`, `morphologicalRule`, `phonologicalRule`, `stratum`, `template`.
    pub kind: String,
    pub id: String,
    pub id_kind: SourceIdKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

impl ConstructRef {
    /// A reference a caller can resolve in FieldWorks.
    pub fn source(kind: &str, id: impl Into<String>, label: Option<String>) -> Self {
        ConstructRef {
            kind: kind.to_string(),
            id: id.into(),
            id_kind: SourceIdKind::SourceId,
            label,
        }
    }

    /// A reference that only means something inside this compiled model. Naming it honestly costs a
    /// field; presenting it as a source ID would send an investigator hunting in the wrong tool.
    pub fn compiler_assigned(kind: &str, index: usize, label: Option<String>) -> Self {
        ConstructRef {
            kind: kind.to_string(),
            id: index.to_string(),
            id_kind: SourceIdKind::CompilerAssigned,
            label,
        }
    }
}

/// Where a piece of evidence came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EvidenceAvailability {
    /// Captured during the original assessment and stored.
    Retained,
    /// Produced by re-running the case now, since reports do not retain full traces; never presented as originally captured.
    Regenerated,
    /// Not obtainable — stated explicitly so a reader can tell "no evidence" from "we did not look".
    Unavailable,
}

/// A body of evidence and its provenance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Evidence {
    pub availability: EvidenceAvailability,
    /// Which engine produced this — `hermitcrab` or `foma-confirm`. Load-bearing: see the module
    /// doc's note on tracing a different engine than the one that found the delta.
    pub engine: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// Why an analysis the caller expected is not present.
///
/// The distinction is the whole value of running both pipelines. Under `foma-confirm` a missing
/// analysis has two completely different causes, and a narrative that cannot tell them apart will
/// confidently explain a grammar "problem" that is actually our bug — and the reviewer will edit a
/// perfectly good grammar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MissingAnalysisCause {
    /// HermitCrab considered the candidate and rejected it — a real grammar fact, not a PanGloss bug.
    HermitcrabRejected,
    /// HermitCrab alone produces the analysis but the FST proposer never offered it — a PanGloss recall gap, not a grammar defect.
    ProposerRecallGap,
    /// Neither pipeline produces it. The grammar does not license it under either engine.
    NeitherPipelineProduces,
    /// Attribution needs a run on both pipelines and only one was available.
    Undetermined,
}

/// One step in the pruned narrative: a candidate that was attempted, and where it died.
///
/// This is deliberately not a trace dump. A single word's trace can run to thousands of nodes;
/// handing that to an AI burns tokens and buries the answer. What a reader needs is which candidate
/// parses were attempted, where each was rejected, and under which typed reason.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NarrativeStep {
    /// The candidate under consideration, as a human-readable morpheme join.
    pub candidate: String,
    /// Where it was rejected.
    pub at: ConstructRef,
    /// The `pg_rules::trace::FailureReason` variant name, carried verbatim so a Rust narrative and
    /// a C# trace name the same thing.
    pub failure_reason: String,
    /// What was observed. Never why the grammar is wrong, and never what to change.
    pub detail: String,
}

/// The caveat every handoff carries, stated in the artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineCaveat {
    pub text: String,
}

impl Default for EngineCaveat {
    fn default() -> Self {
        EngineCaveat {
            text: "Evidence records the engine that produced it. FieldWorks' C# HermitCrab is a \
                   different implementation from the one that produced this assessment; the two are \
                   required to agree on complete cases, so a disagreement observed there is not \
                   necessarily a grammar defect and may be an engine divergence."
                .to_string(),
        }
    }
}

/// The finished handoff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvestigationHandoff {
    pub report_id: String,
    pub model_fingerprint: String,
    pub case_id: String,
    pub input: String,
    pub pipeline: String,
    pub outcome: String,
    /// Analyses the case produced, if it completed.
    pub observed: Vec<AnalysisIdentity>,
    /// Analyses asked about that are absent, each with its attributed cause.
    pub missing: Vec<MissingAnalysis>,
    pub constructs: Vec<ConstructRef>,
    pub evidence: Evidence,
    pub narrative: Vec<NarrativeStep>,
    pub caveat: EngineCaveat,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingAnalysis {
    pub identity: AnalysisIdentity,
    pub cause: MissingAnalysisCause,
}

/// Why a handoff was refused. Nothing partial is emitted: evidence bound to the wrong run is worse
/// than no evidence, because it looks authoritative.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HandoffError {
    /// The named case is not in the report.
    UnknownCase(String),
    /// The model available now differs from the one the report was produced against, so regenerated evidence would describe a different grammar.
    ModelFingerprintMismatch { report: String, current: String },
    /// The caller asked for a pipeline the report was not produced with.
    PipelineMismatch { report: String, requested: String },
}

impl std::fmt::Display for HandoffError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HandoffError::UnknownCase(case_id) => {
                write!(f, "case {case_id} is not in this report")
            }
            HandoffError::ModelFingerprintMismatch { report, current } => write!(
                f,
                "the report was produced against model {report} but the loaded model is {current}; \
                 regenerated evidence would describe a different grammar"
            ),
            HandoffError::PipelineMismatch { report, requested } => write!(
                f,
                "the report was produced with pipeline {report}, not {requested}"
            ),
        }
    }
}

impl std::error::Error for HandoffError {}

/// What the caller wants explained, and what the runtime was able to supply.
///
/// Taking the evidence as input keeps this crate engine-agnostic: producing a trace means running
/// HermitCrab, which lives above `pg-assess`. What is enforced here is the binding, the honesty of
/// the labels, and the shape of the artifact.
#[derive(Debug, Clone, Default)]
pub struct HandoffRequest {
    pub case_id: String,
    /// Identities the caller wants accounted for — typically `compare`'s `removed` set.
    pub asked_about: Vec<AnalysisIdentity>,
    /// The fingerprint of the model loaded right now, if one is loaded.
    pub current_model_fingerprint: Option<String>,
    pub requested_pipeline: Option<String>,
    pub evidence: Option<Evidence>,
    pub narrative: Vec<NarrativeStep>,
    pub constructs: Vec<ConstructRef>,
    /// Per-identity cause attribution, supplied by whoever ran both pipelines. Anything absent
    /// stays `Undetermined` rather than being guessed.
    pub causes: Vec<(AnalysisIdentity, MissingAnalysisCause)>,
}

/// Build a handoff for one case, refusing unless the binding holds.
pub fn investigate(
    report: &AssessmentReport,
    request: &HandoffRequest,
) -> Result<InvestigationHandoff, HandoffError> {
    let case = report
        .cases()
        .iter()
        .find(|c| c.case_id == request.case_id)
        .ok_or_else(|| HandoffError::UnknownCase(request.case_id.clone()))?;

    let recorded_fingerprint = &report.draft().provenance.model_fingerprint;
    if let Some(current) = &request.current_model_fingerprint {
        if current != recorded_fingerprint {
            return Err(HandoffError::ModelFingerprintMismatch {
                report: recorded_fingerprint.clone(),
                current: current.clone(),
            });
        }
    }
    let recorded_pipeline = &report.draft().execution.pipeline;
    if let Some(requested) = &request.requested_pipeline {
        if requested != recorded_pipeline {
            return Err(HandoffError::PipelineMismatch {
                report: recorded_pipeline.clone(),
                requested: requested.clone(),
            });
        }
    }

    let observed: Vec<AnalysisIdentity> = case
        .outcome
        .analyses()
        .map(|set| {
            set.entries()
                .iter()
                .map(|e| e.identity.clone())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let missing = request
        .asked_about
        .iter()
        .filter(|identity| !observed.contains(identity))
        .map(|identity| MissingAnalysis {
            identity: identity.clone(),
            cause: request
                .causes
                .iter()
                .find(|(candidate, _)| candidate == identity)
                .map(|(_, cause)| *cause)
                .unwrap_or(MissingAnalysisCause::Undetermined),
        })
        .collect();

    // No evidence supplied means no evidence: defaulting to something optimistic would overstate what PanGloss knows.
    let evidence = request.evidence.clone().unwrap_or(Evidence {
        availability: EvidenceAvailability::Unavailable,
        engine: recorded_pipeline.clone(),
        note: Some("no trace was requested or the pipeline cannot produce one".to_string()),
    });

    Ok(InvestigationHandoff {
        report_id: report.report_id().to_string(),
        model_fingerprint: recorded_fingerprint.clone(),
        case_id: case.case_id.clone(),
        input: case.input.clone(),
        pipeline: recorded_pipeline.clone(),
        outcome: case.outcome.kind().to_string(),
        observed,
        missing,
        constructs: request.constructs.clone(),
        evidence,
        narrative: request.narrative.clone(),
        caveat: EngineCaveat::default(),
    })
}

impl InvestigationHandoff {
    pub fn to_value(&self) -> Value {
        json!({
            "schema": HANDOFF_SCHEMA,
            "schemaVersion": HANDOFF_SCHEMA_VERSION,
            "binding": {
                "reportId": self.report_id,
                "modelFingerprint": self.model_fingerprint,
                "caseId": self.case_id,
                "input": self.input,
                "pipeline": self.pipeline,
            },
            "outcome": self.outcome,
            "observed": self.observed.iter().map(|i| i.to_canonical_value()).collect::<Vec<_>>(),
            "missing": self.missing.iter().map(|m| json!({
                "identity": m.identity.to_canonical_value(),
                "cause": m.cause,
            })).collect::<Vec<_>>(),
            "constructs": self.constructs,
            "evidence": self.evidence,
            "narrative": self.narrative,
            "caveat": self.caveat,
        })
    }
}

#[cfg(test)]
mod tests;
