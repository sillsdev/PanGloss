//! Phase B composition-path budget guards (`docs/fst-plan/phase-b-compose-budget-design.md`):
//! [`EnumerationBudget`](crate::morphotactics::EnumerationBudget)'s sibling for the P6 composition
//! path (`crate::replace`, `crate::gate`, `crate::uflexc`) -- a path that, until this module, had
//! **no `Result`-returning public API at all** (bare `Option`/`Fsm`/report structs, example drivers
//! `panic!` on failure) and never imported `EnumerationBudget` (the eager-enumeration path's own
//! guard covers `crate::preexpand`/`crate::emit` only, zero references from the composition path).
//!
//! ## Why this budget looks different from [`EnumerationBudget`](crate::morphotactics::EnumerationBudget)
//! `EnumerationBudget` is a shared, cross-thread `AtomicUsize` latch because `crate::preexpand`'s
//! composite builders run their per-root work across a rayon pool. The P6 composition cascade
//! (`crate::replace`'s per-alpha-tuple/per-rule fold, `crate::gate`'s per-group loop) is strictly
//! sequential -- no rayon anywhere in this path -- so a plain `&ComposeBudget` with no atomics and
//! no interior mutability is sufficient; every checked wrapper below just reads `self`'s cap fields
//! and compares them against the `Fsm` the vendored `foma` crate handed back. Revisit this if the
//! per-group loop (`crate::gate::compile_gated_grammar`) is ever parallelized.
//!
//! ## Vendored-crate findings this module's shape depends on (design doc §1, verified by reading
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
//! ## Limitations (verbatim from the design doc -- the honest part, not hidden)
//! - A between-step size check cannot catch a blow-up INSIDE one call: if a single compose/minimize
//!   call OOMs or spins, the check that would run after it never runs. There is nothing in the
//!   vendored crate to checkpoint mid-call; the size caps only bound cost accumulating ACROSS calls.
//! - [`call_with_deadline`]'s wall-clock wrapper *detects*, it does not *stop*: the worker thread is
//!   abandoned (never joined) and keeps running/allocating until it finishes naturally. Treat
//!   [`ComposeError::ComposeStepTimedOut`] as TERMINAL for that grammar (fall back to another
//!   engine), never retry the identical call; a long-lived server embedding this must track
//!   abandoned-thread count itself.
//! - `catch_unwind` is not a safety net for this module either: stack-overflow and allocator-OOM
//!   abort the process, bypassing every check here. Large worker-thread stacks reduce but do not
//!   eliminate overflow risk.
//! - Full "never blow up" for a single adversarial call needs an external supervisor process --
//!   out of Phase B scope (noted for the plan's Phase D).

use std::fmt;
use std::time::Duration;

use foma::constructions::{fsm_compose, fsm_union};
use foma::minimize::fsm_minimize;
use foma::options::FomaOptions;
use foma::types::Fsm;

/// Compile-time check that `Fsm` is `Send` (design doc §1): [`call_with_deadline`]'s wall-clock
/// wrapper depends on being able to move an OWNED `Fsm` into a spawned worker thread and its result
/// back out over an `mpsc` channel. Verified by direct inspection of `foma = "=0.4.0"`'s own
/// `src/types.rs`: `Fsm` owns only `SmolStr`/`Vec<_>`/`Option<Box<_>>`/plain integers via its own
/// `LineTable` storage seam -- no `Rc`/`RefCell`/raw pointer anywhere in its transitive fields. If a
/// future `foma` version ever adds one of those, this line stops compiling -- the design's own
/// documented contingency ("if `Fsm` is NOT `Send`, implement everything EXCEPT
/// `call_with_deadline`/`step_timeout`") applies at that point, rather than the wrapper silently
/// becoming unsound.
#[allow(dead_code)]
fn assert_send<T: Send>() {}
const _: fn() = || {
    assert_send::<Fsm>();
};

// --- Defaults / env overrides (mirrors `crate::morphotactics::EnumerationBudget`'s own
// `HC_ENUM_ENTRY_BUDGET`/`HC_ENUM_PROBE_BUDGET` convention: `HC_*`-prefixed, parsed as the field's
// own numeric type, unset/unparsable falls back to the documented default). ------------------------

/// `HC_COMPOSE_STATE_BUDGET`: ceiling on `Fsm::statecount` after any checked compose/union/minimize
/// call (V1, design doc §4). Calibration basis (design doc §8 item 2, measured on the new
/// `emit_underlying_templated` + `crate::replace`/`crate::gate` path against the real Aweti grammar
/// -- 855 entries, 135 mrules, 18 prules, 14 templates -- the largest real grammar this path has
/// been run against as of this writing): Aweti's templated lexc alone compiles to 23,661 states;
/// the FULL `lexc .o. rules .o. boundary-cleanup` composition, minimized, is 35,846 states. This
/// default sits ~56x above that measured ceiling -- generous headroom for a larger real grammar,
/// while staying far below the eager-enumeration path's own disaster case (`EnumerationBudget`'s
/// own doc: Aweti's *enumeration* path produces an ~8.8GB `apply_up` allocation). Refine with more
/// real-grammar measurements in the plan's Phase D sweeps.
pub(crate) const DEFAULT_STATE_BUDGET: usize = 2_000_000;

/// `HC_COMPOSE_ARC_BUDGET`: ceiling on `Fsm::arccount`, same call sites as
/// [`DEFAULT_STATE_BUDGET`]. Calibration basis (design doc §8 item 2, same Aweti run): the
/// templated lexc alone is 346,727 arcs; the full composed+minimized network is 800,354 arcs. This
/// default sits ~25x above that measured ceiling.
pub(crate) const DEFAULT_ARC_BUDGET: usize = 20_000_000;

/// `HC_COMPOSE_TUPLE_BUDGET`: ceiling on the number of alpha-tuple assignments
/// (`crate::replace::resolve_alpha_tuples`'s `surviving` count) a single subrule may expand to
/// before `compile_rewrite_rule_subset` starts folding them (V3, design doc §4 -- checked BEFORE
/// the expensive per-tuple compile loop, the same "check the search result before the expensive
/// part" shape `EnumerationBudget`'s own doc uses). Default 5,000: Amharic's real worst case (the
/// 20-alpha-variable CV-merger, `reports/08-audit-corrections-and-reframed-architecture.md` §3 item
/// 1) is `nc15=59 x nc16=6 <= 354` surviving tuples -- comfortably under this cap by ~14x.
pub(crate) const DEFAULT_TUPLE_BUDGET: usize = 5_000;

/// `HC_COMPOSE_GROUP_BUDGET`: ceiling on `crate::gate::partition_entries`'s own group count, checked
/// BEFORE any per-group compile work runs (V6, design doc §4 -- the single highest-leverage check
/// in this module, since it gates all downstream V1/V4 work for every group). Default 64:
/// Indonesian (this prototype's only real gated grammar today) needs exactly 2 groups; a grammar
/// with `k` gated subrules is bounded by `2^k` DISTINCT gating vectors in the worst case, so 64
/// covers up to 6 simultaneously-gated subrules with every combination realized -- comfortably
/// above every reference grammar's real gated-subrule count (Indonesian: 1; Amharic: 3) while still
/// catching a pathological grammar before any group's lexc/rule compile even starts. **No graceful
/// fallback by design** (design doc §4 V6): merging/dropping groups is unsound (over/under-firing
/// gated rules), so a breach here always means "fall back to another engine for this grammar", never
/// a partial group set.
pub(crate) const DEFAULT_GROUP_BUDGET: usize = 64;

/// `HC_COMPOSE_LINE_BUDGET`: ceiling on lexc lines written by `crate::emit::emit_underlying_templated`
/// (V4, design doc §4 + §8 item 1 -- checked incrementally, per-group, so a pathological templated
/// grammar bails during the FIRST group's emission rather than after building a multi-GB string) and
/// by `crate::uflexc::emit_underlying_filtered` (the same V4 vector, Indonesian-scoped emitter,
/// checked incrementally at its own root/prefix/suffix line-push sites). Calibration basis: Aweti's
/// real templated lexc emission is 37,510 lines (measured via
/// `examples/p6_aweti_replace_prototype.rs`'s own `counts.lexc_lines`, `TextMode::UnderlyingTokens`
/// path, 2026-07-20). This default sits ~26x above that measured ceiling -- generous headroom while
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

/// `HC_COMPOSE_STEP_TIMEOUT_MS`: wall-clock deadline (design doc §5) for every checked
/// compose/union/minimize call, via [`call_with_deadline`]. **Default OFF** (`None`) -- unlike the
/// four size caps above (default ON, mirroring `EnumerationBudget`'s own always-live convention),
/// this mirrors `pg-rules/src/stratum.rs`'s `StepBudget`'s own opt-in convention: a wall-clock
/// abandon-on-timeout mechanism is a much bigger hammer (design doc §7: "detects, does not stop")
/// than a size check, so it stays opt-in until a caller has a concrete reason to want it.
pub(crate) fn step_timeout_from_env() -> Option<Duration> {
    std::env::var("HC_COMPOSE_STEP_TIMEOUT_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .map(Duration::from_millis)
}

// --- Chain-depth dimension (`docs/adr/0003-apply-time-containment.md`; `openspec/changes/
// harden-foma-resource-safety`'s "Extension" -- STAGING.md: "add the derivation/unapplication
// chain-depth dimension ... that deterministically closes stack overflow (the Aweti 24-level
// chain, the 1 GiB-stack workaround). Owns `compose_budget.rs` and new budget types."). ------------
//
// **This is a schema/budget-type step only** (mirrors `crate::health`'s own "purely additive"
// precedent, module doc: "defines and unit-tests the schema only ... does not instrument any
// compiler pass"). [`ComposeBudget::check_chain_depth`] exists and is unit-tested here, but **no
// call site in `emit.rs`/`preexpand.rs`/`gate.rs`/`replace.rs`/`pg-rules` calls it yet** -- that
// wiring (threading a real per-word derivation/unapplication step counter through the recursive
// apply/derivation path and calling this at each step) is a separate, production-touching
// follow-on change. Until that lands, [`ComposeBudget::chain_depth_cap`] defaults to `None`
// (unbounded) everywhere -- [`ComposeBudget::from_env`], [`ComposeBudget::with_caps`], and
// [`ComposeBudget::unbounded`] all leave it off -- so this addition is a zero-behavior-change
// no-op for every existing caller and test.
//
// Unlike the four size caps above (state/arc/tuple/group -- default ON with a calibrated
// production default), chain depth mirrors [`step_timeout_from_env`]'s **default-OFF** shape:
// there is no calibrated default yet (`calibrate-fst-resource-envelopes`'s own documented
// extension is where that calibration work happens -- STAGING.md), so this dimension stays
// `Option<usize>` and off until a caller opts in, rather than shipping an uncalibrated numeric
// default that could silently start rejecting real grammars.

/// Absolute ceiling for the chain-depth dimension (CONTEXT.md `Absolute resource ceiling`: "A
/// versioned, hard-coded, deliberately high non-disableable limit above all default, app, and
/// caller limits ... an emergency containment boundary, not a normal operating target"). No
/// configured cap -- from [`chain_depth_cap_from_env`] or [`ComposeBudget::with_chain_depth_cap`]
/// -- may exceed this value; both clamp down to it rather than reject, the same "contractually
/// clamp excessive values ... provide no unlimited setting" discipline tasks.md 1.1c documents for
/// every budget dimension in this change. There is no way to configure an unlimited *cap*; the
/// schema-level `None` (this step's default) means "no cap configured yet," never "unlimited by
/// request."
///
/// Ceiling schema version 1 -- bump only via a reviewed commit (design.md's governance for this
/// change; the same "evidence + proposed diff + human-reviewed commit" discipline
/// `calibrate-fst-resource-envelopes` uses for the eventual calibrated default). Chosen
/// deliberately high relative to any plausible calibrated default: the motivating case (ADR 0003;
/// this module's own doc) is Aweti's real 24-level derivation chain, so a ceiling many orders of
/// magnitude above 24 leaves enormous headroom below this emergency boundary for whatever default
/// the later calibration change lands on.
pub(crate) const CHAIN_DEPTH_ABSOLUTE_CEILING: usize = 1_000_000;

/// `HC_COMPOSE_CHAIN_DEPTH_BUDGET`: per-word derivation/unapplication chain-depth cap (ADR 0003).
/// **Default `None` (unbounded/off)** -- see this section's module doc for why this dimension
/// mirrors [`step_timeout_from_env`]'s opt-in shape rather than the four size caps' default-ON
/// shape. When set, parses as `usize` and is clamped to [`CHAIN_DEPTH_ABSOLUTE_CEILING`]
/// (unparsable or unset falls back to `None`, exactly like every other `_from_env` function in
/// this module falls back to its own default on a parse failure).
pub(crate) fn chain_depth_cap_from_env() -> Option<usize> {
    std::env::var("HC_COMPOSE_CHAIN_DEPTH_BUDGET")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .map(clamp_chain_depth_cap)
}

/// The clamp [`chain_depth_cap_from_env`] and [`ComposeBudget::with_chain_depth_cap`] both apply:
/// pulled into its own pure function so this module's tests can exercise the clamp arithmetic
/// directly without touching process-global env state (this module's own "explicit-caps
/// constructors, never env vars" test convention, [`ComposeBudget::with_caps`]'s doc).
pub(crate) fn clamp_chain_depth_cap(configured: usize) -> usize {
    configured.min(CHAIN_DEPTH_ABSOLUTE_CEILING)
}

/// Which size measure [`ComposeError::NetSizeExceeded`] reports.
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

/// Every way a [`ComposeBudget`]-checked call can fail (design doc §3). Each variant carries enough
/// to build a specific, honest message -- never a generic "something blew up" string -- and names
/// the vector (`V1`.."V6"`, design doc §4/`docs/fst-plan/synthetic-stress-grammar-plan.md`) it
/// guards against in its own doc line.
#[derive(Debug, Clone)]
pub enum ComposeError {
    /// V1/V2: a checked compose/union/minimize call returned a network whose `statecount`/`arccount`
    /// exceeds [`ComposeBudget::state_cap`]/[`ComposeBudget::arc_cap`]. `site` names the call site
    /// (design doc §4's own site labels, e.g. `"compile_rewrite_rule_subset alpha-tuple fold"`).
    NetSizeExceeded {
        measure: NetSizeMeasure,
        value: i32,
        limit: usize,
        site: &'static str,
    },
    /// V3: `crate::replace::resolve_alpha_tuples` produced more surviving assignments than
    /// [`ComposeBudget::tuple_cap`], checked BEFORE the per-tuple compile loop runs.
    AlphaTupleBudgetExceeded {
        surviving: usize,
        limit: usize,
        rule_xml_id: String,
    },
    /// V6: `crate::gate::partition_entries` produced more groups than [`ComposeBudget::group_cap`],
    /// checked BEFORE any per-group compile work runs.
    GroupBudgetExceeded {
        groups: usize,
        limit: usize,
        gated_subrules: usize,
    },
    /// V4: a templated/underlying-form lexc emitter wrote more lines than
    /// [`ComposeBudget::line_cap`], checked incrementally so a pathological grammar bails during
    /// the first group's emission.
    EmitLineBudgetExceeded { lines: usize, limit: usize },
    /// V2: [`call_with_deadline`] timed out waiting for a checked call -- the worker thread is
    /// ABANDONED, not killed (module doc). Always terminal for this grammar; never retry the
    /// identical call.
    ComposeStepTimedOut {
        elapsed: Duration,
        limit: Duration,
        site: &'static str,
    },
    /// ADR 0003's chain-depth dimension (this module's "Chain-depth dimension" section):
    /// [`ComposeBudget::check_chain_depth`] found a caller-reported cumulative derivation/
    /// unapplication step count exceeding [`ComposeBudget::chain_depth_cap`]. Deterministically
    /// closes the stack-overflow failure class (the Aweti 24-level chain; the 1 GiB-stack
    /// workaround) instead of merely raising the point at which it recurs. **Not yet produced by
    /// any production call site** -- see this module's "Chain-depth dimension" section doc; this
    /// variant exists so the typed-error schema is complete ahead of the recursion wiring.
    ChainDepthExceeded {
        depth: usize,
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
        }
    }
}

impl std::error::Error for ComposeError {}

/// Default-on composition-path budget (design doc §2): four size/count caps checked eagerly (state,
/// arc, alpha-tuple, gating-group), plus an opt-in wall-clock deadline. Unlike
/// [`crate::morphotactics::EnumerationBudget`], this holds no atomics/interior mutability at all --
/// module doc explains why a plain value is sufficient for this strictly-sequential path.
#[derive(Debug, Clone, Copy)]
pub struct ComposeBudget {
    pub(crate) state_cap: usize,
    pub(crate) arc_cap: usize,
    pub(crate) tuple_cap: usize,
    pub(crate) group_cap: usize,
    pub(crate) line_cap: usize,
    pub(crate) step_timeout: Option<Duration>,
    /// ADR 0003's chain-depth dimension (this module's "Chain-depth dimension" section). `None`
    /// (the default everywhere -- [`Self::from_env`], [`Self::with_caps`], [`Self::unbounded`])
    /// means unbounded/off: [`Self::check_chain_depth`] always returns `Ok` and no existing
    /// caller's behavior changes. `Some(limit)` is already clamped to
    /// [`CHAIN_DEPTH_ABSOLUTE_CEILING`] by whichever constructor set it.
    ///
    /// `#[allow(dead_code)]`: only read by [`Self::chain_depth_cap`]/[`Self::check_chain_depth`],
    /// which are themselves unread by production code at this schema-only step (same precedent
    /// as this module's own `assert_send` helper above) -- the recursion-wiring follow-on this
    /// module's doc describes is what starts reading it for real.
    #[allow(dead_code)]
    pub(crate) chain_depth_cap: Option<usize>,
}

impl ComposeBudget {
    /// Production entry point: every cap from its own `HC_COMPOSE_*` env var (module doc), or the
    /// documented default when unset/unparsable. Mirrors `EnumerationBudget::from_env`'s own
    /// "read env exactly once, in the production entry point" convention -- tests should use
    /// [`Self::with_caps`] instead, so parallel test processes never race process-global env state.
    pub fn from_env() -> Self {
        ComposeBudget {
            state_cap: state_budget_from_env(),
            arc_cap: arc_budget_from_env(),
            tuple_cap: tuple_budget_from_env(),
            group_cap: group_budget_from_env(),
            line_cap: line_budget_from_env(),
            step_timeout: step_timeout_from_env(),
            chain_depth_cap: chain_depth_cap_from_env(),
        }
    }

    /// Explicit-caps constructor -- what tests use (design doc §6: "explicit-caps constructors,
    /// never env vars"), and what [`Self::from_env`] builds internally.
    ///
    /// Does not take a chain-depth cap (design.md's chain-depth extension landed after this
    /// constructor's 6-positional-argument shape was already in wide use across this crate's
    /// tests -- changing its signature would be a breaking, non-additive edit for every existing
    /// call site). `chain_depth_cap` is always `None` (unbounded) here; use
    /// [`Self::with_chain_depth_cap`] to opt a test into an explicit cap.
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

    /// Test-only builder for the chain-depth dimension (mirrors [`Self::unbounded`]'s own
    /// `#[cfg(test)]` scoping): returns `self` with an explicit chain-depth cap, clamped to
    /// [`CHAIN_DEPTH_ABSOLUTE_CEILING`] the same way [`chain_depth_cap_from_env`] clamps a
    /// configured env value. Production code has no call site that needs this yet (this module's
    /// "Chain-depth dimension" section) -- callers/tests exercising [`Self::check_chain_depth`]
    /// use this instead of reaching into the `pub(crate)` field directly.
    #[cfg(test)]
    pub(crate) fn with_chain_depth_cap(mut self, cap: usize) -> Self {
        self.chain_depth_cap = Some(clamp_chain_depth_cap(cap));
        self
    }

    /// This budget's currently configured chain-depth cap, if any (`None` = unbounded/off).
    ///
    /// `#[allow(dead_code)]`: unread outside this module's own tests until the recursion-wiring
    /// follow-on (this module's "Chain-depth dimension" section) has a production caller.
    #[allow(dead_code)]
    pub(crate) fn chain_depth_cap(&self) -> Option<usize> {
        self.chain_depth_cap
    }

    /// Checked chain-depth dimension (ADR 0003; this module's "Chain-depth dimension" section):
    /// a caller reports its current cumulative derivation/unapplication step count for one word,
    /// and this returns [`ComposeError::ChainDepthExceeded`] once `depth` exceeds
    /// [`Self::chain_depth_cap`]. Deterministic logical counter, never a wall-clock check (ADR
    /// 0003/R6: "the primary fast-failure mechanism"). Mirrors [`compose_checked`]/
    /// [`union_checked`]/[`minimize_checked`]'s own "check the crate's own vocabulary of a typed
    /// `ComposeError`, `site` names the call site" shape, but takes a caller-reported logical
    /// count directly rather than measuring a returned [`Fsm`] -- there is no `Fsm` to inspect for
    /// a recursion-depth counter, unlike the size dimensions above.
    ///
    /// `depth <= limit` is accepted (design doc/glossary convention shared with every other cap
    /// in this module: the cap names the last depth that still fits, not the first depth that
    /// doesn't). `None` (the default; see this module's "Chain-depth dimension" section for why)
    /// always returns `Ok` -- this is the zero-behavior-change no-op every existing caller and
    /// test relies on until a follow-on change wires a real depth counter through
    /// `emit.rs`/`preexpand.rs`/`gate.rs`/`replace.rs`/`pg-rules` and calls this at each
    /// derivation/unapplication step.
    ///
    /// `#[allow(dead_code)]`: this is that not-yet-wired API -- see this module's "Chain-depth
    /// dimension" section (same precedent as this module's own `assert_send` helper above and
    /// `crate::health`'s "purely additive" schema step).
    #[allow(dead_code)]
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
}

/// Checks `net`'s `statecount`/`arccount` against `budget`'s caps (V1/V2, design doc §4). Shared by
/// every checked wrapper below.
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
            // The receiver may already be gone (we timed out and returned) -- a `send` failure
            // here just means nobody's listening anymore, not a bug; deliberately ignored.
            let _ = tx.send(result);
        })
        .expect("spawn compose-budget deadline worker thread");
    match rx.recv_timeout(timeout) {
        Ok(net) => Ok(net),
        Err(_) => Err(start.elapsed()),
    }
}

/// Checked `fsm_compose` (V1/V2, design doc §4): optionally runs under [`call_with_deadline`]
/// (only when `budget.step_timeout` is `Some` -- default OFF, module doc), then checks the result's
/// size against `budget`. `site` is a short, stable label identifying the call site (design doc
/// §4's own per-site names) for [`ComposeError`]'s message.
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

/// Checked `fsm_union` -- see [`compose_checked`]'s doc (identical shape, `fsm_union` in place of
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

/// Checked `fsm_minimize` -- see [`compose_checked`]'s doc (unary in place of binary).
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

    // ---------------------------------------------------------------------------------------
    // Chain-depth dimension (ADR 0003 / STAGING.md "Extension"). Schema/budget-type only --
    // these tests exercise `check_chain_depth` directly (no `Fsm`/foma call involved) and prove
    // the default is a zero-behavior-change no-op, exactly like `compose_budget_tests`'s own
    // tests above prove for the four size caps.
    // ---------------------------------------------------------------------------------------

    #[test]
    fn chain_depth_unbounded_budget_never_trips() {
        // `unbounded()` (the same constructor `unbounded_budget_never_trips_on_small_fixture`
        // above already uses for the size dimensions) must also leave chain depth off.
        let budget = ComposeBudget::unbounded();
        assert_eq!(budget.chain_depth_cap(), None);
        // Well past the motivating Aweti 24-level chain (ADR 0003; module doc) and still `Ok`.
        budget
            .check_chain_depth(1_000_000, "chain_depth_unbounded_budget_never_trips")
            .expect("unbounded chain-depth budget must never trip, at any depth");
    }

    #[test]
    fn chain_depth_with_caps_defaults_to_unbounded() {
        // `with_caps` cannot take a 7th positional argument without breaking every existing call
        // site across this crate (this module's own doc on `with_caps`) -- prove it still leaves
        // chain depth off by default, the same zero-behavior-change guarantee `unbounded()` gives.
        let budget =
            ComposeBudget::with_caps(usize::MAX, usize::MAX, usize::MAX, usize::MAX, usize::MAX, None);
        assert_eq!(budget.chain_depth_cap(), None);
        budget
            .check_chain_depth(usize::MAX, "chain_depth_with_caps_defaults_to_unbounded")
            .expect("with_caps' default chain-depth cap must be unbounded");
    }

    #[test]
    fn chain_depth_explicit_cap_does_not_trip_at_or_below_limit() {
        let budget = ComposeBudget::unbounded().with_chain_depth_cap(24);
        budget
            .check_chain_depth(24, "chain_depth_explicit_cap_does_not_trip_at_or_below_limit")
            .expect("depth == cap must be accepted, mirroring every other cap's <= convention");
        budget
            .check_chain_depth(1, "chain_depth_explicit_cap_does_not_trip_at_or_below_limit")
            .expect("depth well below cap must be accepted");
    }

    #[test]
    fn chain_depth_explicit_cap_trips_one_past_limit() {
        // 24: the motivating Aweti derivation-chain depth (ADR 0003; module doc).
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
        // Requesting a cap far above the absolute ceiling must clamp down to the ceiling, not
        // accept the requested value verbatim (CONTEXT.md `Absolute resource ceiling`; this
        // module's doc on `CHAIN_DEPTH_ABSOLUTE_CEILING`).
        let budget = ComposeBudget::unbounded().with_chain_depth_cap(CHAIN_DEPTH_ABSOLUTE_CEILING + 1_000);
        assert_eq!(
            budget.chain_depth_cap(),
            Some(CHAIN_DEPTH_ABSOLUTE_CEILING),
            "a configured cap above the absolute ceiling must clamp to the ceiling itself"
        );
        // One past the (clamped) ceiling must trip, reporting the ceiling as the limit -- never
        // the caller's original, larger requested value.
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
        // Exercises the exact clamp `chain_depth_cap_from_env` applies to a parsed env value,
        // without touching process-global env state (this module's own "tests should use
        // explicit-caps constructors, never env vars" convention -- the clamp itself is a pure
        // function precisely so this is possible).
        assert_eq!(
            clamp_chain_depth_cap(CHAIN_DEPTH_ABSOLUTE_CEILING + 1_000),
            CHAIN_DEPTH_ABSOLUTE_CEILING
        );
        assert_eq!(clamp_chain_depth_cap(24), 24, "a cap under the ceiling must pass through unchanged");
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
}
