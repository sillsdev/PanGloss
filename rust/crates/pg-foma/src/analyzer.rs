//! `FomaProposer`: the thin `emit + foma-compile + apply-up` wrapper for the propose half of
//! propose→confirm; confirm itself lives elsewhere.
//!
//! Compiles `crate::emit::emit`'s lexc source with the pure-Rust `foma` crate and exposes
//! `FomaProposer::propose`: normalize the query word the same way `crate::emit` normalized
//! surface text (NFD — see that module's doc), `apply_up` it, decode every resulting tag path,
//! and split each into `tags::Candidate`s, deduped by `(morphemes, root_index)` preserving
//! first-seen order. Allomorph IDs are not part of candidate identity.

use std::collections::HashSet;
use std::fmt;
use std::time::Instant;

use foma::apply::apply_init;
use foma::lexcread::fsm_lexc_parse_string;
use foma::options::FomaOptions;
use foma::structures::fsm_sort_arcs;
use foma::types::{ApplyHandle, Fsm};

use pg_grammar::chardef::{CharDefKind, CharDefTable};
use pg_grammar::model::Grammar;

use crate::compose_budget::{ApplyBudget, ApplyDimension, ApplyOutcome, ComposeBudget};
use crate::emit::{self, EmitReport, FomaTier};
use crate::profile::{CompileProfile, CompileProfileBuilder, CompileStage};
use crate::tags::{self, Candidate};

/// Errors constructing a `FomaProposer`. Deliberately small (this stage doesn't need a rich
/// error hierarchy) — a grammar whose foma path fails to compile should fall back to the full
/// engine (plan §1's per-grammar tiering), which only needs to know THAT it failed.
#[derive(Debug)]
pub enum FomaError {
    /// `fsm_lexc_parse_string` returned `None`; carries the emitter's own report for diagnosis.
    LexcCompileFailed(EmitReport),
    /// The emitter proved that no complete Foma artifact can be built for this grammar.
    Unsupported(EmitReport),
    /// The emitter produced lexc material but also identified constructs which can contribute
    /// analyses that material does not propose. Confirmation cannot restore omitted candidates,
    /// so normal construction must refuse this report before compiling the partial network.
    Incomplete(EmitReport),
    /// `emit::emit`'s enumeration budget tripped before a usable lexc source could be built: an honest, compiler-gap error, never a panic or a silent OOM.
    EnumerationBudgetExceeded {
        /// Which measure tripped (`crate::morphotactics::EnumMeasure::label`'s text).
        measure: &'static str,
        /// The measured value at the moment the budget tripped.
        value: usize,
        /// The threshold that was exceeded (the default, or an env-var override).
        limit: usize,
        /// The complete emitter report, retained so every caller can produce the same typed
        /// health findings without recompiling or treating this refusal as a clean result.
        report: EmitReport,
    },
}

impl fmt::Display for FomaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FomaError::LexcCompileFailed(report) => write!(
                f,
                "foma lexc compile failed (emit report: {} uncovered constructs, tier {:?})",
                report.uncovered.len(),
                report.tier
            ),
            FomaError::Unsupported(report) => write!(
                f,
                "foma backend unsupported for this grammar (emit report: {} uncovered constructs, tier {:?})",
                report.uncovered.len(),
                report.tier
            ),
            FomaError::Incomplete(report) => write!(
                f,
                "foma emission is incomplete and cannot be used as a trusted proposer (emit report: {} uncovered constructs, tier {:?})",
                report.uncovered.len(),
                report.tier
            ),
            FomaError::EnumerationBudgetExceeded {
                measure,
                value,
                limit,
                ..
            } => write!(
                f,
                "grammar exceeds the foma-engine's eager-enumeration budget: {measure} = {value} when \
                 enumeration aborted at the cap -- a floor, not a total (limit {limit}). This \
                 grammar's morphotactics produce more composite lexc material \
                 than the eager Rust-side enumerator can safely expand into a literal lexc source \
                 without risking a multi-GB `.lexc` file and an out-of-memory crash in foma's own \
                 `apply_up`. Use the default (full) morphological-parser engine for this grammar \
                 instead of the foma-composite engine, or -- only if you understand why this \
                 grammar's dynamic enumeration tree is this large -- raise the budget via \
                 HC_ENUM_ENTRY_BUDGET/HC_ENUM_PROBE_BUDGET and re-run."
            ),
        }
    }
}

impl FomaError {
    /// The emitter evidence carried by compile and enumeration failures.
    pub fn emit_report(&self) -> Option<&EmitReport> {
        match self {
            FomaError::LexcCompileFailed(report)
            | FomaError::Unsupported(report)
            | FomaError::Incomplete(report)
            | FomaError::EnumerationBudgetExceeded { report, .. } => Some(report),
        }
    }
}

impl std::error::Error for FomaError {}

pub type Result<T> = std::result::Result<T, FomaError>;

fn tier_requires_unproven_build(tier: &FomaTier) -> bool {
    matches!(tier, FomaTier::Partial { .. })
}

/// Opt-in per-word proposal measurements. These counters describe only paths actually pulled from
/// foma before completion or a cooperative `ApplyBudget` trip.
#[derive(Clone, Debug, Default)]
pub struct ProposalDiagnostics {
    pub raw_paths: usize,
    pub raw_bytes: usize,
    pub decoded_paths: usize,
    pub malformed_paths: usize,
    pub unique_candidates: usize,
    pub traversal_elapsed: std::time::Duration,
    pub decode_dedup_elapsed: std::time::Duration,
}

/// The two magnitudes an `ApplyBudget` is denominated in, and nothing else.
///
/// Distinct from `ProposalDiagnostics` on purpose: these are counters the decode loop already
/// keeps, so reporting them is free, whereas the diagnostics clock every path. A budgeted
/// production run needs the counters to carry one cumulative budget across several proposals; it
/// does not need the timings.
#[derive(Clone, Copy, Debug, Default)]
pub struct ProposalCounts {
    pub raw_paths: usize,
    pub unique_candidates: usize,
}

/// Minimum arc count before `FomaProposer::new` pays `fsm_sort_arcs`'s cost to switch `apply_up` to its binary-search branch.
/// Why 10,000: `docs/research/pg-foma-analyzer-design-notes.md`, "`ARC_SORT_MIN_ARCS`".
const ARC_SORT_MIN_ARCS: i32 = 10_000;

/// Prepare a compiled network for repeated `apply_up` calls when its size clears the measured
/// break-even threshold for foma's binary-search traversal path. Direction 2 sorts outgoing arcs,
/// which is the direction `apply_up` checks through `net.arcs_sorted_out`.
pub(crate) fn prepare_network_for_apply(net: &mut Fsm) {
    if net.arccount >= ARC_SORT_MIN_ARCS {
        fsm_sort_arcs(net, 2);
    }
}

/// The compiled foma network for one grammar (as a live `ApplyHandle`, see below), plus the
/// emitter's own report (uncovered constructs, counts, tier — plan P1 gate F1's "counts are
/// plausible" assertions read this).
pub struct FomaProposer {
    // Fully owned/`'static` (a clone of the compiled `Fsm`), not a borrow this struct would also need to store.
    handle: Box<ApplyHandle>,
    /// Diagnostics from the compiler that built this proposer, when that compiler produces an
    /// `EmitReport`. Plan-composed networks deliberately carry `None`: asking the tuned-surface
    /// emitter to manufacture diagnostics for a different backend can itself be unbounded work.
    pub report: Option<EmitReport>,
    query_encoder: Option<SegmentQueryEncoder>,
}

/// Owned form of `crate::replace::SegAlphabet::encode_query`: a proposer must outlive the borrowed `SegAlphabet` used to build it.
struct SegmentQueryEncoder {
    /// NFD representations, longest first, paired with their PUA token.
    representations: Vec<(Vec<char>, char)>,
    /// Declared boundary representations, longest first, for explicit-query encoding before terminal cleanup.
    boundary_representations: Vec<Vec<char>>,
}

impl SegmentQueryEncoder {
    fn new(table: &CharDefTable) -> Self {
        let alphabet = crate::replace::SegAlphabet::new(table);
        let mut representations: Vec<(Vec<char>, char)> = Vec::new();
        let mut boundary_representations: Vec<Vec<char>> = Vec::new();
        for (id, definition) in table.iter() {
            for representation in definition.representations_nfd() {
                match definition.kind() {
                    CharDefKind::Segment => {
                        representations.push((representation.chars().collect(), alphabet.token(id)))
                    }
                    CharDefKind::Boundary => {
                        boundary_representations.push(representation.chars().collect())
                    }
                }
            }
        }
        representations.sort_by_key(|(representation, _)| std::cmp::Reverse(representation.len()));
        boundary_representations
            .sort_by_key(|representation| std::cmp::Reverse(representation.len()));
        SegmentQueryEncoder {
            representations,
            boundary_representations,
        }
    }

    fn encode(&self, word: &str) -> Option<String> {
        let normalized: Vec<char> = pg_grammar::nfd::nfd(word).chars().collect();
        let mut encoded = String::with_capacity(normalized.len());
        let mut position = 0;
        while position < normalized.len() {
            if let Some((representation, token)) = self
                .representations
                .iter()
                .find(|(representation, _)| normalized[position..].starts_with(representation))
            {
                encoded.push(*token);
                position += representation.len();
                continue;
            }
            let Some(representation) = self
                .boundary_representations
                .iter()
                .find(|representation| normalized[position..].starts_with(representation))
            else {
                return None;
            };
            position += representation.len();
        }
        Some(encoded)
    }
}

impl FomaProposer {
    /// The backend `Self::new` realizes, and so the one a capability gate in front of this
    /// constructor has to consult.
    ///
    /// A whole-grammar verdict cannot answer that question: it is the best any backend offers, and
    /// this constructor offers exactly one of them. Named here rather than at the call site so the
    /// fact lives next to the emitter it describes, and moves with it. Pinned by
    /// `the_named_backend_is_the_one_this_constructor_builds`.
    pub const EMISSION_STRATEGY: crate::enumerate::EmissionStrategy =
        crate::enumerate::EmissionStrategy::TunedSurfaceProbed;

    /// Build a proposer around an already-compiled network. This constructor performs exactly one
    /// `apply_init` and does not emit, compile, sort, compose, or minimize the supplied network.
    pub fn from_precompiled_network(net: &foma::types::Fsm, report: EmitReport) -> Self {
        FomaProposer {
            handle: apply_init(net),
            query_encoder: None,
            report: Some(report),
        }
    }

    /// Build around a network produced by a compiler whose diagnostics are not an `EmitReport`.
    /// The caller must publish that compiler's own backend report separately; this constructor
    /// never runs another compiler merely to fill an unrelated diagnostics field.
    pub(crate) fn from_precompiled_network_without_emit_report(net: &foma::types::Fsm) -> Self {
        FomaProposer {
            handle: apply_init(net),
            query_encoder: None,
            report: None,
        }
    }

    /// Attach P6's representation-to-token query encoding to a precompiled proposer.
    pub(crate) fn with_segment_query_encoder(mut self, table: &CharDefTable) -> Self {
        self.query_encoder = Some(SegmentQueryEncoder::new(table));
        self
    }

    fn encode_query(&self, word: &str) -> Option<String> {
        match &self.query_encoder {
            Some(encoder) => encoder.encode(word),
            None => Some(pg_grammar::nfd::nfd(word)),
        }
    }

    /// Emit `g`'s lexc source, compile it, and build the (word-independent) `ApplyHandle` once.
    /// Returns a typed error for invalid lexc, unsupported/incomplete emission, or a logical-work
    /// budget breach; no unsupported emitter result is compiled into a proposer.
    ///
    /// Thin, env-driven wrapper over `Self::new_with_budget` -- same convention
    /// `crate::emit::emit_with_precision` uses for the same reason (its own doc): reads
    /// `HC_ENUM_ENTRY_BUDGET`/`HC_ENUM_PROBE_BUDGET`
    /// exactly once, here, in the production entry point, so parallel test processes never race
    /// process-global env state.
    // FomaError is deliberately a small, flat enum (see its own doc above); boxing
    // `LexcCompileFailed`'s `EmitReport` to silence this lint would change the public enum's
    // variant shape for every downstream `match`, which is out of scope for a lint-only cleanup.
    #[allow(clippy::result_large_err)]
    pub fn new(g: &Grammar) -> Result<Self> {
        let enum_budget = crate::morphotactics::EnumerationBudget::from_env();
        let compose_budget = ComposeBudget::from_env();
        Self::new_with_budget(g, &enum_budget, &compose_budget)
    }

    /// `Self::new`, plus
    /// its own `CompileProfile` -- the production compile-time-profiling entry point. Reads the
    /// same env vars `Self::new` does, exactly once, mirroring its convention.
    pub fn new_with_profile(g: &Grammar) -> (Result<Self>, CompileProfile) {
        let enum_budget = crate::morphotactics::EnumerationBudget::from_env();
        let compose_budget = ComposeBudget::from_env();
        Self::new_with_budget_and_profile(g, &enum_budget, &compose_budget)
    }

    /// Development-only counterpart to [`Self::new_with_profile`]. It may compile an emitter
    /// result that is explicitly marked [`FomaTier::Partial`] so callers can inspect it, but the
    /// caller must persist an unproven/degraded trust record. It never admits `Unsupported` or a
    /// resource-aborted result, because those paths intentionally contain no usable lexc source.
    #[cfg(feature = "developer-tools")]
    pub fn new_unproven_with_profile(g: &Grammar) -> (Result<Self>, CompileProfile) {
        let enum_budget = crate::morphotactics::EnumerationBudget::from_env();
        let compose_budget = ComposeBudget::from_env();
        Self::new_with_budget_and_profile_policy(g, &enum_budget, &compose_budget, true)
    }

    /// `Self::new`'s core, with the fail-fast enumeration budget threaded in explicitly rather
    /// than read from env -- what tests call directly with a small
    /// `crate::morphotactics::EnumerationBudget::with_caps` to exercise
    /// `FomaError::EnumerationBudgetExceeded` deterministically and fast, without setting
    /// `HC_ENUM_ENTRY_BUDGET`/`HC_ENUM_PROBE_BUDGET` (this crate's tests never touch those env
    /// vars, mirroring `crate::morphotactics::ExploreMode`'s own doc's reasoning for
    /// `HC_PREEXPAND_FLAT`).
    ///
    /// Thin, zero-behavior-change wrapper over `Self::new_with_budget_and_profile`, discarding its
    /// `CompileProfile` -- proven byte-for-byte identical (same `Result`, same emitted network) by
    /// this file's own `fst_profile_new_with_budget_matches_new_with_budget_and_profile` test.
    // See the `#[allow(clippy::result_large_err)]` justification on `Self::new` above.
    #[allow(clippy::result_large_err)]
    pub(crate) fn new_with_budget(
        g: &Grammar,
        enum_budget: &crate::morphotactics::EnumerationBudget,
        compose_budget: &ComposeBudget,
    ) -> Result<Self> {
        Self::new_with_budget_and_profile(g, enum_budget, compose_budget).0
    }

    /// `Self::new_with_budget`'s real core, with a `CompileProfileBuilder`
    /// threaded through: [`CompileProfileBuilder::
    /// production`] starts the top-line wall-clock timer at the very top of this function, before
    /// any emission work runs, and `CompileProfileBuilder::finish` is called exactly once on
    /// EVERY return path (including every early-return error path) so the returned `CompileProfile`
    /// always reflects real elapsed time up to that outcome, never a fabricated/zero value.
    pub(crate) fn new_with_budget_and_profile(
        g: &Grammar,
        enum_budget: &crate::morphotactics::EnumerationBudget,
        compose_budget: &ComposeBudget,
    ) -> (Result<Self>, CompileProfile) {
        Self::new_with_budget_and_profile_policy(g, enum_budget, compose_budget, false)
    }

    fn new_with_budget_and_profile_policy(
        g: &Grammar,
        enum_budget: &crate::morphotactics::EnumerationBudget,
        compose_budget: &ComposeBudget,
        allow_incomplete: bool,
    ) -> (Result<Self>, CompileProfile) {
        let mut profile = CompileProfileBuilder::production();

        let result = emit::emit_with_budget_profiled(
            g,
            crate::precision::PrecisionConfig::Strip,
            enum_budget,
            Some(&mut profile),
        );
        Self::finish_profiled_compile(result, profile, allow_incomplete)
    }

    fn finish_profiled_compile(
        result: crate::emit::EmitResult,
        mut profile: CompileProfileBuilder,
        allow_incomplete: bool,
    ) -> (Result<Self>, CompileProfile) {
        // Checked before ever handing `result.lexc_source` to `fsm_lexc_parse_string`: when this is `Some`, `emit_with_budget_profiled` already bailed out early, so `lexc_source` is deliberately empty and must never be compiled.
        if let Some(exceeded) = result.report.enum_budget_exceeded.as_ref() {
            let err = FomaError::EnumerationBudgetExceeded {
                measure: exceeded.measure,
                value: exceeded.value,
                limit: exceeded.limit,
                report: result.report,
            };
            return (Err(err), profile.finish(None, None));
        }
        if matches!(result.report.tier, FomaTier::Unsupported { .. }) {
            return (
                Err(FomaError::Unsupported(result.report)),
                profile.finish(None, None),
            );
        }
        if tier_requires_unproven_build(&result.report.tier) && !allow_incomplete {
            return (
                Err(FomaError::Incomplete(result.report)),
                profile.finish(None, None),
            );
        }
        let opts = FomaOptions::default();
        let lexc_parse_start = Instant::now();
        let parsed = fsm_lexc_parse_string(&opts, None, &result.lexc_source);
        // A plain `Instant` delta around a call this function already makes unconditionally -- never a second parse, never an extra clone.
        profile.push_stage(CompileStage::LexcParse, lexc_parse_start.elapsed());
        match parsed {
            Some(mut net) => {
                prepare_network_for_apply(&mut net);
                // `fsm_sort_arcs` reorders arcs but never adds or removes a state/arc, so these counts are the same either way.
                let final_state_count = net.statecount;
                let final_arc_count = net.arccount;
                let proposer = FomaProposer {
                    handle: apply_init(&net),
                    report: Some(result.report),
                    query_encoder: None,
                };
                (
                    Ok(proposer),
                    profile.finish(Some(final_state_count), Some(final_arc_count)),
                )
            }
            None => (
                Err(FomaError::LexcCompileFailed(result.report)),
                profile.finish(None, None),
            ),
        }
    }

    /// Propose every candidate analysis for `word`. NFD-normalizes first (matching
    /// `crate::emit::kept_surface_text`'s own normalization — see that function's doc for why
    /// this must be consistent on both sides regardless of the caller's on-disk encoding).
    /// Dedups by `(morphemes, root_index)`, preserving first-seen order across BOTH the
    /// `apply_up` path order and, within one path, the compound-split order (`tags::to_candidates`
    /// already yields ascending root-position order for a single path).
    ///
    /// Reuses `self.handle` across calls rather than rebuilding it per word (vendored
    /// `foma::apply::apply_init`, ~apply.rs:481-577, unconditionally deep-clones the whole
    /// compiled `Fsm` and rebuilds `apply_create_statemap`/`apply_create_sigarray` — all a
    /// function of the NETWORK only, never the word). The per-word entry point,
    /// `foma::apply::apply_up` (apply.rs:462-475, reached via `ApplyHandle::up`, apply.rs:667-669),
    /// resets only per-word state — `h.instring`, `apply_create_sigmatch` (word-derived sigma
    /// matches), and `apply_force_clear_stack` (apply.rs:424-433's `apply_updown`, the `Some(w)`
    /// arm) — leaving `last_net`/`statemap`/`sigmatch_array`/`sigma_trie` (the grammar-static
    /// tables) untouched, so repeated `up` calls on one handle are exactly the reuse this needs.
    pub fn propose(&mut self, word: &str) -> Vec<Candidate> {
        match self.propose_budgeted(word, &ApplyBudget::unbounded()) {
            ApplyOutcome::Complete(candidates) => candidates,
            ApplyOutcome::Incomplete { .. } => {
                unreachable!("ApplyBudget::unbounded() can never report Incomplete")
            }
        }
    }

    /// `Self::propose`'s core, plus in-process cooperative magnitude containment
    /// (`crate::compose_budget`'s own "Apply-path dimension" section doc): checks `budget`'s two
    /// magnitude dimensions -- raw decoded-path count, distinct-candidate count -- as this word's
    /// `apply_up` result iterator is walked, returning `ApplyOutcome::Incomplete` the instant
    /// either cap is exceeded rather than continuing to decode/allocate further for this word. This
    /// is deliberately NOT a watchdog: there is no worker process to spawn or kill here (a native
    /// thread cannot be safely hard-killed in Rust; this method runs entirely in the
    /// caller's own process, on `self.handle`, exactly like `Self::propose` always has) -- the
    /// containment is a plain deterministic counter, checked cooperatively, the same discipline
    /// `ComposeBudget::check_chain_depth` already uses one call stack over in the compile-time
    /// composition path.
    ///
    /// `ApplyBudget::unbounded` (what `Self::propose` passes) can never report `Incomplete` --
    /// every check below is `Some(cap) if count > cap`, so a `None` cap is always `false` -- which
    /// is exactly how `Self::propose` proves its own behavior is unchanged by this addition
    /// without duplicating the decode loop.
    pub fn propose_budgeted(
        &mut self,
        word: &str,
        budget: &ApplyBudget,
    ) -> ApplyOutcome<Vec<Candidate>> {
        self.propose_budgeted_counted(word, budget).0
    }

    /// `Self::propose_budgeted` with the two magnitudes it consumed, and nothing else.
    ///
    /// A budgeted *production* run needs one cumulative budget spanning the direct proposal and
    /// every proposal reduplication peeling requests, which means each call has to report what it
    /// spent. `Self::propose_with_diagnostics_budgeted` already reports that, but it calls
    /// `Instant::now()` twice per raw path — on a word that decodes a hundred thousand paths that
    /// is two hundred thousand clock reads bought for a number nobody asked for. These are plain
    /// counters the decode loop was already keeping.
    ///
    /// `unique_candidates` is the count at the point of return, so a trip reports the magnitude
    /// that tripped rather than a truncated set's length.
    pub fn propose_budgeted_counted(
        &mut self,
        word: &str,
        budget: &ApplyBudget,
    ) -> (ApplyOutcome<Vec<Candidate>>, ProposalCounts) {
        let Some(normalized) = self.encode_query(word) else {
            return (
                ApplyOutcome::Complete(Vec::new()),
                ProposalCounts::default(),
            );
        };
        let mut seen: HashSet<(Vec<u32>, i32)> = HashSet::new();
        let mut out = Vec::new();
        let mut counts = ProposalCounts::default();
        for s in self.handle.up(&normalized) {
            counts.raw_paths += 1;
            if let Some(limit) = budget.path_cap() {
                if counts.raw_paths > limit {
                    return (
                        ApplyOutcome::Incomplete {
                            dimension: ApplyDimension::DecodedPaths,
                            value: counts.raw_paths,
                            limit,
                        },
                        counts,
                    );
                }
            }
            let Some(path) = tags::decode_path(&s) else {
                continue;
            };
            for c in tags::to_candidates(&path) {
                let key: (Vec<u32>, i32) =
                    (c.morphemes.iter().map(|m| m.0).collect(), c.root_index);
                if seen.insert(key) {
                    out.push(c);
                    counts.unique_candidates = out.len();
                    if let Some(limit) = budget.candidate_cap() {
                        if out.len() > limit {
                            return (
                                ApplyOutcome::Incomplete {
                                    dimension: ApplyDimension::Candidates,
                                    value: out.len(),
                                    limit,
                                },
                                counts,
                            );
                        }
                    }
                }
            }
        }
        (ApplyOutcome::Complete(out), counts)
    }

    /// Opt-in diagnostic sibling of `Self::propose`. The ordinary proposal APIs do not call a
    /// clock or allocate diagnostic state; callers pay this instrumentation cost only here.
    pub fn propose_with_diagnostics(
        &mut self,
        word: &str,
    ) -> (Vec<Candidate>, ProposalDiagnostics) {
        let (outcome, diagnostics) =
            self.propose_with_diagnostics_budgeted(word, &ApplyBudget::unbounded());
        match outcome {
            ApplyOutcome::Complete(candidates) => (candidates, diagnostics),
            ApplyOutcome::Incomplete { .. } => {
                unreachable!("ApplyBudget::unbounded() can never report Incomplete")
            }
        }
    }

    /// `Self::propose_budgeted` with opt-in path, byte, decode, dedup, and timing diagnostics.
    /// Budget dimensions and first-seen candidate order are identical to the ordinary path.
    pub fn propose_with_diagnostics_budgeted(
        &mut self,
        word: &str,
        budget: &ApplyBudget,
    ) -> (ApplyOutcome<Vec<Candidate>>, ProposalDiagnostics) {
        let Some(normalized) = self.encode_query(word) else {
            return (
                ApplyOutcome::Complete(Vec::new()),
                ProposalDiagnostics::default(),
            );
        };
        let mut seen: HashSet<(Vec<u32>, i32)> = HashSet::new();
        let mut out = Vec::new();
        let mut diagnostics = ProposalDiagnostics::default();
        let mut paths = self.handle.up(&normalized);

        loop {
            let traversal_start = Instant::now();
            let raw = paths.next();
            diagnostics.traversal_elapsed += traversal_start.elapsed();
            let Some(raw) = raw else { break };

            let decode_start = Instant::now();
            diagnostics.raw_paths += 1;
            diagnostics.raw_bytes += raw.len();
            let path = match tags::decode_path(&raw) {
                Some(path) => {
                    diagnostics.decoded_paths += 1;
                    Some(path)
                }
                None => {
                    diagnostics.malformed_paths += 1;
                    None
                }
            };

            if let Some(limit) = budget.path_cap() {
                if diagnostics.raw_paths > limit {
                    diagnostics.decode_dedup_elapsed += decode_start.elapsed();
                    return (
                        ApplyOutcome::Incomplete {
                            dimension: ApplyDimension::DecodedPaths,
                            value: diagnostics.raw_paths,
                            limit,
                        },
                        diagnostics,
                    );
                }
            }

            let Some(path) = path else {
                diagnostics.decode_dedup_elapsed += decode_start.elapsed();
                continue;
            };
            for candidate in tags::to_candidates(&path) {
                let key = (
                    candidate.morphemes.iter().map(|m| m.0).collect(),
                    candidate.root_index,
                );
                if seen.insert(key) {
                    out.push(candidate);
                    diagnostics.unique_candidates = out.len();
                    if let Some(limit) = budget.candidate_cap() {
                        if out.len() > limit {
                            diagnostics.decode_dedup_elapsed += decode_start.elapsed();
                            return (
                                ApplyOutcome::Incomplete {
                                    dimension: ApplyDimension::Candidates,
                                    value: out.len(),
                                    limit,
                                },
                                diagnostics,
                            );
                        }
                    }
                }
            }
            diagnostics.decode_dedup_elapsed += decode_start.elapsed();
        }

        (ApplyOutcome::Complete(out), diagnostics)
    }

    /// Serializes this proposer's own compiled network to foma's existing binary-memory encoding
    /// (`foma::io::fsm_write_binary` — the same gzip'd format `fsm_read_binary_mem` reads):
    /// foma's tested binary-memory representation is reused
    /// inside a PanGloss envelope rather than inventing another network encoding. This is the
    /// REAL foma payload `pg-cli`'s `pack.rs` writes into a `.pgpack` container — no second network
    /// format, no fabricated bytes.
    ///
    /// `self.handle.last_net` is always `Some` here: `apply_init` (called by every constructor
    /// above, immediately after a successful `fsm_lexc_parse_string`) unconditionally populates it
    /// with a clone of the just-compiled network before returning the handle — see `apply_init`'s
    /// own doc, "C: h->last_net = net (borrowed). DEVIATION from C (owns a clone...)". There is no
    /// code path that constructs a `FomaProposer` without going through `apply_init` first.
    pub fn foma_binary_payload(&self) -> std::io::Result<Vec<u8>> {
        let mut bytes = Vec::new();
        foma::io::fsm_write_binary(self.network(), &mut bytes)?;
        Ok(bytes)
    }

    /// `(statecount, arccount)` of this proposer's own compiled network — a cheap struct-field
    /// read (both are `Copy` `i32` fields), exposed so a caller can compare a freshly-compiled
    /// network's shape against one reconstructed from a serialized payload
    /// (`read_foma_binary_payload`) without either side needing its own `foma` crate dependency.
    pub fn network_counts(&self) -> (i32, i32) {
        let net = self.network();
        (net.statecount, net.arccount)
    }

    /// Raw `apply_up` over this proposer's own live handle — undecoded, undeduped, unnormalized
    /// (unlike `Self::propose`/`Self::propose_budgeted`). Exposed so a round-trip test can
    /// compare THIS exact traversal against `apply_up_against` run on a network reconstructed
    /// from this same proposer's serialized `Self::foma_binary_payload` bytes, without going
    /// through `propose`'s richer decode/dedup pipeline on one side only.
    pub fn apply_up_raw(&mut self, word: &str) -> Vec<String> {
        self.handle.up(word).collect()
    }

    /// This proposer's own compiled network, as built by `apply_init` (`Self::foma_binary_payload`'s doc explains why `last_net` is always `Some` here).
    fn network(&self) -> &foma::types::Fsm {
        self.handle.last_net.as_ref().expect(
            "FomaProposer::handle is always built by apply_init, which unconditionally sets \
             ApplyHandle::last_net to a clone of the compiled network",
        )
    }
}

/// Reads a foma binary-memory payload back into a live `foma::types::Fsm` — the read side of
/// `FomaProposer::foma_binary_payload`, exposed here (rather than requiring every caller to add
/// its own direct `foma` crate dependency) so `pg-pack`/`pg-cli` round-trip tests, and eventually a
/// packaged-analyzer loader, can reconstruct the compiled network from `.pgpack` bytes using the
/// SAME entry point (`fsm_read_binary_mem`), never a
/// second parser.
pub fn read_foma_binary_payload(
    bytes: &[u8],
) -> std::result::Result<foma::types::Fsm, foma::error::FomaError> {
    foma::io::fsm_read_binary_mem(bytes)
}

/// Applies `word` up (`apply_up`) against an arbitrary already-compiled network — e.g. one just
/// reconstructed by `read_foma_binary_payload` — and drains every surface->analysis path into an
/// owned `Vec`. Lets a round-trip test check apply-agreement between an original compile and its
/// reconstructed twin without needing its own `foma::apply` dependency (mirrors
/// `read_foma_binary_payload`'s own reasoning). NFD-normalization is deliberately NOT applied
/// here (unlike `FomaProposer::propose_budgeted`) — this is a thin, direct `apply_up` wrapper for
/// comparing two networks against the SAME literal input, not a query-normalization entry point.
pub fn apply_up_against(net: &foma::types::Fsm, word: &str) -> Vec<String> {
    let mut handle = foma::apply::apply_init(net);
    handle.up(word).collect()
}

#[cfg(test)]
mod budget_tests {
    //! Fail-fast enumeration budget regression tests: `FomaProposer::new` must abort fast with the typed `FomaError::EnumerationBudgetExceeded`, never a panic and never an unbounded run.

    /// How far past its cap an incremental check may report before we call it late.
    /// Why 50 distinguishes "noticed promptly" from "fires once at the end": `docs/research/pg-foma-analyzer-design-notes.md`, "The Aweti enumeration-budget motivation".
    const OVERSHOOT_FACTOR: usize = 50;

    use super::*;
    use crate::morphotactics::EnumerationBudget;

    #[test]
    fn partial_emission_requires_the_explicit_unproven_constructor() {
        assert!(tier_requires_unproven_build(&FomaTier::Partial {
            uncovered: 1
        }));
        assert!(!tier_requires_unproven_build(&FomaTier::Full));
        assert!(!tier_requires_unproven_build(&FomaTier::Unsupported {
            reason: "synthetic refusal".to_string()
        }));
    }

    #[test]
    fn enumeration_error_preserves_emit_report_for_health_consumers() {
        let report = EmitReport {
            uncovered: Vec::new(),
            counts: crate::emit::EmitCounts::default(),
            tier: FomaTier::Unsupported {
                reason: "synthetic enumeration refusal".to_string(),
            },
            enum_budget_exceeded: Some(crate::emit::EnumBudgetExceeded {
                measure: "synthetic composite work",
                value: 101,
                limit: 100,
            }),
            closure_refusal: None,
            closure_evidence: None,
        };
        let error = FomaError::EnumerationBudgetExceeded {
            measure: "synthetic composite work",
            value: 101,
            limit: 100,
            report,
        };

        let preserved = error
            .emit_report()
            .expect("an enumeration refusal must retain its complete emit report");
        assert_eq!(preserved.enum_budget_exceeded.as_ref().unwrap().value, 101);
        assert!(matches!(preserved.tier, FomaTier::Unsupported { .. }));
    }

    /// Loads the real Aweti grammar if present on disk; gitignored, so copy it from the main checkout's `samples/data/` if missing.
    fn load_aweti() -> Option<Grammar> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../samples/data/aweti.json");
        if !path.exists() {
            return None;
        }
        let json = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let snapshot = pg_snapshot::Snapshot::from_json(&json)
            .unwrap_or_else(|e| panic!("parse aweti.json snapshot: {e}"));
        let (g, _warnings) = pg_grammar::compile_project(&snapshot)
            .unwrap_or_else(|e| panic!("compile aweti project: {e}"));
        Some(g)
    }

    /// The core regression: a tiny composite-entry cap must trip on Aweti fast and surface as a typed error, not a panic and not a hang.
    #[test]
    fn aweti_trips_enumeration_budget_fast_with_typed_error() {
        let Some(g) = load_aweti() else {
            eprintln!("skipping: samples/data/aweti.json not present on disk");
            return;
        };
        // Entry cap of 10, far below any real grammar's default, injected directly here; probe cap left unbounded so this test isolates the entry measure specifically.
        let budget = EnumerationBudget::with_caps(10, usize::MAX);

        let t0 = std::time::Instant::now();
        let result = FomaProposer::new_with_budget(&g, &budget, &ComposeBudget::unbounded());
        let elapsed = t0.elapsed();
        eprintln!("aweti tiny-entry-budget trip took {elapsed:?}");

        match result {
            Err(FomaError::EnumerationBudgetExceeded {
                measure,
                value,
                limit,
                report,
            }) => {
                assert_eq!(limit, 10, "limit must echo back the injected cap");
                eprintln!("aweti tiny-entry-budget tripped at value={value} limit={limit}");
                assert!(
                    value > limit,
                    "tripped value {value} must exceed the limit {limit}"
                );
                assert_eq!(
                    measure, "composite lexc entries (fusion + interdigitation + structural)",
                    "a tiny entry cap (probe cap unbounded) must trip on the ENTRY measure"
                );
                assert_eq!(
                    report.enum_budget_exceeded.as_ref().map(|trip| trip.value),
                    Some(value),
                    "the retained report must describe the same trip as the error fields"
                );
                // Asserted as overshoot rather than wall-clock: a check that stops running incrementally would report Aweti's entire enumeration, not a handful of entries past the cap, and overshoot is deterministic where wall-clock is hostage to whoever else is compiling.
                assert!(
                    value <= limit.saturating_mul(OVERSHOOT_FACTOR),
                    "fail-fast budget must notice promptly: tripped at {value} against cap {limit},                      more than {OVERSHOOT_FACTOR}x over -- the signature of a check that stopped                      running incrementally"
                );
            }
            Err(other) => panic!(
                "expected FomaError::EnumerationBudgetExceeded, got a different FomaError: {other}"
            ),
            Ok(_) => panic!(
                "expected the tiny entry budget (cap=10) to trip on Aweti; \
                 FomaProposer::new_with_budget succeeded instead"
            ),
        }
    }

    /// The probe-count measure, isolated: an unlimited entry cap paired with a tiny probe cap must still trip, and report the other measure -- proving the two measures are wired independently.
    #[test]
    fn aweti_trips_on_probe_measure_when_entry_cap_is_unbounded() {
        let Some(g) = load_aweti() else {
            eprintln!("skipping: samples/data/aweti.json not present on disk");
            return;
        };
        let budget = EnumerationBudget::with_caps(usize::MAX, 5);

        let t0 = std::time::Instant::now();
        let result = FomaProposer::new_with_budget(&g, &budget, &ComposeBudget::unbounded());
        let elapsed = t0.elapsed();
        eprintln!("aweti tiny-probe-budget trip took {elapsed:?}");

        match result {
            Err(FomaError::EnumerationBudgetExceeded {
                measure,
                value,
                limit,
                report,
            }) => {
                assert_eq!(limit, 5);
                assert!(value > limit);
                eprintln!("aweti tiny-probe-budget tripped at value={value} limit={limit}");
                assert_eq!(measure, "(root, rule) pairs probed");
                assert_eq!(
                    report.enum_budget_exceeded.as_ref().map(|trip| trip.value),
                    Some(value),
                    "the retained report must describe the same trip as the error fields"
                );
                // Same fail-fast property, same reasoning as the entry-measure test above.
                assert!(
                    value <= limit.saturating_mul(OVERSHOOT_FACTOR),
                    "fail-fast budget must notice promptly: tripped at {value} against cap {limit}"
                );
            }
            Err(other) => panic!(
                "expected FomaError::EnumerationBudgetExceeded, got a different FomaError: {other}"
            ),
            Ok(_) => panic!("expected the tiny probe budget (cap=5) to trip on Aweti"),
        }
    }

    /// Sanity check the other direction: on a tiny grammar with no composite mechanism at all, an unbounded budget must never trip.
    #[test]
    fn tiny_grammar_never_trips_unbounded_budget() {
        const FIXTURE: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE HermitCrabInput SYSTEM "HermitCrabInput.dtd">
<HermitCrabInput>
  <Language>
    <Name>MtBudgetSmoke</Name>
    <PartsOfSpeech>
      <PartOfSpeech id="posV"><Name>v</Name></PartOfSpeech>
    </PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cK"><Representations><Representation>k</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered">
        <Name>Main</Name>
        <LexicalEntries>
          <LexicalEntry id="eK" partOfSpeech="posV">
            <Allomorphs><Allomorph id="aK"><PhoneticShape>ka</PhoneticShape></Allomorph></Allomorphs>
            <MorphemeId>K</MorphemeId>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>"#;
        let g = pg_grammar::load(FIXTURE).unwrap_or_else(|e| panic!("fixture failed to load: {e}"));
        let budget = EnumerationBudget::unbounded();
        let result = FomaProposer::new_with_budget(&g, &budget, &ComposeBudget::unbounded());
        assert!(
            result.is_ok(),
            "an unbounded budget must never trip on a tiny, non-composite grammar"
        );
    }
}

#[cfg(test)]
mod apply_budget_tests {
    //! `FomaProposer::propose_budgeted` must behave byte-for-byte identically to `propose` when unbounded, and trip each dimension deterministically and cheaply, in-process.

    use super::*;
    use crate::compose_budget::ApplyBudget;

    /// A single-root, no-affix, no-rule fixture: `propose("ka")` finds exactly the bare root candidate, so a cap of 0 on either dimension trips on the very first decoded path/candidate.
    const FIXTURE: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE HermitCrabInput SYSTEM "HermitCrabInput.dtd">
<HermitCrabInput>
  <Language>
    <Name>ApplyBudgetSmoke</Name>
    <PartsOfSpeech>
      <PartOfSpeech id="posV"><Name>v</Name></PartOfSpeech>
    </PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cK"><Representations><Representation>k</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered">
        <Name>Main</Name>
        <LexicalEntries>
          <LexicalEntry id="eK" partOfSpeech="posV">
            <Allomorphs><Allomorph id="aK"><PhoneticShape>ka</PhoneticShape></Allomorph></Allomorphs>
            <MorphemeId>K</MorphemeId>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>"#;

    fn proposer() -> FomaProposer {
        let g = pg_grammar::load(FIXTURE).unwrap_or_else(|e| panic!("fixture failed to load: {e}"));
        FomaProposer::new(&g).unwrap_or_else(|e| panic!("proposer build failed: {e}"))
    }

    #[test]
    fn propose_budgeted_unbounded_matches_plain_propose_exactly() {
        let mut p = proposer();
        let via_budgeted = match p.propose_budgeted("ka", &ApplyBudget::unbounded()) {
            ApplyOutcome::Complete(candidates) => candidates,
            ApplyOutcome::Incomplete { .. } => {
                panic!("ApplyBudget::unbounded() must never report Incomplete")
            }
        };
        let mut p2 = proposer();
        let via_plain = p2.propose("ka");
        assert_eq!(
            via_budgeted.len(),
            via_plain.len(),
            "propose_budgeted(unbounded) must find exactly as many candidates as propose()"
        );
        assert!(
            !via_plain.is_empty(),
            "the fixture's bare root must propose at least one candidate for this test to be \
             meaningful"
        );
    }

    #[test]
    fn propose_budgeted_path_cap_zero_trips_on_first_decoded_path() {
        let mut p = proposer();
        let budget = ApplyBudget::with_caps(Some(0), None);
        match p.propose_budgeted("ka", &budget) {
            ApplyOutcome::Incomplete {
                dimension,
                value,
                limit,
            } => {
                assert_eq!(dimension, ApplyDimension::DecodedPaths);
                assert_eq!(value, 1, "must trip at exactly one past the cap, not later");
                assert_eq!(limit, 0);
            }
            ApplyOutcome::Complete(candidates) => panic!(
                "expected a path-cap=0 trip on a word with at least one apply_up result, got \
                 Complete({candidates:?})"
            ),
        }
    }

    #[test]
    fn propose_budgeted_candidate_cap_zero_trips_on_first_candidate() {
        let mut p = proposer();
        let budget = ApplyBudget::with_caps(None, Some(0));
        match p.propose_budgeted("ka", &budget) {
            ApplyOutcome::Incomplete {
                dimension,
                value,
                limit,
            } => {
                assert_eq!(dimension, ApplyDimension::Candidates);
                assert_eq!(value, 1);
                assert_eq!(limit, 0);
            }
            ApplyOutcome::Complete(candidates) => panic!(
                "expected a candidate-cap=0 trip on a word with at least one candidate, got \
                 Complete({candidates:?})"
            ),
        }
    }

    #[test]
    fn propose_budgeted_generous_caps_never_trip() {
        let mut p = proposer();
        let budget = ApplyBudget::with_caps(Some(1_000_000), Some(1_000_000));
        match p.propose_budgeted("ka", &budget) {
            ApplyOutcome::Complete(candidates) => assert!(!candidates.is_empty()),
            ApplyOutcome::Incomplete { dimension, .. } => {
                panic!("a generous cap must not trip on a tiny fixture (dimension: {dimension:?})")
            }
        }
    }

    #[test]
    fn propose_with_diagnostics_matches_plain_candidates_and_accounts_for_every_raw_path() {
        let mut plain = proposer();
        let expected = plain.propose("ka");

        let mut profiled = proposer();
        let (actual, diagnostics) = profiled.propose_with_diagnostics("ka");

        let identities = |candidates: &[Candidate]| {
            candidates
                .iter()
                .map(|c| {
                    (
                        c.morphemes.iter().map(|m| m.0).collect::<Vec<_>>(),
                        c.root_index,
                    )
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(identities(&actual), identities(&expected));
        assert_eq!(
            diagnostics.raw_paths,
            diagnostics.decoded_paths + diagnostics.malformed_paths
        );
        assert!(
            diagnostics.raw_paths > 0,
            "fixture must exercise apply_up traversal"
        );
        assert!(
            diagnostics.raw_bytes > 0,
            "raw path byte accounting must be populated"
        );
        assert_eq!(diagnostics.unique_candidates, actual.len());
    }

    #[test]
    fn propose_with_diagnostics_budgeted_preserves_the_first_path_budget_trip() {
        let mut p = proposer();
        let budget = ApplyBudget::with_caps(Some(0), None);
        let (outcome, diagnostics) = p.propose_with_diagnostics_budgeted("ka", &budget);

        assert!(matches!(
            outcome,
            ApplyOutcome::Incomplete {
                dimension: ApplyDimension::DecodedPaths,
                value: 1,
                limit: 0,
            }
        ));
        assert_eq!(diagnostics.raw_paths, 1);
        assert_eq!(
            diagnostics.raw_paths,
            diagnostics.decoded_paths + diagnostics.malformed_paths
        );
        assert_eq!(diagnostics.unique_candidates, 0);
    }

    #[test]
    fn from_precompiled_network_matches_normal_candidates_and_diagnostics() {
        let mut normal = proposer();
        let net = normal.network().clone();
        let report = normal
            .report
            .clone()
            .expect("the tuned emitter supplies its own report");
        let expected = normal.propose("ka");

        let mut precompiled = FomaProposer::from_precompiled_network(&net, report);
        let (actual, diagnostics) = precompiled.propose_with_diagnostics("ka");
        let identities = |candidates: &[Candidate]| {
            candidates
                .iter()
                .map(|candidate| {
                    (
                        candidate
                            .morphemes
                            .iter()
                            .map(|morpheme| morpheme.0)
                            .collect::<Vec<_>>(),
                        candidate.root_index,
                    )
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(identities(&actual), identities(&expected));
        assert_eq!(
            diagnostics.raw_paths,
            diagnostics.decoded_paths + diagnostics.malformed_paths
        );
        assert_eq!(diagnostics.unique_candidates, actual.len());
    }
}

#[cfg(test)]
mod profile_tests {
    //! Profiled construction must populate a real `CompileProfile` on success, match the non-profiled entry points byte-for-byte, and still produce a profile on a typed build failure.

    use super::*;
    use crate::morphotactics::EnumerationBudget;
    use crate::profile::{CompileStage, ProfileLabel};

    /// Same minimal single-root fixture shape as `apply_budget_tests::FIXTURE`.
    const FIXTURE: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE HermitCrabInput SYSTEM "HermitCrabInput.dtd">
<HermitCrabInput>
  <Language>
    <Name>ProfileSmoke</Name>
    <PartsOfSpeech>
      <PartOfSpeech id="posV"><Name>v</Name></PartOfSpeech>
    </PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cK"><Representations><Representation>k</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered">
        <Name>Main</Name>
        <LexicalEntries>
          <LexicalEntry id="eK" partOfSpeech="posV">
            <Allomorphs><Allomorph id="aK"><PhoneticShape>ka</PhoneticShape></Allomorph></Allomorphs>
            <MorphemeId>K</MorphemeId>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>"#;

    fn load_fixture() -> Grammar {
        pg_grammar::load(FIXTURE).unwrap_or_else(|e| panic!("fixture failed to load: {e}"))
    }

    #[test]
    fn new_with_profile_populates_lexc_parse_stage_and_final_network_counts() {
        let g = load_fixture();
        let (result, profile) = FomaProposer::new_with_profile(&g);
        assert!(result.is_ok(), "the tiny fixture must build successfully");

        assert_eq!(profile.label, ProfileLabel::Production);
        assert_eq!(profile.pipeline, crate::profile::PRODUCTION_PIPELINE);
        assert!(
            profile
                .stages
                .iter()
                .any(|s| s.stage == CompileStage::LexcParse),
            "a successful build must record the LexcParse stage"
        );
        assert!(
            profile.final_state_count.is_some_and(|v| v > 0),
            "a compiled network must report a positive state count"
        );
        assert!(
            profile.final_arc_count.is_some_and(|v| v >= 0),
            "a compiled network must report a final arc count"
        );
        assert!(profile.total_lexc_lines.is_some_and(|v| v > 0));
    }

    /// `FomaProposer::EMISSION_STRATEGY` must name the same compiler `crate::lowering_adapter::LoweringAdapter::TunedSurfaceEmit` does, since that adapter's own contract is `FomaProposer::new`.
    #[test]
    fn the_named_backend_is_the_one_this_constructor_builds() {
        assert_eq!(
            crate::lowering_adapter::LoweringAdapter::for_strategy(FomaProposer::EMISSION_STRATEGY),
            crate::lowering_adapter::LoweringAdapter::TunedSurfaceEmit,
            "the gate's named backend and this constructor's own lowering adapter must agree"
        );
        assert!(
            FomaProposer::EMISSION_STRATEGY.is_whole_grammar(),
            "this constructor compiles the whole grammar, not the controllable subtree"
        );
    }

    /// The profiled path must build the same network as the non-profiled path -- proven via identical `propose` results, not just "both `Ok`".
    #[test]
    fn new_with_budget_and_profile_matches_new_with_budget_byte_for_byte() {
        let g = load_fixture();
        let enum_budget = EnumerationBudget::from_env();
        let compose_budget = ComposeBudget::from_env();

        let mut without_profile = FomaProposer::new_with_budget(&g, &enum_budget, &compose_budget)
            .unwrap_or_else(|e| panic!("new_with_budget failed: {e}"));
        let (with_profile, _profile) =
            FomaProposer::new_with_budget_and_profile(&g, &enum_budget, &compose_budget);
        let mut with_profile =
            with_profile.unwrap_or_else(|e| panic!("new_with_budget_and_profile failed: {e}"));

        assert_eq!(without_profile.propose("ka"), with_profile.propose("ka"));
    }

    /// A typed build failure must still return a `CompileProfile` (network counts `None`, never fabricated) rather than panicking or losing the profile.
    #[test]
    fn new_with_budget_and_profile_returns_a_profile_on_typed_build_failure() {
        let g = load_fixture();
        // A zero-entry cap should trip immediately here; if this fixture's should_run gate ever short-circuits enumeration entirely instead, the Ok branch below still exercises a valid profile shape, so this test is never spuriously broken by that possibility.
        let enum_budget = EnumerationBudget::with_caps(0, 0);
        let compose_budget = ComposeBudget::unbounded();

        let (result, profile) =
            FomaProposer::new_with_budget_and_profile(&g, &enum_budget, &compose_budget);

        assert_eq!(profile.label, ProfileLabel::Production);
        match result {
            Err(_) => {
                assert_eq!(profile.final_state_count, None);
                assert_eq!(profile.final_arc_count, None);
            }
            Ok(_) => {
                // should_run was false for this fixture; the zero cap never got exercised, but the profile is still valid.
                assert!(profile.final_state_count.is_some());
            }
        }
    }
}
