//! **Protocol** (`CompileWorkerRequest`/`CompileWorkerResult`/`WorkerProtocolLimits`): a versioned,
//!   length-prefixed, bounded wire format over stdin/stdout. `read_frame` mirrors `pg-pack`'s own
//!   `format.rs` validate-before-allocate discipline verbatim: the declared length is checked
//!   against a versioned ceiling BEFORE any buffer of that size is allocated.
//! **Child** (`run_worker_child`): reads exactly one `CompileWorkerRequest` frame, loads and
//!   compiles the named grammar under the request's `ComposeBudget`, and writes one result frame
//!   (plus one length-prefixed raw payload frame for selected success). Wraps the compile call in
//!   `std::panic::catch_unwind` as best-effort panic containment only.
//!
//! # Typed outcomes -> existing health/error vocabulary (do not invent a parallel one)
//! `CompileWorkerOutcome` is the typed result the CHILD reports. Successful ordinary compiles
//! carry the real `crate::health::HealthReport` produced by
//! `crate::health_evaluator::evaluate_health`; child-level failures retain their detail and
//! protocol violations remain distinct from grammar-content failures. The health vocabulary is
//! shared with the rest of the crate; this module does not add a parallel report shape.
//!
//! # Documented gap: grammar-format dispatch duplicates `pg-cli::load_grammar`
//! `load_grammar_for_worker` re-implements the same `.xml`/`.json`/`.fwdata` three-way extension
//! dispatch `pg-cli/src/main.rs::load_grammar` already has, rather than sharing it, since
//! `pg-cli` depends on `pg-foma` (not the reverse) and this module needs to be able to load a
//! grammar entirely inside the spawned child process, independent of any `pg-cli`-specific code.
//! The two dispatch functions must be kept in sync by hand if a fourth format is ever added; flagged
//! here rather than hidden.

use std::io::{self, Read, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::analyzer::FomaError;
use crate::completed_build::{
    compile_completed_backend, sha256_hex, CompileAttempt, CompletedBackendBuildWire,
};
use crate::compose_budget::ComposeBudget;
use crate::enumerate::EmissionStrategy;
use crate::health::{
    FindingCode, HealthFinding, HealthReport, Metric, MetricValue, Phase, Severity, ValueProvenance,
};
// Protocol version and versioned wire limits, mirroring `pg_pack::format`'s `VersionLimits` shape.

/// This worker protocol's own version, carried inside every `CompileWorkerRequest`/
/// `CompileWorkerResult` (the platform-parity contract's "ONE versioned request/result
/// protocol"). Bump only on a
/// wire-incompatible change to either type.
pub const WORKER_PROTOCOL_VERSION: u32 = crate::worker_contract::PROTOCOL_VERSION;

use pg_worker_containment::ExecutionLimits;

/// Versioned, hard-coded ceilings for this protocol (design discipline shared with
/// `pg_pack::format::VersionLimits`). These bound the WIRE MESSAGES themselves (request/result JSON
/// frames), not compile execution policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkerProtocolLimits {
    /// Ceiling on one serialized `CompileWorkerRequest` frame's byte length.
    pub max_request_bytes: u64,
    /// Ceiling on one serialized `CompileWorkerResult` frame's byte length.
    pub max_result_bytes: u64,
}

/// The current protocol's limits. Deliberately generous relative to this protocol's own content (a
/// grammar file PATH plus a handful of numeric budget caps for the request; a
/// `crate::health::HealthReport` plus a few counts for the result) -- these bound the wire
/// framing itself against a hostile/malformed peer, not the compile work the framed message
/// describes (that is `ComposeBudget`'s job, checked separately, inside the child).
pub const WORKER_PROTOCOL_LIMITS: WorkerProtocolLimits = WorkerProtocolLimits {
    max_request_bytes: crate::worker_contract::PROTOCOL_LIMITS.max_request_bytes,
    max_result_bytes: crate::worker_contract::PROTOCOL_LIMITS.max_result_bytes,
};

// Length-prefixed framing: validate-before-allocate, mirroring `pg_pack::format::read_pack`.

/// Every way reading one length-prefixed frame can fail. Never a panic -- a malformed/oversized
/// peer always reaches one of these variants (this module's own version of
/// `pg_pack::format::PgPackError`, scoped to this simpler single-length framing).
#[derive(Debug)]
pub enum FrameError {
    Io(io::Error),
    /// The declared frame length exceeds this protocol version's limit, returned before any buffer of that size is allocated.
    LengthExceedsLimit {
        declared: u64,
        limit: u64,
    },
    /// The declared frame length cannot be represented by this process's `usize`.
    LengthNotAddressable { declared: u64 },
    /// The process could not reserve the declared frame body without panicking.
    AllocationFailed { declared: u64, detail: String },
    /// The frame body's bytes did not parse as valid UTF-8 JSON matching the expected type.
    Json(String),
}

impl std::fmt::Display for FrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FrameError::Io(e) => write!(f, "I/O error reading frame: {e}"),
            FrameError::LengthExceedsLimit { declared, limit } => write!(
                f,
                "declared frame length {declared} exceeds this protocol version's limit of {limit} \
                 byte(s)"
            ),
            FrameError::LengthNotAddressable { declared } => write!(
                f,
                "declared frame length {declared} cannot be represented on this platform"
            ),
            FrameError::AllocationFailed { declared, detail } => write!(
                f,
                "could not allocate declared frame length {declared}: {detail}"
            ),
            FrameError::Json(msg) => write!(f, "invalid frame JSON: {msg}"),
        }
    }
}

impl std::error::Error for FrameError {}

/// Writes `bytes` as one `[u64 little-endian length][bytes]` frame.
fn write_frame<W: Write>(w: &mut W, bytes: &[u8]) -> io::Result<()> {
    w.write_all(&(bytes.len() as u64).to_le_bytes())?;
    w.write_all(bytes)?;
    w.flush()
}

/// Reads one length-prefixed frame from `r`, rejecting a declared length above `max_len` before allocating a buffer of that size.
fn read_frame<R: Read>(r: &mut R, max_len: u64) -> Result<Vec<u8>, FrameError> {
    let len = read_frame_length(r)?;
    let len_usize = validate_frame_length(len, max_len)?;
    read_frame_body(r, len, len_usize)
}

fn read_frame_length<R: Read>(r: &mut R) -> Result<u64, FrameError> {
    let mut len_buf = [0u8; 8];
    r.read_exact(&mut len_buf).map_err(FrameError::Io)?;
    Ok(u64::from_le_bytes(len_buf))
}

fn validate_frame_length(len: u64, max_len: u64) -> Result<usize, FrameError> {
    if len > max_len {
        return Err(FrameError::LengthExceedsLimit {
            declared: len,
            limit: max_len,
        });
    }
    usize::try_from(len).map_err(|_| FrameError::LengthNotAddressable { declared: len })
}

fn read_frame_body<R: Read>(
    r: &mut R,
    declared_len: u64,
    len: usize,
) -> Result<Vec<u8>, FrameError> {
    let mut buf = Vec::new();
    buf.try_reserve_exact(len)
        .map_err(|error| FrameError::AllocationFailed {
            declared: declared_len,
            detail: error.to_string(),
        })?;
    buf.resize(len, 0);
    r.read_exact(&mut buf).map_err(FrameError::Io)?;
    Ok(buf)
}

/// Parses an already-read frame body as one `T`, shared by request and result frame readers.
fn decode_frame_body<T: for<'de> Deserialize<'de>>(body: &[u8]) -> Result<T, FrameError> {
    serde_json::from_slice(body).map_err(|e| FrameError::Json(e.to_string()))
}

// Request

/// Which of `pg-cli`'s three supported grammar-path shapes `CompileWorkerRequest::grammar_path`
/// names (mirrors `pg-cli/src/main.rs::load_grammar`'s own extension dispatch; see this module's
/// top doc "Documented gap").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrammarFormat {
    Xml,
    Json,
    Fwdata,
}

/// One versioned, bounded compile request (the platform-parity contract's "ONE versioned
/// request/result protocol"). Carries a
/// grammar-file PATH rather than embedded grammar bytes -- the worker child runs on the same host
/// and can read the file itself, keeping this frame small (well under [`WorkerProtocolLimits::
/// max_request_bytes`]) regardless of the referenced grammar's own size; the referenced grammar's
/// CONTENT is exactly what `ComposeBudget` and this module's own wall-time limit protect
/// against, not this small request message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompileWorkerRequest {
    pub protocol_version: u32,
    pub grammar_path: String,
    pub grammar_format: GrammarFormat,
    /// `ComposeBudget::chain_depth_cap` -- `None` (unbounded) by default, mirroring that field's
    /// own uncalibrated-default convention (`compose_budget.rs`'s "Chain-depth dimension" doc).
    pub chain_depth_cap: Option<usize>,
    /// Additive selected-backend payload request. `None` preserves the original worker behavior.
    #[serde(default)]
    pub(crate) selected: Option<SelectedCompileRequest>,
}

impl CompileWorkerRequest {
    /// A request for `grammar_path`/`grammar_format` and the remaining compile-time safety caps.
    pub fn new(grammar_path: impl Into<String>, grammar_format: GrammarFormat) -> Self {
        CompileWorkerRequest {
            protocol_version: WORKER_PROTOCOL_VERSION,
            grammar_path: grammar_path.into(),
            grammar_format,
            chain_depth_cap: None,
            selected: None,
        }
    }

    pub fn compose_budget(&self) -> ComposeBudget {
        let mut budget = ComposeBudget {
            chain_depth_cap: None,
        };
        if let Some(cap) = self.chain_depth_cap {
            budget = budget.with_chain_depth_cap(cap);
        }
        budget
    }
}

// Result / typed outcomes the CHILD reports.

/// One versioned compile-worker result (the metadata frame in the child's write sequence, per the
/// platform-parity contract / `run_worker_child`'s doc).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompileWorkerResult {
    pub protocol_version: u32,
    pub outcome: CompileWorkerOutcome,
}

/// Every terminal outcome the CHILD itself can observe and report. External process containment
/// remains the caller's responsibility for failures that prevent a child result from being sent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CompileWorkerOutcome {
    /// The compile completed under budget, carrying the final state/arc counts and the real `HealthReport` from `crate::health_evaluator::evaluate_health`.
    Success {
        final_state_count: Option<i64>,
        final_arc_count: Option<i64>,
        uncovered_count: usize,
        health: HealthReport,
    },
    /// Emission was unsupported/incomplete, or its lexc source failed to compile; never a usable artifact.
    CompileFailed {
        detail: String,
        health: HealthReport,
    },
    /// The named grammar file could not be read, parsed, or compiled into a `pg_grammar::model::Grammar` at all.
    GrammarLoadFailed { detail: String },
    /// The request frame itself was malformed (wrong `protocol_version`, or valid JSON of the wrong shape) -- distinct from a grammar-content problem.
    ProtocolViolation { detail: String },
    /// One exact, fully evidenced selected backend payload produced by the child. The payload is
    /// sent as a separate raw frame immediately after this result frame.
    SelectedSuccess {
        build: CompletedBackendBuildWire,
        payload_byte_len: u64,
        payload_sha256: String,
    },
    /// The selected backend produced a finalized payload larger than the configured serialized
    /// FST execution limit; no raw payload frame is published.
    SelectedExecutionLimitExceeded { actual_bytes: u64, limit_bytes: u64 },
    /// A selected-backend compile failed before a trusted payload could be returned.
    SelectedCompileFailed { detail: String },
}

/// Additional request material for the selected-payload seam.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SelectedCompileRequest {
    pub(crate) attempt_id: String,
    pub(crate) route: String,
    pub(crate) max_serialized_fst_bytes: u64,
}

/// Loads and compiles `grammar_path` into a `pg_grammar::model::Grammar`, mirroring `pg-cli::load_grammar`'s extension dispatch and `Result<_, String>` error shape.
fn load_grammar_for_worker(
    path: &str,
    format: GrammarFormat,
) -> Result<pg_grammar::model::Grammar, String> {
    match format {
        GrammarFormat::Xml => {
            let xml = std::fs::read_to_string(path).map_err(|e| format!("read {path}: {e}"))?;
            pg_grammar::load(&xml).map_err(|e| format!("load {path}: {e:?}"))
        }
        GrammarFormat::Json => {
            let json = std::fs::read_to_string(path).map_err(|e| format!("read {path}: {e}"))?;
            let snapshot = pg_snapshot::Snapshot::from_json(&json)
                .map_err(|e| format!("parse snapshot {path}: {e}"))?;
            let (grammar, _warnings) = pg_grammar::compile_project(&snapshot)
                .map_err(|e| format!("compile {path}: {e:?}"))?;
            Ok(grammar)
        }
        GrammarFormat::Fwdata => {
            let (snapshot, _report) = pg_fwdata::import_file(Path::new(path))
                .map_err(|e| format!("import {path}: {e}"))?;
            let (grammar, _warnings) = pg_grammar::compile_project(&snapshot)
                .map_err(|e| format!("compile {path}: {e:?}"))?;
            Ok(grammar)
        }
    }
}

/// Loads `request`'s grammar and runs it through `FomaProposer::new_with_budget_and_profile` under `request`'s own `ComposeBudget` -- the same production path, wrapped in `catch_unwind` as best-effort panic containment only (does not protect against stack overflow or allocator OOM).
fn compile_grammar_from_request(request: &CompileWorkerRequest) -> CompileWorkerOutcome {
    let grammar = match load_grammar_for_worker(&request.grammar_path, request.grammar_format) {
        Ok(g) => g,
        Err(detail) => return CompileWorkerOutcome::GrammarLoadFailed { detail },
    };

    let compose_budget = request.compose_budget();

    let compiled = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        crate::analyzer::FomaProposer::new_with_budget_and_profile(
            &grammar,
            &compose_budget,
        )
    }));

    let (result, profile) = match compiled {
        Ok(pair) => pair,
        Err(_) => {
            let detail = "the compile call panicked inside the worker process (caught here by \
                catch_unwind; this does not protect against stack overflow or allocator OOM, \
                which abort the process outright and must be observed by an external process \
                boundary)"
                .to_string();
            return CompileWorkerOutcome::CompileFailed {
                health: build_process_failure_health(detail.clone()),
                detail,
            };
        }
    };

    match result {
        Ok(proposer) => {
            let report = proposer
                .report
                .as_ref()
                .expect("FomaProposer::new always runs the tuned emitter and supplies its report");
            let health = crate::health_evaluator::evaluate_health(
                None,
                Some(report),
                &[],
                &[],
            );
            CompileWorkerOutcome::Success {
                final_state_count: profile.final_state_count,
                final_arc_count: profile.final_arc_count,
                uncovered_count: report.uncovered.len(),
                health,
            }
        }
        Err(
            err @ (FomaError::LexcCompileFailed(_)
            | FomaError::Unsupported(_)
            | FomaError::Incomplete(_)),
        ) => {
            let health = crate::health_evaluator::evaluate_foma_error(&err);
            CompileWorkerOutcome::CompileFailed {
                detail: err.to_string(),
                health,
            }
        }
    }
}

fn strategy_from_worker_route(route: &str) -> Result<EmissionStrategy, String> {
    match route {
        "tuned-surface-probed" => Ok(EmissionStrategy::TunedSurfaceProbed),
        "templated-underlying-tokens" => Ok(EmissionStrategy::TemplatedUnderlyingTokens),
        "plan-composed" => Ok(EmissionStrategy::PlanComposed),
        _ => Err(format!("unknown selected backend route {route:?}")),
    }
}

/// Carries a selected payload until the result header and optional raw frame are written.
#[derive(Debug)]
struct WorkerChildOutput {
    outcome: CompileWorkerOutcome,
    selected_payload: Option<Vec<u8>>,
}

/// Finalize the selected result and enforce the serialized-payload limit exactly once.
fn finish_selected_payload(
    build: CompletedBackendBuildWire,
    payload: Vec<u8>,
    limit_bytes: u64,
) -> WorkerChildOutput {
    let actual_bytes = payload.len() as u64;
    if actual_bytes > limit_bytes {
        return WorkerChildOutput {
            outcome: CompileWorkerOutcome::SelectedExecutionLimitExceeded {
                actual_bytes,
                limit_bytes,
            },
            selected_payload: None,
        };
    }

    let payload_sha256 = sha256_hex(&payload);
    WorkerChildOutput {
        outcome: CompileWorkerOutcome::SelectedSuccess {
            build,
            payload_byte_len: actual_bytes,
            payload_sha256,
        },
        selected_payload: Some(payload),
    }
}

fn compile_selected_from_request(
    request: &CompileWorkerRequest,
    selected: &SelectedCompileRequest,
    limits: &ExecutionLimits,
) -> WorkerChildOutput {
    let outcome = (|| {
        let strategy = match strategy_from_worker_route(&selected.route) {
            Ok(strategy) => strategy,
            Err(detail) => {
                return WorkerChildOutput {
                    outcome: CompileWorkerOutcome::SelectedCompileFailed { detail },
                    selected_payload: None,
                }
            }
        };
        let private_request = match CompileAttempt::from_worker_wire(selected.attempt_id.clone()) {
            Ok(request) => request,
            Err(detail) => {
                return WorkerChildOutput {
                    outcome: CompileWorkerOutcome::SelectedCompileFailed { detail },
                    selected_payload: None,
                }
            }
        };
        let grammar = match load_grammar_for_worker(&request.grammar_path, request.grammar_format) {
            Ok(grammar) => grammar,
            Err(detail) => {
                return WorkerChildOutput {
                    outcome: CompileWorkerOutcome::SelectedCompileFailed { detail },
                    selected_payload: None,
                }
            }
        };
        let completed = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            compile_completed_backend(&grammar, strategy, &private_request)
        })) {
            Ok(Ok(build)) => build,
            Ok(Err(error)) => {
                return WorkerChildOutput {
                    outcome: CompileWorkerOutcome::SelectedCompileFailed {
                        detail: error.to_string(),
                    },
                    selected_payload: None,
                }
            }
            Err(_) => {
                return WorkerChildOutput {
                    outcome: CompileWorkerOutcome::SelectedCompileFailed {
                        detail: "selected backend compile panicked inside the worker process"
                            .to_string(),
                    },
                    selected_payload: None,
                }
            }
        };
        let (wire, payload_bytes) = completed.into_wire_and_payload();
        finish_selected_payload(
            wire,
            payload_bytes,
            limits.max_serialized_fst_bytes(),
        )
    })();
    outcome
}

/// The worker CHILD's entry point: reads exactly one `CompileWorkerRequest` frame from `input`, compiles it,
/// and writes one `CompileWorkerResult` frame to `output` (plus one raw payload frame for selected
/// success). Never panics on malformed
/// input -- an oversized/malformed request frame is reported as
/// `CompileWorkerOutcome::ProtocolViolation`, not a crash, so a hostile/buggy parent still gets a
/// clean typed response instead of an opaque non-zero exit.
///
/// Generic over `Read`/`Write` so this same function is both the real production child (`io::
/// stdin()`/`io::stdout()`, wired by `pg-cli`'s hidden subcommand) and directly unit-testable
/// in-process against an in-memory buffer (no subprocess needed to test the protocol and
/// compile-outcome mapping logic).
pub fn run_worker_child<R: Read, W: Write>(mut input: R, mut output: W) -> io::Result<()> {
    let limits = WORKER_PROTOCOL_LIMITS;

    let request_bytes = match read_frame(&mut input, limits.max_request_bytes) {
        Ok(bytes) => bytes,
        Err(e) => {
            return write_result(
                &mut output,
                &CompileWorkerResult {
                    protocol_version: WORKER_PROTOCOL_VERSION,
                    outcome: CompileWorkerOutcome::ProtocolViolation {
                        detail: format!("malformed/oversized request frame: {e}"),
                    },
                },
            );
        }
    };

    let request: CompileWorkerRequest = match decode_frame_body(&request_bytes) {
        Ok(r) => r,
        Err(e) => {
            return write_result(
                &mut output,
                &CompileWorkerResult {
                    protocol_version: WORKER_PROTOCOL_VERSION,
                    outcome: CompileWorkerOutcome::ProtocolViolation {
                        detail: format!("malformed request JSON: {e}"),
                    },
                },
            );
        }
    };

    if request.protocol_version != WORKER_PROTOCOL_VERSION {
        return write_result(
            &mut output,
            &CompileWorkerResult {
                protocol_version: WORKER_PROTOCOL_VERSION,
                outcome: CompileWorkerOutcome::ProtocolViolation {
                    detail: format!(
                        "unsupported protocol_version {} (this worker understands {})",
                        request.protocol_version, WORKER_PROTOCOL_VERSION
                    ),
                },
            },
        );
    }

    let child_output = match request.selected.as_ref() {
        Some(selected) => {
            let selected_limits =
                match ExecutionLimits::for_selected_payload(selected.max_serialized_fst_bytes) {
                    Ok(limits) => limits,
                    Err(error) => {
                        return write_result(
                            &mut output,
                            &CompileWorkerResult {
                                protocol_version: WORKER_PROTOCOL_VERSION,
                                outcome: CompileWorkerOutcome::ProtocolViolation {
                                    detail: format!(
                                        "invalid selected serialized-FST execution limit: {error}"
                                    ),
                                },
                            },
                        )
                    }
                };
            compile_selected_from_request(&request, selected, &selected_limits)
        }
        None => WorkerChildOutput {
            outcome: compile_grammar_from_request(&request),
            selected_payload: None,
        },
    };
    write_child_output(&mut output, child_output)
}

fn bounded_result_json(result: &CompileWorkerResult) -> (Vec<u8>, bool) {
    let json = serde_json::to_vec(result).expect("CompileWorkerResult always serializes");
    if json.len() as u64 <= WORKER_PROTOCOL_LIMITS.max_result_bytes {
        return (json, false);
    }

    let fallback = CompileWorkerResult {
        protocol_version: WORKER_PROTOCOL_VERSION,
        outcome: CompileWorkerOutcome::ProtocolViolation {
            detail: "worker result metadata exceeds the protocol limit".to_string(),
        },
    };
    let fallback_json = serde_json::to_vec(&fallback).expect("protocol violation serializes");
    debug_assert!(fallback_json.len() as u64 <= WORKER_PROTOCOL_LIMITS.max_result_bytes);
    (fallback_json, true)
}

fn write_result<W: Write>(output: &mut W, result: &CompileWorkerResult) -> io::Result<()> {
    let (json, _) = bounded_result_json(result);
    write_frame(output, &json)
}

fn write_child_output<W: Write>(
    output: &mut W,
    child_output: WorkerChildOutput,
) -> io::Result<()> {
    let result = CompileWorkerResult {
        protocol_version: WORKER_PROTOCOL_VERSION,
        outcome: child_output.outcome.clone(),
    };
    let (json, metadata_overflowed) = bounded_result_json(&result);
    write_frame(output, &json)?;
    if !metadata_overflowed {
        if let Some(payload) = child_output.selected_payload {
            write_frame(output, &payload)?;
        }
    }
    Ok(())
}

fn build_process_failure_health(detail: String) -> HealthReport {
    HealthReport::new(vec![HealthFinding {
        code: FindingCode::BuildProcessFailed,
        severity: Severity::NotProductionReady,
        phase: Phase::Compile,
        affected: Vec::new(),
        metric: Metric::UnknownUnboundedWork,
        value: MetricValue::Unbounded,
        provenance: ValueProvenance::Observed,
        threshold: None,
        explanation: detail,
        remedies: Vec::new(),
    }])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn selected_build(payload_fingerprint: String) -> CompletedBackendBuildWire {
        CompletedBackendBuildWire {
            requested_strategy: "templated-underlying-tokens".to_string(),
            realized_strategy: "templated-underlying-tokens".to_string(),
            grammar_identity: "grammar".to_string(),
            attempt_id: "attempt".to_string(),
            completion_proof: crate::completed_build::CompletionProofWire::TemplatedFullEmission {
                uncovered_count: 0,
                skipped_count: 0,
            },
            state_count: 1,
            arc_count: 1,
            model_fingerprint: "model".to_string(),
            payload_fingerprint,
        }
    }

    fn selected_success_with(
        payload_byte_len: u64,
        payload_sha256: String,
        payload_fingerprint: String,
    ) -> CompileWorkerResult {
        CompileWorkerResult {
            protocol_version: WORKER_PROTOCOL_VERSION,
            outcome: CompileWorkerOutcome::SelectedSuccess {
                build: selected_build(payload_fingerprint),
                payload_byte_len,
                payload_sha256,
            },
        }
    }

    fn selected_success(payload: &[u8]) -> CompileWorkerResult {
        let digest = sha256_hex(payload);
        selected_success_with(
            payload.len() as u64,
            digest.clone(),
            digest,
        )
    }

    #[test]
    fn selected_payload_exactly_at_limit_is_completed() {
        let payload = b"four".to_vec();

        let output = finish_selected_payload(
            selected_build(sha256_hex(&payload)),
            payload.clone(),
            payload.len() as u64,
        );

        assert!(matches!(
            output,
            WorkerChildOutput {
                outcome: CompileWorkerOutcome::SelectedSuccess { payload_byte_len: 4, .. },
                selected_payload: Some(actual),
            } if actual == payload
        ));
    }

    #[test]
    fn selected_payload_one_byte_over_limit_emits_no_raw_frame() {
        let payload = b"four".to_vec();

        let output = finish_selected_payload(
            selected_build(sha256_hex(&payload)),
            payload,
            3,
        );

        assert!(matches!(
            output,
            WorkerChildOutput {
                outcome: CompileWorkerOutcome::SelectedExecutionLimitExceeded {
                    actual_bytes: 4,
                    limit_bytes: 3,
                },
                selected_payload: None,
            }
        ));
    }

    #[test]
    fn selected_request_json_contains_no_filesystem_destination() {
        let request = SelectedCompileRequest {
            attempt_id: "attempt-0123456789abcdef0123456789abcdef".to_string(),
            route: "tuned-surface-probed".to_string(),
            max_serialized_fst_bytes: 4,
        };

        let json = serde_json::to_value(request).expect("serialize selected request");

        assert!(json.get("artifact_path").is_none());
        assert!(json.get("artifact_directory").is_none());
        assert!(json.get("artifact_token").is_none());
    }

    #[test]
    fn selected_success_json_contains_transport_metadata_but_no_payload() {
        let payload = b"fst!";
        let result = CompileWorkerResult {
            protocol_version: WORKER_PROTOCOL_VERSION,
            outcome: selected_success(payload).outcome,
        };

        let json = serde_json::to_value(result).expect("serialize selected success");

        assert_eq!(
            json["outcome"]["SelectedSuccess"]["payload_byte_len"],
            4
        );
        assert_eq!(
            json["outcome"]["SelectedSuccess"]["payload_sha256"],
            sha256_hex(payload)
        );
        assert!(
            !json.to_string().contains("payload_bytes"),
            "selected result must contain metadata only: {json}"
        );
    }

    // Framing: bounded, validate-before-allocate, mirroring `pg_pack::format`'s own test shapes.

    #[test]
    fn frame_round_trips_small_payload() {
        let mut buf = Vec::new();
        write_frame(&mut buf, b"synthetic-payload").expect("write");
        let mut cursor = std::io::Cursor::new(buf);
        let read = read_frame(&mut cursor, 1024).expect("read");
        assert_eq!(read, b"synthetic-payload");
    }

    #[test]
    fn read_frame_rejects_declared_length_over_limit_before_allocating() {
        // Declares a length far beyond any reasonable limit with no payload bytes following, so an allocate-before-validate bug would attempt a many-exabyte allocation instead of a clean error.
        let mut buf = Vec::new();
        buf.extend_from_slice(&u64::MAX.to_le_bytes());
        let mut cursor = std::io::Cursor::new(buf);
        let err = read_frame(&mut cursor, 1024).expect_err("must reject before allocating");
        match err {
            FrameError::LengthExceedsLimit { declared, limit } => {
                assert_eq!(declared, u64::MAX);
                assert_eq!(limit, 1024);
            }
            other => panic!("expected LengthExceedsLimit, got {other:?}"),
        }
    }

    #[test]
    fn read_frame_rejects_length_exceeding_protocol_limit_with_short_buffer() {
        let mut buf = Vec::new();
        let huge = WORKER_PROTOCOL_LIMITS.max_request_bytes + 1;
        buf.extend_from_slice(&huge.to_le_bytes());
        // No body bytes -- proves rejection happens strictly from the header, before any attempt to `read_exact` a body.
        let mut cursor = std::io::Cursor::new(buf);
        let err = read_frame(&mut cursor, WORKER_PROTOCOL_LIMITS.max_request_bytes)
            .expect_err("must reject oversized declared length");
        assert!(matches!(err, FrameError::LengthExceedsLimit { .. }));
    }

    #[test]
    fn decode_frame_body_rejects_malformed_json() {
        let err = decode_frame_body::<CompileWorkerRequest>(b"not valid json at all { { {")
            .expect_err("malformed JSON must not deserialize");
        assert!(matches!(err, FrameError::Json(_)));
    }

    #[test]
    fn selected_request_rejects_removed_envelope_fields() {
        let error = serde_json::from_value::<SelectedCompileRequest>(serde_json::json!({
            "attempt_id": "attempt-test",
            "route": "tuned-surface-probed",
            "artifact_directory": "reserved-directory",
            "max_serialized_fst_bytes": 4,
            "envelope_id": "managed-v1",
        }))
        .expect_err("removed envelope fields must be rejected as unknown");
        assert!(
            error.to_string().contains("unknown field"),
            "expected deny_unknown_fields rejection, got {error}"
        );
    }

    // `run_worker_child` in-process: protocol handling plus the grammar-content outcomes reachable without a real adversarial grammar.

    fn call_child(request_bytes: &[u8]) -> CompileWorkerResult {
        let mut input = std::io::Cursor::new(request_bytes.to_vec());
        let mut output = Vec::new();
        run_worker_child(&mut input, &mut output).expect(
            "run_worker_child must not I/O-error \
            against in-memory buffers",
        );
        let len = u64::from_le_bytes(output[0..8].try_into().unwrap()) as usize;
        serde_json::from_slice(&output[8..8 + len]).expect("result must deserialize")
    }

    #[test]
    fn run_worker_child_reports_protocol_violation_for_oversized_request_frame() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&(WORKER_PROTOCOL_LIMITS.max_request_bytes + 1).to_le_bytes());
        let result = call_child(&buf);
        match result.outcome {
            CompileWorkerOutcome::ProtocolViolation { detail } => {
                assert!(detail.contains("oversized") || detail.contains("exceeds"));
            }
            other => panic!("expected ProtocolViolation, got {other:?}"),
        }
    }

    #[test]
    fn run_worker_child_reports_protocol_violation_for_malformed_request_json() {
        let mut buf = Vec::new();
        let body = b"not json";
        buf.extend_from_slice(&(body.len() as u64).to_le_bytes());
        buf.extend_from_slice(body);
        let result = call_child(&buf);
        assert!(matches!(
            result.outcome,
            CompileWorkerOutcome::ProtocolViolation { .. }
        ));
    }

    #[test]
    fn run_worker_child_reports_protocol_violation_for_wrong_protocol_version() {
        let mut request = CompileWorkerRequest::new("does-not-matter.xml", GrammarFormat::Xml);
        request.protocol_version = WORKER_PROTOCOL_VERSION + 1;
        let json = serde_json::to_vec(&request).unwrap();
        let mut buf = Vec::new();
        buf.extend_from_slice(&(json.len() as u64).to_le_bytes());
        buf.extend_from_slice(&json);
        let result = call_child(&buf);
        match result.outcome {
            CompileWorkerOutcome::ProtocolViolation { detail } => {
                assert!(detail.contains("protocol_version"));
            }
            other => panic!("expected ProtocolViolation, got {other:?}"),
        }
    }

    #[test]
    fn run_worker_child_reports_grammar_load_failed_for_missing_file() {
        let request =
            CompileWorkerRequest::new("this-file-does-not-exist-synthetic.xml", GrammarFormat::Xml);
        let json = serde_json::to_vec(&request).unwrap();
        let mut buf = Vec::new();
        buf.extend_from_slice(&(json.len() as u64).to_le_bytes());
        buf.extend_from_slice(&json);
        let result = call_child(&buf);
        assert!(matches!(
            result.outcome,
            CompileWorkerOutcome::GrammarLoadFailed { .. }
        ));
    }

    fn scratch_grammar_file(tag: &str, xml: &str) -> std::path::PathBuf {
        static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "pg-foma-worker-test-{tag}-{}-{n}.xml",
            std::process::id()
        ));
        std::fs::write(&path, xml).expect("write scratch grammar");
        path
    }

    #[test]
    fn run_worker_child_reports_success_for_a_clean_small_grammar() {
        const CLEAN_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>WorkerSuccessFixture</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="table1">
      <Name>Orthography</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="segA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="segK"><Representations><Representation>k</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="segT"><Representations><Representation>t</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses></NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="table1">
        <Name>main</Name>
        <LexicalEntries>
          <LexicalEntry id="e1">
            <Allomorphs><Allomorph id="e1-1"><PhoneticShape>kat</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>kat</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;
        let path = scratch_grammar_file("success", CLEAN_XML);
        let request =
            CompileWorkerRequest::new(path.to_string_lossy().into_owned(), GrammarFormat::Xml);
        let json = serde_json::to_vec(&request).unwrap();
        let mut buf = Vec::new();
        buf.extend_from_slice(&(json.len() as u64).to_le_bytes());
        buf.extend_from_slice(&json);

        let result = call_child(&buf);
        match result.outcome {
            CompileWorkerOutcome::Success { health, .. } => {
                assert_eq!(health.admission(), Severity::WithinLimits);
            }
            other => panic!("expected Success, got {other:?}"),
        }
        let _ = std::fs::remove_file(&path);
    }

}
