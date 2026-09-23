//! Deterministic, budgeted search and confirmed-only ranking for compilation backends.
//! Candidate construction and HC execution are injected through `CandidateEvaluator`, while this
//! module owns search policy, budget enforcement, certification boundaries, Pareto ranking, and
//! replay semantics.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Budget {
    pub candidates: u64,
    pub evaluations: u64,
    /// Wall-clock allowance in nanoseconds. A caller may set `u64::MAX` for no wall-clock limit.
    pub elapsed: u64,
    /// Aggregate build allowance in nanoseconds.
    pub build: u64,
    /// Peak memory allowance in bytes.
    pub memory: u64,
    /// Aggregate full-HC confirmation-work allowance, measured as confirmation calls.
    pub confirmation: u64,
    /// Portion of `elapsed` reserved for finalist confirmation.
    pub reserve: u64,
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            candidates: u64::MAX,
            evaluations: u64::MAX,
            elapsed: u64::MAX,
            build: u64::MAX,
            memory: u64::MAX,
            confirmation: u64::MAX,
            reserve: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct BudgetUsage {
    pub candidates: u64,
    pub evaluations: u64,
    pub elapsed: u64,
    pub build: u64,
    pub memory_peak: u64,
    pub confirmation: u64,
}

impl Budget {
    pub fn search_elapsed(&self) -> u64 {
        self.elapsed.saturating_sub(self.reserve)
    }

    pub fn admits(&self, usage: BudgetUsage) -> bool {
        usage.candidates <= self.candidates
            && usage.evaluations <= self.evaluations
            && usage.elapsed <= self.elapsed
            && usage.build <= self.build
            && usage.memory_peak <= self.memory
            && usage.confirmation <= self.confirmation
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Termination {
    Complete,
    BudgetExhausted,
    NoCandidates,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Strategy {
    Exhaustive,
    DiverseBeam,
    BranchAndBound,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SearchQuality {
    Exact,
    Approximate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CandidateState {
    pub id: String,
    pub family: String,
    pub signature: String,
    /// Admissible lower bound on the final scalar selection objective.
    pub lower_bound: u64,
    /// Exact scalar objective when a cheap/pilot evaluation has already measured it. Branch and
    /// bound never prunes from an estimate that is not marked exact.
    pub exact_objective: Option<u64>,
    pub baseline: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CorpusExclusion {
    /// Zero-based ordinal in the caller's requested slice. This is run-local occurrence evidence,
    /// not a persisted corpus identity.
    pub requested_ordinal: u64,
    pub word: String,
    pub reason: String,
}

/// The configuration a corpus's ELIGIBILITY was derived under.
///
/// # Why this is part of the evidence and not merely a run parameter
/// Eligibility is a function of the corpus AND of the bounds the oracle was run under. Before this
/// existed, two runs at different `--oracle-step-cap` values produced certifications that were
/// byte-indistinguishable in every field a reader could check, so "this corpus is fully eligible"
/// was an unqualified claim that silently meant "…at whatever cap happened to be in force". All
/// three bounds are recorded, including the two that can only ever ABORT a run
/// (`Self::memory_ceiling_bytes`, `Self::liveness_net_ns`) — a run that completes under a
/// 300-second liveness net is not the same evidence as one that completes under a 2-second net,
/// because the second one had a whole class of words it would have refused to finish measuring.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct OracleEligibilityConfig {
    /// The step cap that IS the eligibility classifier: a word that exhausts it is excluded, and
    /// nothing else can exclude a word for cost.
    pub step_cap: u64,
    /// The declared resident-memory ceiling for the oracle preparation pass. Never a classifier —
    /// exceeding it is a typed abort, because a memory reading is load-sensitive and a
    /// load-sensitive classifier is exactly the defect this whole mechanism exists to remove.
    pub memory_ceiling_bytes: u64,
    /// The wall-clock LIVENESS NET, in nanoseconds. Never a classifier, for the same reason as
    /// `memory_ceiling_bytes`; tripping it aborts the preparation run.
    pub liveness_net_ns: u64,
}

/// Transitional, run-local evidence for a requested corpus's eligibility.
///
/// This is deliberately diagnostic evidence on the existing backend certification result, not a
/// second corpus identity architecture. A versioned `CorpusSnapshot`/`CertificationScope`
/// migration remains a known follow-on, not built here.
///
/// It is emitted for COMPLETE corpora too, not only incomplete ones. A run that excludes nothing
/// still has to say so in band — "there were no exclusions" and "nobody looked" are different
/// facts, and a hand-filtered word list fed in from outside used to be indistinguishable from an
/// honest full-corpus run precisely because a zero-exclusion run emitted no ledger at all.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CorpusCompletenessEvidence {
    pub requested: u64,
    pub included: u64,
    pub excluded: u64,
    pub requested_hash: String,
    pub included_hash: String,
    pub excluded_hash: String,
    /// See `OracleEligibilityConfig::step_cap`. Flattened rather than nested so a reader (and a
    /// JSON assertion) reaches it with the same one-level lookup as every count beside it.
    pub oracle_step_cap: u64,
    /// See `OracleEligibilityConfig::memory_ceiling_bytes`.
    pub oracle_memory_ceiling_bytes: u64,
    /// See `OracleEligibilityConfig::liveness_net_ns`.
    pub oracle_liveness_net_ns: u64,
    pub exclusions: Vec<CorpusExclusion>,
}

impl CorpusCompletenessEvidence {
    /// Constructs the complete transitional evidence record from occurrence-level selection data.
    ///
    /// The assertions are intentional: this constructor is the invariant seam for the temporary
    /// run-local schema. The eventual versioned `CorpusSnapshot`/`CertificationScope` must replace
    /// this evidence rather than extending it into an authoritative identity system. Exclusions
    /// are already in requested order because their ordinals are the caller's requested ordinals;
    /// rejecting any other order keeps serialized evidence and its ledger hash deterministic.
    pub(crate) fn from_selection(
        requested: &[String],
        included: &[String],
        exclusions: Vec<CorpusExclusion>,
        oracle: OracleEligibilityConfig,
    ) -> Self {
        assert!(
            exclusions.len() <= requested.len(),
            "corpus evidence cannot exclude more occurrences than requested"
        );
        assert_eq!(
            requested.len() - exclusions.len(),
            included.len(),
            "corpus evidence must account for every requested occurrence"
        );
        assert!(
            exclusions
                .iter()
                .all(|exclusion| { exclusion.requested_ordinal < requested.len() as u64 }),
            "corpus exclusions must identify an occurrence in the requested slice"
        );
        assert!(
            exclusions
                .windows(2)
                .all(|pair| { pair[0].requested_ordinal < pair[1].requested_ordinal }),
            "corpus exclusions must be in strictly increasing requested order"
        );

        Self {
            requested: requested.len() as u64,
            included: included.len() as u64,
            excluded: exclusions.len() as u64,
            requested_hash: hash_words(requested),
            included_hash: hash_words(included),
            // Historical field name kept for wire compatibility; since v2 this also hashes the oracle config, so two runs at different caps can't collide.
            excluded_hash: hash_exclusion_ledger(&exclusions, oracle),
            oracle_step_cap: oracle.step_cap,
            oracle_memory_ceiling_bytes: oracle.memory_ceiling_bytes,
            oracle_liveness_net_ns: oracle.liveness_net_ns,
            exclusions,
        }
    }

    /// Whether the ledger accounts for every requested occurrence. `false` means the artifact is
    /// internally inconsistent, not that words were excluded.
    pub fn reconciles(&self) -> bool {
        self.requested == self.included.saturating_add(self.excluded)
            && self.excluded == self.exclusions.len() as u64
    }
}

fn hash_words(words: &[String]) -> String {
    let mut hash = Sha256::new();
    for word in words {
        hash.update((word.len() as u64).to_le_bytes());
        hash.update(word.as_bytes());
    }
    format!("{:x}", hash.finalize())
}

fn hash_exclusion_ledger(
    exclusions: &[CorpusExclusion],
    oracle: OracleEligibilityConfig,
) -> String {
    let mut hash = Sha256::new();
    hash.update(b"corpus-exclusion-ledger-v2");
    hash.update(oracle.step_cap.to_le_bytes());
    hash.update(oracle.memory_ceiling_bytes.to_le_bytes());
    hash.update(oracle.liveness_net_ns.to_le_bytes());
    for exclusion in exclusions {
        hash.update(exclusion.requested_ordinal.to_le_bytes());
        hash.update((exclusion.word.len() as u64).to_le_bytes());
        hash.update(exclusion.word.as_bytes());
        hash.update((exclusion.reason.len() as u64).to_le_bytes());
        hash.update(exclusion.reason.as_bytes());
    }
    format!("{:x}", hash.finalize())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum Certification {
    EstimateOnly,
    CapabilityRejected {
        reason: String,
    },
    BuildFailed {
        reason: String,
    },
    Truncated {
        stage: String,
        /// Optional additive diagnostic evidence; `default`/`skip_serializing_if` keep legacy `Truncated { stage }` values readable and their wire shape unchanged.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        corpus: Option<CorpusCompletenessEvidence>,
    },
    Unsupported {
        reason: String,
    },
    ResourceBreach {
        dimension: String,
        value: u64,
        limit: u64,
    },
    IdentityMismatch {
        word: String,
        detail: String,
        /// Which direction(s) the mismatch disagreed in -- see `crate::parity::IdentityMismatchDirection`. A caller matching structurally can tell a recall miss from a surviving over-generation without parsing `detail`.
        direction: crate::parity::IdentityMismatchDirection,
    },
    FullHcConfirmed {
        words: u64,
        corpus_hash: String,
    },
}

impl Certification {
    pub fn selectable(&self) -> bool {
        matches!(self, Self::FullHcConfirmed { .. })
    }

    pub fn shortest_disagreement(&self) -> Option<&str> {
        match self {
            Self::IdentityMismatch { word, .. } => Some(word),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Score {
    pub states: u64,
    pub arcs: u64,
    pub build: u64,
    pub apply: u64,
    pub proposals: u64,
    pub confirmation: u64,
    /// Full-HC oracle step ticks consumed confirming this candidate's whole corpus.
    ///
    /// The primary ranking component.
    pub confirmation_steps: u64,
    /// Raw proposer paths `apply_up` yielded across the whole corpus, before tag-decode/dedup --
    /// summed from `FomaWordDiagnostics::raw_paths` (see that field's doc). The propose-side
    /// counterpart to `confirmation_steps`: together they are the leading term of `Self::key`.
    pub raw_paths: u64,
}

impl Score {
    pub fn pareto_vector(&self) -> [u64; 6] {
        [
            self.confirmation_steps,
            self.raw_paths,
            self.confirmation,
            self.proposals,
            self.states,
            self.arcs,
        ]
    }

    /// Ranks candidates by DETERMINISTIC WORK, not by wall-clock.
    ///
    /// # Why work and not time
    /// Every component here is exactly reproducible: measured over eight synthetic fixtures at ten
    /// repetitions each, `confirmation`, `proposals`, `states` and `arcs` had zero spread, while
    /// `build` varied 15-50% and `apply` 6-20% run to run. Ranking by time therefore decided ties by
    /// noise -- observed: two runs of the same grammar with the same seed named DIFFERENT winners
    /// because one candidate happened to build 2.8ms faster. Ranking by work cannot do that, so a
    /// reported winner is now a property of the compilation rather than of the machine it was measured
    /// on, and is comparable across machines and over time with no re-baselining.
    ///
    /// # Why confirmation work comes first
    /// Full-HC confirmation is the cost that dominates propose→confirm: one grammar here proposes
    /// 1064 candidates over 9 words, and confirmation has to adjudicate all of it. `Budget` already
    /// denominates its allowance in this unit ("Aggregate full-HC confirmation-work allowance,
    /// measured as confirmation calls"), so objective and budget now agree on a unit instead of one
    /// counting work while the other ranked seconds.
    ///
    /// This ordering is not cosmetic. Measured on a marker-free fixture, two candidates compiled to
    /// the SAME 11 states / 13 arcs while one did 2 confirmation calls and the other 4; ranking size
    /// first tied them and fell through to build time, which named the candidate doing twice the work.
    ///
    /// # Why minimizing work is safe
    /// Fewer proposals could mean an under-generating network. It cannot be selected: only a
    /// `selectable()` candidate may win (`BackendOptimizationReport::validate`), which requires full-HC
    /// confirmation over the whole corpus. Work-minimization operates strictly behind that gate.
    ///
    /// `build`/`apply` remain in `Score` and in the report as diagnostics -- useful for spotting a
    /// pathological compile -- but deliberately do NOT rank. Candidates that tie on every component
    /// here are genuinely tied, and the `id` tiebreak makes that outcome deterministic rather than
    /// pretending to a preference.
    ///
    /// # Why steps rank above calls
    /// `confirmation_steps` leads because a confirmation CALL is not a constant amount of work: a long
    /// word costs far more to adjudicate than a short one, so ranking by calls under-weights exactly
    /// the expensive words that dominate real cost. Steps are also the unit HC work is already BOUNDED
    /// in (`crate::backend_runtime::DEFAULT_ORACLE_STEP_CAP` caps these same ticks), so the objective
    /// and the cap now measure the same quantity rather than two proxies for it. `confirmation`
    /// (calls) stays as the next component: it separates candidates that happen to consume equal
    /// steps across a different number of adjudications.
    ///
    /// # Why `raw_paths` joins the leading term (and why steps alone were not enough)
    /// `confirmation_steps` prices confirm-side work only. It does NOT price what it costs to
    /// *produce* the candidates confirm then adjudicates -- and chunk fusion (`confirm.rs`'s batched
    /// re-parse) means propose-side blowups do not reliably show up as more steps: fusion absorbs
    /// excess proposals into shared oracle calls at near-zero marginal step cost, so a candidate that
    /// does several-fold more propose-side traversal can still look step-tied with one that does far
    /// less. Measured on Sena's four-corpus shape: the plan-composed path produced 575 proposals over
    /// 42 confirmation calls and 1192 steps, while the hand-spun candidate produced 127 proposals over
    /// 17 calls and 1252 steps. Steps-first picked the 575-proposal candidate on that 60-step margin
    /// (1192 < 1252) -- a preference decided by a number that never priced the 4.5x proposal gap at
    /// all, because steps and propose-side traversal are not coupled by anything the key enforces.
    /// `raw_paths` (the count of raw paths `apply_up` yields before tag-decode/dedup, summed
    /// across the corpus) restores that missing cost deterministically: it is exactly the same kind of
    /// unit as a step -- one traversal action -- so the two are summed rather than chained as separate
    /// lexicographic terms, and a candidate can no longer look cheap by pushing its cost from the
    /// confirm side to the propose side. `proposals` (post-dedup candidate count) cannot substitute
    /// for this: it undercounts exactly the traversal a proposer pays before dedup collapses paths
    /// together, which is the whole quantity this term exists to price. Unit commensurability between
    /// a step and a raw path is asserted 1:1, not derived; if a future corpus shows the sum
    /// mis-ranking, the fallback is to keep steps-first and add `raw_paths` as its own
    /// lexicographic term instead of summing it in.
    pub fn key(&self, id: &str) -> (u64, u64, u64, u64, String) {
        (
            self.confirmation_steps.saturating_add(self.raw_paths),
            self.confirmation,
            self.proposals,
            self.states.saturating_add(self.arcs),
            id.to_owned(),
        )
    }

    pub fn scalar_objective(&self) -> u64 {
        self.states.saturating_add(self.arcs)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchResult {
    pub selected: Vec<CandidateState>,
    pub strategy: Strategy,
    pub quality: SearchQuality,
    pub termination: Termination,
    pub explored: u64,
    pub unexplored: u64,
    pub generated: u64,
    pub expanded: u64,
    pub pruned: u64,
}

pub trait SearchStrategy: Send + Sync {
    fn strategy(&self) -> Strategy;
    fn search(&self, candidates: &[CandidateState], budget: Budget, seed: u64) -> SearchResult;
}

fn empty_result(strategy: Strategy) -> SearchResult {
    SearchResult {
        selected: Vec::new(),
        strategy,
        quality: SearchQuality::Exact,
        termination: Termination::NoCandidates,
        explored: 0,
        unexplored: 0,
        generated: 0,
        expanded: 0,
        pruned: 0,
    }
}

fn capacity(budget: Budget, len: usize) -> usize {
    budget.candidates.min(budget.evaluations).min(len as u64) as usize
}

fn stable_seed_rank(seed: u64, text: &str) -> u64 {
    let mut hash = seed ^ 0xcbf2_9ce4_8422_2325;
    for byte in text.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn baseline_first(mut candidates: Vec<CandidateState>) -> Vec<CandidateState> {
    candidates.sort_by_key(|candidate| (!candidate.baseline, candidate.id.clone()));
    candidates
}

#[derive(Debug, Clone, Copy)]
pub struct Exhaustive;

impl SearchStrategy for Exhaustive {
    fn strategy(&self) -> Strategy {
        Strategy::Exhaustive
    }

    fn search(&self, candidates: &[CandidateState], budget: Budget, _seed: u64) -> SearchResult {
        if candidates.is_empty() {
            return empty_result(self.strategy());
        }
        let ordered = baseline_first(candidates.to_vec());
        let cap = capacity(budget, ordered.len());
        let selected = ordered[..cap].to_vec();
        let complete = cap == candidates.len();
        SearchResult {
            selected,
            strategy: self.strategy(),
            quality: if complete {
                SearchQuality::Exact
            } else {
                SearchQuality::Approximate
            },
            termination: if complete {
                Termination::Complete
            } else {
                Termination::BudgetExhausted
            },
            explored: cap as u64,
            unexplored: (candidates.len() - cap) as u64,
            generated: candidates.len() as u64,
            expanded: cap as u64,
            pruned: 0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct DiverseBeam {
    pub width: usize,
}

impl SearchStrategy for DiverseBeam {
    fn strategy(&self) -> Strategy {
        Strategy::DiverseBeam
    }

    fn search(&self, candidates: &[CandidateState], budget: Budget, seed: u64) -> SearchResult {
        if candidates.is_empty() {
            return empty_result(self.strategy());
        }
        let cap = self.width.min(capacity(budget, candidates.len()));
        let mut remaining = candidates.to_vec();
        remaining.sort_by_key(|candidate| {
            (
                !candidate.baseline,
                candidate.lower_bound,
                stable_seed_rank(seed, &candidate.id),
                candidate.id.clone(),
            )
        });
        let mut selected = Vec::new();
        let mut families = BTreeSet::new();
        let mut signatures = BTreeSet::new();
        while selected.len() < cap && !remaining.is_empty() {
            let best = remaining
                .iter()
                .enumerate()
                .min_by_key(|(_, candidate)| {
                    (
                        !candidate.baseline,
                        families.contains(&candidate.family),
                        signatures.contains(&candidate.signature),
                        candidate.lower_bound,
                        stable_seed_rank(seed, &candidate.id),
                        candidate.id.clone(),
                    )
                })
                .map(|(index, _)| index)
                .expect("remaining is non-empty");
            let candidate = remaining.remove(best);
            families.insert(candidate.family.clone());
            signatures.insert(candidate.signature.clone());
            selected.push(candidate);
        }
        let complete = selected.len() == candidates.len();
        SearchResult {
            selected,
            strategy: self.strategy(),
            quality: if complete {
                SearchQuality::Exact
            } else {
                SearchQuality::Approximate
            },
            termination: if complete {
                Termination::Complete
            } else {
                Termination::BudgetExhausted
            },
            explored: cap as u64,
            unexplored: (candidates.len() - cap) as u64,
            generated: candidates.len() as u64,
            expanded: cap as u64,
            pruned: 0,
        }
    }
}

/// Branch-and-bound over fully specified candidates. `lower_bound` must be admissible and
/// `exact_objective` must only be populated by a completed low-cost evaluation. A candidate is
/// pruned only when its lower bound is strictly worse than the incumbent exact objective.
#[derive(Debug, Clone, Copy)]
pub struct BranchAndBound;

impl SearchStrategy for BranchAndBound {
    fn strategy(&self) -> Strategy {
        Strategy::BranchAndBound
    }

    fn search(&self, candidates: &[CandidateState], budget: Budget, seed: u64) -> SearchResult {
        if candidates.is_empty() {
            return empty_result(self.strategy());
        }
        let cap = capacity(budget, candidates.len());
        let mut ordered = candidates.to_vec();
        ordered.sort_by_key(|candidate| {
            (
                !candidate.baseline,
                candidate.lower_bound,
                stable_seed_rank(seed, &candidate.id),
                candidate.id.clone(),
            )
        });
        let mut selected = Vec::new();
        let mut incumbent = u64::MAX;
        let mut pruned = 0usize;
        let mut budget_unexplored = 0usize;
        for candidate in ordered {
            if candidate.lower_bound > incumbent {
                pruned += 1;
                continue;
            }
            if selected.len() >= cap {
                budget_unexplored += 1;
                continue;
            }
            if let Some(objective) = candidate.exact_objective {
                incumbent = incumbent.min(objective);
            }
            selected.push(candidate);
        }
        let complete = selected.len() + pruned == candidates.len();
        SearchResult {
            explored: selected.len() as u64,
            unexplored: budget_unexplored as u64,
            generated: candidates.len() as u64,
            expanded: selected.len() as u64,
            pruned: pruned as u64,
            selected,
            strategy: self.strategy(),
            quality: if complete {
                SearchQuality::Exact
            } else {
                SearchQuality::Approximate
            },
            termination: if complete {
                Termination::Complete
            } else {
                Termination::BudgetExhausted
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConstraintTopology {
    pub strong_pruning: bool,
    pub compositional: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PilotCosts {
    pub p50: u64,
    pub p95: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdaptivePolicy {
    pub exhaustive_budget_numerator: u64,
    pub exhaustive_budget_denominator: u64,
    pub beam_width: usize,
    pub pilot_candidate_cap: usize,
    pub pilot_word_cap: usize,
    pub strong_pruning_ppm: u32,
}

impl Default for AdaptivePolicy {
    fn default() -> Self {
        Self {
            exhaustive_budget_numerator: 1,
            exhaustive_budget_denominator: 2,
            beam_width: 16,
            pilot_candidate_cap: 8,
            pilot_word_cap: 8,
            strong_pruning_ppm: 250_000,
        }
    }
}

pub fn exhaustive_admitted_with_policy(
    static_count: u64,
    p95: u64,
    remaining_elapsed: u64,
    policy: AdaptivePolicy,
) -> bool {
    let admitted = remaining_elapsed.saturating_mul(policy.exhaustive_budget_numerator)
        / policy.exhaustive_budget_denominator.max(1);
    static_count.saturating_mul(p95) <= admitted
}

pub fn exhaustive_admitted(static_count: u64, p95: u64, remaining_elapsed: u64) -> bool {
    exhaustive_admitted_with_policy(
        static_count,
        p95,
        remaining_elapsed,
        AdaptivePolicy::default(),
    )
}

pub fn choose_strategy_with_policy(
    static_count: u64,
    pilot: PilotCosts,
    budget: Budget,
    topology: ConstraintTopology,
    policy: AdaptivePolicy,
) -> Strategy {
    if exhaustive_admitted_with_policy(static_count, pilot.p95, budget.search_elapsed(), policy) {
        Strategy::Exhaustive
    } else if topology.strong_pruning || topology.compositional {
        Strategy::BranchAndBound
    } else {
        Strategy::DiverseBeam
    }
}

pub fn choose_strategy(
    static_count: u64,
    pilot: PilotCosts,
    budget: Budget,
    topology: ConstraintTopology,
) -> Strategy {
    choose_strategy_with_policy(
        static_count,
        pilot,
        budget,
        topology,
        AdaptivePolicy::default(),
    )
}

pub trait StrategyRegistry {
    fn get(&self, strategy: Strategy) -> Option<Box<dyn SearchStrategy>>;
}

#[derive(Debug, Clone, Copy)]
pub struct DefaultStrategyRegistry {
    pub beam_width: usize,
}

impl Default for DefaultStrategyRegistry {
    fn default() -> Self {
        Self { beam_width: 16 }
    }
}

impl StrategyRegistry for DefaultStrategyRegistry {
    fn get(&self, strategy: Strategy) -> Option<Box<dyn SearchStrategy>> {
        Some(match strategy {
            Strategy::Exhaustive => Box::new(Exhaustive),
            Strategy::DiverseBeam => Box::new(DiverseBeam {
                width: self.beam_width,
            }),
            Strategy::BranchAndBound => Box::new(BranchAndBound),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfirmationEvidence {
    pub certification: Certification,
    pub score: Option<Score>,
    pub usage: BudgetUsage,
    /// Orthogonal to `certification`: true when this candidate's completed FST is not eligible for
    /// production publication, independent of whether it also reproduced the oracle. A caller
    /// building this from `crate::backend_runtime::RuntimeEvaluation` reads it off that type's own
    /// `production_health`, never re-derives it.
    pub production_blocks_publication: bool,
}

pub trait CandidateEvaluator {
    fn evaluate(&mut self, candidate: &CandidateState, remaining: Budget) -> ConfirmationEvidence;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvaluatedCandidate {
    pub candidate: CandidateState,
    pub evidence: ConfirmationEvidence,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OptimizationOutcome {
    pub search: SearchResult,
    pub evaluated: Vec<EvaluatedCandidate>,
    pub frontier: Vec<String>,
    pub winner: Option<String>,
    pub usage: BudgetUsage,
}

pub fn optimize_with_evaluator(
    candidates: &[CandidateState],
    budget: Budget,
    seed: u64,
    strategy: &dyn SearchStrategy,
    evaluator: &mut dyn CandidateEvaluator,
) -> OptimizationOutcome {
    let mut search = strategy.search(candidates, budget, seed);
    let selected_count = search.selected.len() as u64;
    // `reserve` is a real floor on `elapsed`, not only a selector nudge: the exploratory sweep may spend at most `search_elapsed()`, leaving `reserve` nanoseconds of the caller's deadline unspent.
    let search_elapsed = budget.search_elapsed();
    let mut usage = BudgetUsage::default();
    let mut evaluated = Vec::new();
    for candidate in &search.selected {
        if usage.evaluations >= budget.evaluations {
            break;
        }
        // The baseline is element zero and always evaluated: a reserve that swallows the whole deadline must still not strip the run of it.
        if !evaluated.is_empty() && usage.elapsed >= search_elapsed {
            break;
        }
        let remaining = Budget {
            candidates: budget.candidates.saturating_sub(usage.candidates),
            evaluations: budget.evaluations.saturating_sub(usage.evaluations),
            elapsed: search_elapsed.saturating_sub(usage.elapsed),
            build: budget.build.saturating_sub(usage.build),
            memory: budget.memory.saturating_sub(usage.memory_peak),
            confirmation: budget.confirmation.saturating_sub(usage.confirmation),
            reserve: budget.reserve,
        };
        let evidence = evaluator.evaluate(candidate, remaining);
        usage.candidates = usage.candidates.saturating_add(1);
        usage.evaluations = usage.evaluations.saturating_add(1);
        usage.elapsed = usage.elapsed.saturating_add(evidence.usage.elapsed);
        usage.build = usage.build.saturating_add(evidence.usage.build);
        usage.memory_peak = usage.memory_peak.max(evidence.usage.memory_peak);
        usage.confirmation = usage
            .confirmation
            .saturating_add(evidence.usage.confirmation);
        evaluated.push(EvaluatedCandidate {
            candidate: candidate.clone(),
            evidence,
        });
        if !budget.admits(usage) {
            break;
        }
    }
    let evaluated_count = evaluated.len() as u64;
    if evaluated_count < selected_count {
        let deficit = selected_count - evaluated_count;
        search.quality = SearchQuality::Approximate;
        search.termination = Termination::BudgetExhausted;
        search.explored = search.explored.saturating_sub(deficit);
        search.unexplored = search.unexplored.saturating_add(deficit);
    } else if !budget.admits(usage) {
        // Termination only, never quality: the last selected candidate's measured cost breached a budget dimension after the fact, but it was still evaluated, so SearchQuality's "looked at everything selected?" answer stays yes.
        // See docs/research/pg-foma-recipe-optimizer-design-notes.md for why downgrading quality here breaks report-writing entirely.
        search.termination = Termination::BudgetExhausted;
    }
    // A production-blocked candidate gets NO ranking row, so it can neither win nor reach the frontier.
    let ranking: Vec<(String, Certification, Score)> = evaluated
        .iter()
        .filter(|item| !item.evidence.production_blocks_publication)
        .filter_map(|item| {
            item.evidence.score.map(|score| {
                (
                    item.candidate.id.clone(),
                    item.evidence.certification.clone(),
                    score,
                )
            })
        })
        .collect();
    let frontier = pareto_frontier(&ranking);
    let winner = select_confirmed(&ranking);
    OptimizationOutcome {
        search,
        evaluated,
        frontier,
        winner,
        usage,
    }
}

pub fn select_confirmed(items: &[(String, Certification, Score)]) -> Option<String> {
    items
        .iter()
        .filter(|(_, certification, _)| certification.selectable())
        .min_by_key(|(id, _, score)| score.key(id))
        .map(|(id, _, _)| id.clone())
}

pub fn pareto_frontier(items: &[(String, Certification, Score)]) -> Vec<String> {
    let confirmed: Vec<_> = items
        .iter()
        .filter(|(_, certification, _)| certification.selectable())
        .collect();
    let mut frontier: Vec<String> = confirmed
        .iter()
        .filter(|candidate| {
            !confirmed
                .iter()
                .any(|other| other.0 != candidate.0 && dominates(&other.2, &candidate.2))
        })
        .map(|(id, _, _)| id.clone())
        .collect();
    frontier.sort();
    frontier
}

fn dominates(left: &Score, right: &Score) -> bool {
    let left = left.pareto_vector();
    let right = right.pareto_vector();
    left.iter().zip(right).all(|(a, b)| *a <= b) && left.iter().zip(right).any(|(a, b)| *a < b)
}

#[cfg(test)]
mod tests;
