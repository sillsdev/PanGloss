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

use crate::analyzer::{FomaError, FomaProposer, ProposalDiagnostics};
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

#[cfg(feature = "test-concurrency-hook")]
#[doc(hidden)]
pub mod test_confirmation_concurrency {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    static ARMED: AtomicBool = AtomicBool::new(false);
    static ACTIVE: AtomicUsize = AtomicUsize::new(0);
    static MAX_ACTIVE: AtomicUsize = AtomicUsize::new(0);
    pub fn arm() {
        ACTIVE.store(0, Ordering::SeqCst);
        MAX_ACTIVE.store(0, Ordering::SeqCst);
        ARMED.store(true, Ordering::SeqCst);
    }
    pub fn max_active() -> usize {
        MAX_ACTIVE.load(Ordering::SeqCst)
    }
    pub(super) fn take_arm() -> bool {
        ARMED.swap(false, Ordering::SeqCst)
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) struct Guard(bool);
    #[cfg(not(target_arch = "wasm32"))]
    impl Guard {
        pub(super) fn enter(enabled: bool) -> Self {
            if !enabled {
                return Self(false);
            }
            let active = ACTIVE.fetch_add(1, Ordering::SeqCst) + 1;
            MAX_ACTIVE.fetch_max(active, Ordering::SeqCst);
            std::thread::sleep(std::time::Duration::from_millis(20));
            Self(true)
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    impl Drop for Guard {
        fn drop(&mut self) {
            if self.0 {
                ACTIVE.fetch_sub(1, Ordering::SeqCst);
            }
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
    pub confirmed_analyses: usize,
    pub confirmation_elapsed: Duration,
}

/// A complete ordinary [`FomaOutcome`] paired with opt-in runtime diagnostics.
pub struct ProfiledFomaOutcome {
    pub outcome: FomaOutcome,
    pub diagnostics: FomaWordDiagnostics,
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
}

impl<'g> FomaAnalyzer<'g> {
    /// Emit + foma-compile `g` (via [`FomaProposer::new`]), build the redup peeler, an UNCAPPED
    /// verify `Morpher` (`Morpher::new(g, usize::MAX)` — see [`crate::confirm::confirm_all`]'s doc
    /// for why a cap here would be a silent parity bug, not a performance knob), and the
    /// morpheme-owner reverse map confirm needs. `Err` iff the grammar's emitted lexc source itself
    /// fails to foma-compile. Per the revised plan §0 there is no per-grammar fallback tier: this
    /// composite IS the mainline for every grammar, so a compile failure here is an emitter gap to
    /// fix (later plan stages), not a routing decision — the `Err` just surfaces it to the caller.
    pub fn new(g: &'g Grammar) -> Result<Self, FomaError> {
        let proposer = FomaProposer::new(g)?;
        Ok(FomaAnalyzer {
            g,
            proposer,
            peeler: ReduplicationPeeler::new(g),
            morpher: Morpher::new(g, usize::MAX),
            owners: confirm::build_morpheme_owners(g),
            peel_budget: ComposeBudget::from_env(),
        })
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
        let proposed = self.propose_candidates_with_diagnostics_budgeted(word, budget);
        let (candidates, peel_used, peel_chain_depth_error, proposal, proposal_calls) =
            match proposed {
                Ok(complete) => complete,
                Err((dimension, value, limit, proposal, proposal_calls)) => {
                    return ProfiledFomaApplyOutcome::Incomplete {
                        dimension,
                        value,
                        limit,
                        diagnostics: FomaWordDiagnostics {
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
            proposal,
            proposal_calls,
            confirm_batch_calls: 1,
            confirmation_groups: confirmation.confirmation_groups,
            confirmation_calls: confirmation.confirmation_calls,
            confirmed_analyses: outcome.confirmed,
            confirmation_elapsed,
        };
        ProfiledFomaApplyOutcome::Complete(ProfiledFomaOutcome {
            outcome,
            diagnostics,
        })
    }

    fn propose_candidates_with_diagnostics_budgeted(
        &mut self,
        word: &str,
        budget: &ApplyBudget,
    ) -> std::result::Result<
        (
            Vec<Candidate>,
            bool,
            Option<ComposeError>,
            ProposalDiagnostics,
            usize,
        ),
        (ApplyDimension, usize, usize, ProposalDiagnostics, usize),
    > {
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
    /// after every `from_cached`.
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
        FomaAnalyzer {
            g,
            proposer,
            peeler,
            morpher: Morpher::new(g, usize::MAX),
            owners,
            peel_budget: ComposeBudget::from_env(),
        }
    }

    /// The inverse of [`Self::from_cached`]: reclaim the three owned pieces this analyzer was
    /// built from (or built fresh in [`Self::new`]), discarding only the transient `Morpher<'g>`
    /// borrow (recreated for free on the next [`Self::from_cached`] call).
    pub fn into_parts(
        self,
    ) -> (
        FomaProposer,
        ReduplicationPeeler,
        Vec<Option<MorphemeOwner>>,
    ) {
        (self.proposer, self.peeler, self.owners)
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
    #[cfg(feature = "test-concurrency-hook")]
    let observe_confirmation = test_confirmation_concurrency::take_arm();
    let buckets_per_word: Vec<TimedConfirmedBuckets> = pool.install(|| {
        words
            .par_iter()
            .zip(proposed.par_iter())
            .map(|(word, proposal)| {
                #[cfg(feature = "test-concurrency-hook")]
                let _concurrency =
                    test_confirmation_concurrency::Guard::enter(observe_confirmation);
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
