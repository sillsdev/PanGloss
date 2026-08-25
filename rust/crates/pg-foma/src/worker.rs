//! Compile-worker supervisor subsystem. Compile-time work runs in a killable native
//! worker process under the parent supervisor, distinct
//! from `crate::compose_budget`'s in-process cooperative APPLY-side budgets, under which
//! apply-time (word analysis) runs in-process. Do not add
//! anything here that touches per-word `propose`/`apply_up` -- that is `compose_budget.rs`'s
//! `ApplyBudget`/`ApplyOutcome`, unchanged by this module.
//!
//! # The contract this module implements
//! **Platform parity.** Windows and Linux are equal,
//! first-class native production targets. Both use ONE compiler worker, ONE versioned
//! request/result protocol, standard-library `Child::try_wait`/`Child::kill` wall-time control,
//! compiler budgets, and bounded input/output. Broader host process-tree policy is outside this
//! wire-and-artifact contract; callers may provide that policy around this one-worker seam. WASM
//! is analysis-only and needs no compile supervisor.
//!
//! **Fast-failure primacy.** Deterministic logical counters are the primary fast-failure mechanism;
//! cooperative
//! elapsed checks and the parent wall timeout are outer safeguards. This module's wall-clock check
//! is exactly that outer safeguard -- `CompileWorkerRequest::compose_budget` is what the
//! child compiles UNDER (the same `crate::compose_budget::ComposeBudget` every other production
//! call site uses), never a substitute for it.
//!
//! This module is the
//! compile half of the compile-vs-apply split; `compose_budget.rs`'s own module doc names the exact gap this module closes:
//! "Full 'never blow up' for a single adversarial call needs an external supervisor process." This
//! module is that external supervisor process.
//!
//! # Why this whole module is non-wasm only
//! `#[cfg(not(target_arch = "wasm32"))]`-gated in `lib.rs`, and its three extra dependencies
//! (`pg-snapshot`, `pg-fwdata`) are scoped to the identical target cfg in this crate's
//! `Cargo.toml` -- not merely dead code on wasm32, genuinely absent from `pg-wasm`'s dependency
//! graph, mirroring the contract above: "WASM is analysis-only and needs no compile supervisor." `wasm32-unknown-
//! unknown` has no `std::process::Command`, so a process-spawning supervisor cannot exist there by
//! construction, not merely by choice.
//!
//! # Three pieces
//! - **Protocol** (`CompileWorkerRequest`/`CompileWorkerResult`/`WorkerProtocolLimits`): a versioned,
//!   length-prefixed, bounded wire format over stdin/stdout. `read_frame` mirrors `pg-pack`'s own
//!   `format.rs` validate-before-allocate discipline verbatim: the declared length is checked
//!   against a versioned ceiling BEFORE any buffer of that size is allocated.
//! - **Child** (`run_worker_child`): reads exactly one `CompileWorkerRequest` frame, loads and
//!   compiles the named grammar under the request's `ComposeBudget`, and writes exactly one
//!   `CompileWorkerResult` frame. Wraps the compile call in `std::panic::catch_unwind` --
//!   best-effort only; `compose_budget.rs`'s own doc is explicit that "stack-overflow and
//!   allocator-OOM abort the process, bypassing every check" including `catch_unwind` -- that is
//!   exactly why the supervisor below exists: it observes the whole child PROCESS, not just one
//!   `Result`.
//! - **Supervisor** (`run_compile_worker`): spawns a child process (`std::process::Command`),
//!   writes the request, drains stdout/stderr on capped reader threads, and polls
//!   `Child::try_wait` in a loop that checks a wall deadline -- killing the child (`Child::kill`)
//!   and returning a typed `WorkerOutcome` the
//!   instant any bound is breached. This module uses only the standard-library child seam; host
//!   process-tree policy remains the caller's responsibility.
//!
//! # Typed outcomes -> existing health/error vocabulary (do not invent a parallel one)
//! `CompileWorkerOutcome` (what the CHILD reports) reuses `crate::compose_budget::ComposeError`
//! verbatim for a real budget trip (today, the ordering-multiplicity dimension --
//! `crate::analyzer::FomaProposer::new_with_budget_and_profile` is the one production call site
//! that can return `Err` from an actual `crate::compose_budget::ComposeError`-carrying
//! `crate::analyzer::FomaError` variant before ever handing lexc to the foma compiler) and feeds
//! every measurement into `crate::health_evaluator::evaluate_health` to build a real
//! `crate::health::HealthReport` -- never a second, parallel report shape. `WorkerOutcome`
//! (what the PARENT reports for outcomes the child never got to write -- a wall-timeout kill, a
//! flooded pipe, a crash, a malformed protocol message) maps each into the SAME
//! `crate::health::HealthReport`/`crate::health::HealthFinding` vocabulary via
//! `WorkerOutcome::health_report`. A genuine external-monitor abort (wall-timeout, output
//! cap, child crash) uses `crate::health::FindingCode::HostContainmentFired`; a spawn/protocol
//! fault where no child ever ran to be contained (`SpawnFailed`, `ProtocolViolation`) is instead a
//! build-process fault and uses `crate::health::FindingCode::BuildProcessFailed` (see
//! `WorkerOutcome::health_report`'s own doc for the reasoning). The health vocabulary remains
//! shared with the rest of the crate; this module does not add a parallel report shape.
//!
//! # Opt-in, additive, default path unchanged
//! Nothing in this module is called by `crate::analyzer::FomaProposer::new`/`new_with_profile`,
//! `crate::composite::FomaAnalyzer::new`, or any other existing production entry point --
//! spawning a worker is something a caller (`pg-cli`'s pack path and hidden
//! `__compile-worker-child` subcommand) opts into explicitly. The in-process compile path's
//! behavior, output, and exit codes are unchanged by this module's mere existence.
//!
//! # Documented gap: grammar-format dispatch duplicates `pg-cli::load_grammar`
//! `load_grammar_for_worker` re-implements the same `.xml`/`.json`/`.fwdata` three-way extension
//! dispatch `pg-cli/src/main.rs::load_grammar` already has, rather than sharing it, since
//! `pg-cli` depends on `pg-foma` (not the reverse) and this module needs to be able to load a
//! grammar entirely inside the spawned child process, independent of any `pg-cli`-specific code.
//! The two dispatch functions must be kept in sync by hand if a fourth format is ever added; flagged
//! here rather than hidden.

use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::analyzer::FomaError;
use crate::backend_selection::BackendSelection;
use crate::completed_build::{
    compile_completed_backend, select_completed_build, sha256_hex, CompileAttempt,
    CompletedBackendBuildWire,
};
use crate::compose_budget::ComposeBudget;
use crate::enumerate::EmissionStrategy;
use crate::health::{
    FindingCode, HealthFinding, HealthReport, Metric, MetricValue, Phase, Severity, ValueProvenance,
};
use pg_grammar::model::Grammar;

// Protocol version and versioned wire limits, mirroring `pg_pack::format`'s `VersionLimits` shape.

/// This worker protocol's own version, carried inside every `CompileWorkerRequest`/
/// `CompileWorkerResult` (the platform-parity contract's "ONE versioned request/result
/// protocol"). Bump only on a
/// wire-incompatible change to either type.
pub const WORKER_PROTOCOL_VERSION: u32 = crate::worker_contract::PROTOCOL_VERSION;

/// Finite, caller-configurable execution limits for one supervised worker attempt.
///
/// These values describe external containment policy. They are configuration only in this
/// contract slice; enforcement and provenance wiring remain separate work.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExecutionLimits {
    max_serialized_fst_bytes: u64,
    max_committed_memory_bytes: u64,
    max_wall_time: Duration,
}

/// The reason an execution limit configuration was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionLimitError {
    ZeroSerializedFstBytes,
    ZeroCommittedMemoryBytes,
    ZeroWallTime,
}

impl std::fmt::Display for ExecutionLimitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let field = match self {
            Self::ZeroSerializedFstBytes => "max_serialized_fst_bytes",
            Self::ZeroCommittedMemoryBytes => "max_committed_memory_bytes",
            Self::ZeroWallTime => "max_wall_time",
        };
        write!(f, "{field} must be positive")
    }
}

impl std::error::Error for ExecutionLimitError {}

/// Ratified finite defaults: 1 GiB serialized payload, 10 GiB committed memory, and 10 minutes.
pub const DEFAULT_EXECUTION_LIMITS: ExecutionLimits = ExecutionLimits {
    max_serialized_fst_bytes: 1024 * 1024 * 1024,
    max_committed_memory_bytes: 10 * 1024 * 1024 * 1024,
    max_wall_time: Duration::from_secs(10 * 60),
};

impl ExecutionLimits {
    /// Creates an execution-limit configuration; every dimension must be positive.
    pub fn try_new(
        max_serialized_fst_bytes: u64,
        max_committed_memory_bytes: u64,
        max_wall_time: Duration,
    ) -> Result<Self, ExecutionLimitError> {
        if max_serialized_fst_bytes == 0 {
            return Err(ExecutionLimitError::ZeroSerializedFstBytes);
        }
        if max_committed_memory_bytes == 0 {
            return Err(ExecutionLimitError::ZeroCommittedMemoryBytes);
        }
        if max_wall_time == Duration::ZERO {
            return Err(ExecutionLimitError::ZeroWallTime);
        }
        Ok(Self {
            max_serialized_fst_bytes,
            max_committed_memory_bytes,
            max_wall_time,
        })
    }

    pub const fn max_serialized_fst_bytes(self) -> u64 {
        self.max_serialized_fst_bytes
    }

    pub const fn max_committed_memory_bytes(self) -> u64 {
        self.max_committed_memory_bytes
    }

    pub const fn max_wall_time(self) -> Duration {
        self.max_wall_time
    }

    /// Constructs the child-side serialized-FST limit.
    fn for_selected_payload(max_serialized_fst_bytes: u64) -> Result<Self, ExecutionLimitError> {
        Self::try_new(
            max_serialized_fst_bytes,
            DEFAULT_EXECUTION_LIMITS.max_committed_memory_bytes,
            DEFAULT_EXECUTION_LIMITS.max_wall_time,
        )
    }
}

impl Default for ExecutionLimits {
    fn default() -> Self {
        DEFAULT_EXECUTION_LIMITS
    }
}

/// Versioned, hard-coded ceilings for this protocol (design discipline shared with
/// `pg_pack::format::VersionLimits`). These bound the WIRE MESSAGES themselves (request/result JSON
/// frames and captured stderr), not compile execution policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkerProtocolLimits {
    /// Ceiling on one serialized `CompileWorkerRequest` frame's byte length.
    pub max_request_bytes: u64,
    /// Ceiling on one serialized `CompileWorkerResult` frame's byte length.
    pub max_result_bytes: u64,
    /// Ceiling on total captured stderr bytes the supervisor retains from the child.
    pub max_captured_stderr_bytes: u64,
}

/// The current protocol's limits. Deliberately generous relative to this protocol's own content (a
/// grammar file PATH plus a handful of numeric budget caps for the request; a
/// `crate::health::HealthReport` plus a few counts for the result) -- these bound the wire
/// framing itself against a hostile/malformed peer, not the compile work the framed message
/// describes (that is `ComposeBudget`'s job, checked separately, inside the child).
pub const WORKER_PROTOCOL_LIMITS: WorkerProtocolLimits = WorkerProtocolLimits {
    max_request_bytes: crate::worker_contract::PROTOCOL_LIMITS.max_request_bytes,
    max_result_bytes: crate::worker_contract::PROTOCOL_LIMITS.max_result_bytes,
    max_captured_stderr_bytes: crate::worker_contract::PROTOCOL_LIMITS.max_captured_stderr_bytes,
};

/// Looks up the versioned limits for a protocol version. `None` for any version this build
/// doesn't understand.
pub const fn limits_for_version(version: u32) -> Option<WorkerProtocolLimits> {
    match version {
        WORKER_PROTOCOL_VERSION => Some(WORKER_PROTOCOL_LIMITS),
        _ => None,
    }
}

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
    let mut len_buf = [0u8; 8];
    r.read_exact(&mut len_buf).map_err(FrameError::Io)?;
    let len = u64::from_le_bytes(len_buf);
    if len > max_len {
        return Err(FrameError::LengthExceedsLimit {
            declared: len,
            limit: max_len,
        });
    }
    // Only allocated AFTER the ceiling check above has passed.
    let mut buf = vec![0u8; len as usize];
    r.read_exact(&mut buf).map_err(FrameError::Io)?;
    Ok(buf)
}

/// Parses an already-read frame body as one `T`; shared by `read_frame` and `parse_result_frame`'s incrementally accumulated buffers.
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
    /// `ComposeBudget::state_cap`.
    pub state_cap: usize,
    /// `ComposeBudget::arc_cap`.
    pub arc_cap: usize,
    /// `ComposeBudget::tuple_cap`.
    pub tuple_cap: usize,
    /// `ComposeBudget::group_cap`.
    pub group_cap: usize,
    /// `ComposeBudget::line_cap`.
    pub line_cap: usize,
    /// `ComposeBudget::chain_depth_cap` -- `None` (unbounded) by default, mirroring that field's
    /// own uncalibrated-default convention (`compose_budget.rs`'s "Chain-depth dimension" doc).
    pub chain_depth_cap: Option<usize>,
    /// `ComposeBudget::ordering_multiplicity_cap`.
    pub ordering_multiplicity_cap: Option<usize>,
    /// Additive selected-backend payload request. `None` preserves the original worker behavior.
    #[serde(default)]
    pub(crate) selected: Option<SelectedCompileRequest>,
}

impl CompileWorkerRequest {
    /// A request for `grammar_path`/`grammar_format` under this crate's own documented DEFAULT
    /// compose-budget caps (`compose_budget::DEFAULT_*` -- the same defaults
    /// `ComposeBudget::from_env` falls back to when no `HC_COMPOSE_*` env var is set), explicit
    /// rather than reading env itself: the request is the single source of truth for what budget
    /// the CHILD process runs under, so a caller who wants different limits should build them
    /// explicitly (mirrors `ComposeBudget::with_caps`'s own "explicit-caps constructors, never env
    /// vars" convention one layer down).
    pub fn new(grammar_path: impl Into<String>, grammar_format: GrammarFormat) -> Self {
        CompileWorkerRequest {
            protocol_version: WORKER_PROTOCOL_VERSION,
            grammar_path: grammar_path.into(),
            grammar_format,
            state_cap: crate::compose_budget::DEFAULT_STATE_BUDGET,
            arc_cap: crate::compose_budget::DEFAULT_ARC_BUDGET,
            tuple_cap: crate::compose_budget::DEFAULT_TUPLE_BUDGET,
            group_cap: crate::compose_budget::DEFAULT_GROUP_BUDGET,
            line_cap: crate::compose_budget::DEFAULT_LINE_BUDGET,
            chain_depth_cap: None,
            ordering_multiplicity_cap: Some(
                crate::compose_budget::DEFAULT_ORDERING_MULTIPLICITY_BUDGET,
            ),
            selected: None,
        }
    }

    pub fn compose_budget(&self) -> ComposeBudget {
        let mut budget = ComposeBudget::with_caps(
            self.state_cap,
            self.arc_cap,
            self.tuple_cap,
            self.group_cap,
            self.line_cap,
            None,
        );
        if let Some(cap) = self.chain_depth_cap {
            budget = budget.with_chain_depth_cap(cap);
        }
        if let Some(cap) = self.ordering_multiplicity_cap {
            budget = budget.with_ordering_multiplicity_cap(cap);
        }
        budget
    }
}

// Result / typed outcomes the CHILD reports.

/// One versioned compile-worker result (the child's one write, per the platform-parity contract /
/// `run_worker_child`'s doc).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompileWorkerResult {
    pub protocol_version: u32,
    pub outcome: CompileWorkerOutcome,
}

/// Every terminal outcome the CHILD itself can observe and report (see `WorkerOutcome` for the
/// outcomes only the PARENT can observe -- a kill, a crash, a flooded pipe).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CompileWorkerOutcome {
    /// The compile completed under budget, carrying the final state/arc counts and the real `HealthReport` from `crate::health_evaluator::evaluate_health`.
    Success {
        final_state_count: Option<i64>,
        final_arc_count: Option<i64>,
        uncovered_count: usize,
        health: HealthReport,
    },
    /// A deterministic `ComposeBudget`/enumeration budget tripped before or during compilation;
    /// `detail` is the originating error's `Display` text and `health` retains its findings.
    BudgetTripped {
        detail: String,
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
    /// One exact, fully evidenced selected backend payload produced by the child.
    SelectedSuccess {
        build: CompletedBackendBuildWire,
        artifact: SelectedArtifactDescriptor,
    },
    /// The selected backend produced a finalized payload larger than the configured serialized
    /// FST execution limit; no artifact is published.
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

/// Evidence for the selected payload written by the worker child.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectedArtifactDescriptor {
    pub(crate) byte_len: u64,
    pub(crate) sha256: String,
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

    let enum_budget = crate::morphotactics::EnumerationBudget::from_env();
    let compose_budget = request.compose_budget();

    let compiled = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        crate::analyzer::FomaProposer::new_with_budget_and_profile(
            &grammar,
            &enum_budget,
            &compose_budget,
        )
    }));

    let (result, profile) = match compiled {
        Ok(pair) => pair,
        Err(_) => {
            let detail = "the compile call panicked inside the worker process (caught here by \
                catch_unwind; this does not protect against stack overflow or allocator OOM, \
                which abort the process outright -- the supervisor's wall-timeout/exit-status \
                checks are what catch those, reported as WorkerOutcome::ChildCrashed)"
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
                Some(&profile),
            );
            CompileWorkerOutcome::Success {
                final_state_count: profile.final_state_count,
                final_arc_count: profile.final_arc_count,
                uncovered_count: report.uncovered.len(),
                health,
            }
        }
        Err(err @ FomaError::UnorderedOrderingMultiplicityExceeded { .. }) => {
            let health = crate::health_evaluator::evaluate_foma_error(&err, Some(&profile));
            CompileWorkerOutcome::BudgetTripped {
                detail: err.to_string(),
                health,
            }
        }
        Err(err @ FomaError::EnumerationBudgetExceeded { .. }) => {
            let health = crate::health_evaluator::evaluate_foma_error(&err, Some(&profile));
            CompileWorkerOutcome::BudgetTripped {
                detail: err.to_string(),
                health,
            }
        }
        Err(
            err @ (FomaError::LexcCompileFailed(_)
            | FomaError::Unsupported(_)
            | FomaError::Incomplete(_)),
        ) => {
            let health = crate::health_evaluator::evaluate_foma_error(&err, Some(&profile));
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

/// Remove the attempt's direct artifact path. A missing path is already clean.
fn cleanup_selected_output(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!(
            "remove selected artifact {}: {error}",
            path.display()
        )),
    }
}

/// Publish the payload directly into its attempt-owned path.
fn publish_selected_payload(
    path: &Path,
    payload_bytes: &[u8],
    limit_bytes: u64,
) -> Result<SelectedArtifactDescriptor, CompileWorkerOutcome> {
    let actual_bytes = payload_bytes.len() as u64;
    if actual_bytes > limit_bytes {
        return Err(CompileWorkerOutcome::SelectedExecutionLimitExceeded {
            actual_bytes,
            limit_bytes,
        });
    }

    let mut file = match OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(file) => file,
        Err(error) => {
            return Err(CompileWorkerOutcome::SelectedCompileFailed {
                detail: format!("create selected artifact {}: {error}", path.display()),
            });
        }
    };

    let write_result = file.write_all(payload_bytes).and_then(|_| file.sync_all());
    if let Err(error) = write_result {
        let detail = format!("write selected artifact {}: {error}", path.display());
        let detail = match cleanup_selected_output(path) {
            Ok(()) => detail,
            Err(cleanup_error) => format!("{detail}; {cleanup_error}"),
        };
        return Err(CompileWorkerOutcome::SelectedCompileFailed { detail });
    }

    Ok(SelectedArtifactDescriptor {
        byte_len: actual_bytes,
        sha256: sha256_hex(payload_bytes),
    })
}

#[cfg(test)]
fn write_selected_artifact(
    path: &Path,
    payload_bytes: &[u8],
) -> Result<SelectedArtifactDescriptor, String> {
    match publish_selected_payload(path, payload_bytes, u64::MAX) {
        Ok(descriptor) => Ok(descriptor),
        Err(CompileWorkerOutcome::SelectedExecutionLimitExceeded { .. }) => {
            Err("selected payload exceeded the publication limit".to_string())
        }
        Err(CompileWorkerOutcome::SelectedCompileFailed { detail }) => Err(detail),
        Err(other) => Err(format!("selected payload publication failed: {other:?}")),
    }
}

fn selected_artifact_path_for_attempt(attempt_id: &str) -> Result<PathBuf, String> {
    let bytes = attempt_id.as_bytes();
    let valid = bytes.len() == 40
        && bytes.starts_with(b"attempt-")
        && bytes[8..]
            .iter()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte));
    if !valid {
        return Err(
            "attempt id must be attempt- followed by 32 lowercase hex characters".to_string(),
        );
    }

    let temp_root = fs::canonicalize(std::env::temp_dir())
        .map_err(|error| format!("canonicalize system temp directory: {error}"))?;
    Ok(temp_root.join(format!("pangloss-selected-{attempt_id}.fst")))
}

fn compile_selected_from_request(
    request: &CompileWorkerRequest,
    selected: &SelectedCompileRequest,
    limits: &ExecutionLimits,
) -> CompileWorkerOutcome {
    let artifact_path = match selected_artifact_path_for_attempt(&selected.attempt_id) {
        Ok(path) => path,
        Err(detail) => return CompileWorkerOutcome::SelectedCompileFailed { detail },
    };
    let mut artifact_created = false;
    let outcome = (|| {
        let strategy = match strategy_from_worker_route(&selected.route) {
            Ok(strategy) => strategy,
            Err(detail) => return CompileWorkerOutcome::SelectedCompileFailed { detail },
        };
        let private_request = match CompileAttempt::from_worker_wire(selected.attempt_id.clone()) {
            Ok(request) => request,
            Err(detail) => return CompileWorkerOutcome::SelectedCompileFailed { detail },
        };
        let grammar = match load_grammar_for_worker(&request.grammar_path, request.grammar_format) {
            Ok(grammar) => grammar,
            Err(detail) => return CompileWorkerOutcome::SelectedCompileFailed { detail },
        };
        let completed = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            compile_completed_backend(&grammar, strategy, &private_request)
        })) {
            Ok(Ok(build)) => build,
            Ok(Err(error)) => {
                return CompileWorkerOutcome::SelectedCompileFailed {
                    detail: error.to_string(),
                }
            }
            Err(_) => {
                return CompileWorkerOutcome::SelectedCompileFailed {
                    detail: "selected backend compile panicked inside the worker process"
                        .to_string(),
                }
            }
        };
        let (wire, payload_bytes) = completed.into_wire_and_payload();
        let artifact = match publish_selected_payload(
            &artifact_path,
            &payload_bytes,
            limits.max_serialized_fst_bytes(),
        ) {
            Ok(artifact) => artifact,
            Err(outcome) => return outcome,
        };
        artifact_created = true;
        let result_size = match serde_json::to_vec(&CompileWorkerResult {
            protocol_version: WORKER_PROTOCOL_VERSION,
            outcome: CompileWorkerOutcome::SelectedSuccess {
                build: wire.clone(),
                artifact: artifact.clone(),
            },
        }) {
            Ok(bytes) => bytes.len() as u64,
            Err(error) => {
                return CompileWorkerOutcome::SelectedCompileFailed {
                    detail: format!("selected build result could not be serialized: {error}"),
                }
            }
        };
        if result_size > WORKER_PROTOCOL_LIMITS.max_result_bytes {
            return CompileWorkerOutcome::SelectedCompileFailed {
                detail: format!(
                    "selected build result is {result_size} byte(s), exceeding the {}-byte protocol limit",
                    WORKER_PROTOCOL_LIMITS.max_result_bytes
                ),
            };
        }
        CompileWorkerOutcome::SelectedSuccess {
            build: wire,
            artifact,
        }
    })();
    if artifact_created && !matches!(&outcome, CompileWorkerOutcome::SelectedSuccess { .. }) {
        if let Err(cleanup_error) = cleanup_selected_output(&artifact_path) {
            return CompileWorkerOutcome::SelectedCompileFailed {
                detail: format!("{outcome:?}; {cleanup_error}"),
            };
        }
    }
    outcome
}

/// The worker CHILD's entry point: reads exactly one `CompileWorkerRequest` frame from `input`, compiles it,
/// and writes exactly one `CompileWorkerResult` frame to `output`. Never panics on malformed
/// input -- an oversized/malformed request frame is reported as
/// `CompileWorkerOutcome::ProtocolViolation`, not a crash, so a hostile/buggy parent still gets a
/// clean typed response instead of an opaque non-zero exit.
///
/// Generic over `Read`/`Write` so this same function is both the real production child (`io::
/// stdin()`/`io::stdout()`, wired by `pg-cli`'s hidden subcommand) and directly unit-testable
/// in-process against an in-memory buffer (no subprocess needed to test the protocol/compile-outcome
/// mapping logic; only the supervisor's own kill/timeout behavior needs a real spawned process,
/// exercised by this crate's `tests/worker_execution_limits_contract.rs`).
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

    let outcome = match request.selected.as_ref() {
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
        None => compile_grammar_from_request(&request),
    };
    write_result(
        &mut output,
        &CompileWorkerResult {
            protocol_version: WORKER_PROTOCOL_VERSION,
            outcome,
        },
    )
}

fn write_result<W: Write>(output: &mut W, result: &CompileWorkerResult) -> io::Result<()> {
    let json = serde_json::to_vec(result).expect("CompileWorkerResult always serializes");
    write_frame(output, &json)
}

// Supervisor (parent side).

/// Which captured output stream breached its byte cap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputStream {
    Stdout,
    Stderr,
}

/// Every terminal outcome the SUPERVISOR (parent) itself observes -- as opposed to
/// `CompileWorkerOutcome`, which the child observes and reports about itself. A
/// `WorkerOutcome::Completed` wraps a real `CompileWorkerOutcome`; every other variant is an
/// outcome the child never got to report because the supervisor killed it or it never produced a
/// valid result frame at all.
#[derive(Debug, Clone)]
pub enum WorkerOutcome {
    /// The child ran to completion and reported its own typed outcome.
    Completed(CompileWorkerOutcome),
    /// The child was killed after `elapsed` exceeded `limit` -- an external execution limit, not a grammar-health verdict.
    WallTimeoutKilled { elapsed: Duration, limit: Duration },
    /// Captured stdout or stderr reached its byte cap and the child was killed; all four wire streams have versioned limits enforced by the parent.
    OutputLimitExceeded {
        stream: OutputStream,
        limit_bytes: u64,
    },
    /// The child exited abnormally (panic-as-abort, stack overflow, allocator OOM, or an external kill) without producing a valid result frame; `detail` names the observed `ExitStatus` and why parsing failed.
    ChildCrashed { detail: String },
    /// `std::process::Command::spawn` itself failed (e.g. the child executable does not exist), distinct from every outcome above, which all require a live child process.
    SpawnFailed { detail: String },
    /// The request could not even be serialized/sized within `WorkerProtocolLimits::max_request_bytes`, so no child was spawned at all.
    ProtocolViolation { detail: String },
}

impl WorkerOutcome {
    /// Maps this outcome into the existing `HealthReport`/`HealthFinding` vocabulary (the
    /// fast-failure-primacy contract: the report must carry the effective limit, the reached
    /// metric, and partial measurements where available) -- never a second, parallel report shape.
    /// `WorkerOutcome::Completed` returns
    /// the child's own real report unchanged; every other variant builds ONE synthetic finding
    /// describing the parent-observed supervisor event.
    ///
    /// **Two different facts, two different codes.** `WallTimeoutKilled`/
    /// `OutputLimitExceeded`/`ChildCrashed` are all genuine external-monitor aborts -- the
    /// supervisor protecting the host from a runaway child -- so they use
    /// `FindingCode::HostContainmentFired` at `Severity::MachineLimit`. `ChildCrashed` is kept
    /// here too even though its own doc admits the cause is ambiguous (panic-as-abort, stack
    /// overflow, allocator OOM, or an external kill): every one of those is still the supervisor
    /// observing an abnormal exit it did not itself schedule, not a normal build-tooling fault.
    /// `SpawnFailed`/`ProtocolViolation` are different in kind: no child process was ever running
    /// to be contained (the executable does not exist, or the request could not even be framed),
    /// so nothing here is host protection at all -- these use `FindingCode::BuildProcessFailed`
    /// (`FindingClass::Process`) at `Severity::NotProductionReady`, matching
    /// `crate::backend_selection`'s own use of the same code for an operational build fault rather
    /// than `Severity::MachineLimit`, which would misname a tooling fault as host containment.
    pub fn health_report(&self) -> HealthReport {
        match self {
            WorkerOutcome::Completed(outcome) => match outcome {
                CompileWorkerOutcome::Success { health, .. }
                | CompileWorkerOutcome::BudgetTripped { health, .. } => health.clone(),
                CompileWorkerOutcome::CompileFailed { health, .. }
                    if !health.findings.is_empty() =>
                {
                    health.clone()
                }
                CompileWorkerOutcome::CompileFailed { detail, .. }
                | CompileWorkerOutcome::GrammarLoadFailed { detail }
                | CompileWorkerOutcome::ProtocolViolation { detail }
                | CompileWorkerOutcome::SelectedCompileFailed { detail } => {
                    build_process_failure_health(detail.clone())
                }
                CompileWorkerOutcome::SelectedExecutionLimitExceeded {
                    actual_bytes,
                    limit_bytes,
                } => HealthReport::new(vec![HealthFinding {
                    code: FindingCode::ResourceBudgetReached,
                    severity: Severity::NotProductionReady,
                    phase: Phase::Compile,
                    affected: Vec::new(),
                    metric: Metric::PayloadBytes,
                    value: MetricValue::Bytes(*actual_bytes),
                    provenance: ValueProvenance::Observed,
                    threshold: Some(MetricValue::Bytes(*limit_bytes)),
                    explanation: format!(
                        "The selected compile produced a serialized FST of {actual_bytes} byte(s), \
                         exceeding the configured {limit_bytes}-byte execution limit. No artifact \
                         was published."
                    ),
                    remedies: Vec::new(),
                }]),
                CompileWorkerOutcome::SelectedSuccess { .. } => HealthReport::new(Vec::new()),
            },
            WorkerOutcome::WallTimeoutKilled { elapsed, limit } => {
                HealthReport::new(vec![HealthFinding {
                    code: FindingCode::HostContainmentFired,
                    severity: Severity::MachineLimit,
                    phase: Phase::Compile,
                    affected: Vec::new(),
                    metric: Metric::ElapsedMillis,
                    value: MetricValue::Millis(elapsed.as_millis() as u64),
                    provenance: ValueProvenance::Observed,
                    threshold: Some(MetricValue::Millis(limit.as_millis() as u64)),
                    explanation: format!(
                        "The compile worker process was killed after {elapsed:?}, exceeding its \
                         configured wall-time limit of {limit:?}. This is an execution-limit \
                         outcome, not a grammar-capability verdict."
                    ),
                    remedies: Vec::new(),
                }])
            }
            WorkerOutcome::OutputLimitExceeded {
                stream,
                limit_bytes,
            } => HealthReport::new(vec![HealthFinding {
                code: FindingCode::HostContainmentFired,
                severity: Severity::MachineLimit,
                phase: Phase::Compile,
                affected: Vec::new(),
                metric: Metric::UnknownUnboundedWork,
                value: MetricValue::Bytes(*limit_bytes),
                provenance: ValueProvenance::Observed,
                threshold: Some(MetricValue::Bytes(*limit_bytes)),
                explanation: format!(
                    "The compile worker process was killed after its captured {stream:?} \
                         output reached the {limit_bytes}-byte protocol limit."
                ),
                remedies: Vec::new(),
            }]),
            // Cause is ambiguous (own doc: panic/stack-overflow/OOM/external kill); kept as containment, not a tooling fault.
            WorkerOutcome::ChildCrashed { detail } => HealthReport::new(vec![HealthFinding {
                code: FindingCode::HostContainmentFired,
                severity: Severity::MachineLimit,
                phase: Phase::Compile,
                affected: Vec::new(),
                metric: Metric::UnknownUnboundedWork,
                value: MetricValue::Unbounded,
                provenance: ValueProvenance::Observed,
                threshold: None,
                explanation: format!(
                    "The compile worker process terminated abnormally without producing a valid \
                     result frame: {detail}"
                ),
                remedies: Vec::new(),
            }]),
            // No child process ever ran to be contained, so this is a process fault, not host containment.
            WorkerOutcome::SpawnFailed { detail } | WorkerOutcome::ProtocolViolation { detail } => {
                HealthReport::new(vec![HealthFinding {
                    code: FindingCode::BuildProcessFailed,
                    severity: Severity::NotProductionReady,
                    phase: Phase::Compile,
                    affected: Vec::new(),
                    metric: Metric::UnknownUnboundedWork,
                    value: MetricValue::Unbounded,
                    provenance: ValueProvenance::Observed,
                    threshold: None,
                    explanation: detail.clone(),
                    remedies: Vec::new(),
                }])
            }
        }
    }
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

/// Reads `reader` to EOF on a dedicated thread, accumulating into `buf` up to `cap` and setting `overflow` (never unset) once exceeded, then keeps draining so a flooding child cannot deadlock on a full pipe.
fn spawn_capped_reader<R: Read + Send + 'static>(
    mut reader: R,
    cap: u64,
    buf: Arc<Mutex<Vec<u8>>>,
    overflow: Arc<AtomicBool>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut chunk = [0u8; 8192];
        loop {
            match reader.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => {
                    let mut guard = buf.lock().unwrap_or_else(|e| e.into_inner());
                    if (guard.len() as u64) + (n as u64) > cap {
                        overflow.store(true, Ordering::SeqCst);
                        drop(guard);
                        // Keep draining (discarding) so the child never blocks on a full pipe; the poll loop kills it shortly after observing `overflow`.
                        loop {
                            match reader.read(&mut chunk) {
                                Ok(0) | Err(_) => break,
                                Ok(_) => continue,
                            }
                        }
                        break;
                    }
                    guard.extend_from_slice(&chunk[..n]);
                }
                Err(_) => break,
            }
        }
    })
}

/// Parses an accumulated stdout buffer as one length-prefixed `CompileWorkerResult` frame, applying the same `max_result_bytes` check `read_frame` does for a live stream.
fn parse_result_frame(buf: &[u8]) -> Result<CompileWorkerResult, String> {
    if buf.len() < 8 {
        return Err(format!(
            "only {} byte(s), too short for a length prefix",
            buf.len()
        ));
    }
    let len = u64::from_le_bytes(buf[0..8].try_into().expect("checked length above"));
    let limits = WORKER_PROTOCOL_LIMITS;
    if len > limits.max_result_bytes {
        return Err(format!(
            "declared result length {len} exceeds the {}-byte protocol limit",
            limits.max_result_bytes
        ));
    }
    let needed = 8u64
        .checked_add(len)
        .ok_or_else(|| "declared result length overflows frame-size arithmetic".to_string())?;
    if needed != buf.len() as u64 {
        return Err(format!(
            "frame length mismatch: header declares {len} body byte(s) ({needed} total), but \
             {} byte(s) were captured",
            buf.len()
        ));
    }
    let result: CompileWorkerResult = decode_frame_body(&buf[8..]).map_err(|e| e.to_string())?;
    if result.protocol_version != WORKER_PROTOCOL_VERSION {
        return Err(format!(
            "unsupported result protocol version {}; expected {}",
            result.protocol_version, WORKER_PROTOCOL_VERSION
        ));
    }
    Ok(result)
}

fn classify_exit(
    status: io::Result<ExitStatus>,
    stdout_buf: &Arc<Mutex<Vec<u8>>>,
) -> WorkerOutcome {
    let buf = stdout_buf.lock().unwrap_or_else(|e| e.into_inner()).clone();
    match parse_result_frame(&buf) {
        Ok(result) => WorkerOutcome::Completed(result.outcome),
        Err(parse_err) if parse_err.starts_with("unsupported result protocol version ") => {
            WorkerOutcome::ProtocolViolation { detail: parse_err }
        }
        Err(parse_err) => match status {
            Ok(status) if status.success() => WorkerOutcome::ProtocolViolation {
                detail: format!(
                    "child exited successfully (status {status:?}) but produced no valid result \
                     frame: {parse_err}"
                ),
            },
            Ok(status) => WorkerOutcome::ChildCrashed {
                detail: format!(
                    "child exited with {status:?}; no valid result frame ({parse_err})"
                ),
            },
            Err(wait_err) => WorkerOutcome::ChildCrashed {
                detail: format!(
                    "failed to determine child exit status ({wait_err}); no valid result frame \
                     ({parse_err})"
                ),
            },
        },
    }
}

/// The parent-side supervisor (the platform-parity contract's "standard-library
/// `Child::try_wait`/`Child::kill` wall-time control"): spawns `child_exe child_args...` (expected to eventually call `run_worker_child` on
/// its own stdin/stdout -- e.g. `pangloss`'s hidden `__compile-worker-child` subcommand, or this
/// crate's own `worker_test_child` test-support binary), writes `request` to its stdin, and polls
/// until the child exits or `limits.max_wall_time()` is breached -- whichever comes first -- returning exactly one
/// typed `WorkerOutcome`.
///
/// `std::process::Command`/`Child::try_wait`/`Child::kill` plus two capped reader threads are the
/// worker mechanism; any broader host process-tree policy belongs to the caller.
/// Windows-compatible: every API used here is cross-platform in the standard library, with no
/// Unix-only assumption (no signals, no `/proc` path assumed directly).
pub fn run_compile_worker(
    child_exe: &Path,
    child_args: &[String],
    request: &CompileWorkerRequest,
    limits: &ExecutionLimits,
) -> WorkerOutcome {
    let protocol_limits = WORKER_PROTOCOL_LIMITS;
    let request_json = match serde_json::to_vec(request) {
        Ok(bytes) => bytes,
        Err(e) => {
            return WorkerOutcome::ProtocolViolation {
                detail: format!("failed to serialize compile-worker request: {e}"),
            };
        }
    };
    if request_json.len() as u64 > protocol_limits.max_request_bytes {
        return WorkerOutcome::ProtocolViolation {
            detail: format!(
                "request is {} byte(s), exceeding the {}-byte protocol limit",
                request_json.len(),
                protocol_limits.max_request_bytes
            ),
        };
    }

    let mut child = match Command::new(child_exe)
        .args(child_args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            return WorkerOutcome::SpawnFailed {
                detail: format!("spawning {}: {e}", child_exe.display()),
            };
        }
    };

    let mut stdin = child.stdin.take().expect("piped stdin");
    let stdin_handle = std::thread::spawn(move || {
        // Best-effort: a hung child that never reads stdin blocks only this thread, not the supervisor's poll loop, which still enforces the wall deadline.
        let _ = write_frame(&mut stdin, &request_json);
    });

    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");

    let stdout_buf = Arc::new(Mutex::new(Vec::new()));
    let stdout_overflow = Arc::new(AtomicBool::new(false));
    let stdout_handle = spawn_capped_reader(
        stdout,
        protocol_limits.max_result_bytes.saturating_add(8), // frame header + body
        Arc::clone(&stdout_buf),
        Arc::clone(&stdout_overflow),
    );

    let stderr_buf = Arc::new(Mutex::new(Vec::new()));
    let stderr_overflow = Arc::new(AtomicBool::new(false));
    let stderr_handle = spawn_capped_reader(
        stderr,
        protocol_limits.max_captured_stderr_bytes,
        Arc::clone(&stderr_buf),
        Arc::clone(&stderr_overflow),
    );

    let start = Instant::now();
    let poll_interval = Duration::from_millis(50);

    let outcome = loop {
        match child.try_wait() {
            Ok(Some(_status)) => {
                break classify_exit(child.wait(), &stdout_buf);
            }
            Ok(None) => {}
            Err(e) => {
                break WorkerOutcome::ChildCrashed {
                    detail: format!("try_wait failed: {e}"),
                };
            }
        }

        if stdout_overflow.load(Ordering::SeqCst) {
            let _ = child.kill();
            let _ = child.wait();
            break WorkerOutcome::OutputLimitExceeded {
                stream: OutputStream::Stdout,
                limit_bytes: protocol_limits.max_result_bytes,
            };
        }
        if stderr_overflow.load(Ordering::SeqCst) {
            let _ = child.kill();
            let _ = child.wait();
            break WorkerOutcome::OutputLimitExceeded {
                stream: OutputStream::Stderr,
                limit_bytes: protocol_limits.max_captured_stderr_bytes,
            };
        }

        let elapsed = start.elapsed();
        if elapsed >= limits.max_wall_time() {
            let _ = child.kill();
            let _ = child.wait();
            break WorkerOutcome::WallTimeoutKilled {
                elapsed,
                limit: limits.max_wall_time(),
            };
        }

        std::thread::sleep(poll_interval);
    };

    let _ = stdin_handle.join();
    let _ = stdout_handle.join();
    let _ = stderr_handle.join();
    let _ = stderr_buf; // captured for future diagnostics surfacing; not read on every path today.

    outcome
}

/// Compile exactly `selection.preferred()` in one contained worker and hand its raw payload through
/// the ordinary trusted selector. The caller cannot supply a route independently of selection;
/// no lower-ranked route is attempted when the preferred route fails.
pub fn run_selected_compile_worker(
    child_exe: &Path,
    child_args: &[String],
    grammar_path: &str,
    grammar_format: GrammarFormat,
    grammar: &Grammar,
    selection: &BackendSelection,
    request: &CompileAttempt,
    limits: &ExecutionLimits,
) -> Result<crate::completed_build::SelectedBackendBuild, crate::completed_build::CompletedBuildError>
{
    let preferred = selection
        .preferred()
        .ok_or(crate::completed_build::CompletedBuildError::NoMatchingCompletedBuild)?;
    let artifact_path = selected_artifact_path_for_attempt(request.attempt_id().as_str())
        .map_err(crate::completed_build::CompletedBuildError::Compiler)?;
    let parent_owns_artifact = match fs::symlink_metadata(&artifact_path) {
        Ok(_) => false,
        Err(error) if error.kind() == io::ErrorKind::NotFound => true,
        Err(_) => false,
    };
    let selected = SelectedCompileRequest {
        attempt_id: request.attempt_id().as_str().to_string(),
        route: preferred.label().to_string(),
        max_serialized_fst_bytes: limits.max_serialized_fst_bytes(),
    };
    let mut worker_request = CompileWorkerRequest::new(grammar_path.to_string(), grammar_format);
    worker_request.selected = Some(selected);
    let outcome = run_compile_worker(child_exe, child_args, &worker_request, limits);
    let result = match outcome {
        WorkerOutcome::Completed(CompileWorkerOutcome::SelectedSuccess { build, artifact }) => {
            match read_selected_artifact(
                &artifact_path,
                &artifact,
                build,
                limits.max_serialized_fst_bytes(),
            ) {
                Ok((wire, payload)) => {
                    match crate::completed_build::CompletedBackendBuild::from_wire(wire, payload) {
                        Ok(build) => select_completed_build(
                            selection,
                            [build],
                            request,
                            &crate::backend_runtime::grammar_identity(grammar),
                        ),
                        Err(error) => Err(error),
                    }
                }
                Err(error) => Err(error),
            }
        }
        WorkerOutcome::Completed(CompileWorkerOutcome::SelectedExecutionLimitExceeded {
            actual_bytes,
            limit_bytes,
        }) => Err(crate::completed_build::CompletedBuildError::Compiler(format!(
            "selected serialized FST is {actual_bytes} byte(s), exceeding the {limit_bytes}-byte execution limit"
        ))),
        WorkerOutcome::Completed(CompileWorkerOutcome::SelectedCompileFailed { detail }) => {
            Err(crate::completed_build::CompletedBuildError::Compiler(detail))
        }
        other => Err(crate::completed_build::CompletedBuildError::Compiler(format!(
            "selected compile worker did not return a payload: {other:?}"
        ))),
    };
    let cleanup_result = if parent_owns_artifact {
        cleanup_selected_output(&artifact_path)
    } else {
        Ok(())
    };
    match (result, cleanup_result) {
        (Ok(build), Ok(())) => Ok(build),
        (Ok(_), Err(cleanup_error)) => Err(crate::completed_build::CompletedBuildError::Compiler(
            cleanup_error,
        )),
        (Err(error), Ok(())) => Err(error),
        (Err(error), Err(cleanup_error)) => {
            Err(crate::completed_build::CompletedBuildError::Compiler(
                format!("{error}; selected artifact cleanup also failed: {cleanup_error}"),
            ))
        }
    }
}

fn read_selected_artifact(
    expected_path: &Path,
    descriptor: &SelectedArtifactDescriptor,
    wire: CompletedBackendBuildWire,
    max_serialized_fst_bytes: u64,
) -> Result<(CompletedBackendBuildWire, Vec<u8>), crate::completed_build::CompletedBuildError> {
    let expected_path = expected_path.to_path_buf();
    let expected_display = expected_path.display().to_string();
    let file = OpenOptions::new()
        .read(true)
        .open(&expected_path)
        .map_err(|error| {
            crate::completed_build::CompletedBuildError::Compiler(format!(
                "selected artifact open failed for {expected_display}: {error}"
            ))
        })?;
    let metadata = file.metadata().map_err(|error| {
        crate::completed_build::CompletedBuildError::Compiler(format!(
            "selected artifact metadata failed for {expected_display}: {error}"
        ))
    })?;
    if !metadata.is_file() {
        return Err(crate::completed_build::CompletedBuildError::Compiler(
            format!("selected artifact path is not a regular file: {expected_display}"),
        ));
    }
    let actual_len = metadata.len();
    if actual_len > max_serialized_fst_bytes {
        return Err(crate::completed_build::CompletedBuildError::Compiler(format!(
            "selected artifact is {actual_len} byte(s), exceeding the {max_serialized_fst_bytes}-byte execution limit"
        )));
    }
    if descriptor.byte_len != actual_len {
        return Err(crate::completed_build::CompletedBuildError::Compiler(
            format!(
                "selected artifact length mismatch: descriptor {}, file {}",
                descriptor.byte_len, actual_len
            ),
        ));
    }
    let mut payload = Vec::new();
    file.take(max_serialized_fst_bytes.saturating_add(1))
        .read_to_end(&mut payload)
        .map_err(|error| {
            crate::completed_build::CompletedBuildError::Compiler(format!(
                "selected artifact read failed for {expected_display}: {error}"
            ))
        })?;
    let payload_len = payload.len() as u64;
    if payload_len != descriptor.byte_len || payload_len > max_serialized_fst_bytes {
        return Err(crate::completed_build::CompletedBuildError::Compiler(
            format!(
                "selected artifact length changed while reading: descriptor {}, read {}, limit {}",
                descriptor.byte_len, payload_len, max_serialized_fst_bytes
            ),
        ));
    }
    let actual_digest = sha256_hex(&payload);
    if descriptor.sha256 != actual_digest {
        return Err(crate::completed_build::CompletedBuildError::Compiler(
            format!(
                "selected artifact digest mismatch: descriptor {}, file {}",
                descriptor.sha256, actual_digest
            ),
        ));
    }
    if wire.payload_fingerprint != actual_digest {
        return Err(crate::completed_build::CompletedBuildError::Compiler(
            format!(
                "selected payload evidence digest mismatch: evidence {}, file {}",
                wire.payload_fingerprint, actual_digest
            ),
        ));
    }
    Ok((wire, payload))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch_attempt_id() -> String {
        static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        format!("attempt-{:08x}{:024x}", std::process::id(), n)
    }

    fn scratch_artifact_path() -> PathBuf {
        selected_artifact_path_for_attempt(&scratch_attempt_id()).expect("derive scratch artifact")
    }

    #[test]
    fn selected_artifact_publish_is_atomic_and_described_by_actual_bytes() {
        let artifact_path = scratch_artifact_path();
        let payload = b"fst!";

        let descriptor =
            write_selected_artifact(&artifact_path, payload).expect("publish selected artifact");

        assert_eq!(fs::read(&artifact_path).expect("read artifact"), payload);
        assert_eq!(descriptor.byte_len, payload.len() as u64);
        assert_eq!(descriptor.sha256, sha256_hex(payload));
        cleanup_selected_output(&artifact_path).expect("clean selected output");
    }

    #[test]
    fn selected_payload_over_limit_publishes_nothing() {
        let artifact_path = scratch_artifact_path();

        let outcome = publish_selected_payload(&artifact_path, b"four", 3)
            .expect_err("four bytes must exceed a three-byte limit");

        assert!(matches!(
            outcome,
            CompileWorkerOutcome::SelectedExecutionLimitExceeded {
                actual_bytes: 4,
                limit_bytes: 3
            }
        ));
        assert!(!artifact_path.exists());
    }

    #[test]
    fn selected_publish_never_clobbers_or_removes_an_existing_file() {
        let artifact_path = scratch_artifact_path();
        fs::write(&artifact_path, b"sentinel").expect("seed existing artifact");

        let outcome = publish_selected_payload(&artifact_path, b"replacement", u64::MAX)
            .expect_err("create-new publication must reject an existing artifact");

        assert!(matches!(
            outcome,
            CompileWorkerOutcome::SelectedCompileFailed { .. }
        ));
        assert_eq!(
            fs::read(&artifact_path).expect("read sentinel"),
            b"sentinel"
        );
        fs::remove_file(&artifact_path).expect("remove sentinel");
    }

    #[test]
    fn selected_output_cleanup_removes_the_attempt_owned_file() {
        let artifact_path = scratch_artifact_path();
        fs::write(&artifact_path, b"partial").expect("seed partial artifact");

        cleanup_selected_output(&artifact_path).expect("clean selected output");

        assert!(!artifact_path.exists());
    }

    #[test]
    fn selected_artifact_path_is_a_fixed_direct_child_of_canonical_temp() {
        let attempt_id = "attempt-0123456789abcdef0123456789abcdef";

        let artifact_path =
            selected_artifact_path_for_attempt(attempt_id).expect("derive selected artifact");
        let temp_root = fs::canonicalize(std::env::temp_dir()).expect("canonical temp");

        assert_eq!(artifact_path.parent(), Some(temp_root.as_path()));
        assert_eq!(
            artifact_path.file_name().and_then(|name| name.to_str()),
            Some("pangloss-selected-attempt-0123456789abcdef0123456789abcdef.fst")
        );
    }

    #[test]
    fn selected_artifact_path_rejects_every_non_generated_attempt_id_shape() {
        for invalid in [
            "",
            "attempt",
            "attempt-0123",
            "../outside",
            "attempt-../../outside",
            "attempt-0123456789abcdef0123456789abcdeg",
            "attempt-0123456789ABCDEF0123456789ABCDEF",
            "attempt-0123456789abcdef0123456789abcdef0",
            "C:\\outside",
            "attempt-0123456789abcdef/123456789abcdef0",
            "attempt-0123456789abcdef\\123456789abcdef0",
            "attempt-0123456789abcdef0123456789abcdeé",
        ] {
            assert!(
                selected_artifact_path_for_attempt(invalid).is_err(),
                "malformed attempt id must not become a path: {invalid:?}"
            );
        }
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
    fn selected_success_json_contains_descriptor_but_no_payload() {
        let result = CompileWorkerResult {
            protocol_version: WORKER_PROTOCOL_VERSION,
            outcome: CompileWorkerOutcome::SelectedSuccess {
                build: CompletedBackendBuildWire {
                    requested_strategy: "templated-underlying-tokens".to_string(),
                    realized_strategy: "templated-underlying-tokens".to_string(),
                    grammar_identity: "grammar".to_string(),
                    attempt_id: "attempt".to_string(),
                    completion_proof:
                        crate::completed_build::CompletionProofWire::TemplatedFullEmission {
                            uncovered_count: 0,
                            skipped_count: 0,
                        },
                    state_count: 1,
                    arc_count: 1,
                    model_fingerprint: "model".to_string(),
                    payload_fingerprint: sha256_hex(b"fst!"),
                },
                artifact: SelectedArtifactDescriptor {
                    byte_len: 4,
                    sha256: sha256_hex(b"fst!"),
                },
            },
        };

        let json = serde_json::to_value(result).expect("serialize selected success");

        assert_eq!(
            json["outcome"]["SelectedSuccess"]["artifact"]["byte_len"],
            4
        );
        assert!(
            !json.to_string().contains("payload_bytes"),
            "selected result must contain metadata only: {json}"
        );
    }

    #[test]
    fn selected_serialized_size_limit_is_an_internal_resource_finding() {
        let outcome =
            WorkerOutcome::Completed(CompileWorkerOutcome::SelectedExecutionLimitExceeded {
                actual_bytes: 4,
                limit_bytes: 3,
            });

        let health = outcome.health_report();

        assert_eq!(health.admission(), Severity::NotProductionReady);
        assert_eq!(health.findings[0].code, FindingCode::ResourceBudgetReached);
        assert_eq!(health.findings[0].metric, Metric::PayloadBytes);
    }

    #[test]
    fn selected_parent_read_is_bounded_before_accepting_payload() {
        let artifact_path = scratch_artifact_path();
        fs::write(&artifact_path, b"four").expect("write oversized artifact");
        let descriptor = SelectedArtifactDescriptor {
            byte_len: 4,
            sha256: sha256_hex(b"four"),
        };
        let wire = CompletedBackendBuildWire {
            requested_strategy: "templated-underlying-tokens".to_string(),
            realized_strategy: "templated-underlying-tokens".to_string(),
            grammar_identity: "grammar".to_string(),
            attempt_id: "attempt-0123456789abcdef0123456789abcdef".to_string(),
            completion_proof: crate::completed_build::CompletionProofWire::TemplatedFullEmission {
                uncovered_count: 0,
                skipped_count: 0,
            },
            state_count: 1,
            arc_count: 1,
            model_fingerprint: "model".to_string(),
            payload_fingerprint: sha256_hex(b"four"),
        };

        let error = read_selected_artifact(&artifact_path, &descriptor, wire, 3)
            .expect_err("parent must reject before accepting an over-limit payload");

        assert!(error.to_string().contains("execution limit"), "{error}");
        cleanup_selected_output(&artifact_path).expect("clean selected output");
    }

    #[test]
    fn completed_worker_failures_never_become_empty_or_admissible() {
        let cases = vec![
            CompileWorkerOutcome::GrammarLoadFailed {
                detail: "synthetic load failure".to_string(),
            },
            CompileWorkerOutcome::ProtocolViolation {
                detail: "synthetic protocol failure".to_string(),
            },
            CompileWorkerOutcome::CompileFailed {
                detail: "synthetic caught panic".to_string(),
                health: HealthReport::new(Vec::new()),
            },
        ];

        for outcome in cases {
            let health = WorkerOutcome::Completed(outcome).health_report();
            assert!(!health.findings.is_empty());
            // A tooling fault still refuses publication, but naming it a host abort would be wrong.
            assert_eq!(health.admission(), Severity::NotProductionReady);
            assert!(health
                .findings
                .iter()
                .any(|finding| finding.code == FindingCode::BuildProcessFailed));
        }
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

    #[test]
    fn parse_result_frame_rejects_declared_length_over_limit() {
        let mut buf = Vec::new();
        let huge = WORKER_PROTOCOL_LIMITS.max_result_bytes + 1;
        buf.extend_from_slice(&huge.to_le_bytes());
        let err = parse_result_frame(&buf).expect_err("must reject");
        assert!(err.contains("exceeds"));
    }

    #[test]
    fn parse_result_frame_rejects_short_buffer() {
        let err = parse_result_frame(&[1, 2, 3]).expect_err("too short");
        assert!(err.contains("too short"));
    }

    #[test]
    fn parse_result_frame_rejects_a_stale_worker_protocol() {
        let stale = CompileWorkerResult {
            protocol_version: WORKER_PROTOCOL_VERSION - 1,
            outcome: CompileWorkerOutcome::ProtocolViolation {
                detail: "synthetic stale result".to_string(),
            },
        };
        let body = serde_json::to_vec(&stale).expect("serialize stale result");
        let mut frame = Vec::new();
        write_frame(&mut frame, &body).expect("frame stale result");

        let error = parse_result_frame(&frame)
            .expect_err("a pre-cleanup child result must not enter a lockstep parent");
        assert!(error.contains("protocol version"), "error: {error}");
        assert!(
            error.contains(&(WORKER_PROTOCOL_VERSION - 1).to_string()),
            "error: {error}"
        );
        assert!(
            error.contains(&WORKER_PROTOCOL_VERSION.to_string()),
            "error: {error}"
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

    /// A synthetic grammar with an `Unordered` stratum whose loose-rule count exceeds a small `ordering_multiplicity_cap`, tripping a real `ComposeError` before lexc compilation.
    const UNORDERED_GRAMMAR_XML: &str = r#"<HermitCrabInput><Language><Name>WorkerBudgetTripFixture</Name>
      <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
      <CharacterDefinitionTable id="t1"><Name>Main</Name>
        <SegmentDefinitions><SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions>
      </CharacterDefinitionTable>
      <NaturalClasses><SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="ca" /></SegmentNaturalClass></NaturalClasses>
      <Strata>
        <Stratum characterDefinitionTable="t1" morphologicalRules="mr1 mr2 mr3" morphologicalRuleOrder="unordered">
          <Name>S</Name>
          <MorphologicalRuleDefinitions>
            <MorphologicalRule id="mr1"><Name>R1</Name><MorphologicalSubrules>
              <MorphologicalSubrule id="s1"><MorphologicalInput><PhoneticSequence id="in1"><SimpleContext naturalClass="ncAll" /></PhoneticSequence></MorphologicalInput>
                <MorphologicalOutput><CopyFromInput index="in1" /><InsertSegments><PhoneticShape>a</PhoneticShape></InsertSegments></MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules></MorphologicalRule>
            <MorphologicalRule id="mr2"><Name>R2</Name><MorphologicalSubrules>
              <MorphologicalSubrule id="s2"><MorphologicalInput><PhoneticSequence id="in2"><SimpleContext naturalClass="ncAll" /></PhoneticSequence></MorphologicalInput>
                <MorphologicalOutput><CopyFromInput index="in2" /><InsertSegments><PhoneticShape>a</PhoneticShape></InsertSegments></MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules></MorphologicalRule>
            <MorphologicalRule id="mr3"><Name>R3</Name><MorphologicalSubrules>
              <MorphologicalSubrule id="s3"><MorphologicalInput><PhoneticSequence id="in3"><SimpleContext naturalClass="ncAll" /></PhoneticSequence></MorphologicalInput>
                <MorphologicalOutput><CopyFromInput index="in3" /><InsertSegments><PhoneticShape>a</PhoneticShape></InsertSegments></MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules></MorphologicalRule>
          </MorphologicalRuleDefinitions>
          <LexicalEntries>
            <LexicalEntry id="e1"><Allomorphs><Allomorph id="a1"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs></LexicalEntry>
          </LexicalEntries>
        </Stratum>
      </Strata>
    </Language></HermitCrabInput>"#;

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
    fn run_worker_child_reports_budget_tripped_for_a_real_ordering_multiplicity_breach() {
        let path = scratch_grammar_file("budget-trip", UNORDERED_GRAMMAR_XML);
        let mut request =
            CompileWorkerRequest::new(path.to_string_lossy().into_owned(), GrammarFormat::Xml);
        // 3 loose rules in the fixture's Unordered stratum; cap = 2 must trip.
        request.ordering_multiplicity_cap = Some(2);
        let json = serde_json::to_vec(&request).unwrap();
        let mut buf = Vec::new();
        buf.extend_from_slice(&(json.len() as u64).to_le_bytes());
        buf.extend_from_slice(&json);

        let result = call_child(&buf);
        match result.outcome {
            CompileWorkerOutcome::BudgetTripped { detail, health } => {
                assert!(detail.contains("ordering-multiplicity"), "detail: {detail}");
                assert_eq!(health.admission(), Severity::NotProductionReady);
                assert!(health
                    .findings
                    .iter()
                    .any(|f| f.metric == Metric::OrderingRuleCount));
            }
            other => panic!("expected BudgetTripped, got {other:?}"),
        }
        let _ = std::fs::remove_file(&path);
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

    // WorkerOutcome -> HealthReport mapping.

    #[test]
    fn worker_outcome_wall_timeout_maps_to_machine_limit_host_containment_fired() {
        let outcome = WorkerOutcome::WallTimeoutKilled {
            elapsed: Duration::from_secs(5),
            limit: Duration::from_secs(2),
        };
        let health = outcome.health_report();
        assert_eq!(health.admission(), Severity::MachineLimit);
        assert_eq!(health.findings[0].code, FindingCode::HostContainmentFired);
        assert_eq!(health.findings[0].metric, Metric::ElapsedMillis);
    }

    #[test]
    fn worker_outcome_completed_success_passes_through_the_real_health_report_unchanged() {
        let real_health = HealthReport::new(vec![]);
        let outcome = WorkerOutcome::Completed(CompileWorkerOutcome::Success {
            final_state_count: Some(3),
            final_arc_count: Some(4),
            uncovered_count: 0,
            health: real_health.clone(),
        });
        assert_eq!(outcome.health_report(), real_health);
    }

    /// An external-monitor abort answers "was the attempt contained", never a grammar question.
    #[test]
    fn host_containment_is_not_a_grammar_verdict() {
        assert_eq!(
            FindingCode::HostContainmentFired.class(),
            crate::health::FindingClass::Containment
        );

        let outcomes = vec![
            WorkerOutcome::WallTimeoutKilled {
                elapsed: Duration::from_secs(5),
                limit: Duration::from_secs(2),
            },
            WorkerOutcome::OutputLimitExceeded {
                stream: OutputStream::Stdout,
                limit_bytes: 1024,
            },
            WorkerOutcome::ChildCrashed {
                detail: "synthetic crash".to_string(),
            },
        ];
        for outcome in outcomes {
            let health = outcome.health_report();
            assert_eq!(health.admission(), Severity::MachineLimit);
            assert!(
                health
                    .findings
                    .iter()
                    .all(|f| f.code == FindingCode::HostContainmentFired),
                "{outcome:?} must carry only HostContainmentFired: {health:?}"
            );
        }
    }

    /// Neither ever ran a child to contain, so both are process faults rather than host limits.
    #[test]
    fn spawn_and_protocol_failures_are_process_faults_not_host_limits() {
        assert_eq!(
            FindingCode::BuildProcessFailed.class(),
            crate::health::FindingClass::Process
        );

        for outcome in [
            WorkerOutcome::SpawnFailed {
                detail: "synthetic spawn failure".to_string(),
            },
            WorkerOutcome::ProtocolViolation {
                detail: "synthetic protocol failure".to_string(),
            },
        ] {
            let health = outcome.health_report();
            assert_eq!(health.findings.len(), 1);
            assert_eq!(health.findings[0].code, FindingCode::BuildProcessFailed);
            assert_ne!(
                health.findings[0].code,
                FindingCode::HostContainmentFired,
                "{outcome:?} never ran a child, so it must not read as host containment"
            );
            assert_eq!(health.admission(), Severity::NotProductionReady);
        }
    }
}
