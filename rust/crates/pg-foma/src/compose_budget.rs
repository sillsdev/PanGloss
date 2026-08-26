//! Composition-path budget guards:
//! [`EnumerationBudget`](crate::morphotactics::EnumerationBudget)'s sibling for the composition
//! path (`crate::replace`, `crate::gate`, `crate::uflexc`) -- a path that, until this module, had
//! **no `Result`-returning public API at all** (bare `Option`/`Fsm`/report structs, example drivers
//! `panic!` on failure) and never imported `EnumerationBudget` (the eager-enumeration path's own
//! guard covers `crate::preexpand`/`crate::emit` only, zero references from the composition path).
//!
//! ## Why this budget looks different from [`EnumerationBudget`](crate::morphotactics::EnumerationBudget)
//! `EnumerationBudget` is a shared, cross-thread `AtomicUsize` latch because `crate::preexpand`'s
//! composite builders run their per-root work across a rayon pool. The composition cascade
//! (`crate::replace`'s per-alpha-tuple/per-rule fold, `crate::gate`'s per-group loop) is strictly
//! sequential -- no rayon anywhere in this path -- so a plain `&ComposeBudget` with no atomics and
//! no interior mutability is sufficient; every checked wrapper below just reads `self`'s cap fields
//! and compares them against the `Fsm` the vendored `foma` crate handed back. Revisit this if the
//! per-group loop (`crate::gate::compile_gated_grammar`) is ever parallelized.
//!
//! ## Vendored-crate findings this module's shape depends on (verified by reading
//! `foma = "=0.4.0"`'s own source, not inferred)
//! - `Fsm` exposes `pub statecount: i32` / `pub arccount: i32` (`types.rs`) -- size checks are free
//!   after every call.
//! - **No mid-operation hook exists anywhere**: `fsm_compose`, `fsm_union`, `fsm_minimize` are
//!   synchronous tight loops with no callback/cancellation point. A between-step size check
//!   therefore cannot catch a blow-up INSIDE one call -- see the module-level limitations note
//!   below.
//! - `fsm_compose` **internally minimizes both operands** before composing (`products.rs`) -- every
//!   compose step already pays a determinize (worst-case exponential), so the real risk hides
//!   inside every ordinary compose call, not just at an explicit final minimize.
//! - `fsm_union` does **not** minimize (cheap per step) -- `crate::gate`'s per-group union fold
//!   accumulates a non-minimal net whose eventual minimize is the true worst-case moment.
//!
//! ## Limitations (the honest part, not hidden)
//! - A between-step size check cannot catch a blow-up INSIDE one call: if a single compose/minimize
//!   call OOMs or spins, the check that would run after it never runs. There is nothing in the
//!   vendored crate to checkpoint mid-call; the size caps only bound cost accumulating ACROSS calls.
//! - `call_with_deadline`'s wall-clock wrapper *detects*, it does not *stop*: the worker thread is
//!   abandoned (never joined) and keeps running/allocating until it finishes naturally. Treat
//!   `ComposeError::ComposeStepTimedOut` as TERMINAL for that grammar (fall back to another
//!   engine), never retry the identical call; a long-lived server embedding this must track
//!   abandoned-thread count itself.
//! - `catch_unwind` is not a safety net for this module either: stack-overflow and allocator-OOM
//!   abort the process, bypassing every check here. Large worker-thread stacks reduce but do not
//!   eliminate overflow risk.
//! - Full "never blow up" for a single adversarial call needs an external supervisor process --
//!   out of scope here.

use std::fmt;
use std::time::Duration;

use foma::constructions::{fsm_compose, fsm_union};
use foma::minimize::fsm_minimize;
use foma::options::FomaOptions;
use foma::types::Fsm;

/// Compile-time proof that `Fsm` is `Send`, required to move it into `call_with_deadline`'s worker thread; a future non-`Send` field breaks this build rather than silently miscompiling.
#[allow(dead_code)]
fn assert_send<T: Send>() {}
const _: fn() = || {
    assert_send::<Fsm>();
};

// Defaults / env overrides: `HC_*`-prefixed, parsed as the field's own type, falling back to the documented default when unset/unparsable.

/// `HC_COMPOSE_STATE_BUDGET`: ceiling on `Fsm::statecount` after any checked compose/union/minimize
/// call. Calibration basis (measured on the
/// `emit_underlying_templated` + `crate::replace`/`crate::gate` path against the real Aweti grammar
/// -- 855 entries, 135 mrules, 18 prules, 14 templates -- the largest real grammar this path has
/// been run against as of this writing): Aweti's templated lexc alone compiles to 23,661 states;
/// the FULL `lexc .o. rules .o. boundary-cleanup` composition, minimized, is 35,846 states. This
/// default sits ~56x above that measured ceiling -- generous headroom for a larger real grammar,
/// while staying far below the eager-enumeration path's own disaster case (`EnumerationBudget`'s
/// own doc: Aweti's *enumeration* path produces an ~8.8GB `apply_up` allocation). Refine with more
/// real-grammar measurements as larger grammars are onboarded.
pub(crate) const DEFAULT_STATE_BUDGET: usize = 2_000_000;

/// `HC_COMPOSE_ARC_BUDGET`: ceiling on `Fsm::arccount`, same call sites as
/// `DEFAULT_STATE_BUDGET`. Calibration basis (same Aweti run): the
/// templated lexc alone is 346,727 arcs; the full composed+minimized network is 800,354 arcs. This
/// default sits ~25x above that measured ceiling.
pub(crate) const DEFAULT_ARC_BUDGET: usize = 20_000_000;

/// `HC_COMPOSE_TUPLE_BUDGET`: ceiling on the number of alpha-tuple assignments
/// (`crate::replace::resolve_alpha_tuples`'s `surviving` count) a single subrule may expand to
/// before `compile_rewrite_rule_subset` starts folding them -- checked BEFORE
/// the expensive per-tuple compile loop, the same "check the search result before the expensive
/// part" shape `EnumerationBudget`'s own doc uses. Default 5,000: Amharic's real worst case (the
/// 20-alpha-variable CV-merger) is `nc15=59 x nc16=6 <= 354` surviving tuples -- comfortably under
/// this cap by ~14x.
pub(crate) const DEFAULT_TUPLE_BUDGET: usize = 5_000;

/// `HC_COMPOSE_GROUP_BUDGET`: ceiling on `crate::gate::partition_entries`'s own group count, checked
/// BEFORE any per-group compile work runs -- the single highest-leverage check
/// in this module, since it gates all downstream work for every group. Default 64:
/// Indonesian (this prototype's only real gated grammar today) needs exactly 2 groups; a grammar
/// with `k` gated subrules is bounded by `2^k` DISTINCT gating vectors in the worst case, so 64
/// covers up to 6 simultaneously-gated subrules with every combination realized -- comfortably
/// above every reference grammar's real gated-subrule count (Indonesian: 1; Amharic: 3) while still
/// catching a pathological grammar before any group's lexc/rule compile even starts. **No graceful
/// fallback by design**: merging/dropping groups is unsound (over/under-firing
/// gated rules), so a breach here always means "fall back to another engine for this grammar", never
/// a partial group set.
pub(crate) const DEFAULT_GROUP_BUDGET: usize = 64;

/// `HC_COMPOSE_LINE_BUDGET`: ceiling on lexc lines written by `crate::emit::emit_underlying_templated`
/// (checked incrementally, per-group, so a pathological templated
/// grammar bails during the FIRST group's emission rather than after building a multi-GB string) and
/// by `crate::uflexc::emit_underlying_filtered` (the same check, Indonesian-scoped emitter,
/// checked incrementally at its own root/prefix/suffix line-push sites). Calibration basis: Aweti's
/// real templated lexc emission is 37,510 lines (measured via
/// `examples/p6_aweti_replace_prototype.rs`'s own `counts.lexc_lines`, `TextMode::UnderlyingTokens`
/// path). This default sits ~26x above that measured ceiling -- generous headroom while
/// staying far below a multi-GB `.lexc` file (`EnumerationBudget`'s own doc cites a 691MB/9.7M-line
/// lexc as the eager-enumeration path's disaster case for this same grammar).
pub(crate) const DEFAULT_LINE_BUDGET: usize = 1_000_000;

pub(crate) fn state_budget_from_env() -> usize {
    std::env::var("HC_COMPOSE_STATE_BUDGET")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_STATE_BUDGET)
}

pub(crate) fn arc_budget_from_env() -> usize {
    std::env::var("HC_COMPOSE_ARC_BUDGET")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_ARC_BUDGET)
}

pub(crate) fn tuple_budget_from_env() -> usize {
    std::env::var("HC_COMPOSE_TUPLE_BUDGET")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_TUPLE_BUDGET)
}

pub(crate) fn group_budget_from_env() -> usize {
    std::env::var("HC_COMPOSE_GROUP_BUDGET")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_GROUP_BUDGET)
}

pub(crate) fn line_budget_from_env() -> usize {
    std::env::var("HC_COMPOSE_LINE_BUDGET")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_LINE_BUDGET)
}

/// `HC_COMPOSE_STEP_TIMEOUT_MS`: wall-clock deadline for every checked
/// compose/union/minimize call, via `call_with_deadline`. **Default OFF** (`None`) -- unlike the
/// four size caps above (default ON, mirroring `EnumerationBudget`'s own always-live convention),
/// this mirrors `pg-rules/src/stratum.rs`'s `StepBudget`'s own opt-in convention: a wall-clock
/// abandon-on-timeout mechanism is a much bigger hammer (it detects, but does not stop, a runaway
/// call) than a size check, so it stays opt-in until a caller has a concrete reason to want it.
pub(crate) fn step_timeout_from_env() -> Option<Duration> {
    std::env::var("HC_COMPOSE_STEP_TIMEOUT_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .map(Duration::from_millis)
}

// Chain-depth dimension: closes stack overflow from a deep derivation/unapplication chain, but only where wired (`check_chain_depth`'s
// callers) and off by default. See docs/research/pg-foma-compose-budget-design-notes.md for scope and why the default is off.

/// Absolute ceiling for the chain-depth dimension: a
/// versioned, hard-coded, deliberately high non-disableable limit above all default, app, and
/// caller limits — an emergency containment boundary, not a normal operating target. No
/// configured cap -- from `chain_depth_cap_from_env` or `ComposeBudget::with_chain_depth_cap`
/// -- may exceed this value; both clamp down to it rather than reject, the same "contractually
/// clamp excessive values, provide no unlimited setting" discipline every budget dimension in this
/// module follows. There is no way to configure an unlimited *cap*; the
/// schema-level `None` default means "no cap configured yet," never "unlimited by
/// request."
///
/// Ceiling schema version 1 -- bump only via a reviewed commit, the same "evidence + proposed diff
/// + human-reviewed commit" discipline every calibrated default in this module uses. Chosen
/// deliberately high relative to any plausible calibrated default: the motivating case
/// is Aweti's real 24-level derivation chain, so a ceiling many orders of
/// magnitude above 24 leaves enormous headroom below this emergency boundary for whatever default
/// the later calibration change lands on.
pub(crate) const CHAIN_DEPTH_ABSOLUTE_CEILING: usize = 1_000_000;

/// `HC_COMPOSE_CHAIN_DEPTH_BUDGET`: per-word derivation/unapplication chain-depth cap.
/// **Default `None` (unbounded/off)** -- see this section's module doc for why this dimension
/// mirrors `step_timeout_from_env`'s opt-in shape rather than the four size caps' default-ON
/// shape. When set, parses as `usize` and is clamped to `CHAIN_DEPTH_ABSOLUTE_CEILING`
/// (unparsable or unset falls back to `None`, exactly like every other `_from_env` function in
/// this module falls back to its own default on a parse failure).
pub(crate) fn chain_depth_cap_from_env() -> Option<usize> {
    std::env::var("HC_COMPOSE_CHAIN_DEPTH_BUDGET")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .map(clamp_chain_depth_cap)
}

/// The clamp `chain_depth_cap_from_env` and `ComposeBudget::with_chain_depth_cap` both apply:
/// pulled into its own pure function so this module's tests can exercise the clamp arithmetic
/// directly without touching process-global env state (this module's own "explicit-caps
/// constructors, never env vars" test convention, `ComposeBudget::with_caps`'s doc).
pub(crate) fn clamp_chain_depth_cap(configured: usize) -> usize {
    configured.min(CHAIN_DEPTH_ABSOLUTE_CEILING)
}

// Ordering-multiplicity dimension: bounds an `Unordered` stratum's own combinatorial rule-ordering walk, a distinct quantity from
// chain depth. See docs/research/pg-foma-compose-budget-design-notes.md for the judgment call and the calibration basis.
pub(crate) const DEFAULT_ORDERING_MULTIPLICITY_BUDGET: usize = 100;

pub(crate) fn ordering_multiplicity_budget_from_env() -> usize {
    std::env::var("HC_COMPOSE_ORDERING_MULTIPLICITY_BUDGET")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_ORDERING_MULTIPLICITY_BUDGET)
}

// Apply-path dimension: in-process cooperative magnitude counting for `apply_up`'s decode loop, since a watchdog cannot safely
// hard-kill a thread serving the caller. See docs/research/pg-foma-compose-budget-design-notes.md for why and what it closes.

/// `HC_APPLY_PATH_BUDGET`: ceiling on the raw number of `apply_up` result strings
/// `crate::analyzer::FomaProposer::propose_budgeted` decodes for one word, checked as they are
/// produced (before `tags::decode_path` even runs on the current one) -- the "check before the
/// expensive part" discipline this module's every other dimension already uses, applied to the
/// cheapest possible per-item test (an integer compare) so the cap itself never becomes the cost
/// center it exists to prevent.
pub(crate) fn apply_path_budget_from_env() -> Option<usize> {
    std::env::var("HC_APPLY_PATH_BUDGET")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
}

/// `HC_APPLY_CANDIDATE_BUDGET`: ceiling on the number of DISTINCT `(morphemes, root_index)`
/// candidates `crate::analyzer::FomaProposer::propose_budgeted` has accumulated for one word,
/// checked immediately after each new candidate is inserted into the dedup set -- catches a
/// network that decodes few raw paths but each path fans out into many distinct candidates (e.g. a
/// heavily compounding/templated grammar), a case `apply_path_budget_from_env`'s raw-path count
/// alone would not bound.
pub(crate) fn apply_candidate_budget_from_env() -> Option<usize> {
    std::env::var("HC_APPLY_CANDIDATE_BUDGET")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
}

// The apply-path dimension's first calibrated default, resolved only by `backend_runtime::RuntimeBudget` (ordinary `pangloss`
// analysis is untouched). See docs/research/pg-foma-compose-budget-design-notes.md for the derivation of 1,000,000.
pub const DEFAULT_EVALUATION_APPLY_PATH_BUDGET: usize = 1_000_000;

/// The distinct-candidate half of `DEFAULT_EVALUATION_APPLY_PATH_BUDGET`'s calibration.
///
/// Set to the same figure deliberately. On the measured fixture the two counts are EQUAL
/// (2,985,984 raw paths, 2,985,984 distinct candidates) because every path decodes to a distinct
/// morpheme sequence, so nothing in the evidence distinguishes them; splitting them would imply a
/// calibration nobody has performed. They stay separate fields because they bound genuinely
/// different shapes (see `apply_candidate_budget_from_env`'s own doc), and a future measurement
/// can move one without the other.
pub const DEFAULT_EVALUATION_APPLY_CANDIDATE_BUDGET: usize = 1_000_000;

/// Which magnitude `ApplyOutcome::Incomplete` reports tripped. Mirrors `NetSizeMeasure`'s own
/// `label()` shape (a stable, short string for a caller-facing message/report field) one level up
/// from the compile-time size dimensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyDimension {
    /// `ApplyBudget::path_cap`: raw `apply_up` result count.
    DecodedPaths,
    /// `ApplyBudget::candidate_cap`: distinct `(morphemes, root_index)` candidate count.
    Candidates,
}

impl ApplyDimension {
    pub const fn label(self) -> &'static str {
        match self {
            ApplyDimension::DecodedPaths => "decoded apply_up paths",
            ApplyDimension::Candidates => "distinct candidates",
        }
    }
}

/// The "a word either completes (possibly with zero analyses) or returns a typed
/// incomplete outcome naming the dimension and value it hit" contract, generic over the completed
/// payload (`Vec<Candidate>` for `crate::analyzer::FomaProposer::propose_budgeted`). Deliberately
/// NOT a `Result`/`ComposeError`: this is not a compile-time failure to surface as `Err` up a
/// `?`-chain, it is a normal, expected, reportable per-word outcome a diagnostic caller inspects
/// directly -- a word either completes or returns a typed
/// incomplete outcome, never "fails".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyOutcome<T> {
    /// The budget never tripped; a complete result, possibly with zero analyses.
    Complete(T),
    /// A magnitude cap was reached; `value` is the count at the moment of the trip (always `limit + 1` by construction).
    Incomplete {
        dimension: ApplyDimension,
        value: usize,
        limit: usize,
    },
}

/// In-process, cooperative, magnitude-only apply-path budget (this section's own module
/// doc). Unlike `ComposeBudget` (compile-time, checked between whole-network operations), both
/// dimensions here are checked per-item inside a single word's decode loop -- see
/// `crate::analyzer::FomaProposer::propose_budgeted`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ApplyBudget {
    path_cap: Option<usize>,
    candidate_cap: Option<usize>,
}

impl ApplyBudget {
    /// Production entry point: both caps from their own `HC_APPLY_*` env var, `None`
    /// (unbounded/off) when unset or unparsable -- mirrors `chain_depth_cap_from_env`'s own
    /// "no calibrated default yet" shape, not the four always-on compile-time size caps.
    pub fn from_env() -> Self {
        ApplyBudget {
            path_cap: apply_path_budget_from_env(),
            candidate_cap: apply_candidate_budget_from_env(),
        }
    }

    /// Explicit-caps constructor for tests and callers (e.g. a diagnostic report) that want a
    /// deterministic
    /// cap without touching process-global env state -- this module's own "explicit-caps
    /// constructors, never env vars" test convention.
    pub fn with_caps(path_cap: Option<usize>, candidate_cap: Option<usize>) -> Self {
        ApplyBudget {
            path_cap,
            candidate_cap,
        }
    }

    /// A budget that can never trip -- what `crate::analyzer::FomaProposer::propose` uses
    /// internally so its own behavior is provably unchanged by this addition.
    pub fn unbounded() -> Self {
        ApplyBudget {
            path_cap: None,
            candidate_cap: None,
        }
    }

    /// The effective decoded-path cap, or `None` for unbounded.
    ///
    /// Public because an assessment report records the envelope that was actually in force, and
    /// reading it off the budget is the only way to record one that arrived from the environment
    /// rather than from a command-line flag.
    pub fn path_cap(&self) -> Option<usize> {
        self.path_cap
    }

    /// The effective distinct-candidate cap, or `None` for unbounded.
    pub fn candidate_cap(&self) -> Option<usize> {
        self.candidate_cap
    }
}

/// Which size measure `ComposeError::NetSizeExceeded` reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetSizeMeasure {
    States,
    Arcs,
}

impl NetSizeMeasure {
    pub(crate) fn label(self) -> &'static str {
        match self {
            NetSizeMeasure::States => "states",
            NetSizeMeasure::Arcs => "arcs",
        }
    }
}

/// Every way a `ComposeBudget`-checked call can fail. Each variant carries enough
/// to build a specific, honest message -- never a generic "something blew up" string -- and names
/// the dimension it guards against in its own doc line.
#[derive(Debug, Clone)]
pub enum ComposeError {
    /// A checked compose/union/minimize call returned a network exceeding `state_cap`/`arc_cap`; `site` names the call site.
    NetSizeExceeded {
        measure: NetSizeMeasure,
        value: i32,
        limit: usize,
        site: &'static str,
    },
    /// `resolve_alpha_tuples` produced more surviving assignments than `tuple_cap`, checked before the per-tuple compile loop runs.
    AlphaTupleBudgetExceeded {
        surviving: usize,
        limit: usize,
        rule_xml_id: String,
    },
    /// `partition_entries` produced more groups than `group_cap`, checked before any per-group compile work runs.
    GroupBudgetExceeded {
        groups: usize,
        limit: usize,
        gated_subrules: usize,
    },
    /// A templated/underlying-form lexc emitter wrote more lines than `line_cap`, checked incrementally per group.
    EmitLineBudgetExceeded { lines: usize, limit: usize },
    /// `call_with_deadline` timed out; the worker thread is abandoned, not killed. Always terminal for this grammar; never retry.
    ComposeStepTimedOut {
        elapsed: Duration,
        limit: Duration,
        site: &'static str,
    },
    /// `check_chain_depth` found a cumulative derivation/unapplication step count exceeding `chain_depth_cap`.
    ChainDepthExceeded {
        depth: usize,
        limit: usize,
        site: &'static str,
    },
    /// `check_ordering_multiplicity` found an `Unordered` stratum's loose-rule count exceeding `ordering_multiplicity_cap`.
    OrderingMultiplicityExceeded {
        rule_count: usize,
        limit: usize,
        site: &'static str,
    },
}

impl fmt::Display for ComposeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ComposeError::NetSizeExceeded {
                measure,
                value,
                limit,
                site,
            } => write!(
                f,
                "composition budget exceeded at {site:?}: {value} {measure} (limit {limit}). This \
                 grammar's phonological-rule/gated-lexicon composition produces a network larger \
                 than this path's size budget allows -- use the default (full) morphological-parser \
                 engine for this grammar instead of the P6 composition path, or -- only if you \
                 understand why this grammar's composed network is this large -- raise the budget \
                 via HC_COMPOSE_STATE_BUDGET/HC_COMPOSE_ARC_BUDGET and re-run.",
                measure = measure.label()
            ),
            ComposeError::AlphaTupleBudgetExceeded {
                surviving,
                limit,
                rule_xml_id,
            } => write!(
                f,
                "alpha-tuple budget exceeded for rule {rule_xml_id:?}: {surviving} surviving tuple \
                 assignments (limit {limit}). Raise HC_COMPOSE_TUPLE_BUDGET only if you understand \
                 why this rule's alpha-variable joint-agreement constraint admits this many tuples."
            ),
            ComposeError::GroupBudgetExceeded {
                groups,
                limit,
                gated_subrules,
            } => write!(
                f,
                "gating group budget exceeded: {groups} distinct gating groups from {gated_subrules} \
                 gated subrule(s) (limit {limit}). Merging or dropping groups is unsound (it would \
                 over- or under-fire a gated rule), so this is always a fall-back-engine signal, \
                 never a partial result; raise HC_COMPOSE_GROUP_BUDGET only if you understand why \
                 this grammar realizes this many distinct gating vectors."
            ),
            ComposeError::EmitLineBudgetExceeded { lines, limit } => write!(
                f,
                "lexc emission line budget exceeded: {lines} lines written (limit {limit}). This \
                 grammar's templated/underlying-form lexc emission produces more literal lexc \
                 material than this path's line budget allows; raise HC_COMPOSE_LINE_BUDGET only if \
                 you understand why this grammar's emission is this large."
            ),
            ComposeError::ComposeStepTimedOut {
                elapsed,
                limit,
                site,
            } => write!(
                f,
                "composition step at {site:?} exceeded its wall-clock deadline ({elapsed:?} elapsed, \
                 limit {limit:?}). The worker thread computing it was ABANDONED (not killed) and may \
                 still be running/allocating -- treat this as TERMINAL for this grammar, never retry \
                 the identical call; use the default (full) morphological-parser engine instead."
            ),
            ComposeError::ChainDepthExceeded { depth, limit, site } => write!(
                f,
                "chain-depth budget exceeded at {site:?}: {depth} nested derivation/unapplication \
                 steps (limit {limit}). This deterministically closes the stack-overflow failure \
                 class (ADR 0003; docs/adr/0003-apply-time-containment.md) instead of relying on a \
                 larger call stack -- raise HC_COMPOSE_CHAIN_DEPTH_BUDGET only if you understand why \
                 this grammar's derivation chain is this deep."
            ),
            ComposeError::OrderingMultiplicityExceeded {
                rule_count,
                limit,
                site,
            } => write!(
                f,
                "ordering-multiplicity budget exceeded at {site:?}: {rule_count} loose rules in an \
                 Unordered stratum (limit {limit}). MorphRuleOrder::Unordered's any-order/any-subset \
                 combination cascade (pg_rules::cascade::Cascade::combination) admits up to a \
                 factorial (or, under multi-application, exponential) number of admissible rule \
                 orderings in the loose-rule count -- this grammar's own unordered-application.\
                 unbounded configuration is honestly unsupported, never silently truncated; raise \
                 HC_COMPOSE_ORDERING_MULTIPLICITY_BUDGET only if you understand why this stratum's \
                 rule count is this large."
            ),
        }
    }
}

impl std::error::Error for ComposeError {}

/// Default-on composition-path budget (design doc §2): four size/count caps checked eagerly (state,
/// arc, alpha-tuple, gating-group), plus an opt-in wall-clock deadline. Unlike
/// `crate::morphotactics::EnumerationBudget`, this holds no atomics/interior mutability at all --
/// module doc explains why a plain value is sufficient for this strictly-sequential path.
#[derive(Debug, Clone, Copy)]
pub struct ComposeBudget {
    pub(crate) state_cap: usize,
    pub(crate) arc_cap: usize,
    pub(crate) tuple_cap: usize,
    pub(crate) group_cap: usize,
    pub(crate) line_cap: usize,
    pub(crate) step_timeout: Option<Duration>,
    /// This crate's chain-depth dimension (this module's "Chain-depth dimension" section). `None`
    /// (the default everywhere -- `Self::from_env`, `Self::with_caps`, `Self::unbounded`)
    /// means unbounded/off, at any depth -- pinned by `chain_depth_unbounded_budget_never_trips`
    /// -- so no existing caller's behavior changes. `Some(limit)` is already clamped to
    /// `CHAIN_DEPTH_ABSOLUTE_CEILING` by whichever constructor set it.
    ///
    /// **Read by production code**: `crate::peel::ReduplicationPeeler`'s nested-reduplication
    /// recursion is wired through
    /// `Self::check_chain_depth` (`crate::peel`'s own module doc, "Chain depth and nested
    /// reduplication" section) -- the only real (non-test) consumer of this dimension. Still
    /// `None` everywhere `Self::from_env` is called with `HC_COMPOSE_CHAIN_DEPTH_BUDGET` unset
    /// (the production default), so every existing caller's behavior is unchanged until an
    /// operator opts in.
    pub(crate) chain_depth_cap: Option<usize>,
    /// This module's own "Ordering-multiplicity dimension" extension: `Some(cap)` bounds
    /// an `Unordered` stratum's own
    /// loose-rule count; `None` means unbounded/off. Unlike `Self::chain_depth_cap` (default
    /// `None`, uncalibrated), `Self::from_env` defaults this to
    /// `Some(DEFAULT_ORDERING_MULTIPLICITY_BUDGET)` -- a real, if conservative, calibrated default
    /// ships with THIS change (mirroring the four size caps' own default-ON convention), since
    /// promoting `unordered-application.chain-depth-bounded` off `Refuse` needs a concrete
    /// bound to promote AGAINST, not an uncalibrated placeholder. `Self::with_caps`/
    /// `Self::unbounded` leave it `None` (mirrors `Self::chain_depth_cap`'s own "tests opt in
    /// via an explicit builder" convention) -- use `Self::with_ordering_multiplicity_cap`.
    pub(crate) ordering_multiplicity_cap: Option<usize>,
}

impl ComposeBudget {
    /// Production entry point: every cap from its own `HC_COMPOSE_*` env var (module doc), or the
    /// documented default when unset/unparsable. Mirrors `EnumerationBudget::from_env`'s own
    /// "read env exactly once, in the production entry point" convention -- tests should use
    /// `Self::with_caps` instead, so parallel test processes never race process-global env state.
    pub fn from_env() -> Self {
        ComposeBudget {
            state_cap: state_budget_from_env(),
            arc_cap: arc_budget_from_env(),
            tuple_cap: tuple_budget_from_env(),
            group_cap: group_budget_from_env(),
            line_cap: line_budget_from_env(),
            step_timeout: step_timeout_from_env(),
            chain_depth_cap: chain_depth_cap_from_env(),
            ordering_multiplicity_cap: Some(ordering_multiplicity_budget_from_env()),
        }
    }

    /// Overrides the per-composition-step deadline while retaining every configured size cap.
    pub fn with_step_timeout(mut self, timeout: Option<Duration>) -> Self {
        self.step_timeout = timeout;
        self
    }

    /// Explicit-caps constructor -- what tests use ("explicit-caps constructors,
    /// never env vars"), and what `Self::from_env` builds internally.
    ///
    /// Does not take a chain-depth cap: the chain-depth extension landed after this
    /// constructor's 6-positional-argument shape was already in wide use across this crate's
    /// tests -- changing its signature would be a breaking, non-additive edit for every existing
    /// call site. `chain_depth_cap` is always `None` (unbounded) here; use
    /// `Self::with_chain_depth_cap` to opt a test into an explicit cap.
    pub fn with_caps(
        state_cap: usize,
        arc_cap: usize,
        tuple_cap: usize,
        group_cap: usize,
        line_cap: usize,
        step_timeout: Option<Duration>,
    ) -> Self {
        ComposeBudget {
            state_cap,
            arc_cap,
            tuple_cap,
            group_cap,
            line_cap,
            step_timeout,
            chain_depth_cap: None,
            ordering_multiplicity_cap: None,
        }
    }

    /// A budget that can never trip (`usize::MAX` on every count cap, no wall-clock deadline) --
    /// for callers/tests that need a `&ComposeBudget` to satisfy a function signature but aren't
    /// exercising this mechanism (mirrors `EnumerationBudget::unbounded`'s own doc/shape).
    #[cfg(test)]
    pub(crate) fn unbounded() -> Self {
        ComposeBudget {
            state_cap: usize::MAX,
            arc_cap: usize::MAX,
            tuple_cap: usize::MAX,
            group_cap: usize::MAX,
            line_cap: usize::MAX,
            step_timeout: None,
            chain_depth_cap: None,
            ordering_multiplicity_cap: None,
        }
    }

    pub(crate) fn line_cap(&self) -> usize {
        self.line_cap
    }

    pub(crate) fn tuple_cap(&self) -> usize {
        self.tuple_cap
    }

    pub(crate) fn group_cap(&self) -> usize {
        self.group_cap
    }

    /// Explicit-caps builder for the chain-depth dimension (mirrors `Self::with_caps`'s own
    /// "explicit-caps constructors, never env vars" convention for tests): returns `self` with an
    /// explicit chain-depth cap, clamped to `CHAIN_DEPTH_ABSOLUTE_CEILING` the same way
    /// `chain_depth_cap_from_env` clamps a configured env value. Promoted from a `#[cfg(test)]`
    /// `pub(crate)` helper to plain `pub` by `cover-template-truncation-reduplication`: that
    /// change's own `pg-foma` integration tests (`tests/*.rs`, compiled as a SEPARATE crate from
    /// this one) need to construct a small explicit cap without touching process-global env
    /// state, and `pub(crate)`/`#[cfg(test)]` items in this crate's `src/` are invisible there --
    /// only `crate::peel::ReduplicationPeeler` itself needs no special access (it takes a
    /// `&ComposeBudget` its caller already built).
    pub fn with_chain_depth_cap(mut self, cap: usize) -> Self {
        self.chain_depth_cap = Some(clamp_chain_depth_cap(cap));
        self
    }

    /// This budget's currently configured chain-depth cap, if any (`None` = unbounded/off).
    ///
    /// `#[allow(dead_code)]`: `Self::check_chain_depth` reads the `chain_depth_cap` field
    /// directly rather than through this accessor; only this module's own tests call it (a plain
    /// `--lib` build never does).
    #[allow(dead_code)]
    pub(crate) fn chain_depth_cap(&self) -> Option<usize> {
        self.chain_depth_cap
    }

    /// Checked chain-depth dimension (this module's "Chain-depth dimension" section):
    /// a caller reports its current cumulative derivation/unapplication step count for one word,
    /// and this returns `ComposeError::ChainDepthExceeded` once `depth` exceeds
    /// `Self::chain_depth_cap`. Deterministic logical counter, never a wall-clock check
    /// (deterministic logical counters are the primary fast-failure mechanism). Mirrors `compose_checked`/
    /// `union_checked`/`minimize_checked`'s own "check the crate's own vocabulary of a typed
    /// `ComposeError`, `site` names the call site" shape, but takes a caller-reported logical
    /// count directly rather than measuring a returned `Fsm` -- there is no `Fsm` to inspect for
    /// a recursion-depth counter, unlike the size dimensions above.
    ///
    /// `depth <= limit` is accepted (the convention shared with every other cap
    /// in this module: the cap names the last depth that still fits, not the first depth that
    /// doesn't). `None` (the default; see this module's "Chain-depth dimension" section for why)
    /// never trips, at any depth (pinned by `chain_depth_unbounded_budget_never_trips`).
    ///
    /// `crate::peel::ReduplicationPeeler::propose_for_residual` calls this once per genuine
    /// nested-reduplication layer it is about to use (see that module's own doc for why the
    /// check sits at "a real match was found," not at recursive-function entry -- the distinction
    /// that keeps an ordinary single-layer word from tripping a small cap just because one more,
    /// ultimately-empty, layer was cheaply attempted). `emit.rs`/`preexpand.rs`/`gate.rs`/
    /// `replace.rs`/`pg-rules`' OWN derivation/unapplication recursion (the general Aweti
    /// 24-level-chain case) still has no call site here -- that remains a
    /// separate, larger follow-on; this is the first, narrower one.
    pub(crate) fn check_chain_depth(
        &self,
        depth: usize,
        site: &'static str,
    ) -> Result<(), ComposeError> {
        match self.chain_depth_cap {
            None => Ok(()),
            Some(limit) if depth <= limit => Ok(()),
            Some(limit) => Err(ComposeError::ChainDepthExceeded { depth, limit, site }),
        }
    }

    /// Explicit-caps builder for the ordering-multiplicity dimension (mirrors
    /// `Self::with_chain_depth_cap`'s own shape) -- what tests use to exercise
    /// `Self::check_ordering_multiplicity` deterministically, without touching
    /// `HC_COMPOSE_ORDERING_MULTIPLICITY_BUDGET`.
    pub fn with_ordering_multiplicity_cap(mut self, cap: usize) -> Self {
        self.ordering_multiplicity_cap = Some(cap);
        self
    }

    /// This budget's currently configured ordering-multiplicity cap, if any (`None` = unbounded/
    /// off -- only `Self::with_caps`/`Self::unbounded`, never `Self::from_env`, which always
    /// configures a real default; see this module's "Ordering-multiplicity dimension" section).
    ///
    /// `#[allow(dead_code)]`: `Self::check_ordering_multiplicity` reads the
    /// `ordering_multiplicity_cap` field directly rather than through this accessor (mirrors
    /// `Self::chain_depth_cap`'s own doc); only this module's own tests call it.
    #[allow(dead_code)]
    pub(crate) fn ordering_multiplicity_cap(&self) -> Option<usize> {
        self.ordering_multiplicity_cap
    }

    /// Checked ordering-multiplicity dimension (this module's "Ordering-multiplicity dimension"
    /// section): `Ok` iff `rule_count` (an
    /// `Unordered` stratum's own loose-rule count) does not exceed `Self::ordering_multiplicity_cap`
    /// (unconfigured/`None` always `Ok`, the same zero-behavior-change-when-unset shape every other
    /// dimension in this module uses). `rule_count <= limit` is accepted, mirroring
    /// `Self::check_chain_depth`'s own "the cap names the last value that still fits" convention.
    ///
    /// **Wired for real** by `crate::unordered::check_unordered_strata_bound` -- called once per
    /// grammar, before `crate::analyzer::FomaProposer` ever hands a lexc source to the foma
    /// compiler (the second real production consumer of this module's chain-depth-shaped budget
    /// discipline, after `crate::peel::ReduplicationPeeler`'s own per-word chain-depth check).
    pub(crate) fn check_ordering_multiplicity(
        &self,
        rule_count: usize,
        site: &'static str,
    ) -> Result<(), ComposeError> {
        match self.ordering_multiplicity_cap {
            None => Ok(()),
            Some(limit) if rule_count <= limit => Ok(()),
            Some(limit) => Err(ComposeError::OrderingMultiplicityExceeded {
                rule_count,
                limit,
                site,
            }),
        }
    }
}

/// Checks `net`'s `statecount`/`arccount` against `budget`'s caps; shared by every checked wrapper below.
fn check_size(net: &Fsm, budget: &ComposeBudget, site: &'static str) -> Result<(), ComposeError> {
    if net.statecount < 0 || net.statecount as usize > budget.state_cap {
        return Err(ComposeError::NetSizeExceeded {
            measure: NetSizeMeasure::States,
            value: net.statecount,
            limit: budget.state_cap,
            site,
        });
    }
    if net.arccount < 0 || net.arccount as usize > budget.arc_cap {
        return Err(ComposeError::NetSizeExceeded {
            measure: NetSizeMeasure::Arcs,
            value: net.arccount,
            limit: budget.arc_cap,
            site,
        });
    }
    Ok(())
}

/// Wall-clock wrapper (design doc §5): runs `f` on a dedicated 256MB-stack worker thread (the same
/// large-stack convention this crate's own P6 example drivers already use around this exact call
/// shape, e.g. `examples/p6_replace_prototype.rs`'s `STACK_BYTES`) and waits up to `timeout` via an
/// `mpsc` channel. `Err(elapsed)` means the worker thread is ABANDONED (module doc: dropping the
/// unjoined `JoinHandle` detaches it; it keeps running/allocating until it finishes naturally) --
/// this function does not and cannot kill it, only stop waiting for it.
pub(crate) fn call_with_deadline<F>(f: F, timeout: Duration) -> Result<Fsm, Duration>
where
    F: FnOnce() -> Fsm + Send + 'static,
{
    const DEADLINE_WORKER_STACK_BYTES: usize = 256 * 1024 * 1024;

    let (tx, rx) = std::sync::mpsc::channel();
    let start = std::time::Instant::now();
    std::thread::Builder::new()
        .stack_size(DEADLINE_WORKER_STACK_BYTES)
        .spawn(move || {
            let result = f();
            // The receiver may already be gone (timed out); an ignored `send` failure just means nobody's listening.
            let _ = tx.send(result);
        })
        .expect("spawn compose-budget deadline worker thread");
    match rx.recv_timeout(timeout) {
        Ok(net) => Ok(net),
        Err(_) => Err(start.elapsed()),
    }
}

/// Checked `fsm_compose` (V1/V2, design doc §4): optionally runs under `call_with_deadline`
/// (only when `budget.step_timeout` is `Some` -- default OFF, module doc), then checks the result's
/// size against `budget`. `site` is a short, stable label identifying the call site (design doc
/// §4's own per-site names) for `ComposeError`'s message.
pub(crate) fn compose_checked(
    opts: &FomaOptions,
    a: Fsm,
    b: Fsm,
    budget: &ComposeBudget,
    site: &'static str,
) -> Result<Fsm, ComposeError> {
    let net = match budget.step_timeout {
        None => fsm_compose(opts, a, b),
        Some(timeout) => {
            let opts = opts.clone();
            call_with_deadline(move || fsm_compose(&opts, a, b), timeout).map_err(|elapsed| {
                ComposeError::ComposeStepTimedOut {
                    elapsed,
                    limit: timeout,
                    site,
                }
            })?
        }
    };
    check_size(&net, budget, site)?;
    Ok(net)
}

/// Checked `fsm_union` -- see `compose_checked`'s doc (identical shape, `fsm_union` in place of
/// `fsm_compose`). Recall `fsm_union` does NOT minimize internally (module doc): the size check
/// here can catch a union whose accumulated non-minimal state count is already large, even before
/// any eventual minimize.
pub(crate) fn union_checked(
    opts: &FomaOptions,
    a: Fsm,
    b: Fsm,
    budget: &ComposeBudget,
    site: &'static str,
) -> Result<Fsm, ComposeError> {
    let net = match budget.step_timeout {
        None => fsm_union(opts, a, b),
        Some(timeout) => {
            let opts = opts.clone();
            call_with_deadline(move || fsm_union(&opts, a, b), timeout).map_err(|elapsed| {
                ComposeError::ComposeStepTimedOut {
                    elapsed,
                    limit: timeout,
                    site,
                }
            })?
        }
    };
    check_size(&net, budget, site)?;
    Ok(net)
}

/// Checked `fsm_minimize` -- see `compose_checked`'s doc (unary in place of binary).
pub(crate) fn minimize_checked(
    opts: &FomaOptions,
    a: Fsm,
    budget: &ComposeBudget,
    site: &'static str,
) -> Result<Fsm, ComposeError> {
    let net = match budget.step_timeout {
        None => fsm_minimize(opts, a),
        Some(timeout) => {
            let opts = opts.clone();
            call_with_deadline(move || fsm_minimize(&opts, a), timeout).map_err(|elapsed| {
                ComposeError::ComposeStepTimedOut {
                    elapsed,
                    limit: timeout,
                    site,
                }
            })?
        }
    };
    check_size(&net, budget, site)?;
    Ok(net)
}

#[cfg(test)]
mod compose_budget_tests {
    use super::*;
    use foma::regex::fsm_parse_regex;

    fn tiny_net(opts: &FomaOptions, s: &str) -> Fsm {
        fsm_parse_regex(opts, s, None, None)
            .unwrap_or_else(|| panic!("regex {s:?} failed to compile"))
    }

    #[test]
    fn unbounded_budget_never_trips_on_small_fixture() {
        let opts = FomaOptions::default();
        let budget = ComposeBudget::unbounded();
        let a = tiny_net(&opts, "a");
        let b = tiny_net(&opts, "a -> b");
        let composed = compose_checked(&opts, a, b, &budget, "test").expect("small compose fits");
        assert!(composed.statecount > 0);
    }

    #[test]
    fn state_budget_trips_on_tiny_cascade() {
        let opts = FomaOptions::default();
        // cap=0 states: even a single-state identity net must trip this.
        let budget =
            ComposeBudget::with_caps(0, usize::MAX, usize::MAX, usize::MAX, usize::MAX, None);
        let a = tiny_net(&opts, "a");
        let b = tiny_net(&opts, "a -> b");
        let err = compose_checked(&opts, a, b, &budget, "state_budget_trips_on_tiny_cascade")
            .expect_err("cap=0 states must trip");
        match err {
            ComposeError::NetSizeExceeded {
                measure: NetSizeMeasure::States,
                limit,
                ..
            } => {
                assert_eq!(limit, 0);
            }
            other => panic!("expected NetSizeExceeded(States), got {other:?}"),
        }
    }

    #[test]
    fn arc_budget_trips_on_tiny_cascade() {
        let opts = FomaOptions::default();
        let budget =
            ComposeBudget::with_caps(usize::MAX, 0, usize::MAX, usize::MAX, usize::MAX, None);
        let a = tiny_net(&opts, "a");
        let b = tiny_net(&opts, "a -> b");
        let err = compose_checked(&opts, a, b, &budget, "arc_budget_trips_on_tiny_cascade")
            .expect_err("cap=0 arcs must trip");
        match err {
            ComposeError::NetSizeExceeded {
                measure: NetSizeMeasure::Arcs,
                limit,
                ..
            } => {
                assert_eq!(limit, 0);
            }
            other => panic!("expected NetSizeExceeded(Arcs), got {other:?}"),
        }
    }

    #[test]
    fn union_checked_respects_budget() {
        let opts = FomaOptions::default();
        let budget =
            ComposeBudget::with_caps(0, usize::MAX, usize::MAX, usize::MAX, usize::MAX, None);
        let a = tiny_net(&opts, "a");
        let b = tiny_net(&opts, "b");
        let err = union_checked(&opts, a, b, &budget, "union_checked_respects_budget")
            .expect_err("cap=0 states must trip");
        assert!(matches!(
            err,
            ComposeError::NetSizeExceeded {
                measure: NetSizeMeasure::States,
                ..
            }
        ));
    }

    #[test]
    fn minimize_checked_respects_budget() {
        let opts = FomaOptions::default();
        let budget =
            ComposeBudget::with_caps(0, usize::MAX, usize::MAX, usize::MAX, usize::MAX, None);
        let a = tiny_net(&opts, "a -> b, c -> d");
        let err = minimize_checked(&opts, a, &budget, "minimize_checked_respects_budget")
            .expect_err("cap=0 states must trip");
        assert!(matches!(
            err,
            ComposeError::NetSizeExceeded {
                measure: NetSizeMeasure::States,
                ..
            }
        ));
    }

    #[test]
    fn deadline_fast_closure_passes() {
        let opts = FomaOptions::default();
        let a = tiny_net(&opts, "a");
        let net = call_with_deadline(move || a, Duration::from_secs(5))
            .expect("a closure returning immediately must not time out");
        assert!(net.statecount > 0);
    }

    #[test]
    fn deadline_slow_closure_trips_fast() {
        let start = std::time::Instant::now();
        let opts = FomaOptions::default();
        let a = tiny_net(&opts, "a");
        let elapsed_budget = Duration::from_millis(50);
        let result = call_with_deadline(
            move || {
                std::thread::sleep(Duration::from_secs(5));
                a
            },
            elapsed_budget,
        );
        assert!(
            result.is_err(),
            "a 5s sleep must time out against a 50ms deadline"
        );
        assert!(
            start.elapsed() < Duration::from_secs(2),
            "call_with_deadline must return promptly once the deadline passes, not wait for the \
             abandoned worker thread to finish (took {:?})",
            start.elapsed()
        );
    }

    #[test]
    fn compose_step_timed_out_display_is_specific() {
        let err = ComposeError::ComposeStepTimedOut {
            elapsed: Duration::from_millis(120),
            limit: Duration::from_millis(50),
            site: "unit-test-site",
        };
        let msg = err.to_string();
        assert!(msg.contains("unit-test-site"));
        assert!(msg.contains("ABANDONED"));
    }

    // Chain-depth dimension: exercises `check_chain_depth` directly, no `Fsm`/foma call involved.

    #[test]
    fn chain_depth_unbounded_budget_never_trips() {
        // `unbounded()` must also leave chain depth off, like the size dimensions above.
        let budget = ComposeBudget::unbounded();
        assert_eq!(budget.chain_depth_cap(), None);
        // Well past the motivating Aweti 24-level chain and still `Ok`.
        budget
            .check_chain_depth(1_000_000, "chain_depth_unbounded_budget_never_trips")
            .expect("unbounded chain-depth budget must never trip, at any depth");
    }

    #[test]
    fn chain_depth_with_caps_defaults_to_unbounded() {
        // `with_caps` has no chain-depth parameter; prove it still leaves chain depth off by default.
        let budget = ComposeBudget::with_caps(
            usize::MAX,
            usize::MAX,
            usize::MAX,
            usize::MAX,
            usize::MAX,
            None,
        );
        assert_eq!(budget.chain_depth_cap(), None);
        budget
            .check_chain_depth(usize::MAX, "chain_depth_with_caps_defaults_to_unbounded")
            .expect("with_caps' default chain-depth cap must be unbounded");
    }

    #[test]
    fn chain_depth_explicit_cap_does_not_trip_at_or_below_limit() {
        let budget = ComposeBudget::unbounded().with_chain_depth_cap(24);
        budget
            .check_chain_depth(
                24,
                "chain_depth_explicit_cap_does_not_trip_at_or_below_limit",
            )
            .expect("depth == cap must be accepted, mirroring every other cap's <= convention");
        budget
            .check_chain_depth(
                1,
                "chain_depth_explicit_cap_does_not_trip_at_or_below_limit",
            )
            .expect("depth well below cap must be accepted");
    }

    #[test]
    fn chain_depth_explicit_cap_trips_one_past_limit() {
        // 24: the motivating Aweti derivation-chain depth (module doc).
        let budget = ComposeBudget::unbounded().with_chain_depth_cap(24);
        let err = budget
            .check_chain_depth(25, "chain_depth_explicit_cap_trips_one_past_limit")
            .expect_err("depth == cap + 1 must trip");
        match err {
            ComposeError::ChainDepthExceeded { depth, limit, site } => {
                assert_eq!(depth, 25);
                assert_eq!(limit, 24);
                assert_eq!(site, "chain_depth_explicit_cap_trips_one_past_limit");
            }
            other => panic!("expected ChainDepthExceeded, got {other:?}"),
        }
    }

    #[test]
    fn chain_depth_absolute_ceiling_clamps_excessive_configured_cap() {
        // A cap far above the absolute ceiling must clamp down to it, not pass through verbatim.
        let budget =
            ComposeBudget::unbounded().with_chain_depth_cap(CHAIN_DEPTH_ABSOLUTE_CEILING + 1_000);
        assert_eq!(
            budget.chain_depth_cap(),
            Some(CHAIN_DEPTH_ABSOLUTE_CEILING),
            "a configured cap above the absolute ceiling must clamp to the ceiling itself"
        );
        // One past the clamped ceiling must trip, reporting the ceiling as the limit, not the original request.
        let err = budget
            .check_chain_depth(
                CHAIN_DEPTH_ABSOLUTE_CEILING + 1,
                "chain_depth_absolute_ceiling_clamps_excessive_configured_cap",
            )
            .expect_err("one past the clamped ceiling must trip");
        match err {
            ComposeError::ChainDepthExceeded { limit, .. } => {
                assert_eq!(limit, CHAIN_DEPTH_ABSOLUTE_CEILING);
            }
            other => panic!("expected ChainDepthExceeded, got {other:?}"),
        }
    }

    #[test]
    fn chain_depth_cap_from_env_clamps_to_absolute_ceiling() {
        // Exercises `chain_depth_cap_from_env`'s clamp via the pure function, without touching process-global env state.
        assert_eq!(
            clamp_chain_depth_cap(CHAIN_DEPTH_ABSOLUTE_CEILING + 1_000),
            CHAIN_DEPTH_ABSOLUTE_CEILING
        );
        assert_eq!(
            clamp_chain_depth_cap(24),
            24,
            "a cap under the ceiling must pass through unchanged"
        );
    }

    #[test]
    fn chain_depth_exceeded_display_is_specific() {
        let err = ComposeError::ChainDepthExceeded {
            depth: 25,
            limit: 24,
            site: "unit-test-site",
        };
        let msg = err.to_string();
        assert!(msg.contains("unit-test-site"));
        assert!(msg.contains("25"));
        assert!(msg.contains("24"));
        assert!(msg.contains("HC_COMPOSE_CHAIN_DEPTH_BUDGET"));
    }

    // Ordering-multiplicity dimension.

    #[test]
    fn ordering_multiplicity_unbounded_budget_never_trips() {
        let budget = ComposeBudget::unbounded();
        assert_eq!(budget.ordering_multiplicity_cap(), None);
        budget
            .check_ordering_multiplicity(
                1_000_000,
                "ordering_multiplicity_unbounded_budget_never_trips",
            )
            .expect("unbounded ordering-multiplicity budget must never trip, at any rule count");
    }

    #[test]
    fn ordering_multiplicity_with_caps_defaults_to_unbounded() {
        // `with_caps` has no ordering-multiplicity parameter; prove it still leaves this dimension off by default.
        let budget = ComposeBudget::with_caps(
            usize::MAX,
            usize::MAX,
            usize::MAX,
            usize::MAX,
            usize::MAX,
            None,
        );
        assert_eq!(budget.ordering_multiplicity_cap(), None);
        budget
            .check_ordering_multiplicity(
                usize::MAX,
                "ordering_multiplicity_with_caps_defaults_to_unbounded",
            )
            .expect("with_caps' default ordering-multiplicity cap must be unbounded");
    }

    #[test]
    fn ordering_multiplicity_explicit_cap_does_not_trip_at_or_below_limit() {
        let budget = ComposeBudget::unbounded().with_ordering_multiplicity_cap(6);
        budget
            .check_ordering_multiplicity(
                6,
                "ordering_multiplicity_explicit_cap_does_not_trip_at_or_below_limit",
            )
            .expect("rule_count == cap must be accepted");
        budget
            .check_ordering_multiplicity(
                2,
                "ordering_multiplicity_explicit_cap_does_not_trip_at_or_below_limit",
            )
            .expect("rule_count well below cap must be accepted");
    }

    #[test]
    fn ordering_multiplicity_explicit_cap_trips_one_past_limit() {
        let budget = ComposeBudget::unbounded().with_ordering_multiplicity_cap(6);
        let err = budget
            .check_ordering_multiplicity(
                7,
                "ordering_multiplicity_explicit_cap_trips_one_past_limit",
            )
            .expect_err("rule_count == cap + 1 must trip");
        match err {
            ComposeError::OrderingMultiplicityExceeded {
                rule_count,
                limit,
                site,
            } => {
                assert_eq!(rule_count, 7);
                assert_eq!(limit, 6);
                assert_eq!(
                    site,
                    "ordering_multiplicity_explicit_cap_trips_one_past_limit"
                );
            }
            other => panic!("expected OrderingMultiplicityExceeded, got {other:?}"),
        }
    }

    #[test]
    fn ordering_multiplicity_from_env_defaults_to_a_calibrated_bound() {
        // Unlike chain_depth_cap, from_env must configure a real, calibrated default for this dimension.
        let budget = ComposeBudget::from_env();
        assert_eq!(
            budget.ordering_multiplicity_cap(),
            Some(DEFAULT_ORDERING_MULTIPLICITY_BUDGET),
            "from_env must default this dimension ON (unlike chain_depth_cap), absent an env override"
        );
    }

    #[test]
    fn ordering_multiplicity_exceeded_display_is_specific() {
        let err = ComposeError::OrderingMultiplicityExceeded {
            rule_count: 7,
            limit: 6,
            site: "unit-test-site",
        };
        let msg = err.to_string();
        assert!(msg.contains("unit-test-site"));
        assert!(msg.contains('7'));
        assert!(msg.contains('6'));
        assert!(msg.contains("HC_COMPOSE_ORDERING_MULTIPLICITY_BUDGET"));
    }

    // Apply-path dimension: schema/budget-type tests only; decode-loop wiring is `analyzer.rs`'s own `propose_budgeted` tests.

    #[test]
    fn apply_budget_unbounded_has_no_caps() {
        let budget = ApplyBudget::unbounded();
        assert_eq!(budget.path_cap(), None);
        assert_eq!(budget.candidate_cap(), None);
    }

    #[test]
    fn apply_budget_with_caps_round_trips_each_dimension_independently() {
        let budget = ApplyBudget::with_caps(Some(10), None);
        assert_eq!(budget.path_cap(), Some(10));
        assert_eq!(budget.candidate_cap(), None);

        let budget = ApplyBudget::with_caps(None, Some(5));
        assert_eq!(budget.path_cap(), None);
        assert_eq!(budget.candidate_cap(), Some(5));
    }

    #[test]
    fn apply_budget_from_env_defaults_to_unbounded_when_unset() {
        // Reads real env (only `from_env` does); asserts the fallback only when the var is genuinely unset in this test process.
        if std::env::var("HC_APPLY_PATH_BUDGET").is_err() {
            assert_eq!(ApplyBudget::from_env().path_cap(), None);
        }
        if std::env::var("HC_APPLY_CANDIDATE_BUDGET").is_err() {
            assert_eq!(ApplyBudget::from_env().candidate_cap(), None);
        }
    }

    #[test]
    fn apply_dimension_label_is_stable_and_distinct() {
        assert_eq!(
            ApplyDimension::DecodedPaths.label(),
            "decoded apply_up paths"
        );
        assert_eq!(ApplyDimension::Candidates.label(), "distinct candidates");
        assert_ne!(
            ApplyDimension::DecodedPaths.label(),
            ApplyDimension::Candidates.label()
        );
    }

    #[test]
    fn apply_outcome_complete_and_incomplete_are_distinguishable() {
        let complete: ApplyOutcome<Vec<u32>> = ApplyOutcome::Complete(vec![1, 2, 3]);
        assert_eq!(complete, ApplyOutcome::Complete(vec![1, 2, 3]));

        let incomplete: ApplyOutcome<Vec<u32>> = ApplyOutcome::Incomplete {
            dimension: ApplyDimension::Candidates,
            value: 11,
            limit: 10,
        };
        match incomplete {
            ApplyOutcome::Incomplete {
                dimension,
                value,
                limit,
            } => {
                assert_eq!(dimension, ApplyDimension::Candidates);
                assert_eq!(value, 11);
                assert_eq!(limit, 10);
            }
            ApplyOutcome::Complete(_) => panic!("expected Incomplete"),
        }
    }
}
