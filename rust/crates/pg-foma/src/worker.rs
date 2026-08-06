//! Compile-worker watchdog subsystem. Compile-time work runs in a killable native
//! worker process under the parent watchdog, distinct
//! from `crate::compose_budget`'s in-process cooperative APPLY-side budgets, under which
//! apply-time (word analysis) runs in-process. Do not add
//! anything here that touches per-word `propose`/`apply_up` -- that is `compose_budget.rs`'s
//! `ApplyBudget`/`ApplyOutcome`, unchanged by this module.
//!
//! # The contract this module implements
//! **Platform parity.** Windows and Linux are equal,
//! first-class native production targets. Both use ONE compiler worker, ONE versioned
//! request/result protocol, standard-library `Child::try_wait`/`Child::kill` wall-time control,
//! deterministic compiler budgets, bounded input/output, and sampled RSS through a
//! Rust-1.90-compatible `sysinfo` release. Production compilation launches no descendants, so Job
//! Objects, cgroups, process-tree management, Tokio, and `processkit` are out of scope. Sampled RSS
//! is reported with its interval and observed peak; it is NOT called a hard memory ceiling. WASM is
//! analysis-only and needs no compile watchdog.
//!
//! **Fast-failure primacy.** Deterministic logical counters are the primary fast-failure mechanism;
//! cooperative
//! elapsed checks and the parent wall timeout are outer safeguards. This module's wall-clock/RSS
//! checks are exactly that outer safeguard -- `CompileWorkerRequest::compose_budget` is what the
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
//! (`sysinfo`, `pg-snapshot`, `pg-fwdata`) are scoped to the identical target cfg in this crate's
//! `Cargo.toml` -- not merely dead code on wasm32, genuinely absent from `pg-wasm`'s dependency
//! graph, mirroring the contract above: "WASM is analysis-only and needs no compile watchdog." `wasm32-unknown-
//! unknown` has no `std::process::Command`, so a process-spawning watchdog cannot exist there by
//! construction, not merely by choice.
//!
//! # Three pieces
//! - **Protocol** (`CompileWorkerRequest`/`CompileWorkerResult`/`WorkerLimits`): a versioned,
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
//!   `Child::try_wait` in a loop that also samples the child's RSS via `sysinfo` and checks a wall
//!   deadline -- killing the child (`Child::kill`) and returning a typed `WorkerOutcome` the
//!   instant any bound is breached. No Tokio, no process tree, no Job Objects/cgroups (platform
//!   parity, above); the
//!   only "descendant" is the one worker process itself.
//!
//! # Typed outcomes -> existing health/error vocabulary (do not invent a parallel one)
//! `CompileWorkerOutcome` (what the CHILD reports) reuses `crate::compose_budget::ComposeError`
//! verbatim for a real budget trip (today, the ordering-multiplicity dimension --
//! `crate::analyzer::FomaProposer::new_with_budget_and_profile` is the one production call site
//! that can return `Err` from an actual `crate::compose_budget::ComposeError`-carrying
//! `crate::analyzer::FomaError` variant before ever handing lexc to the foma compiler) and feeds
//! every measurement into `crate::health_evaluator::evaluate_health` to build a real
//! `crate::health::HealthReport` -- never a second, parallel report shape. `WorkerOutcome`
//! (what the PARENT reports for outcomes the child never got to write -- a wall-timeout kill, an
//! RSS breach, a flooded pipe, a crash, a malformed protocol message) maps each into the SAME
//! `crate::health::HealthReport`/`crate::health::HealthFinding` vocabulary via
//! `WorkerOutcome::health_report`, reusing `crate::health::FindingCode::ResourceBudgetReached`
//! (this module's own judgment call, documented on that function: none of `crate::health::Metric`'s
//! existing variants name "parent-observed wall-clock kill" or "sampled child RSS", so one new
//! variant, `crate::health::Metric::SampledCompileRssBytes`, is appended, never inserted or
//! renumbered -- the
//! same "new codes/variants only ever append" discipline `crate::health`'s own module doc
//! documents, and the same reuse this crate's `OrderingRuleCount`/`enum_budget_finding` precedents
//! already established for "no existing variant fits, but the finding vocabulary itself is closed
//! against inventing a whole second schema").
//!
//! # Sampled RSS is not a hard ceiling (the platform-parity contract's exact wording matters)
//! `WorkerOutcome::RssLimitExceeded`'s own doc, `sample_rss_mb`'s own doc, and every place this
//! module surfaces a sampled RSS value in prose all say the same thing the contract above says verbatim:
//! **allocation can occur between samples, so a sampled value below the limit is never proof the
//! child never exceeded it, and a sampled value above the limit is a real observed measurement,
//! not a kernel-enforced ceiling.** The kill this module performs on breach is real (the child
//! process really is killed); the *measurement* that triggered it is a periodic sample, not a
//! continuously enforced hardware/kernel limit the way, say, a cgroup memory controller would be.
//!
//! # Opt-in, additive, default path unchanged
//! Nothing in this module is called by `crate::analyzer::FomaProposer::new`/`new_with_profile`,
//! `crate::composite::FomaAnalyzer::new`, or any other existing production entry point --
//! spawning a worker is something a caller (`pangloss pack --watchdog`, `pg-cli`'s own hidden
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

use std::io::{self, Read, Write};
use std::path::Path;
use std::process::{Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::analyzer::FomaError;
use crate::compose_budget::{ComposeBudget, ComposeError};
use crate::health::{
    FindingCode, HealthFinding, HealthReport, Metric, MetricValue, Phase, Severity, ValueProvenance,
};

// Protocol version and versioned wire limits, mirroring `pg_pack::format`'s `VersionLimits` shape.

/// This worker protocol's own version, carried inside every `CompileWorkerRequest`/
/// `CompileWorkerResult` (the platform-parity contract's "ONE versioned request/result
/// protocol"). Bump only on a
/// wire-incompatible change to either type.
pub const WORKER_PROTOCOL_VERSION: u32 = 1;

/// Versioned, hard-coded ceilings for this protocol (design discipline shared with
/// `pg_pack::format::VersionLimits`: every configurable dimension has a hard-coded, versioned,
/// deliberately high absolute ceiling).
/// These bound the WIRE MESSAGES themselves (request/result JSON frames, captured stdout/stderr) --
/// a completely different thing from `ComposeBudget`'s logical compile-work caps or this
/// envelope's own wall-clock/RSS fields, all three of which travel INSIDE a request that itself
/// must first pass these frame-level limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkerLimits {
    /// Ceiling on one serialized `CompileWorkerRequest` frame's byte length.
    pub max_request_bytes: u64,
    /// Ceiling on one serialized `CompileWorkerResult` frame's byte length.
    pub max_result_bytes: u64,
    /// Ceiling on total captured stderr bytes the supervisor retains from the child.
    pub max_captured_stderr_bytes: u64,
    /// Absolute ceiling on a caller-requested wall-clock timeout -- see `WatchdogEnvelope::clamped`.
    pub max_wall_timeout_ms: u64,
    /// Absolute ceiling on a caller-requested RSS guardrail, in mebibytes.
    pub max_rss_limit_mb: u64,
    /// Floor on the RSS sampling interval -- prevents a caller from requesting a busy-loop sample
    /// rate that would itself become the resource problem.
    pub min_rss_sample_interval_ms: u64,
}

/// Protocol version 1's limits. Deliberately generous relative to this protocol's own content (a
/// grammar file PATH plus a handful of numeric budget caps for the request; a
/// `crate::health::HealthReport` plus a few counts for the result) -- these bound the wire
/// framing itself against a hostile/malformed peer, not the compile work the framed message
/// describes (that is `ComposeBudget`'s job, checked separately, inside the child).
pub const V1_WORKER_LIMITS: WorkerLimits = WorkerLimits {
    max_request_bytes: 4 * 1024 * 1024,         // 4 MiB
    max_result_bytes: 16 * 1024 * 1024,         // 16 MiB
    max_captured_stderr_bytes: 4 * 1024 * 1024, // 4 MiB
    max_wall_timeout_ms: 24 * 60 * 60 * 1000,   // 24h emergency ceiling
    max_rss_limit_mb: 256 * 1024,               // 256 GiB emergency ceiling
    min_rss_sample_interval_ms: 10,
};

/// Looks up the versioned limits for a protocol version. `None` for any version this build
/// doesn't understand.
pub const fn limits_for_version(version: u32) -> Option<WorkerLimits> {
    match version {
        1 => Some(V1_WORKER_LIMITS),
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
/// and can read the file itself, keeping this frame small (well under [`WorkerLimits::
/// max_request_bytes`]) regardless of the referenced grammar's own size; the referenced grammar's
/// CONTENT is exactly what `ComposeBudget` and this module's own wall-time/RSS guardrails protect
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
}

impl CompileWorkerRequest {
    /// A request for `grammar_path`/`grammar_format` under this crate's own documented DEFAULT
    /// compose-budget caps (`compose_budget::DEFAULT_*` -- the same defaults
    /// `ComposeBudget::from_env` falls back to when no `HC_COMPOSE_*` env var is set), explicit
    /// rather than reading env itself: the request is the single source of truth for what budget
    /// the CHILD process runs under, so a caller who wants a different envelope should build one
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
        }
    }

    /// Builds the `ComposeBudget` this request describes -- what `run_worker_child` compiles
    /// under. Mirrors `ComposeBudget::from_env`'s own field-by-field construction, just sourced
    /// from this request's fields instead of process env.
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
    /// A deterministic `ComposeBudget`/enumeration budget tripped before or during compilation; `detail` is the originating error's `Display` text, and `health` is empty for `FomaError::EnumerationBudgetExceeded` since that variant does not expose its `EmitReport`.
    BudgetTripped {
        detail: String,
        health: HealthReport,
    },
    /// The emitted lexc source itself failed to compile (`crate::analyzer::FomaError::LexcCompileFailed`) -- a grammar-content/emitter gap, not a resource budget.
    CompileFailed {
        detail: String,
        health: HealthReport,
    },
    /// The named grammar file could not be read, parsed, or compiled into a `pg_grammar::model::Grammar` at all.
    GrammarLoadFailed { detail: String },
    /// The request frame itself was malformed (wrong `protocol_version`, or valid JSON of the wrong shape) -- distinct from a grammar-content problem.
    ProtocolViolation { detail: String },
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
            return CompileWorkerOutcome::CompileFailed {
                detail: "the compile call panicked inside the worker process (caught here by \
                    catch_unwind; this does not protect against stack overflow or allocator OOM, \
                    which abort the process outright -- the supervisor's wall-timeout/exit-status \
                    checks are what catch those, reported as WorkerOutcome::ChildCrashed)"
                    .to_string(),
                health: HealthReport::new(Vec::new()),
            };
        }
    };

    match result {
        Ok(proposer) => {
            let health = crate::health_evaluator::evaluate_health(
                None,
                Some(&proposer.report),
                &[],
                &[],
                Some(&profile),
            );
            CompileWorkerOutcome::Success {
                final_state_count: profile.final_state_count,
                final_arc_count: profile.final_arc_count,
                uncovered_count: proposer.report.uncovered.len(),
                health,
            }
        }
        Err(FomaError::UnorderedOrderingMultiplicityExceeded { rule_count, limit }) => {
            let compose_error = ComposeError::OrderingMultiplicityExceeded {
                rule_count,
                limit,
                site: "worker-child compile (unordered-stratum cardinality gate)",
            };
            let health = crate::health_evaluator::evaluate_health(
                None,
                None,
                std::slice::from_ref(&compose_error),
                &[],
                Some(&profile),
            );
            CompileWorkerOutcome::BudgetTripped {
                detail: compose_error.to_string(),
                health,
            }
        }
        Err(err @ FomaError::EnumerationBudgetExceeded { .. }) => {
            // `FomaError::EnumerationBudgetExceeded` does not expose its `EmitReport`, so no real `HealthReport` can be built here without recomputing the compile; `detail` still carries the full message.
            CompileWorkerOutcome::BudgetTripped {
                detail: err.to_string(),
                health: HealthReport::new(Vec::new()),
            }
        }
        Err(err @ FomaError::LexcCompileFailed(_)) => {
            let health = match &err {
                FomaError::LexcCompileFailed(report) => crate::health_evaluator::evaluate_health(
                    None,
                    Some(report),
                    &[],
                    &[],
                    Some(&profile),
                ),
                _ => unreachable!(),
            };
            CompileWorkerOutcome::CompileFailed {
                detail: err.to_string(),
                health,
            }
        }
    }
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
/// mapping logic; only the supervisor's own kill/timeout/RSS behavior needs a real spawned process,
/// exercised by this crate's `tests/worker_supervisor.rs`).
pub fn run_worker_child<R: Read, W: Write>(mut input: R, mut output: W) -> io::Result<()> {
    let limits = V1_WORKER_LIMITS;

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

    let outcome = compile_grammar_from_request(&request);
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

/// The parent-requested wall-time/RSS envelope, clamped to `WorkerLimits`' absolute ceilings --
/// contractually clamps excessive values and provides no unlimited setting, applied here to the
/// watchdog envelope exactly as `compose_budget::clamp_chain_depth_cap` applies
/// it to the chain-depth dimension.
#[derive(Debug, Clone, Copy)]
pub struct WatchdogEnvelope {
    pub wall_timeout: Duration,
    pub rss_limit_mb: u64,
    pub rss_sample_interval: Duration,
}

impl WatchdogEnvelope {
    /// Clamps every field to `V1_WORKER_LIMITS`' absolute ceilings/floors -- a total function with
    /// no rejection path, mirroring `crate::compose_budget::clamp_chain_depth_cap`'s own "clamp,
    /// don't reject" convention.
    pub fn clamped(
        wall_timeout: Duration,
        rss_limit_mb: u64,
        rss_sample_interval: Duration,
    ) -> Self {
        let limits = V1_WORKER_LIMITS;
        let wall_timeout_ms = (wall_timeout.as_millis() as u64).min(limits.max_wall_timeout_ms);
        let rss_limit_mb = rss_limit_mb.min(limits.max_rss_limit_mb);
        let interval_ms =
            (rss_sample_interval.as_millis() as u64).max(limits.min_rss_sample_interval_ms);
        WatchdogEnvelope {
            wall_timeout: Duration::from_millis(wall_timeout_ms.max(1)),
            rss_limit_mb: rss_limit_mb.max(1),
            rss_sample_interval: Duration::from_millis(interval_ms),
        }
    }

    /// A generous default envelope for an interactive/offline diagnostic invocation (`pangloss pack
    /// --watchdog`): 2 minutes wall time, 4 GiB sampled-RSS guardrail, sampled every 200ms.
    /// Calibrated defaults for this envelope are later work (mirrors every other budget dimension's
    /// own "provisional, explicit, revisited by `calibrate-fst-resource-envelopes`" convention) --
    /// this is a deliberately conservative, documented placeholder, not a final number.
    pub fn default_envelope() -> Self {
        Self::clamped(Duration::from_secs(120), 4096, Duration::from_millis(200))
    }
}

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
    /// The child was killed after `elapsed` exceeded `limit` -- an outer host-safety watchdog, not the normal compiler-health cutoff; reaching this means an uninstrumented stall or too small an envelope.
    WallTimeoutKilled { elapsed: Duration, limit: Duration },
    /// A sampled RSS reading exceeded `limit_mb` and the child was killed; `sampled_mb` is the triggering sample and `peak_mb` the run's highest, but sampling is periodic, not a hard ceiling.
    RssLimitExceeded {
        sampled_mb: u64,
        limit_mb: u64,
        interval: Duration,
        peak_mb: u64,
    },
    /// Captured stdout or stderr reached its byte cap and the child was killed; all four wire streams have versioned limits enforced by the parent.
    OutputLimitExceeded {
        stream: OutputStream,
        limit_bytes: u64,
    },
    /// The child exited abnormally (panic-as-abort, stack overflow, allocator OOM, or an external kill) without producing a valid result frame; `detail` names the observed `ExitStatus` and why parsing failed.
    ChildCrashed { detail: String },
    /// `std::process::Command::spawn` itself failed (e.g. the child executable does not exist), distinct from every outcome above, which all require a live child process.
    SpawnFailed { detail: String },
    /// The request could not even be serialized/sized within `WorkerLimits::max_request_bytes`, so no child was spawned at all.
    ProtocolViolation { detail: String },
}

impl WorkerOutcome {
    /// Maps this outcome into the existing `HealthReport`/`HealthFinding` vocabulary (the
    /// fast-failure-primacy contract: the report must carry the effective envelope, the reached
    /// metric, and partial measurements where available) -- never a second, parallel report shape.
    /// `WorkerOutcome::Completed` returns
    /// the child's own real report unchanged; every other variant builds ONE synthetic finding
    /// describing the parent-observed watchdog event, reusing `FindingCode::ResourceBudgetReached`
    /// throughout.
    ///
    /// **Judgment call** (flagged, mirroring this crate's own "Judgment calls" convention in
    /// `health_evaluator.rs`): `FindingCode`'s ten registered codes are all compile/apply-work-
    /// shaped; none names "the parent's own wall-clock kill" or "a flooded output pipe" specifically.
    /// `FindingCode::ResourceBudgetReached` is the closest existing code -- every one of these
    /// outcomes IS a compilation attempt reaching an enforced boundary and stopping, exactly what
    /// that code's own registered meaning says -- reused rather than inventing per-outcome codes
    /// this schema's own "new codes only ever append, chosen to cover every dimension... without
    /// inventing per-construct codes no instrumentation exists to emit yet" discipline would flag as
    /// premature. `Metric::SampledCompileRssBytes` is the one genuinely new `Metric` variant
    /// this module appends (no existing variant names a sampled memory quantity at all -- see that
    /// variant's own doc).
    pub fn health_report(&self) -> HealthReport {
        match self {
            WorkerOutcome::Completed(outcome) => match outcome {
                CompileWorkerOutcome::Success { health, .. }
                | CompileWorkerOutcome::BudgetTripped { health, .. }
                | CompileWorkerOutcome::CompileFailed { health, .. } => health.clone(),
                CompileWorkerOutcome::GrammarLoadFailed { .. }
                | CompileWorkerOutcome::ProtocolViolation { .. } => HealthReport::new(Vec::new()),
            },
            WorkerOutcome::WallTimeoutKilled { elapsed, limit } => {
                HealthReport::new(vec![HealthFinding {
                    code: FindingCode::ResourceBudgetReached,
                    severity: Severity::Critical,
                    phase: Phase::Compile,
                    affected: Vec::new(),
                    metric: Metric::ElapsedMillis,
                    value: MetricValue::Millis(elapsed.as_millis() as u64),
                    provenance: ValueProvenance::Observed,
                    threshold: Some(MetricValue::Millis(limit.as_millis() as u64)),
                    explanation: format!(
                        "The compile worker process was killed after {elapsed:?} of wall-clock \
                         time (limit {limit:?}). This is the outer host-safety watchdog (R6), not \
                         the normal compiler-health cutoff -- reaching it means either a genuinely \
                         uninstrumented stall or an envelope set too small for legitimate work."
                    ),
                    remedies: Vec::new(),
                    override_record: None,
                }])
            }
            WorkerOutcome::RssLimitExceeded {
                sampled_mb,
                limit_mb,
                interval,
                peak_mb,
            } => HealthReport::new(vec![HealthFinding {
                code: FindingCode::ResourceBudgetReached,
                severity: Severity::Critical,
                phase: Phase::Compile,
                affected: Vec::new(),
                metric: Metric::SampledCompileRssBytes,
                value: MetricValue::Bytes(sampled_mb.saturating_mul(1024 * 1024)),
                provenance: ValueProvenance::Observed,
                threshold: Some(MetricValue::Bytes(limit_mb.saturating_mul(1024 * 1024))),
                explanation: format!(
                    "The compile worker process was killed after a sampled RSS reading of \
                     {sampled_mb} MiB exceeded its {limit_mb} MiB guardrail (sampled every \
                     {interval:?}; peak observed sample: {peak_mb} MiB). This is a SAMPLED value, \
                     not a hard memory ceiling -- allocation between samples means the process's \
                     real peak RSS could have been higher than any individual sample recorded."
                ),
                remedies: Vec::new(),
                override_record: None,
            }]),
            WorkerOutcome::OutputLimitExceeded {
                stream,
                limit_bytes,
            } => HealthReport::new(vec![HealthFinding {
                code: FindingCode::ResourceBudgetReached,
                severity: Severity::Critical,
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
                override_record: None,
            }]),
            WorkerOutcome::ChildCrashed { detail } => HealthReport::new(vec![HealthFinding {
                code: FindingCode::ResourceBudgetReached,
                severity: Severity::Critical,
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
                override_record: None,
            }]),
            WorkerOutcome::SpawnFailed { detail } | WorkerOutcome::ProtocolViolation { detail } => {
                HealthReport::new(vec![HealthFinding {
                    code: FindingCode::ResourceBudgetReached,
                    severity: Severity::Critical,
                    phase: Phase::Compile,
                    affected: Vec::new(),
                    metric: Metric::UnknownUnboundedWork,
                    value: MetricValue::Unbounded,
                    provenance: ValueProvenance::Observed,
                    threshold: None,
                    explanation: detail.clone(),
                    remedies: Vec::new(),
                    override_record: None,
                }])
            }
        }
    }
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
    let limits = V1_WORKER_LIMITS;
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
    decode_frame_body(&buf[8..]).map_err(|e| e.to_string())
}

/// Samples one process's RSS in mebibytes via `sysinfo`, refreshing only that PID; returns `None` if the process has already exited (a benign race, not an error).
fn sample_rss_mb(sys: &mut sysinfo::System, pid: sysinfo::Pid) -> Option<u64> {
    sys.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[pid]), true);
    sys.process(pid).map(|p| p.memory() / (1024 * 1024))
}

fn classify_exit(
    status: io::Result<ExitStatus>,
    stdout_buf: &Arc<Mutex<Vec<u8>>>,
) -> WorkerOutcome {
    let buf = stdout_buf.lock().unwrap_or_else(|e| e.into_inner()).clone();
    match parse_result_frame(&buf) {
        Ok(result) => WorkerOutcome::Completed(result.outcome),
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
/// until the child exits or `envelope` is breached -- whichever comes first -- returning exactly one
/// typed `WorkerOutcome`.
///
/// No Tokio, no process tree, no Job Objects/cgroups (platform parity, above) -- `std::process::Command`/`Child::
/// try_wait`/`Child::kill` plus two capped reader threads and a `sysinfo` sample per poll tick are
/// the entire mechanism. Windows-compatible: every API used here (`Command`, `Child::kill`/
/// `try_wait`, `sysinfo::System`) is cross-platform in the standard library / `sysinfo` itself, with
/// no Unix-only assumption (no signals, no `/proc` path assumed directly).
pub fn run_compile_worker(
    child_exe: &Path,
    child_args: &[String],
    request: &CompileWorkerRequest,
    envelope: &WatchdogEnvelope,
) -> WorkerOutcome {
    let limits = V1_WORKER_LIMITS;

    let request_json = match serde_json::to_vec(request) {
        Ok(bytes) => bytes,
        Err(e) => {
            return WorkerOutcome::ProtocolViolation {
                detail: format!("failed to serialize compile-worker request: {e}"),
            };
        }
    };
    if request_json.len() as u64 > limits.max_request_bytes {
        return WorkerOutcome::ProtocolViolation {
            detail: format!(
                "request is {} byte(s), exceeding the {}-byte protocol limit",
                request_json.len(),
                limits.max_request_bytes
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

    let pid = child.id();
    let sysinfo_pid = sysinfo::Pid::from_u32(pid);

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
        limits.max_result_bytes.saturating_add(8), // frame header + body
        Arc::clone(&stdout_buf),
        Arc::clone(&stdout_overflow),
    );

    let stderr_buf = Arc::new(Mutex::new(Vec::new()));
    let stderr_overflow = Arc::new(AtomicBool::new(false));
    let stderr_handle = spawn_capped_reader(
        stderr,
        limits.max_captured_stderr_bytes,
        Arc::clone(&stderr_buf),
        Arc::clone(&stderr_overflow),
    );

    let mut sys = sysinfo::System::new();
    let mut peak_rss_mb: u64 = 0;
    let start = Instant::now();
    let poll_interval = envelope.rss_sample_interval.min(Duration::from_millis(50));

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
                limit_bytes: limits.max_result_bytes,
            };
        }
        if stderr_overflow.load(Ordering::SeqCst) {
            let _ = child.kill();
            let _ = child.wait();
            break WorkerOutcome::OutputLimitExceeded {
                stream: OutputStream::Stderr,
                limit_bytes: limits.max_captured_stderr_bytes,
            };
        }

        let elapsed = start.elapsed();
        if elapsed >= envelope.wall_timeout {
            let _ = child.kill();
            let _ = child.wait();
            break WorkerOutcome::WallTimeoutKilled {
                elapsed,
                limit: envelope.wall_timeout,
            };
        }

        if let Some(rss_mb) = sample_rss_mb(&mut sys, sysinfo_pid) {
            peak_rss_mb = peak_rss_mb.max(rss_mb);
            if rss_mb > envelope.rss_limit_mb {
                let _ = child.kill();
                let _ = child.wait();
                break WorkerOutcome::RssLimitExceeded {
                    sampled_mb: rss_mb,
                    limit_mb: envelope.rss_limit_mb,
                    interval: envelope.rss_sample_interval,
                    peak_mb: peak_rss_mb,
                };
            }
        }

        std::thread::sleep(poll_interval);
    };

    let _ = stdin_handle.join();
    let _ = stdout_handle.join();
    let _ = stderr_handle.join();
    let _ = stderr_buf; // captured for future diagnostics surfacing; not read on every path today.

    outcome
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let huge = V1_WORKER_LIMITS.max_request_bytes + 1;
        buf.extend_from_slice(&huge.to_le_bytes());
        // No body bytes -- proves rejection happens strictly from the header, before any attempt to `read_exact` a body.
        let mut cursor = std::io::Cursor::new(buf);
        let err = read_frame(&mut cursor, V1_WORKER_LIMITS.max_request_bytes)
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
    fn parse_result_frame_rejects_declared_length_over_limit() {
        let mut buf = Vec::new();
        let huge = V1_WORKER_LIMITS.max_result_bytes + 1;
        buf.extend_from_slice(&huge.to_le_bytes());
        let err = parse_result_frame(&buf).expect_err("must reject");
        assert!(err.contains("exceeds"));
    }

    #[test]
    fn parse_result_frame_rejects_short_buffer() {
        let err = parse_result_frame(&[1, 2, 3]).expect_err("too short");
        assert!(err.contains("too short"));
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
        buf.extend_from_slice(&(V1_WORKER_LIMITS.max_request_bytes + 1).to_le_bytes());
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
                assert_eq!(health.admission(), Severity::Critical);
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
                assert_eq!(health.admission(), Severity::Ideal);
            }
            other => panic!("expected Success, got {other:?}"),
        }
        let _ = std::fs::remove_file(&path);
    }

    // Envelope clamping.

    #[test]
    fn watchdog_envelope_clamps_wall_timeout_to_absolute_ceiling() {
        let envelope = WatchdogEnvelope::clamped(
            Duration::from_millis(V1_WORKER_LIMITS.max_wall_timeout_ms + 1_000_000),
            1,
            Duration::from_millis(1),
        );
        assert_eq!(
            envelope.wall_timeout,
            Duration::from_millis(V1_WORKER_LIMITS.max_wall_timeout_ms)
        );
    }

    #[test]
    fn watchdog_envelope_clamps_rss_limit_to_absolute_ceiling() {
        let envelope = WatchdogEnvelope::clamped(
            Duration::from_secs(1),
            V1_WORKER_LIMITS.max_rss_limit_mb + 1_000_000,
            Duration::from_millis(1),
        );
        assert_eq!(envelope.rss_limit_mb, V1_WORKER_LIMITS.max_rss_limit_mb);
    }

    #[test]
    fn watchdog_envelope_floors_sample_interval() {
        let envelope = WatchdogEnvelope::clamped(Duration::from_secs(1), 1024, Duration::ZERO);
        assert_eq!(
            envelope.rss_sample_interval,
            Duration::from_millis(V1_WORKER_LIMITS.min_rss_sample_interval_ms)
        );
    }

    // RSS sampling only; a live breach-and-kill needs a real adversarial process, exercised end-to-end in `tests/worker_supervisor.rs`.

    #[test]
    fn sample_rss_mb_reports_a_nonzero_value_for_this_process() {
        let mut sys = sysinfo::System::new();
        let pid = sysinfo::Pid::from_u32(std::process::id());
        let sampled = sample_rss_mb(&mut sys, pid);
        assert!(
            sampled.is_some(),
            "must find this running process's own PID"
        );
    }

    #[test]
    fn sample_rss_mb_returns_none_for_an_implausible_pid() {
        // A PID vanishingly unlikely to be running on any test host.
        let mut sys = sysinfo::System::new();
        let pid = sysinfo::Pid::from_u32(u32::MAX - 1);
        assert_eq!(sample_rss_mb(&mut sys, pid), None);
    }

    // WorkerOutcome -> HealthReport mapping.

    #[test]
    fn worker_outcome_wall_timeout_maps_to_critical_resource_budget_reached() {
        let outcome = WorkerOutcome::WallTimeoutKilled {
            elapsed: Duration::from_secs(5),
            limit: Duration::from_secs(2),
        };
        let health = outcome.health_report();
        assert_eq!(health.admission(), Severity::Critical);
        assert_eq!(health.findings[0].code, FindingCode::ResourceBudgetReached);
        assert_eq!(health.findings[0].metric, Metric::ElapsedMillis);
    }

    #[test]
    fn worker_outcome_rss_exceeded_maps_to_sampled_compile_rss_bytes_and_says_not_a_ceiling() {
        let outcome = WorkerOutcome::RssLimitExceeded {
            sampled_mb: 500,
            limit_mb: 400,
            interval: Duration::from_millis(200),
            peak_mb: 500,
        };
        let health = outcome.health_report();
        assert_eq!(health.findings[0].metric, Metric::SampledCompileRssBytes);
        assert!(health.findings[0]
            .explanation
            .contains("not a hard memory ceiling"));
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
}
