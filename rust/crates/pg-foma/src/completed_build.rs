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
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::analyzer::{prepare_network_for_apply, read_foma_binary_payload, FomaProposer};
use crate::backend_runtime::{finished_net_digest, grammar_identity};
use crate::backend_selection::BackendSelection;
use crate::characterization::ClosureTerminal;
use crate::composite::FomaAnalyzer;
use crate::emit::{EmitReport, FomaTier};
use crate::enumerate::EmissionStrategy;
use crate::templated_compile::compile_templated_morphotactics;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AttemptId(String);

impl AttemptId {
    fn new(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into();
        if value.trim().is_empty() {
            Err("attempt id must not be empty".to_string())
        } else {
            Ok(Self(value))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn fresh_attempt_id() -> Result<AttemptId, String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|error| format!("could not create attempt id: {error}"))?;
    let mut encoded = String::with_capacity("attempt-".len() + bytes.len() * 2);
    encoded.push_str("attempt-");
    for byte in bytes {
        encoded.push_str(&format!("{byte:02x}"));
    }
    AttemptId::new(encoded)
}

/// Identity for one compilation attempt. It is deliberately independent of execution limits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompileAttempt {
    attempt_id: AttemptId,
}

impl CompileAttempt {
    pub fn try_new() -> Result<Self, String> {
        Ok(Self {
            attempt_id: fresh_attempt_id()?,
        })
    }

    pub fn attempt_id(&self) -> &AttemptId {
        &self.attempt_id
    }

    pub(crate) fn from_worker_wire(attempt_id: impl Into<String>) -> Result<Self, String> {
        Ok(Self {
            attempt_id: AttemptId::new(attempt_id)?,
        })
    }
}

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
                    worklist_empty,
                    pending_successor_count,
                },
            ) => *worklist_empty && *pending_successor_count == 0,
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

    pub(crate) fn into_wire(&self) -> CompletedBackendBuildWire {
        let proof = match &self.evidence.completion_proof {
            CompletionProof::TunedClosure {
                terminal,
                worklist_empty,
                pending_successor_count,
            } => CompletionProofWire::TunedClosure {
                terminal: *terminal,
                worklist_empty: *worklist_empty,
                pending_successor_count: *pending_successor_count,
            },
            CompletionProof::TemplatedFullEmission {
                uncovered_count,
                skipped_count,
            } => CompletionProofWire::TemplatedFullEmission {
                uncovered_count: *uncovered_count,
                skipped_count: *skipped_count,
            },
        };
        CompletedBackendBuildWire {
            requested_strategy: self.evidence.requested_strategy.label().to_string(),
            realized_strategy: self.evidence.realized_strategy.label().to_string(),
            grammar_identity: self.evidence.grammar_identity.clone(),
            attempt_id: self.evidence.attempt_id.clone(),
            completion_proof: proof,
            state_count: self.evidence.state_count,
            arc_count: self.evidence.arc_count,
            model_fingerprint: self.evidence.model_fingerprint.clone(),
            payload_fingerprint: self.evidence.payload_fingerprint.clone(),
            payload_bytes: self.payload_bytes.clone(),
        }
    }

    pub(crate) fn from_wire(wire: CompletedBackendBuildWire) -> Result<Self, CompletedBuildError> {
        let requested_strategy = strategy_from_label(&wire.requested_strategy)?;
        let realized_strategy = strategy_from_label(&wire.realized_strategy)?;
        let completion_proof = match wire.completion_proof {
            CompletionProofWire::TunedClosure {
                terminal,
                worklist_empty,
                pending_successor_count,
            } => CompletionProof::TunedClosure {
                terminal,
                worklist_empty,
                pending_successor_count,
            },
            CompletionProofWire::TemplatedFullEmission {
                uncovered_count,
                skipped_count,
            } => CompletionProof::TemplatedFullEmission {
                uncovered_count,
                skipped_count,
            },
        };
        Ok(Self {
            evidence: CompletedBackendBuildEvidence {
                requested_strategy,
                realized_strategy,
                grammar_identity: wire.grammar_identity,
                attempt_id: wire.attempt_id,
                completion_proof,
                state_count: wire.state_count,
                arc_count: wire.arc_count,
                model_fingerprint: wire.model_fingerprint,
                payload_fingerprint: wire.payload_fingerprint,
            },
            payload_bytes: wire.payload_bytes,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompletedBackendBuildWire {
    pub requested_strategy: String,
    pub realized_strategy: String,
    pub grammar_identity: String,
    pub attempt_id: String,
    pub completion_proof: CompletionProofWire,
    pub state_count: i32,
    pub arc_count: i32,
    pub model_fingerprint: String,
    pub payload_fingerprint: String,
    pub payload_bytes: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum CompletionProofWire {
    TunedClosure {
        terminal: ClosureTerminal,
        worklist_empty: bool,
        pending_successor_count: usize,
    },
    TemplatedFullEmission {
        uncovered_count: usize,
        skipped_count: usize,
    },
}

fn strategy_from_label(label: &str) -> Result<EmissionStrategy, CompletedBuildError> {
    match label {
        "tuned-surface-probed" => Ok(EmissionStrategy::TunedSurfaceProbed),
        "templated-underlying-tokens" => Ok(EmissionStrategy::TemplatedUnderlyingTokens),
        "plan-composed" => Ok(EmissionStrategy::PlanComposed),
        _ => Err(CompletedBuildError::IncompleteEvidence(format!(
            "unknown completed-build strategy label {label:?}"
        ))),
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
    IncompleteClosure {
        terminal: ClosureTerminal,
        rule_pairs_visited: usize,
        pending_successor_count: usize,
        pending_rule_ordinals: Vec<u32>,
        worklist_empty: bool,
    },
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
    AttemptMismatch {
        expected: String,
        actual: String,
    },
    StrategyMismatch {
        requested: EmissionStrategy,
        realized: EmissionStrategy,
    },
    PreferredBuildMissing(EmissionStrategy),
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
            Self::IncompleteClosure {
                terminal,
                rule_pairs_visited,
                pending_successor_count,
                pending_rule_ordinals,
                worklist_empty,
            } => write!(
                f,
                "completed-build closure is incomplete: terminal={terminal:?}, \
                 rule_pairs_visited={rule_pairs_visited}, \
                 pending_successor_count={pending_successor_count}, \
                 pending_rule_ordinals={pending_rule_ordinals:?}, \
                 worklist_empty={worklist_empty}"
            ),
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
            Self::PreferredBuildMissing(strategy) => {
                write!(
                    f,
                    "preferred completed backend build is missing for {strategy:?}"
                )
            }
            Self::NoMatchingCompletedBuild => f.write_str("no matching completed backend build"),
        }
    }
}

impl std::error::Error for CompletedBuildError {}

/// Compile one explicitly requested backend exactly once and retain its finalized binary payload.
pub fn compile_completed_backend(
    grammar: &Grammar,
    requested_strategy: EmissionStrategy,
    request: &CompileAttempt,
) -> Result<CompletedBackendBuild, CompletedBuildError> {
    let grammar_id = grammar_identity(grammar);
    match requested_strategy {
        EmissionStrategy::TunedSurfaceProbed => {
            let emitted = crate::emit::emit_tuned_surface_for_request(grammar);
            let report = emitted.report;
            let closure = report.closure_evidence.as_ref().ok_or_else(|| {
                CompletedBuildError::IncompleteEvidence(
                    "TunedSurface emitted no closure certificate".to_string(),
                )
            })?;
            validate_closure(closure)?;
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
    request: &CompileAttempt,
    expected_grammar_identity: &str,
) -> Result<SelectedBackendBuild, CompletedBuildError>
where
    I: IntoIterator<Item = CompletedBackendBuild>,
{
    let builds: Vec<_> = completed_builds.into_iter().collect();
    let Some(preferred) = selection.preferred() else {
        return Err(CompletedBuildError::NoMatchingCompletedBuild);
    };
    let build = builds
        .iter()
        .find(|build| build.evidence.realized_strategy == preferred)
        .ok_or(CompletedBuildError::PreferredBuildMissing(preferred))?;
    validate_selected_build(build, request, expected_grammar_identity)?;
    Ok(SelectedBackendBuild {
        build: build.clone(),
    })
}

fn validate_selected_build(
    build: &CompletedBackendBuild,
    request: &CompileAttempt,
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
) -> Result<(), CompletedBuildError> {
    if closure.terminal != ClosureTerminal::Complete
        || closure.evidence.pending_successor_count != 0
        || !closure.evidence.worklist_empty
    {
        return Err(CompletedBuildError::IncompleteClosure {
            terminal: closure.terminal,
            rule_pairs_visited: closure.evidence.rule_pairs_visited,
            pending_successor_count: closure.evidence.pending_successor_count,
            pending_rule_ordinals: closure.evidence.pending_rule_ordinals.clone(),
            worklist_empty: closure.evidence.worklist_empty,
        });
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
    request: &CompileAttempt,
    proposer: FomaProposer,
    model_fingerprint: String,
    completion_proof: CompletionProof,
) -> Result<CompletedBackendBuild, CompletedBuildError> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::characterization::{CharacterizationResult, ClosureEvidence};

    #[test]
    fn incomplete_closure_error_preserves_frontier_evidence() {
        let closure = CharacterizationResult {
            terminal: ClosureTerminal::Incomplete(
                crate::characterization::ClosureStopReason::WorkBudgetReached,
            ),
            evidence: ClosureEvidence {
                rule_pairs_visited: 10_000,
                synthesized_successors: 10_001,
                maximum_depth: 42,
                per_depth_counts: vec![1, 9_999],
                pending_successor_count: 3,
                pending_rule_ordinals: vec![7, 19],
                worklist_empty: false,
            },
        };

        let error = validate_closure(&closure).expect_err("closure must be rejected");
        assert_eq!(
            error,
            CompletedBuildError::IncompleteClosure {
                terminal: closure.terminal,
                rule_pairs_visited: 10_000,
                pending_successor_count: 3,
                pending_rule_ordinals: vec![7, 19],
                worklist_empty: false,
            }
        );
        let rendered = error.to_string();
        assert!(rendered.contains("WorkBudgetReached"));
        assert!(rendered.contains("rule_pairs_visited=10000"));
        assert!(rendered.contains("pending_successor_count=3"));
        assert!(rendered.contains("pending_rule_ordinals=[7, 19]"));
        assert!(rendered.contains("worklist_empty=false"));
    }
}
