//! `FomaAnalyzer` (plan §1 architecture / P2 "propose→confirm composite", gate F2): the public
//! product API tying [`crate::analyzer::FomaProposer`] (propose), [`crate::peel::ReduplicationPeeler`]
//! (D6), and [`crate::confirm`] (verify + D4 multiplicity recovery) into one `analyze_word` call
//! whose output shape mirrors `pg_parse::ParseOutcome`'s essentials — `analyses`/`structured`,
//! parallel by index, `pg-parse/src/morpher.rs:79-120` — plus diagnostics.
//!
//! Pipeline (plan §1's diagram): `propose(word)` UNION `peel_candidates(word, propose)`, deduped by
//! `(morphemes, root_index)` (plan §2: "Allomorph IDs are NOT part of candidate identity"), then
//! `confirm_all` on each surviving candidate, concatenating every match. Over-generation is pruned
//! silently by `confirm_all` (candidates that don't re-derive under restricted search simply
//! contribute zero matches); under-generation would be a recall bug in `propose`/`peel_candidates`
//! themselves (P1's job, already gated).

use std::time::Duration;

#[cfg(not(target_arch = "wasm32"))]
use rayon::prelude::*;

use pg_grammar::model::Grammar;
use pg_parse::{Morpher, WordAnalysis};

use crate::analyzer::{FomaError, FomaProposer, ProposalCounts, ProposalDiagnostics};
use crate::compose_budget::{
    ApplyBudget, ApplyDimension, ApplyOutcome, ComposeBudget, ComposeError,
};
use crate::confirm::{self, MorphemeOwner};
use crate::peel::ReduplicationPeeler;
use crate::tags::Candidate;

/// Serialized proposal result detached from the mutable foma apply handle.
pub struct ProposedWord {
    candidates: Vec<Candidate>,
    peel_used: bool,
    /// See [`FomaOutcome::peel_chain_depth_error`]'s own doc.
    peel_chain_depth_error: Option<ComposeError>,
    propose_elapsed: Duration,
}

type ConfirmedBuckets = Vec<Vec<(WordAnalysis, String, String)>>;
type TimedConfirmedBuckets = (ConfirmedBuckets, Duration);

/// `Ok` payload of [`FomaAnalyzer::propose_candidates_with_diagnostics_budgeted`]: the deduped
/// candidate set plus peel/diagnostics bookkeeping.
type ProposeCandidatesOk = (
    Vec<Candidate>,
    bool,
    Option<ComposeError>,
    ProposalDiagnostics,
    usize,
);
/// `Err` payload of [`FomaAnalyzer::propose_candidates_with_diagnostics_budgeted`]: which budget
/// dimension tripped, the measured value/limit, and diagnostics for the measured prefix.
type ProposeCandidatesErr = (ApplyDimension, usize, usize, ProposalDiagnostics, usize);

#[cfg(all(feature = "test-concurrency-hook", not(target_arch = "wasm32")))]
#[doc(hidden)]
pub mod test_confirmation_concurrency {
    use std::sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Barrier,
    };

    #[derive(Clone)]
    pub struct Probe {
        state: Arc<State>,
    }

    struct State {
        active: AtomicUsize,
        max_active: AtomicUsize,
        rendezvous: Barrier,
        arrivals: AtomicUsize,
        rendezvous_enabled: AtomicBool,
    }

    impl Probe {
        pub(super) fn new() -> Self {
            Self {
                state: Arc::new(State {
                    active: AtomicUsize::new(0),
                    max_active: AtomicUsize::new(0),
                    rendezvous: Barrier::new(2),
                    arrivals: AtomicUsize::new(0),
                    rendezvous_enabled: AtomicBool::new(false),
                }),
            }
        }

        pub(super) fn prepare(&self, possible_concurrency: usize) {
            self.state.active.store(0, Ordering::SeqCst);
            self.state.max_active.store(0, Ordering::SeqCst);
            self.state.arrivals.store(0, Ordering::SeqCst);
            self.state
                .rendezvous_enabled
                .store(possible_concurrency > 1, Ordering::SeqCst);
        }

        pub fn max_active(&self) -> usize {
            self.state.max_active.load(Ordering::SeqCst)
        }
    }

    pub(super) struct Guard(Arc<State>);
    impl Guard {
        pub(super) fn enter(probe: &Probe) -> Self {
            let state = Arc::clone(&probe.state);
            let active = state.active.fetch_add(1, Ordering::SeqCst) + 1;
            state.max_active.fetch_max(active, Ordering::SeqCst);
            if state.rendezvous_enabled.load(Ordering::SeqCst)
                && state.arrivals.fetch_add(1, Ordering::SeqCst) < 2
            {
                state.rendezvous.wait();
            }
            Self(state)
        }
    }
    impl Drop for Guard {
        fn drop(&mut self) {
            self.0.active.fetch_sub(1, Ordering::SeqCst);
        }
    }
}

/// Per-word timer for [`FomaAnalyzer::analyze_words`]'s reported durations.
/// `std::time::Instant::now()` COMPILES on wasm32-unknown-unknown but ABORTS at runtime
/// ("time not implemented on this platform") — the same compiles-but-aborts trap as
/// `SystemTime::now`/`thread::spawn` (see `rust/tools/f4-wasm-smoke.js`'s reason for existing).
/// The wasm32 arm therefore reports `Duration::ZERO` instead of timing; native is unchanged.
#[cfg(not(target_arch = "wasm32"))]
mod word_timer {
    pub struct Timer(std::time::Instant);
    pub fn start() -> Timer {
        Timer(std::time::Instant::now())
    }
    impl Timer {
        pub fn elapsed(&self) -> std::time::Duration {
            self.0.elapsed()
        }
    }
}
#[cfg(target_arch = "wasm32")]
mod word_timer {
    pub struct Timer;
    pub fn start() -> Timer {
        Timer
    }
    impl Timer {
        pub fn elapsed(&self) -> std::time::Duration {
            std::time::Duration::ZERO
        }
    }
}

/// The outcome of [`FomaAnalyzer::analyze_word`] — the `pg_parse::ParseOutcome`-compatible shape
/// plan P2 calls for (`analyses`/`structured`), plus diagnostics the P2 gate's numbers come from:
/// how many distinct candidates were proposed before confirm, how many survived confirm, and
/// whether the reduplication peel contributed any candidate for this particular word.
pub struct FomaOutcome {
    /// `(morpheme-join, surface)` pairs, one per confirmed analysis — parallel to `structured` by
    /// index, exactly like `pg_parse::ParseOutcome::analyses`/`structured`.
    pub analyses: Vec<(String, String)>,
    pub structured: Vec<WordAnalysis>,
    /// Distinct `(morphemes, root_index)` candidates offered to confirm (propose UNION peel,
    /// deduped) — the over-generation half of the P2 gate's headline number.
    pub candidates_generated: usize,
    /// `structured.len()` — kept as its own field (rather than making callers re-derive it) since
    /// it is the OTHER half of the same headline number (candidates_generated vs confirmed).
    pub confirmed: usize,
    /// Whether [`crate::peel::ReduplicationPeeler::peel_candidates`] returned at least one
    /// candidate for this word (regardless of whether it survived the union dedup against
    /// `propose`'s own output) — the redup gate's own diagnostic (plan P2 gate item "redup words
    /// round-trip"). `false` whenever [`Self::peel_chain_depth_error`] is `Some` too (a refused
    /// peel contributes zero candidates for this word).
    pub peel_used: bool,
    /// `Some` iff [`crate::peel::ReduplicationPeeler::peel_candidates`] returned
    /// [`crate::compose_budget::ComposeError::ChainDepthExceeded`] for this word (ADR 0003; see
    /// `crate::peel`'s own module doc) — a genuinely deep nested-reduplication chain exceeded the
    /// configured [`crate::compose_budget::ComposeBudget::chain_depth_cap`]. This word's
    /// `analyses`/`structured`/`candidates_generated` still reflect whatever `propose` (the FST
    /// proposer alone, unaffected) found on its own; the peel's own contribution for this word was
    /// refused rather than silently dropped, and this field is the typed, honest record of that —
    /// never surfaced as a panic or a silent recall gap. `None` (the overwhelming common case,
    /// including every grammar with `chain_depth_cap` unconfigured — production's default) means
    /// the peel completed normally, whether or not it found anything.
    pub peel_chain_depth_error: Option<ComposeError>,
}

/// Opt-in runtime measurements for the exact proposal/peel/confirm pipeline used by
/// [`FomaAnalyzer::analyze_word_with_diagnostics`]. Proposal counters are summed across the direct
/// word proposal and every root proposal requested by reduplication peeling.
#[derive(Clone, Debug, Default)]
pub struct FomaWordDiagnostics {
    pub proposal: ProposalDiagnostics,
    pub proposal_calls: usize,
    pub confirm_batch_calls: usize,
    pub confirmation_groups: usize,
    pub confirmation_calls: usize,
    /// Full-HC oracle step ticks consumed confirming this word -- see
    /// `confirm::ConfirmBatchDiagnostics::confirmation_steps` for why this, and not the call count,
    /// is the unit worth ranking candidates on.
    pub confirmation_steps: usize,
    /// Raw proposer paths `apply_up` yielded for this word (direct proposal plus every proposal a
    /// reduplication peel requested), before tag-decode/dedup -- equal to `proposal.raw_paths`,
    /// duplicated at this level so a caller pricing propose-side work reads it next to
    /// `confirmation_steps` instead of reaching through `proposal`. See
    /// `crate::recipe_optimizer::Score::key`'s doc for why propose-side work needs its own counted
    /// unit alongside confirm-side steps.
    pub raw_paths: usize,
    pub confirmed_analyses: usize,
    pub confirmation_elapsed: Duration,
}

/// A complete ordinary [`FomaOutcome`] paired with opt-in runtime diagnostics.
pub struct ProfiledFomaOutcome {
    pub outcome: FomaOutcome,
    pub diagnostics: FomaWordDiagnostics,
}

/// A profiled outcome that retains the already-owned final candidate vector for an opt-in
/// equivalence observation. The vector is moved here only after confirmation; it is never a clone
/// of raw Foma paths or of the peeler's internal residual queries.
#[doc(hidden)]
pub(crate) struct ProfiledFomaOutcomeWithCandidates {
    pub(crate) outcome: FomaOutcome,
    pub(crate) diagnostics: FomaWordDiagnostics,
    pub(crate) candidates: Vec<Candidate>,
}

/// Bounded diagnostics result. An incomplete proposal is never sent to confirmation.
pub enum ProfiledFomaApplyOutcome {
    Complete(ProfiledFomaOutcome),
    Incomplete {
        dimension: ApplyDimension,
        value: usize,
        limit: usize,
        diagnostics: FomaWordDiagnostics,
    },
}

enum ProfiledFomaApplyOutcomeWithCandidates {
    Complete(ProfiledFomaOutcomeWithCandidates),
    Incomplete {
        dimension: ApplyDimension,
        value: usize,
        limit: usize,
        diagnostics: FomaWordDiagnostics,
    },
}

/// The production counterpart of [`ProfiledFomaApplyOutcome`]: a budgeted analysis that either
/// finished or was stopped by a deterministic magnitude cap, with no profiling attached.
///
/// The distinction between `Incomplete` and a `Complete` outcome carrying an empty analysis list is
/// the one the whole assessment contract rests on. An empty complete result is a positive claim
/// that the grammar analyzes this word no way at all; an incomplete result is the refusal to make
/// that claim. Collapsing the second into the first turns every budget trip into a confident
/// assertion of ungrammaticality.
pub enum FomaApplyOutcome {
    Complete(FomaOutcome),
    Incomplete {
        dimension: ApplyDimension,
        value: usize,
        limit: usize,
    },
}

fn accumulate_proposal_counts(total: &mut ProposalCounts, next: ProposalCounts) {
    total.raw_paths += next.raw_paths;
    total.unique_candidates += next.unique_candidates;
}

fn remaining_apply_budget_from_counts(budget: &ApplyBudget, used: &ProposalCounts) -> ApplyBudget {
    ApplyBudget::with_caps(
        budget
            .path_cap()
            .map(|limit| limit.saturating_sub(used.raw_paths)),
        budget
            .candidate_cap()
            .map(|limit| limit.saturating_sub(used.unique_candidates)),
    )
}

fn accumulate_proposal_diagnostics(total: &mut ProposalDiagnostics, next: ProposalDiagnostics) {
    total.raw_paths += next.raw_paths;
    total.raw_bytes += next.raw_bytes;
    total.decoded_paths += next.decoded_paths;
    total.malformed_paths += next.malformed_paths;
    total.unique_candidates += next.unique_candidates;
    total.traversal_elapsed += next.traversal_elapsed;
    total.decode_dedup_elapsed += next.decode_dedup_elapsed;
}

fn remaining_apply_budget(budget: &ApplyBudget, used: &ProposalDiagnostics) -> ApplyBudget {
    ApplyBudget::with_caps(
        budget
            .path_cap()
            .map(|limit| limit.saturating_sub(used.raw_paths)),
        budget
            .candidate_cap()
            .map(|limit| limit.saturating_sub(used.unique_candidates)),
    )
}

/// One grammar's compiled foma proposer, uncapped verify [`Morpher`], prebuilt morpheme-owner map,
/// and redup peeler, owned together (plan §1: "propose→confirm composite"). `'g` ties this to the
/// same `&Grammar` borrow the verify `Morpher` itself needs.
pub struct FomaAnalyzer<'g> {
    g: &'g Grammar,
    proposer: FomaProposer,
    peeler: ReduplicationPeeler,
    morpher: Morpher<'g>,
    owners: Vec<Option<MorphemeOwner>>,
    /// The [`ComposeBudget`] [`Self::propose_candidates`] threads into every
    /// [`ReduplicationPeeler::peel_candidates`] call (ADR 0003; `crate::peel`'s own module doc).
    /// Built ONCE here from `HC_COMPOSE_*` env vars (mirrors [`ComposeBudget::from_env`]'s own "read
    /// env exactly once, in the production entry point" convention/doc) rather than per word —
    /// [`ComposeBudget`] is `Copy`, so re-reading it per word would be pure waste, not a correctness
    /// concern either way. Production's default (`HC_COMPOSE_CHAIN_DEPTH_BUDGET` unset) leaves
    /// `chain_depth_cap` at `None` (unbounded) — this addition is a zero-behavior-change no-op for
    /// every existing caller of this type unless that env var is explicitly set.
    peel_budget: ComposeBudget,
    #[cfg(all(feature = "test-concurrency-hook", not(target_arch = "wasm32")))]
    confirmation_concurrency_probe: Option<test_confirmation_concurrency::Probe>,
}

impl<'g> FomaAnalyzer<'g> {
    /// Emit + foma-compile `g` (via [`FomaProposer::new`]), build the redup peeler, an UNCAPPED
    /// verify `Morpher` (`Morpher::new(g, usize::MAX)` — see [`crate::confirm::confirm_all`]'s doc
    /// for why a cap here would be a silent parity bug, not a performance knob), and the
    /// morpheme-owner reverse map confirm needs. `Err` iff the grammar's emitted lexc source itself
    /// fails to foma-compile. Per the revised plan §0 there is no per-grammar fallback tier: this
    /// composite IS the mainline for every grammar, so a compile failure here is an emitter gap to
    /// fix (later plan stages), not a routing decision — the `Err` just surfaces it to the caller.
    // See the `#[allow(clippy::result_large_err)]` justification on `FomaProposer::new`
    // (crate::analyzer): FomaError is a deliberately small, flat enum and boxing its largest
    // variant would change the public enum's shape for every downstream `match`.
    #[allow(clippy::result_large_err)]
    pub fn new(g: &'g Grammar) -> Result<Self, FomaError> {
        let proposer = FomaProposer::new(g)?;
        Ok(FomaAnalyzer {
            g,
            proposer,
            peeler: ReduplicationPeeler::new(g),
            morpher: Morpher::new(g, usize::MAX),
            owners: confirm::build_morpheme_owners(g),
            peel_budget: ComposeBudget::from_env(),
            #[cfg(all(feature = "test-concurrency-hook", not(target_arch = "wasm32")))]
            confirmation_concurrency_probe: None,
        })
    }

    /// Build the ordinary propose→peel→confirm analyzer around an already-compiled proposer.
    /// Every non-proposer field is initialized identically to [`Self::new`].
    pub fn from_precompiled_proposer(g: &'g Grammar, proposer: FomaProposer) -> Self {
        Self::from_cached(
            g,
            proposer,
            ReduplicationPeeler::new(g),
            confirm::build_morpheme_owners(g),
        )
    }

    #[cfg(all(feature = "test-concurrency-hook", not(target_arch = "wasm32")))]
    #[doc(hidden)]
    pub fn arm_confirmation_concurrency_probe(&mut self) -> test_confirmation_concurrency::Probe {
        let probe = test_confirmation_concurrency::Probe::new();
        self.confirmation_concurrency_probe = Some(probe.clone());
        probe
    }

    /// `propose(word)` UNION `peel_candidates(word, propose)` (deduped by `(morphemes, root_index)`,
    /// first-seen order) → `confirm_all` on every surviving candidate → concatenate every match.
    /// Empty (never panics) for a word neither the proposer nor the peel can reach at all, and for
    /// a word the engine itself would only reach via `guess_root` (this crate never sets it —
    /// `confirm_all` always calls `parse_word_selected` with `ParseOptions::default()` — so the
    /// result here is consistent with `Morpher::parse_word_opts(word, &ParseOptions::default())`
    /// under the SAME options, matching P2's own gate requirement).
    pub fn analyze_word(&mut self, word: &str) -> FomaOutcome {
        let (candidates, peel_used, peel_chain_depth_error) = self.propose_candidates(word);
        let candidates_generated = candidates.len();
        // `HC_DEBUG_CANDIDATES=1` (diagnostic-only, off by default, same env-gated-diagnostic
        // precedent as `HC_PREEXPAND_FLAT`/`HC_PREEXPAND_PROBE_CAP`): prints the proposed-candidate
        // count right after `propose_candidates` returns, before `confirm_batch` runs -- the fast
        // way to tell whether a runaway is in propose/peel vs. confirm without a debugger attached
        // (`docs/fst-plan/morphotactic-composite-pruning.md`'s Aweti investigation used this to
        // localize an allocation-failure crash to inside `propose_candidates`).
        if std::env::var("HC_DEBUG_CANDIDATES").is_ok() {
            eprintln!(
                "[HC_DEBUG_CANDIDATES] word={word:?} candidates_generated={candidates_generated}"
            );
        }
        let mut analyses = Vec::new();
        let mut structured = Vec::new();
        // Batched confirm (John, 2026-07-15): ONE union re-parse routes every outcome analysis to
        // its candidate's bucket — content-identical to per-candidate confirm_all (soundness
        // argument in `confirm::confirm_batch`'s doc) at 1/N the re-parse cost. Buckets come back
        // in candidate order, each in outcome order, preserving the previous concatenation order.
        for bucket in confirm::confirm_batch(self.g, &self.owners, &self.morpher, &candidates, word)
        {
            for (wa, join, surface) in bucket {
                structured.push(wa);
                analyses.push((join, surface));
            }
        }

        FomaOutcome {
            confirmed: structured.len(),
            analyses,
            structured,
            candidates_generated,
            peel_used,
            peel_chain_depth_error,
        }
    }

    /// [`Self::analyze_word`] under a deterministic magnitude budget, with no profiling.
    ///
    /// This is the production budgeted path. Until it existed, the only way to obtain a typed
    /// incomplete from the foma pipeline was
    /// [`Self::analyze_word_with_diagnostics_budgeted`], which clocks every decoded path — so
    /// `pg-cli`'s `diagnose` compiled a *second* standalone proposer just to measure against a
    /// budget, and the production pipeline itself remained unbounded and therefore unable to report
    /// `incomplete` at all. Both of those follow from the missing entry point, not from anything
    /// intrinsic.
    ///
    /// One `budget` is shared cumulatively by the direct proposal and every proposal reduplication
    /// peeling requests, matching the diagnostic path's semantics exactly. A trip returns before
    /// confirmation runs: partial candidates are never confirmed, because a partial confirm would
    /// produce an analysis set that looks authoritative and is not.
    ///
    /// [`ApplyBudget::unbounded`] can never trip, so [`Self::analyze_word`] delegating here is a
    /// behavior-preserving no-op for every existing caller.
    pub fn analyze_word_budgeted(&mut self, word: &str, budget: &ApplyBudget) -> FomaApplyOutcome {
        let (candidates, peel_used, peel_chain_depth_error) =
            match self.propose_candidates_budgeted(word, budget) {
                Ok(complete) => complete,
                Err((dimension, value, limit)) => {
                    return FomaApplyOutcome::Incomplete {
                        dimension,
                        value,
                        limit,
                    }
                }
            };

        let candidates_generated = candidates.len();
        let mut analyses = Vec::new();
        let mut structured = Vec::new();
        for bucket in confirm::confirm_batch(self.g, &self.owners, &self.morpher, &candidates, word)
        {
            for (wa, join, surface) in bucket {
                structured.push(wa);
                analyses.push((join, surface));
            }
        }

        FomaApplyOutcome::Complete(FomaOutcome {
            confirmed: structured.len(),
            analyses,
            structured,
            candidates_generated,
            peel_used,
            peel_chain_depth_error,
        })
    }

    /// [`Self::propose_candidates`] under one cumulative budget, using counters rather than the
    /// clocked diagnostic proposal.
    #[allow(clippy::type_complexity)]
    fn propose_candidates_budgeted(
        &mut self,
        word: &str,
        budget: &ApplyBudget,
    ) -> std::result::Result<
        (Vec<Candidate>, bool, Option<ComposeError>),
        (ApplyDimension, usize, usize),
    > {
        let mut used = ProposalCounts::default();
        let trip = |dimension: ApplyDimension, used: &ProposalCounts| match dimension {
            ApplyDimension::DecodedPaths => (
                dimension,
                used.raw_paths,
                budget.path_cap().expect("path trip requires a path cap"),
            ),
            ApplyDimension::Candidates => (
                dimension,
                used.unique_candidates,
                budget
                    .candidate_cap()
                    .expect("candidate trip requires a candidate cap"),
            ),
        };

        let direct_budget = remaining_apply_budget_from_counts(budget, &used);
        let (direct, direct_counts) = self.proposer.propose_budgeted_counted(word, &direct_budget);
        accumulate_proposal_counts(&mut used, direct_counts);
        let mut candidates = match direct {
            ApplyOutcome::Complete(candidates) => candidates,
            ApplyOutcome::Incomplete { dimension, .. } => return Err(trip(dimension, &used)),
        };

        let peel_budget = self.peel_budget;
        let mut incomplete_dimension = None;
        let peel_result = {
            let proposer = &mut self.proposer;
            let mut propose_fn = |root: &str| {
                if incomplete_dimension.is_some() {
                    return Vec::new();
                }
                let call_budget = remaining_apply_budget_from_counts(budget, &used);
                let (outcome, next) = proposer.propose_budgeted_counted(root, &call_budget);
                accumulate_proposal_counts(&mut used, next);
                match outcome {
                    ApplyOutcome::Complete(candidates) => candidates,
                    ApplyOutcome::Incomplete { dimension, .. } => {
                        incomplete_dimension = Some(dimension);
                        Vec::new()
                    }
                }
            };
            self.peeler
                .peel_candidates(self.g, word, &peel_budget, &mut propose_fn)
        };

        if let Some(dimension) = incomplete_dimension {
            return Err(trip(dimension, &used));
        }

        let (peeled, peel_chain_depth_error) = match peel_result {
            Ok(peeled) => (peeled, None),
            Err(error) => (Vec::new(), Some(error)),
        };
        let peel_used = !peeled.is_empty();
        for candidate in peeled {
            let already_present = candidates.iter().any(|existing| {
                existing.root_index == candidate.root_index
                    && existing.morphemes == candidate.morphemes
            });
            if !already_present {
                candidates.push(candidate);
            }
        }

        Ok((candidates, peel_used, peel_chain_depth_error))
    }

    /// Opt-in diagnostic sibling of [`Self::analyze_word`].
    pub fn analyze_word_with_diagnostics(&mut self, word: &str) -> ProfiledFomaOutcome {
        match self.analyze_word_with_diagnostics_budgeted(word, &ApplyBudget::unbounded()) {
            ProfiledFomaApplyOutcome::Complete(profiled) => profiled,
            ProfiledFomaApplyOutcome::Incomplete { .. } => {
                unreachable!("ApplyBudget::unbounded() can never report Incomplete")
            }
        }
    }

    /// Bounded diagnostic pipeline. One shared [`ApplyBudget`] is consumed by the direct proposal
    /// and every proposal requested by reduplication peeling. A trip returns diagnostics for the
    /// measured prefix and never confirms partial candidates.
    pub fn analyze_word_with_diagnostics_budgeted(
        &mut self,
        word: &str,
        budget: &ApplyBudget,
    ) -> ProfiledFomaApplyOutcome {
        match self.analyze_word_with_diagnostics_budgeted_with_candidates(word, budget) {
            ProfiledFomaApplyOutcomeWithCandidates::Complete(profiled) => {
                ProfiledFomaApplyOutcome::Complete(ProfiledFomaOutcome {
                    outcome: profiled.outcome,
                    diagnostics: profiled.diagnostics,
                })
            }
            ProfiledFomaApplyOutcomeWithCandidates::Incomplete {
                dimension,
                value,
                limit,
                diagnostics,
            } => ProfiledFomaApplyOutcome::Incomplete {
                dimension,
                value,
                limit,
                diagnostics,
            },
        }
    }

    /// Diagnostic pipeline for a cache-aware equivalence observation. The final deduplicated
    /// candidate vector is moved out only after confirmation, so the measured apply has no clone
    /// or analyzer-level capture callback.
    #[doc(hidden)]
    pub(crate) fn analyze_word_with_diagnostics_and_candidates(
        &mut self,
        word: &str,
    ) -> ProfiledFomaOutcomeWithCandidates {
        match self
            .analyze_word_with_diagnostics_budgeted_with_candidates(word, &ApplyBudget::unbounded())
        {
            ProfiledFomaApplyOutcomeWithCandidates::Complete(profiled) => profiled,
            ProfiledFomaApplyOutcomeWithCandidates::Incomplete { .. } => {
                unreachable!("ApplyBudget::unbounded() can never report Incomplete")
            }
        }
    }

    fn analyze_word_with_diagnostics_budgeted_with_candidates(
        &mut self,
        word: &str,
        budget: &ApplyBudget,
    ) -> ProfiledFomaApplyOutcomeWithCandidates {
        let proposed = self.propose_candidates_with_diagnostics_budgeted(word, budget);
        let (candidates, peel_used, peel_chain_depth_error, proposal, proposal_calls) =
            match proposed {
                Ok(complete) => complete,
                Err((dimension, value, limit, proposal, proposal_calls)) => {
                    return ProfiledFomaApplyOutcomeWithCandidates::Incomplete {
                        dimension,
                        value,
                        limit,
                        diagnostics: FomaWordDiagnostics {
                            raw_paths: proposal.raw_paths,
                            proposal,
                            proposal_calls,
                            ..FomaWordDiagnostics::default()
                        },
                    };
                }
            };

        let confirmation_timer = word_timer::start();
        let (buckets, confirmation) = confirm::confirm_batch_with_diagnostics(
            self.g,
            &self.owners,
            &self.morpher,
            &candidates,
            word,
        );
        let confirmation_elapsed = confirmation_timer.elapsed();
        let mut analyses = Vec::new();
        let mut structured = Vec::new();
        for bucket in buckets {
            for (analysis, join, surface) in bucket {
                structured.push(analysis);
                analyses.push((join, surface));
            }
        }
        let outcome = FomaOutcome {
            confirmed: structured.len(),
            analyses,
            structured,
            candidates_generated: candidates.len(),
            peel_used,
            peel_chain_depth_error,
        };
        let diagnostics = FomaWordDiagnostics {
            raw_paths: proposal.raw_paths,
            proposal,
            proposal_calls,
            confirm_batch_calls: 1,
            confirmation_groups: confirmation.confirmation_groups,
            confirmation_calls: confirmation.confirmation_calls,
            confirmation_steps: confirmation.confirmation_steps,
            confirmed_analyses: outcome.confirmed,
            confirmation_elapsed,
        };
        ProfiledFomaApplyOutcomeWithCandidates::Complete(ProfiledFomaOutcomeWithCandidates {
            outcome,
            diagnostics,
            candidates,
        })
    }

    fn propose_candidates_with_diagnostics_budgeted(
        &mut self,
        word: &str,
        budget: &ApplyBudget,
    ) -> std::result::Result<ProposeCandidatesOk, ProposeCandidatesErr> {
        let mut proposal = ProposalDiagnostics::default();
        let mut proposal_calls = 1;
        let direct_budget = remaining_apply_budget(budget, &proposal);
        let (direct, direct_diagnostics) = self
            .proposer
            .propose_with_diagnostics_budgeted(word, &direct_budget);
        accumulate_proposal_diagnostics(&mut proposal, direct_diagnostics);
        let mut candidates = match direct {
            ApplyOutcome::Complete(candidates) => candidates,
            ApplyOutcome::Incomplete { dimension, .. } => {
                let (value, limit) = match dimension {
                    ApplyDimension::DecodedPaths => (
                        proposal.raw_paths,
                        budget.path_cap().expect("path trip requires a path cap"),
                    ),
                    ApplyDimension::Candidates => (
                        proposal.unique_candidates,
                        budget
                            .candidate_cap()
                            .expect("candidate trip requires a candidate cap"),
                    ),
                };
                return Err((dimension, value, limit, proposal, proposal_calls));
            }
        };

        let peel_budget = self.peel_budget;
        let mut incomplete_dimension = None;
        let peel_result = {
            let proposer = &mut self.proposer;
            let mut propose_fn = |root: &str| {
                if incomplete_dimension.is_some() {
                    return Vec::new();
                }
                let call_budget = remaining_apply_budget(budget, &proposal);
                let (outcome, next) =
                    proposer.propose_with_diagnostics_budgeted(root, &call_budget);
                proposal_calls += 1;
                accumulate_proposal_diagnostics(&mut proposal, next);
                match outcome {
                    ApplyOutcome::Complete(candidates) => candidates,
                    ApplyOutcome::Incomplete { dimension, .. } => {
                        incomplete_dimension = Some(dimension);
                        Vec::new()
                    }
                }
            };
            self.peeler
                .peel_candidates(self.g, word, &peel_budget, &mut propose_fn)
        };

        if let Some(dimension) = incomplete_dimension {
            let (value, limit) = match dimension {
                ApplyDimension::DecodedPaths => (
                    proposal.raw_paths,
                    budget.path_cap().expect("path trip requires a path cap"),
                ),
                ApplyDimension::Candidates => (
                    proposal.unique_candidates,
                    budget
                        .candidate_cap()
                        .expect("candidate trip requires a candidate cap"),
                ),
            };
            return Err((dimension, value, limit, proposal, proposal_calls));
        }

        let (peeled, peel_chain_depth_error) = match peel_result {
            Ok(peeled) => (peeled, None),
            Err(error) => (Vec::new(), Some(error)),
        };
        let peel_used = !peeled.is_empty();
        for candidate in peeled {
            let already_present = candidates.iter().any(|existing| {
                existing.root_index == candidate.root_index
                    && existing.morphemes == candidate.morphemes
            });
            if !already_present {
                candidates.push(candidate);
            }
        }

        Ok((
            candidates,
            peel_used,
            peel_chain_depth_error,
            proposal,
            proposal_calls,
        ))
    }

    /// `propose(word)` UNION `peel_candidates(word, propose)`, deduped by `(morphemes,
    /// root_index)` — the pre-confirm half of [`Self::analyze_word`]/[`Self::analyze_words`],
    /// factored out so the batch path can run this stage sequentially over every word (see
    /// [`Self::analyze_words`]'s doc for why it stays sequential) before handing the results to
    /// confirm. Returns the deduped candidate list, whether the redup peel contributed anything
    /// for this word, and (ADR 0003) `Some` iff the peel hit its configured chain-depth budget for
    /// this word (`self.peel_budget`; [`FomaOutcome::peel_chain_depth_error`]'s own doc) — a refused
    /// peel contributes zero candidates of its own for this word, but never touches `propose`'s
    /// own (unaffected) candidates.
    fn propose_candidates(&mut self, word: &str) -> (Vec<Candidate>, bool, Option<ComposeError>) {
        let mut candidates: Vec<Candidate> = self.proposer.propose(word);

        // Disjoint field borrows: `proposer` borrows only `self.proposer` (mutably); the
        // `peel_candidates` call below borrows only `self.peeler` (immutably) and copies `self.g`
        // (a `&Grammar`) — no conflict, since neither touches the other's field. `self.peel_budget`
        // is `Copy`, copied out by value for the same reason.
        let peel_budget = self.peel_budget;
        let (peeled, peel_chain_depth_error): (Vec<Candidate>, Option<ComposeError>) = {
            let proposer = &mut self.proposer;
            let mut propose_fn = |r: &str| proposer.propose(r);
            match self
                .peeler
                .peel_candidates(self.g, word, &peel_budget, &mut propose_fn)
            {
                Ok(peeled) => (peeled, None),
                Err(e) => (Vec::new(), Some(e)),
            }
        };
        let peel_used = !peeled.is_empty();

        for c in peeled {
            let already_present = candidates.iter().any(|existing| {
                existing.root_index == c.root_index && existing.morphemes == c.morphemes
            });
            if !already_present {
                candidates.push(c);
            }
        }

        // Plan §2/D4: distinct candidates yield disjoint matched sequences (confirm's
        // `analyses_match` is keyed on exactly a candidate's own `(morphemes, root_index)`), so no
        // cross-candidate double-count is possible once this list itself has no duplicate key —
        // asserted here (debug-only: a real invariant of the dedup above, not a runtime check meant
        // to fire in release).
        debug_assert!(
            {
                let mut seen: Vec<(Vec<u32>, i32)> = Vec::with_capacity(candidates.len());
                candidates.iter().all(|c| {
                    let key = (c.morphemes.iter().map(|m| m.0).collect::<Vec<_>>(), c.root_index);
                    if seen.contains(&key) {
                        false
                    } else {
                        seen.push(key);
                        true
                    }
                })
            },
            "propose UNION peel produced a duplicate (morphemes, root_index) candidate for {word:?}"
        );

        (candidates, peel_used, peel_chain_depth_error)
    }

    /// Batch entry point (perf pass, 2026-07-16): analyze every word in `words`, running PROPOSE
    /// sequentially (unchanged) but CONFIRM in parallel across words.
    ///
    /// **Why propose stays sequential:** [`FomaProposer::propose`] takes `&mut self` because it
    /// drives the single foma `ApplyHandle` this analyzer owns — that handle is deliberately
    /// built ONCE per grammar and reused (`analyzer.rs`'s own doc: `apply_init` deep-clones the
    /// whole compiled network, so rebuilding or cloning one per worker thread would cost far more
    /// than the propose stage itself saves). There is exactly one handle, so propose for N words
    /// cannot run on more than one thread without either a lock (serializing it anyway) or N
    /// redundant network clones — neither is a real win, so this stage stays a plain sequential
    /// loop over `words`, identical in content and cost to N calls to [`Self::analyze_word`]'s own
    /// propose half.
    ///
    /// **Why confirm parallelizes across words:** a tracer measured `pg_foma::confirm::confirm_batch`
    /// as the dominant cost of `analyze_word` on real corpora, and it is safe to run concurrently,
    /// one word per task: [`Morpher`] is `Sync` (its only interior-mutable state, an
    /// `AnalysisScope` behind a `RefCell`, is created fresh inside `parse_word_core_selected` per
    /// call — never a shared field), `RuleCache` and `pg_fst::Fst` hold no interior mutability at
    /// all, and `pg-parse`/`pg-rules`/`pg-fst` are all `#![forbid(unsafe_code)]` — so two words'
    /// confirm calls touch no shared mutable state. This runs on a DEDICATED rayon pool (not the
    /// global default pool) with large worker stacks
    /// ([`crate::emit::PROBE_STACK_BYTES`] — same size [`crate::preexpand::build_composites`] and
    /// [`crate::junctions::PhonologyProbe`] already use, and for the same reason: the analysis
    /// cascade `confirm_batch`'s pinned `parse_word_selected` recurses through can overflow
    /// rayon's default 2-8MB stacks on heavy words). `wasm32-unknown-unknown` cannot spawn OS
    /// threads (a rayon pool ctor would panic there), so that target keeps the plain sequential
    /// loop instead — matching this crate's existing convention
    /// (`junctions.rs`/`preexpand.rs`'s own `#[cfg(target_arch = "wasm32")]` fallback arms).
    ///
    /// Returns one `(`[`FomaOutcome`]`, elapsed)` pair per input word, in the SAME order as
    /// `words` — independent of dispatch/completion order or thread count
    /// (`par_iter().map(..).collect::<Vec<_>>()` over an `IndexedParallelIterator`, like
    /// [`crate::preexpand::build_composites`]'s own doc notes, preserves original input order),
    /// each `FomaOutcome` content-identical to calling [`Self::analyze_word`] on that word alone.
    /// `elapsed` is that word's own propose (stage 1) plus confirm (stage 2) wall time — timed
    /// separately per word in each stage (stage 2's timer runs inside that word's own parallel
    /// task, so it reflects real per-word cost, not a share of the pool's total wall time) and
    /// summed, mirroring `pg_parse::batch::BatchWordOutcome::elapsed`'s own per-word-not-per-batch
    /// convention.
    pub fn analyze_words(&mut self, words: &[String]) -> Vec<(FomaOutcome, Duration)> {
        self.analyze_words_with_threads(words, 0)
    }

    pub fn analyze_words_with_threads(
        &mut self,
        words: &[String],
        max_threads: usize,
    ) -> Vec<(FomaOutcome, Duration)> {
        let proposed = self.propose_words(words);
        #[cfg(all(feature = "test-concurrency-hook", not(target_arch = "wasm32")))]
        {
            confirm_proposed_words_with_probe(
                self.g,
                &self.owners,
                words,
                proposed,
                max_threads,
                self.confirmation_concurrency_probe.as_ref(),
            )
        }
        #[cfg(not(all(feature = "test-concurrency-hook", not(target_arch = "wasm32"))))]
        confirm_proposed_words(self.g, &self.owners, words, proposed, max_threads)
    }

    /// Run the mutable foma proposal phase without retaining the confirming analyzer.
    pub fn propose_words(&mut self, words: &[String]) -> Vec<ProposedWord> {
        words
            .iter()
            .map(|word| {
                let t0 = word_timer::start();
                let (candidates, peel_used, peel_chain_depth_error) = self.propose_candidates(word);
                ProposedWord {
                    candidates,
                    peel_used,
                    peel_chain_depth_error,
                    propose_elapsed: t0.elapsed(),
                }
            })
            .collect()
    }

    pub fn grammar(&self) -> &'g Grammar {
        self.g
    }

    /// Arm (or leave unarmed) the internal confirming [`Morpher`]'s `--word-timeout-ms` deadline
    /// (`pg_parse::Morpher::with_word_timeout`). `None` (also [`Self::new`]/[`Self::from_cached`]'s
    /// implicit default) is a complete no-op — behavior stays byte-identical to before this
    /// existed. NOTE: this only threads through the `Morpher` [`Self::new`]/[`Self::from_cached`]
    /// just built for THIS instance — [`Self::into_parts`]/[`Self::from_cached`]'s cached-pieces
    /// round trip does not persist the timeout (the `Morpher<'g>` it rebuilds is never one of the
    /// cached pieces, per that method's own doc), so a caller using that path must call this again
    /// after every `from_cached`. The [`Self::into_parts_with_morpher`]/
    /// [`Self::from_cached_with_morpher`] round trip DOES persist it, precisely because it carries
    /// the same `Morpher<'g>` across instead of rebuilding one.
    pub fn with_word_timeout(self, timeout: Option<Duration>) -> Self {
        FomaAnalyzer {
            morpher: self.morpher.with_word_timeout(timeout),
            ..self
        }
    }

    /// Rehydrate a `FomaAnalyzer` from previously-built OWNED pieces (a compiled [`FomaProposer`]
    /// — the expensive `emit`+foma-compile step — a [`ReduplicationPeeler`], and an owners map)
    /// plus a fresh borrow of `g` for this call. Plan P4 (`docs/fst-plan/foma-fst-plan.md`
    /// "`PanGlossGrammar::new` builds `FomaAnalyzer`"): a long-lived host (`pg-wasm`'s
    /// `PanGlossGrammar`) that also OWNS the `Grammar` these borrow from can't store a
    /// `FomaAnalyzer<'g>` as a sibling field of that same `Grammar` (that would be a
    /// self-referential struct) — but it CAN store the three owned pieces here and reconstruct a
    /// short-lived `FomaAnalyzer` from them plus `&self.grammar` for the duration of one call,
    /// exactly the way `Morpher<'g>` itself is never stored, only ever built fresh per call from
    /// an owned `&Grammar`. Pair with [`Self::into_parts`] to hand the (unchanged) owned pieces
    /// back to long-term storage once the call is done.
    pub fn from_cached(
        g: &'g Grammar,
        proposer: FomaProposer,
        peeler: ReduplicationPeeler,
        owners: Vec<Option<MorphemeOwner>>,
    ) -> Self {
        Self::from_cached_with_morpher(g, proposer, peeler, owners, Morpher::new(g, usize::MAX))
    }

    /// [`Self::from_cached`] with the confirming [`Morpher`] supplied by the caller instead of built
    /// here, so a caller that constructs MANY analyzers over one `&Grammar` pays for it once.
    ///
    /// **`Morpher::new` is not cheap, and this exists because a doc comment on
    /// [`Self::into_parts`] used to claim it was.** It builds `RootAllomorphIndex::build(g)`,
    /// `collect_lexical_patterns(g)` and — the expensive one — `RuleCache::build(g)`, which compiles
    /// EVERY phonological/morphological matcher FST in the grammar (`pg_rules::cache`'s own module
    /// doc: "build once, at `Morpher` construction"). Rebuilding it per analyzer pays a
    /// grammar-wide FST compilation for an object that is identical across all of them.
    ///
    /// Sharing one instance is sound because the `Morpher` is **immutable in use**: every confirm
    /// path here reaches it as `&self.morpher` through `crate::confirm::confirm_batch`, `RuleCache`
    /// is documented read-only after `build` (and is already shared across every
    /// `pangloss batch --threads=N` worker as a single `&RuleCache`), and it carries no per-word
    /// state — a `parse_word` call's mutable state lives in its own `StepBudget`/`AnalyzerConfig`,
    /// not on the `Morpher`. The only `Morpher` fields a caller can vary (`cap`, `memo`,
    /// `word_timeout`, `max_stem_count`) are construction-time knobs, so a supplied `Morpher` also
    /// lets a caller set them once for a whole batch of analyzers. Pair with
    /// [`Self::into_parts_with_morpher`] to hand it back for the next one.
    pub fn from_cached_with_morpher(
        g: &'g Grammar,
        proposer: FomaProposer,
        peeler: ReduplicationPeeler,
        owners: Vec<Option<MorphemeOwner>>,
        morpher: Morpher<'g>,
    ) -> Self {
        FomaAnalyzer {
            g,
            proposer,
            peeler,
            morpher,
            owners,
            peel_budget: ComposeBudget::from_env(),
            #[cfg(all(feature = "test-concurrency-hook", not(target_arch = "wasm32")))]
            confirmation_concurrency_probe: None,
        }
    }

    /// The inverse of [`Self::from_cached`]: reclaim the three owned pieces this analyzer was
    /// built from (or built fresh in [`Self::new`]), DROPPING its `Morpher<'g>`.
    ///
    /// Dropping the morpher is a real cost, not a free discard — the next [`Self::from_cached`]
    /// rebuilds it with `Morpher::new`, which compiles every matcher FST in the grammar
    /// ([`Self::from_cached_with_morpher`]'s doc has the accounting). An earlier version of this
    /// comment claimed the morpher was "recreated for free on the next `from_cached` call", which
    /// is false and is exactly why nobody looked: a caller that round-trips one analyzer per
    /// candidate over a fixed grammar was paying a grammar-wide FST compilation per candidate. Use
    /// [`Self::into_parts_with_morpher`] to keep it instead.
    pub fn into_parts(
        self,
    ) -> (
        FomaProposer,
        ReduplicationPeeler,
        Vec<Option<MorphemeOwner>>,
    ) {
        let (proposer, peeler, owners, _morpher) = self.into_parts_with_morpher();
        (proposer, peeler, owners)
    }

    /// [`Self::into_parts`] that also hands back the confirming [`Morpher`] instead of dropping it —
    /// the reclaim half of [`Self::from_cached_with_morpher`]. Nothing in this type mutates the
    /// morpher, so what comes back is the same object that went in.
    pub fn into_parts_with_morpher(
        self,
    ) -> (
        FomaProposer,
        ReduplicationPeeler,
        Vec<Option<MorphemeOwner>>,
        Morpher<'g>,
    ) {
        (self.proposer, self.peeler, self.owners, self.morpher)
    }
}

/// Confirm detached proposals using at most `max_threads` workers (`0` = available cores).
pub fn confirm_proposed_words(
    g: &Grammar,
    owners: &[Option<MorphemeOwner>],
    words: &[String],
    proposed: Vec<ProposedWord>,
    max_threads: usize,
) -> Vec<(FomaOutcome, Duration)> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let mut builder =
            rayon::ThreadPoolBuilder::new().stack_size(crate::emit::PROBE_STACK_BYTES);
        if max_threads > 0 {
            builder = builder.num_threads(max_threads);
        }
        let pool = builder
            .build()
            .expect("build detached foma confirmation rayon pool");
        confirm_proposed_words_in_pool(g, owners, words, proposed, &pool)
    }

    #[cfg(target_arch = "wasm32")]
    {
        let _ = max_threads;
        if words.is_empty() {
            return Vec::new();
        }
        assert_eq!(words.len(), proposed.len(), "one proposal record per word");
        let morpher = Morpher::new(g, usize::MAX);
        let buckets_per_word: Vec<TimedConfirmedBuckets> = words
            .iter()
            .zip(proposed.iter())
            .map(|(word, proposal)| {
                let t0 = word_timer::start();
                let buckets =
                    confirm::confirm_batch(g, owners, &morpher, &proposal.candidates, word);
                (buckets, t0.elapsed())
            })
            .collect();
        finish_confirmed(proposed, buckets_per_word)
    }
}

/// Confirm detached proposals inside a caller-owned pool, allowing a host batch to reuse that
/// same requested-size pool for its later overlay-union phase.
#[cfg(not(target_arch = "wasm32"))]
pub fn confirm_proposed_words_in_pool(
    g: &Grammar,
    owners: &[Option<MorphemeOwner>],
    words: &[String],
    proposed: Vec<ProposedWord>,
    pool: &rayon::ThreadPool,
) -> Vec<(FomaOutcome, Duration)> {
    if words.is_empty() {
        return Vec::new();
    }
    assert_eq!(words.len(), proposed.len(), "one proposal record per word");
    let morpher = Morpher::new(g, usize::MAX);
    let buckets_per_word: Vec<TimedConfirmedBuckets> = pool.install(|| {
        words
            .par_iter()
            .zip(proposed.par_iter())
            .map(|(word, proposal)| {
                let t0 = word_timer::start();
                let buckets =
                    confirm::confirm_batch(g, owners, &morpher, &proposal.candidates, word);
                (buckets, t0.elapsed())
            })
            .collect()
    });
    finish_confirmed(proposed, buckets_per_word)
}

#[cfg(all(feature = "test-concurrency-hook", not(target_arch = "wasm32")))]
fn confirm_proposed_words_with_probe(
    g: &Grammar,
    owners: &[Option<MorphemeOwner>],
    words: &[String],
    proposed: Vec<ProposedWord>,
    max_threads: usize,
    probe: Option<&test_confirmation_concurrency::Probe>,
) -> Vec<(FomaOutcome, Duration)> {
    let mut builder = rayon::ThreadPoolBuilder::new().stack_size(crate::emit::PROBE_STACK_BYTES);
    if max_threads > 0 {
        builder = builder.num_threads(max_threads);
    }
    let pool = builder
        .build()
        .expect("build detached foma confirmation rayon pool");
    if let Some(probe) = probe {
        probe.prepare(words.len().min(pool.current_num_threads()));
    }
    confirm_proposed_words_in_pool_with_probe(g, owners, words, proposed, &pool, probe)
}

#[cfg(all(feature = "test-concurrency-hook", not(target_arch = "wasm32")))]
/// Test-only variant of [`confirm_proposed_words_in_pool`] that reports confirmation concurrency
/// through an analyzer-owned probe.
#[doc(hidden)]
pub fn confirm_proposed_words_in_pool_with_probe(
    g: &Grammar,
    owners: &[Option<MorphemeOwner>],
    words: &[String],
    proposed: Vec<ProposedWord>,
    pool: &rayon::ThreadPool,
    probe: Option<&test_confirmation_concurrency::Probe>,
) -> Vec<(FomaOutcome, Duration)> {
    if let Some(probe) = probe {
        probe.prepare(words.len().min(pool.current_num_threads()));
    }
    if words.is_empty() {
        return Vec::new();
    }
    assert_eq!(words.len(), proposed.len(), "one proposal record per word");
    let morpher = Morpher::new(g, usize::MAX);
    let buckets_per_word: Vec<TimedConfirmedBuckets> = pool.install(|| {
        words
            .par_iter()
            .zip(proposed.par_iter())
            .map(|(word, proposal)| {
                let _concurrency = probe.map(test_confirmation_concurrency::Guard::enter);
                let t0 = word_timer::start();
                let buckets =
                    confirm::confirm_batch(g, owners, &morpher, &proposal.candidates, word);
                (buckets, t0.elapsed())
            })
            .collect()
    });
    finish_confirmed(proposed, buckets_per_word)
}

fn finish_confirmed(
    proposed: Vec<ProposedWord>,
    buckets_per_word: Vec<TimedConfirmedBuckets>,
) -> Vec<(FomaOutcome, Duration)> {
    proposed
        .into_iter()
        .zip(buckets_per_word)
        .map(|(proposal, (buckets, confirm_elapsed))| {
            let candidates_generated = proposal.candidates.len();
            let mut analyses = Vec::new();
            let mut structured = Vec::new();
            for bucket in buckets {
                for (wa, join, surface) in bucket {
                    structured.push(wa);
                    analyses.push((join, surface));
                }
            }
            let outcome = FomaOutcome {
                confirmed: structured.len(),
                analyses,
                structured,
                candidates_generated,
                peel_used: proposal.peel_used,
                peel_chain_depth_error: proposal.peel_chain_depth_error,
            };
            (outcome, proposal.propose_elapsed + confirm_elapsed)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pg_parse::ParseOptions;

    fn sample_path(name: &str) -> Option<std::path::PathBuf> {
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let path = manifest_dir.join("../../../samples/data").join(name);
        path.exists().then_some(path)
    }

    fn load_sena() -> Option<Grammar> {
        let path = sample_path("sena-hc.xml")?;
        let xml = std::fs::read_to_string(&path).expect("read grammar");
        Some(pg_grammar::load(&xml).unwrap_or_else(|e| panic!("failed to load grammar: {e}")))
    }

    /// A word with no proposed candidates at all returns an empty, non-panicking outcome.
    #[test]
    #[ignore = "needs local gitignored corpus data (samples/data/sena-hc.xml); run with --include-ignored"]
    fn unknown_word_returns_empty_outcome() {
        let Some(g) = load_sena() else {
            eprintln!("skipping: sena-hc.xml not present on disk");
            return;
        };
        let mut analyzer = FomaAnalyzer::new(&g).expect("sena compiles");
        let outcome = analyzer.analyze_word("zzzqxxxnonsense");
        assert!(outcome.structured.is_empty());
        assert!(outcome.analyses.is_empty());
        assert_eq!(outcome.confirmed, 0);
        assert!(!outcome.peel_used);
    }

    /// Sanity: `mbali` confirms to a non-empty outcome whose size does not exceed
    /// `candidates_generated` (confirm only prunes, never invents).
    #[test]
    #[ignore = "needs local gitignored corpus data (samples/data/sena-hc.xml); run with --include-ignored"]
    fn mbali_confirms_within_candidate_bound() {
        let Some(g) = load_sena() else {
            eprintln!("skipping: sena-hc.xml not present on disk");
            return;
        };
        let mut analyzer = FomaAnalyzer::new(&g).expect("sena compiles");
        let outcome = analyzer.analyze_word("mbali");
        assert!(!outcome.structured.is_empty());
        assert!(outcome.confirmed <= outcome.candidates_generated);
        let morpher = Morpher::new(&g, usize::MAX);
        let engine = morpher.parse_word_opts("mbali", &ParseOptions::default());
        assert_eq!(outcome.structured.len(), engine.structured.len());
    }

    /// Perf pass regression guard (2026-07-16): [`FomaAnalyzer::analyze_words`]'s parallel-confirm
    /// batch path must produce, per word, the exact same confirmed-analysis multiset as calling
    /// [`FomaAnalyzer::analyze_word`] on that word alone — compared via `pg_parse::result_signature`
    /// (order-independent over the analysis set), the same fingerprint the CLI's own TSV rows use.
    /// Includes a word with zero candidates (`"zzzqxxxnonsense"`) alongside real corpus words so the
    /// batch path's empty-outcome handling is covered too, not just the confirms-something case.
    #[test]
    #[ignore = "needs local gitignored corpus data (samples/data/sena-hc.xml); run with --include-ignored"]
    fn analyze_words_matches_analyze_word_per_word() {
        let Some(g) = load_sena() else {
            eprintln!("skipping: sena-hc.xml not present on disk");
            return;
        };
        let words: Vec<String> = ["mbali", "zzzqxxxnonsense", "mbali"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        let mut analyzer = FomaAnalyzer::new(&g).expect("sena compiles");
        let sequential: Vec<String> = words
            .iter()
            .map(|w| pg_parse::result_signature(&analyzer.analyze_word(w).analyses))
            .collect();

        let batched = analyzer.analyze_words(&words);
        assert_eq!(batched.len(), words.len());
        let parallel: Vec<String> = batched
            .iter()
            .map(|(outcome, _)| pg_parse::result_signature(&outcome.analyses))
            .collect();

        assert_eq!(
            sequential, parallel,
            "analyze_words must match analyze_word per word, in order"
        );
    }

    const DIAGNOSTICS_FIXTURE: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE HermitCrabInput SYSTEM "HermitCrabInput.dtd">
<HermitCrabInput>
  <Language>
    <Name>CompositeDiagnosticsSmoke</Name>
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

    #[test]
    fn analyze_word_with_diagnostics_matches_normal_pipeline_and_accounts_exactly() {
        let g = pg_grammar::load(DIAGNOSTICS_FIXTURE)
            .unwrap_or_else(|e| panic!("fixture failed to load: {e}"));
        let mut normal = FomaAnalyzer::new(&g).expect("normal analyzer compiles");
        let expected = normal.analyze_word("ka");
        let mut diagnostic = FomaAnalyzer::new(&g).expect("diagnostic analyzer compiles");
        let profiled = diagnostic.analyze_word_with_diagnostics("ka");

        assert_eq!(
            pg_parse::result_signature(&profiled.outcome.analyses),
            pg_parse::result_signature(&expected.analyses)
        );
        assert_eq!(
            profiled.outcome.candidates_generated,
            expected.candidates_generated
        );
        assert_eq!(profiled.outcome.confirmed, expected.confirmed);
        assert_eq!(profiled.diagnostics.confirm_batch_calls, 1);
        assert_eq!(
            profiled.diagnostics.confirmation_calls,
            profiled.diagnostics.confirmation_groups
        );
        assert!(profiled.diagnostics.confirmation_groups <= profiled.outcome.candidates_generated);
        assert_eq!(
            profiled.diagnostics.confirmed_analyses,
            profiled.outcome.confirmed
        );
        assert_eq!(
            profiled.diagnostics.proposal.raw_paths,
            profiled.diagnostics.proposal.decoded_paths
                + profiled.diagnostics.proposal.malformed_paths
        );
    }

    #[test]
    fn from_precompiled_proposer_matches_normal_results_and_capped_diagnostics() {
        let g = pg_grammar::load(DIAGNOSTICS_FIXTURE)
            .unwrap_or_else(|e| panic!("fixture failed to load: {e}"));
        let mut normal = FomaAnalyzer::new(&g).expect("normal analyzer compiles");
        let expected = normal.analyze_word("ka");

        let proposer = FomaProposer::new(&g).expect("precompiled proposer compiles");
        let mut precompiled = FomaAnalyzer::from_precompiled_proposer(&g, proposer);
        let budget = ApplyBudget::with_caps(Some(100), Some(100));
        let profiled = match precompiled.analyze_word_with_diagnostics_budgeted("ka", &budget) {
            ProfiledFomaApplyOutcome::Complete(profiled) => profiled,
            ProfiledFomaApplyOutcome::Incomplete { dimension, .. } => {
                panic!("generous tiny-fixture budget tripped: {dimension:?}")
            }
        };

        assert_eq!(
            pg_parse::result_signature(&profiled.outcome.analyses),
            pg_parse::result_signature(&expected.analyses)
        );
        assert_eq!(
            profiled.outcome.candidates_generated,
            expected.candidates_generated
        );
        assert_eq!(profiled.outcome.confirmed, expected.confirmed);
        assert_eq!(
            profiled.diagnostics.proposal.raw_paths,
            profiled.diagnostics.proposal.decoded_paths
                + profiled.diagnostics.proposal.malformed_paths
        );
    }

    #[test]
    fn analyze_word_with_diagnostics_budgeted_stops_before_confirming_partial_candidates() {
        let g = pg_grammar::load(DIAGNOSTICS_FIXTURE)
            .unwrap_or_else(|e| panic!("fixture failed to load: {e}"));
        let mut analyzer = FomaAnalyzer::new(&g).expect("analyzer compiles");
        let budget = crate::compose_budget::ApplyBudget::with_caps(Some(0), None);

        match analyzer.analyze_word_with_diagnostics_budgeted("ka", &budget) {
            ProfiledFomaApplyOutcome::Incomplete {
                dimension,
                value,
                limit,
                diagnostics,
            } => {
                assert_eq!(
                    dimension,
                    crate::compose_budget::ApplyDimension::DecodedPaths
                );
                assert_eq!(value, 1);
                assert_eq!(limit, 0);
                assert_eq!(diagnostics.confirm_batch_calls, 0);
                assert_eq!(diagnostics.confirmation_calls, 0);
                assert_eq!(diagnostics.confirmation_groups, 0);
                assert_eq!(
                    diagnostics.proposal.raw_paths,
                    diagnostics.proposal.decoded_paths + diagnostics.proposal.malformed_paths
                );
            }
            ProfiledFomaApplyOutcome::Complete(_) => {
                panic!("path-cap=0 must not confirm a partial proposal")
            }
        }
    }

    #[test]
    fn an_unbounded_budget_leaves_analyze_word_unchanged() {
        // The whole safety argument for routing production through the budgeted entry point: every
        // cap check is `Some(cap) if count > cap`, so `None` can never trip.
        let g = pg_grammar::load(DIAGNOSTICS_FIXTURE)
            .unwrap_or_else(|e| panic!("fixture failed to load: {e}"));
        let mut analyzer = FomaAnalyzer::new(&g).expect("analyzer compiles");
        let expected = analyzer.analyze_word("ka");

        match analyzer.analyze_word_budgeted("ka", &ApplyBudget::unbounded()) {
            FomaApplyOutcome::Complete(outcome) => {
                assert_eq!(
                    pg_parse::result_signature(&outcome.analyses),
                    pg_parse::result_signature(&expected.analyses)
                );
                assert_eq!(outcome.candidates_generated, expected.candidates_generated);
                assert_eq!(outcome.confirmed, expected.confirmed);
                assert_eq!(outcome.peel_used, expected.peel_used);
            }
            FomaApplyOutcome::Incomplete { dimension, .. } => {
                panic!("an unbounded budget tripped on {dimension:?}")
            }
        }
    }

    #[test]
    fn the_budgeted_production_path_agrees_with_the_diagnostic_one() {
        // Two budgeted paths would be two contracts. They share the cumulative-budget semantics and
        // must agree on results, or `assess` and `diagnose` would disagree about the same word.
        let g = pg_grammar::load(DIAGNOSTICS_FIXTURE)
            .unwrap_or_else(|e| panic!("fixture failed to load: {e}"));
        let mut analyzer = FomaAnalyzer::new(&g).expect("analyzer compiles");
        let budget = ApplyBudget::with_caps(Some(100), Some(100));

        let production = match analyzer.analyze_word_budgeted("ka", &budget) {
            FomaApplyOutcome::Complete(outcome) => outcome,
            FomaApplyOutcome::Incomplete { dimension, .. } => {
                panic!("generous tiny-fixture budget tripped: {dimension:?}")
            }
        };
        let diagnostic = match analyzer.analyze_word_with_diagnostics_budgeted("ka", &budget) {
            ProfiledFomaApplyOutcome::Complete(profiled) => profiled.outcome,
            ProfiledFomaApplyOutcome::Incomplete { dimension, .. } => {
                panic!("generous tiny-fixture budget tripped: {dimension:?}")
            }
        };

        assert_eq!(
            pg_parse::result_signature(&production.analyses),
            pg_parse::result_signature(&diagnostic.analyses)
        );
        assert_eq!(
            production.candidates_generated,
            diagnostic.candidates_generated
        );
    }

    #[test]
    fn a_tripped_production_budget_reports_the_dimension_and_confirms_nothing() {
        let g = pg_grammar::load(DIAGNOSTICS_FIXTURE)
            .unwrap_or_else(|e| panic!("fixture failed to load: {e}"));
        let mut analyzer = FomaAnalyzer::new(&g).expect("analyzer compiles");

        match analyzer.analyze_word_budgeted("ka", &ApplyBudget::with_caps(Some(0), None)) {
            FomaApplyOutcome::Incomplete {
                dimension,
                value,
                limit,
            } => {
                assert_eq!(dimension, ApplyDimension::DecodedPaths);
                assert_eq!(value, 1);
                assert_eq!(limit, 0);
            }
            // The distinction the assessment contract rests on: this must not come back as a
            // complete outcome with an empty analysis list, which would read as "no analysis
            // exists" rather than "we stopped looking".
            FomaApplyOutcome::Complete(_) => {
                panic!("path-cap=0 must not confirm a partial proposal")
            }
        }
    }

    #[test]
    fn a_candidate_cap_trip_reports_the_candidate_dimension() {
        let g = pg_grammar::load(DIAGNOSTICS_FIXTURE)
            .unwrap_or_else(|e| panic!("fixture failed to load: {e}"));
        let mut analyzer = FomaAnalyzer::new(&g).expect("analyzer compiles");

        match analyzer.analyze_word_budgeted("ka", &ApplyBudget::with_caps(None, Some(0))) {
            FomaApplyOutcome::Incomplete {
                dimension, limit, ..
            } => {
                assert_eq!(dimension, ApplyDimension::Candidates);
                assert_eq!(limit, 0);
            }
            FomaApplyOutcome::Complete(_) => {
                panic!("candidate-cap=0 must not confirm a partial proposal")
            }
        }
    }

    #[test]
    #[ignore = "needs local gitignored corpus data (samples/data/sena-hc.xml); run with --include-ignored"]
    fn sena_diagnostics_preserve_results_and_report_real_confirmation_topology() {
        let Some(g) = load_sena() else {
            eprintln!("skipping: sena-hc.xml not present on disk");
            return;
        };
        let mut normal = FomaAnalyzer::new(&g).expect("sena compiles");
        let expected = normal.analyze_word("mbali");
        let mut diagnostic = FomaAnalyzer::new(&g).expect("sena compiles");
        let profiled = diagnostic.analyze_word_with_diagnostics("mbali");

        assert_eq!(
            pg_parse::result_signature(&profiled.outcome.analyses),
            pg_parse::result_signature(&expected.analyses)
        );
        assert_eq!(profiled.diagnostics.confirm_batch_calls, 1);
        assert_eq!(
            profiled.diagnostics.confirmation_calls,
            profiled.diagnostics.confirmation_groups
        );
        assert!(
            profiled.diagnostics.confirmation_groups
                <= profiled.outcome.candidates_generated,
            "confirm_batch may fuse candidates, so group count is bounded by, not equal to, candidate count"
        );
        assert_eq!(
            profiled.diagnostics.confirmed_analyses,
            profiled.outcome.confirmed
        );
        assert_eq!(
            profiled.diagnostics.proposal.raw_paths,
            profiled.diagnostics.proposal.decoded_paths
                + profiled.diagnostics.proposal.malformed_paths
        );
    }
}
