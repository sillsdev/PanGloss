//! Trusted handoff from one selected backend build to the runtime.
//!
//! A [`CompletedBackendBuild`] is deliberately not constructible by callers.  It contains the
//! exact Foma binary-memory payload returned by one compiler route plus the immutable evidence
//! that makes that payload eligible for normal selection.  The selector checks the evidence again
//! before handing the value to runtime; neither selection nor runtime recompiles a grammar.

use std::fmt;

use foma::lexcread::fsm_lexc_parse_string;
use foma::options::FomaOptions;
use pg_grammar::model::Grammar;
use sha2::{Digest, Sha256};

use crate::analyzer::{prepare_network_for_apply, read_foma_binary_payload, FomaProposer};
use crate::backend_runtime::{finished_net_digest, grammar_identity};
use crate::backend_selection::BackendSelection;
use crate::characterization::ClosureTerminal;
use crate::composite::FomaAnalyzer;
use crate::emit::{EmitReport, FomaTier};
use crate::enumerate::EmissionStrategy;
use crate::resource_envelope::{CompileEnvelopeRequest, ResourceEnvelope, ResourceEnvelopeId};
use crate::templated_compile::compile_templated_morphotactics;

/// Which kind of route-specific evidence made a completed payload eligible for selection.
///
/// The proof is deliberately not interchangeable: a templated emission certificate never claims
/// to have run TunedSurface closure, and a TunedSurface closure certificate never stands in for a
/// templated emitter's Full report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionProofKind {
    TunedClosure,
    TemplatedFullEmission,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CompletionProof {
    TunedClosure {
        terminal: ClosureTerminal,
        envelope_id: ResourceEnvelopeId,
        envelope_digest: String,
        worklist_empty: bool,
        pending_successor_count: usize,
    },
    TemplatedFullEmission {
        uncovered_count: usize,
        skipped_count: usize,
    },
}

impl CompletionProof {
    fn kind(&self) -> CompletionProofKind {
        match self {
            Self::TunedClosure { .. } => CompletionProofKind::TunedClosure,
            Self::TemplatedFullEmission { .. } => CompletionProofKind::TemplatedFullEmission,
        }
    }
}

/// Immutable evidence attached to one completed backend payload.
///
/// The fields stay private so a caller cannot manufacture a trusted artifact by assembling values
/// that merely look consistent.  Read-only accessors are intentionally small and language-neutral;
/// the report/diagnostic layers can project this value without owning the payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletedBackendBuildEvidence {
    requested_strategy: EmissionStrategy,
    realized_strategy: EmissionStrategy,
    grammar_identity: String,
    attempt_id: String,
    envelope_id: ResourceEnvelopeId,
    envelope_digest: String,
    completion_proof: CompletionProof,
    state_count: i32,
    arc_count: i32,
    model_fingerprint: String,
    payload_fingerprint: String,
}

impl CompletedBackendBuildEvidence {
    pub fn requested_strategy(&self) -> EmissionStrategy {
        self.requested_strategy
    }

    pub fn realized_strategy(&self) -> EmissionStrategy {
        self.realized_strategy
    }

    pub fn grammar_identity(&self) -> &str {
        &self.grammar_identity
    }

    pub fn attempt_id(&self) -> &str {
        &self.attempt_id
    }

    pub fn envelope_id(&self) -> ResourceEnvelopeId {
        self.envelope_id
    }

    pub fn envelope_digest(&self) -> &str {
        &self.envelope_digest
    }

    pub fn completion_proof_kind(&self) -> CompletionProofKind {
        self.completion_proof.kind()
    }

    pub fn state_count(&self) -> i32 {
        self.state_count
    }

    pub fn arc_count(&self) -> i32 {
        self.arc_count
    }

    pub fn model_fingerprint(&self) -> &str {
        &self.model_fingerprint
    }

    pub fn payload_fingerprint(&self) -> &str {
        &self.payload_fingerprint
    }

    pub fn is_trusted_complete(&self) -> bool {
        if self.requested_strategy != self.realized_strategy
            || self.attempt_id.is_empty()
            || self.model_fingerprint.is_empty()
            || self.payload_fingerprint.is_empty()
        {
            return false;
        }

        match (&self.realized_strategy, &self.completion_proof) {
            (
                EmissionStrategy::TunedSurfaceProbed,
                CompletionProof::TunedClosure {
                    terminal: ClosureTerminal::Complete,
                    envelope_id,
                    envelope_digest,
                    worklist_empty,
                    pending_successor_count,
                },
            ) => {
                *envelope_id == self.envelope_id
                    && envelope_digest == &self.envelope_digest
                    && *worklist_empty
                    && *pending_successor_count == 0
            }
            (
                EmissionStrategy::TemplatedUnderlyingTokens,
                CompletionProof::TemplatedFullEmission {
                    uncovered_count,
                    skipped_count,
                },
            ) => *uncovered_count == 0 && *skipped_count == 0,
            _ => false,
        }
    }
}

/// A compiler-owned, finalized Foma payload and its evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletedBackendBuild {
    evidence: CompletedBackendBuildEvidence,
    payload_bytes: Vec<u8>,
}

impl CompletedBackendBuild {
    pub fn evidence(&self) -> &CompletedBackendBuildEvidence {
        &self.evidence
    }

    pub fn payload_bytes(&self) -> &[u8] {
        &self.payload_bytes
    }
}

/// The selector's opaque choice.  It cannot be constructed from a caller-supplied strategy or
/// evidence, and its runtime handoff validates the grammar identity a second time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectedBackendBuild {
    build: CompletedBackendBuild,
}

impl SelectedBackendBuild {
    pub fn strategy(&self) -> EmissionStrategy {
        self.build.evidence.realized_strategy
    }

    pub fn evidence(&self) -> &CompletedBackendBuildEvidence {
        self.build.evidence()
    }

    pub fn payload_bytes(&self) -> &[u8] {
        self.build.payload_bytes()
    }

    /// Reconstructs the ordinary propose -> peel -> confirm analyzer from the exact payload.
    /// This method does not call either backend compiler.
    pub fn into_analyzer<'g>(
        self,
        grammar: &'g Grammar,
    ) -> Result<FomaAnalyzer<'g>, CompletedBuildError> {
        let expected_grammar_identity = grammar_identity(grammar);
        if self.evidence().grammar_identity != expected_grammar_identity {
            return Err(CompletedBuildError::GrammarIdentityMismatch {
                expected: self.evidence().grammar_identity.clone(),
                actual: expected_grammar_identity,
            });
        }
        validate_payload(&self.build)?;
        let net = read_foma_binary_payload(&self.build.payload_bytes)
            .map_err(|error| CompletedBuildError::PayloadRead(error.to_string()))?;
        let mut proposer = FomaProposer::from_precompiled_network_without_emit_report(&net);
        if self.strategy() == EmissionStrategy::TemplatedUnderlyingTokens {
            proposer = proposer.with_segment_query_encoder(crate::emit::surface_table(grammar));
        }
        Ok(FomaAnalyzer::from_precompiled_proposer(grammar, proposer))
    }
}

/// Failure at the trusted compiler/runtime boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompletedBuildError {
    UnsupportedStrategy(EmissionStrategy),
    Compiler(String),
    IncompleteEvidence(String),
    PayloadEmpty,
    PayloadRead(String),
    PayloadDigestMismatch {
        expected: String,
        actual: String,
    },
    ModelFingerprintMismatch {
        expected: String,
        actual: String,
    },
    GrammarIdentityMismatch {
        expected: String,
        actual: String,
    },
    EnvelopeMismatch {
        expected: ResourceEnvelopeId,
        actual: ResourceEnvelopeId,
    },
    EnvelopeDigestMismatch {
        expected: String,
        actual: String,
    },
    AttemptMismatch {
        expected: String,
        actual: String,
    },
    StrategyMismatch {
        requested: EmissionStrategy,
        realized: EmissionStrategy,
    },
    NoMatchingCompletedBuild,
}

impl fmt::Display for CompletedBuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedStrategy(strategy) => {
                write!(f, "unsupported completed-build strategy {strategy:?}")
            }
            Self::Compiler(error) => write!(f, "backend compiler failed: {error}"),
            Self::IncompleteEvidence(reason) => {
                write!(f, "completed-build evidence is incomplete: {reason}")
            }
            Self::PayloadEmpty => f.write_str("completed-build payload is empty"),
            Self::PayloadRead(error) => write!(f, "completed-build payload is unreadable: {error}"),
            Self::PayloadDigestMismatch { expected, actual } => write!(
                f,
                "payload digest mismatch: expected {expected}, got {actual}"
            ),
            Self::ModelFingerprintMismatch { expected, actual } => write!(
                f,
                "model fingerprint mismatch: expected {expected}, got {actual}"
            ),
            Self::GrammarIdentityMismatch { expected, actual } => write!(
                f,
                "grammar identity mismatch: expected {expected}, got {actual}"
            ),
            Self::EnvelopeMismatch { expected, actual } => {
                write!(f, "envelope mismatch: expected {expected}, got {actual}")
            }
            Self::EnvelopeDigestMismatch { expected, actual } => write!(
                f,
                "envelope digest mismatch: expected {expected}, got {actual}"
            ),
            Self::AttemptMismatch { expected, actual } => write!(
                f,
                "compile attempt mismatch: expected {expected}, got {actual}"
            ),
            Self::StrategyMismatch {
                requested,
                realized,
            } => write!(
                f,
                "backend strategy mismatch: requested {requested:?}, realized {realized:?}"
            ),
            Self::NoMatchingCompletedBuild => f.write_str("no matching completed backend build"),
        }
    }
}

impl std::error::Error for CompletedBuildError {}

/// Compile one explicitly requested backend exactly once and retain its finalized binary payload.
pub fn compile_completed_backend(
    grammar: &Grammar,
    requested_strategy: EmissionStrategy,
    request: &CompileEnvelopeRequest,
) -> Result<CompletedBackendBuild, CompletedBuildError> {
    let envelope = ResourceEnvelope::for_id(request.envelope_id());
    let grammar_id = grammar_identity(grammar);
    match requested_strategy {
        EmissionStrategy::TunedSurfaceProbed => {
            let emitted = crate::emit::emit_tuned_surface_for_request(grammar, request);
            let report = emitted.report;
            let closure = report.closure_evidence.as_ref().ok_or_else(|| {
                CompletedBuildError::IncompleteEvidence(
                    "TunedSurface emitted no closure certificate".to_string(),
                )
            })?;
            validate_closure(closure, &envelope)?;
            validate_emit_report(&report)?;
            let mut network =
                fsm_lexc_parse_string(&FomaOptions::default(), None, &emitted.lexc_source)
                    .ok_or_else(|| {
                        CompletedBuildError::Compiler(
                            "TunedSurface lexc failed to compile".to_string(),
                        )
                    })?;
            prepare_network_for_apply(&mut network);
            let model_fingerprint = finished_net_digest(&network);
            let completion_proof = CompletionProof::TunedClosure {
                terminal: closure.terminal,
                envelope_id: closure.evidence.envelope_id,
                envelope_digest: closure.evidence.envelope_digest.clone(),
                worklist_empty: closure.evidence.worklist_empty,
                pending_successor_count: closure.evidence.pending_successor_count,
            };
            let proposer = FomaProposer::from_precompiled_network(&network, report);
            build_from_proposer(
                requested_strategy,
                grammar_id,
                request,
                proposer,
                model_fingerprint,
                completion_proof,
            )
        }
        EmissionStrategy::TemplatedUnderlyingTokens => {
            let output = compile_templated_morphotactics(grammar)
                .map_err(|error| CompletedBuildError::Compiler(error.to_string()))?;
            let report = output.proposer.report.as_ref().ok_or_else(|| {
                CompletedBuildError::IncompleteEvidence(
                    "templated compiler emitted no report".to_string(),
                )
            })?;
            validate_emit_report(report)?;
            let skipped = output.profile.skipped_rules.len() + report.counts.allomorphs_skipped;
            if skipped != 0 {
                return Err(CompletedBuildError::IncompleteEvidence(format!(
                    "templated compiler skipped {skipped} rule/allomorph items"
                )));
            }
            let completion_proof = CompletionProof::TemplatedFullEmission {
                uncovered_count: report.uncovered.len(),
                skipped_count: skipped,
            };
            let model_fingerprint = finished_net_digest(&output.network);
            build_from_proposer(
                requested_strategy,
                grammar_id,
                request,
                output.proposer,
                model_fingerprint,
                completion_proof,
            )
        }
        strategy => Err(CompletedBuildError::UnsupportedStrategy(strategy)),
    }
}

/// Select the highest-preference report that has a matching, fully trusted completed build.
/// Caller-provided preferred/selected values are intentionally not accepted.
pub fn select_completed_build<I>(
    selection: &BackendSelection,
    completed_builds: I,
    request: &CompileEnvelopeRequest,
    expected_grammar_identity: &str,
) -> Result<SelectedBackendBuild, CompletedBuildError>
where
    I: IntoIterator<Item = CompletedBackendBuild>,
{
    let builds: Vec<_> = completed_builds.into_iter().collect();
    for strategy in selection.selected() {
        if let Some(build) = builds
            .iter()
            .find(|build| build.evidence.realized_strategy == strategy)
        {
            validate_selected_build(build, request, expected_grammar_identity)?;
            return Ok(SelectedBackendBuild {
                build: build.clone(),
            });
        }
    }
    Err(CompletedBuildError::NoMatchingCompletedBuild)
}

fn validate_selected_build(
    build: &CompletedBackendBuild,
    request: &CompileEnvelopeRequest,
    expected_grammar_identity: &str,
) -> Result<(), CompletedBuildError> {
    let evidence = &build.evidence;
    if evidence.grammar_identity != expected_grammar_identity {
        return Err(CompletedBuildError::GrammarIdentityMismatch {
            expected: expected_grammar_identity.to_string(),
            actual: evidence.grammar_identity.clone(),
        });
    }
    let expected_attempt = request.attempt_id().as_str();
    if evidence.attempt_id != expected_attempt {
        return Err(CompletedBuildError::AttemptMismatch {
            expected: expected_attempt.to_string(),
            actual: evidence.attempt_id.clone(),
        });
    }
    let expected_envelope = ResourceEnvelope::for_id(request.envelope_id());
    if evidence.envelope_id != expected_envelope.id() {
        return Err(CompletedBuildError::EnvelopeMismatch {
            expected: expected_envelope.id(),
            actual: evidence.envelope_id,
        });
    }
    if evidence.envelope_digest != expected_envelope.digest() {
        return Err(CompletedBuildError::EnvelopeDigestMismatch {
            expected: expected_envelope.digest(),
            actual: evidence.envelope_digest.clone(),
        });
    }
    if evidence.requested_strategy != evidence.realized_strategy {
        return Err(CompletedBuildError::StrategyMismatch {
            requested: evidence.requested_strategy,
            realized: evidence.realized_strategy,
        });
    }
    if !evidence.is_trusted_complete() {
        return Err(CompletedBuildError::IncompleteEvidence(
            "zero-gap complete evidence is required for normal selection".to_string(),
        ));
    }
    validate_payload(build)
}

fn validate_payload(build: &CompletedBackendBuild) -> Result<(), CompletedBuildError> {
    if build.payload_bytes.is_empty() {
        return Err(CompletedBuildError::PayloadEmpty);
    }
    let payload_digest = sha256_hex(&build.payload_bytes);
    if payload_digest != build.evidence.payload_fingerprint {
        return Err(CompletedBuildError::PayloadDigestMismatch {
            expected: build.evidence.payload_fingerprint.clone(),
            actual: payload_digest,
        });
    }
    let network = read_foma_binary_payload(&build.payload_bytes)
        .map_err(|error| CompletedBuildError::PayloadRead(error.to_string()))?;
    let model_digest = finished_net_digest(&network);
    if model_digest != build.evidence.model_fingerprint {
        return Err(CompletedBuildError::ModelFingerprintMismatch {
            expected: build.evidence.model_fingerprint.clone(),
            actual: model_digest,
        });
    }
    Ok(())
}

fn validate_closure(
    closure: &crate::characterization::CharacterizationResult,
    envelope: &ResourceEnvelope,
) -> Result<(), CompletedBuildError> {
    if closure.evidence.envelope_id != envelope.id() {
        return Err(CompletedBuildError::EnvelopeMismatch {
            expected: envelope.id(),
            actual: closure.evidence.envelope_id,
        });
    }
    if closure.evidence.envelope_digest != envelope.digest() {
        return Err(CompletedBuildError::EnvelopeDigestMismatch {
            expected: envelope.digest(),
            actual: closure.evidence.envelope_digest.clone(),
        });
    }
    if closure.terminal != ClosureTerminal::Complete
        || closure.evidence.pending_successor_count != 0
        || !closure.evidence.worklist_empty
    {
        return Err(CompletedBuildError::IncompleteEvidence(
            "closure certificate is not complete".to_string(),
        ));
    }
    Ok(())
}

fn validate_emit_report(report: &EmitReport) -> Result<(), CompletedBuildError> {
    if !matches!(report.tier, FomaTier::Full) {
        return Err(CompletedBuildError::IncompleteEvidence(format!(
            "emitter tier is {:?}, not Full",
            report.tier
        )));
    }
    if !report.uncovered.is_empty() || report.counts.allomorphs_skipped != 0 {
        return Err(CompletedBuildError::IncompleteEvidence(
            "emitter reported uncovered or skipped material".to_string(),
        ));
    }
    Ok(())
}

fn build_from_proposer(
    requested_strategy: EmissionStrategy,
    grammar_identity: String,
    request: &CompileEnvelopeRequest,
    proposer: FomaProposer,
    model_fingerprint: String,
    completion_proof: CompletionProof,
) -> Result<CompletedBackendBuild, CompletedBuildError> {
    let envelope = ResourceEnvelope::for_id(request.envelope_id());
    let payload_bytes = proposer
        .foma_binary_payload()
        .map_err(|error| CompletedBuildError::Compiler(error.to_string()))?;
    if payload_bytes.is_empty() {
        return Err(CompletedBuildError::PayloadEmpty);
    }
    let payload_fingerprint = sha256_hex(&payload_bytes);
    let network = read_foma_binary_payload(&payload_bytes)
        .map_err(|error| CompletedBuildError::PayloadRead(error.to_string()))?;
    let actual_model_fingerprint = finished_net_digest(&network);
    if actual_model_fingerprint != model_fingerprint {
        return Err(CompletedBuildError::ModelFingerprintMismatch {
            expected: model_fingerprint,
            actual: actual_model_fingerprint,
        });
    }
    let (state_count, arc_count) = (network.statecount, network.arccount);
    let evidence = CompletedBackendBuildEvidence {
        requested_strategy,
        realized_strategy: requested_strategy,
        grammar_identity,
        attempt_id: request.attempt_id().as_str().to_string(),
        envelope_id: envelope.id(),
        envelope_digest: envelope.digest(),
        completion_proof,
        state_count,
        arc_count,
        model_fingerprint: actual_model_fingerprint,
        payload_fingerprint,
    };
    Ok(CompletedBackendBuild {
        evidence,
        payload_bytes,
    })
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
