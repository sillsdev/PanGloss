//! Composition-path budget guards for the emit/gate/uflexc paths whose network operations can
//! genuinely exceed configured limits. The ordinary replacement cascade uses direct foma
//! operations and reports unsupported rules through its existing `Option` contract.
//!
//! ## Why this budget is path-local
//! The composition cascade
//! (`crate::replace`'s per-alpha-tuple/per-rule fold, `crate::gate`'s per-group loop) is strictly
//! sequential -- no rayon anywhere in this path -- so a plain `&ComposeBudget` with no atomics and
//! no interior mutability is sufficient. Revisit this if the
//! per-group loop (`crate::gate::compile_gated_grammar`) is ever parallelized.
//!
//! ## Vendored-crate findings this module's shape depends on (verified by reading
//! `foma = "=0.4.0"`'s own source, not inferred)
//!
//! ## Limitations (the honest part, not hidden)
//! - `catch_unwind` is not a safety net for this module either: stack-overflow and allocator-OOM
//!   abort the process, bypassing every check here. Large worker-thread stacks reduce but do not
//!   eliminate overflow risk.
//! - Full "never blow up" for a single adversarial call needs an external supervisor process --
//!   out of scope here.

use std::fmt;

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
/// is opt-in rather than default-ON. When set, parses as `usize` and is clamped to
/// `CHAIN_DEPTH_ABSOLUTE_CEILING`
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
/// directly without touching process-global env state.
pub(crate) fn clamp_chain_depth_cap(configured: usize) -> usize {
    configured.min(CHAIN_DEPTH_ABSOLUTE_CEILING)
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

/// Which magnitude `ApplyOutcome::Incomplete` reports tripped.
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

/// Every way a `ComposeBudget`-checked call can fail.
#[derive(Debug, Clone)]
pub enum ComposeError {
    /// `check_chain_depth` found a cumulative derivation/unapplication step count exceeding `chain_depth_cap`.
    ChainDepthExceeded {
        depth: usize,
        limit: usize,
        site: &'static str,
    },
}

impl fmt::Display for ComposeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
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

#[derive(Debug, Clone, Copy)]
pub struct ComposeBudget {
    /// This crate's chain-depth dimension (this module's "Chain-depth dimension" section). `None`
    /// (the default everywhere -- `Self::from_env` and `Self::unbounded`)
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
}

impl ComposeBudget {
    /// Production entry point: every cap from its own `HC_COMPOSE_*` env var (module doc), or the
    /// documented default when unset/unparsable. The environment is read exactly once, in the
    /// production entry point.
    pub fn from_env() -> Self {
        ComposeBudget {
            chain_depth_cap: chain_depth_cap_from_env(),
        }
    }

    /// A budget with no configured chain-depth cap --
    /// for callers/tests that need a `&ComposeBudget` to satisfy a function signature but aren't
    /// exercising this mechanism.
    #[cfg(test)]
    pub(crate) fn unbounded() -> Self {
        ComposeBudget {
            chain_depth_cap: None,
        }
    }

    /// Explicit-caps builder for the chain-depth dimension: returns `self` with an
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
    /// (deterministic logical counters are the primary fast-failure mechanism). This uses the
    /// crate's own typed `ComposeError` vocabulary and names the call site, but takes a
    /// caller-reported logical
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

}

#[cfg(test)]
mod compose_budget_tests {
    use super::*;
    // Chain-depth dimension: exercises `check_chain_depth` directly, no `Fsm`/foma call involved.

    #[test]
    fn chain_depth_unbounded_budget_never_trips() {
        // `unbounded()` must also leave chain depth off, like the size dimensions above.
        let budget = ComposeBudget::unbounded();
        // Well past the motivating Aweti 24-level chain and still `Ok`.
        budget
            .check_chain_depth(1_000_000, "chain_depth_unbounded_budget_never_trips")
            .expect("unbounded chain-depth budget must never trip, at any depth");
    }

    #[test]
    fn chain_depth_with_caps_defaults_to_unbounded() {
        // An unbounded budget leaves chain depth off by default.
        let budget = ComposeBudget::unbounded();
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
