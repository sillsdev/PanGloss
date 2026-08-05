//! Deterministic, budgeted search and confirmed-only ranking for compilation recipes.
//! Candidate construction and HC execution are injected through [`CandidateEvaluator`], while this
//! module owns search policy, budget enforcement, certification boundaries, Pareto ranking, and
//! replay semantics.

use std::collections::{BTreeMap, BTreeSet};

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
    BaselineOnly,
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
/// ([`Self::memory_ceiling_bytes`], [`Self::liveness_net_ns`]) — a run that completes under a
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
/// This is deliberately diagnostic evidence on the existing recipe certification result, not a
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
    /// See [`OracleEligibilityConfig::step_cap`]. Flattened rather than nested so a reader (and a
    /// JSON assertion) reaches it with the same one-level lookup as every count beside it.
    pub oracle_step_cap: u64,
    /// See [`OracleEligibilityConfig::memory_ceiling_bytes`].
    pub oracle_memory_ceiling_bytes: u64,
    /// See [`OracleEligibilityConfig::liveness_net_ns`].
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
            // Keep the historical field name for wire compatibility, but make its meaning
            // explicit: this is the exclusion ledger hash, including ordinal, word, and reason --
            // and, since v2, the oracle configuration the ledger was derived under, so that two
            // runs at different caps cannot produce the same ledger hash over the same words.
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
    StaticRejected {
        reason: String,
    },
    EstimateOnly,
    CapabilityRejected {
        reason: String,
    },
    BuildFailed {
        reason: String,
    },
    Timeout {
        stage: String,
    },
    Truncated {
        stage: String,
        /// Optional additive diagnostic evidence. `default` keeps legacy serialized
        /// `Truncated { stage }` values readable; `skip_serializing_if` keeps those values' wire
        /// shape unchanged. This field is transitional and non-authoritative until the versioned
        /// `CorpusSnapshot`/`CertificationScope` schema lands.
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
    },
    /// **No longer produced.** Retained so reports written before the parity relation moved to
    /// deduplicated [`pg_parse::identity::AnalysisIdentity`] set equality still deserialize, and so
    /// that such a report keeps ranking as the non-selectable failure it was recorded as.
    ///
    /// It used to mean "the two engines found a different NUMBER of analyses for this word".
    /// Multiplicity is not part of the parity relation (see [`crate::parity`]): two analyses
    /// reaching one identity by different derivational paths are one member of the set, so a
    /// difference in count is not by itself a disagreement. The count difference that IS a
    /// disagreement -- different numbers of DISTINCT identities -- is necessarily also a set
    /// difference and is reported as [`Self::IdentityMismatch`], whose detail names both
    /// cardinalities. Do not reintroduce a producer for this variant.
    MultiplicityMismatch {
        word: String,
        expected: u64,
        actual: u64,
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
            Self::IdentityMismatch { word, .. } | Self::MultiplicityMismatch { word, .. } => {
                Some(word)
            }
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
    /// The primary ranking component. `#[serde(default)]` so reports written before this field
    /// existed still parse; a `0` from such a report simply ranks as no recorded work, which is what
    /// "we did not measure it" honestly means here.
    #[serde(default)]
    pub confirmation_steps: u64,
    /// Raw proposer paths `apply_up` yielded across the whole corpus, before tag-decode/dedup --
    /// summed from `FomaWordDiagnostics::raw_paths` (see that field's doc). The propose-side
    /// counterpart to `confirmation_steps`: together they are the leading term of [`Self::key`].
    /// `#[serde(default)]` keeps older reports readable. Their containing report carries a
    /// `score_schema_version`; recipe_report.rs's test
    /// validation_rejects_unknown_report_and_score_schema_versions pins that a legacy version
    /// is rejected, so this default cannot be compared as if it meant measured zero work.
    #[serde(default)]
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
    /// `selectable()` candidate may win (`RecipeOptimizationReport::validate`), which requires full-HC
    /// confirmation over the whole corpus. Work-minimization operates strictly behind that gate.
    ///
    /// `build`/`apply` remain in [`Score`] and in the report as diagnostics -- useful for spotting a
    /// pathological compile -- but deliberately do NOT rank. Candidates that tie on every component
    /// here are genuinely tied, and the `id` tiebreak makes that outcome deterministic rather than
    /// pretending to a preference.
    ///
    /// # Why steps rank above calls
    /// `confirmation_steps` leads because a confirmation CALL is not a constant amount of work: a long
    /// word costs far more to adjudicate than a short one, so ranking by calls under-weights exactly
    /// the expensive words that dominate real cost. Steps are also the unit HC work is already BOUNDED
    /// in ([`crate::recipe_runtime::DEFAULT_ORACLE_STEP_CAP`] caps these same ticks), so the objective
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
    pub seed: u64,
    pub parameters: BTreeMap<String, String>,
}

pub trait SearchStrategy: Send + Sync {
    fn strategy(&self) -> Strategy;
    fn search(&self, candidates: &[CandidateState], budget: Budget, seed: u64) -> SearchResult;
}

fn empty_result(strategy: Strategy, seed: u64) -> SearchResult {
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
        seed,
        parameters: BTreeMap::new(),
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

    fn search(&self, candidates: &[CandidateState], budget: Budget, seed: u64) -> SearchResult {
        if candidates.is_empty() {
            return empty_result(self.strategy(), seed);
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
            seed,
            parameters: BTreeMap::from([("candidate-cap".to_owned(), cap.to_string())]),
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
            return empty_result(self.strategy(), seed);
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
            seed,
            parameters: BTreeMap::from([("beam-width".to_owned(), self.width.to_string())]),
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
            return empty_result(self.strategy(), seed);
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
            seed,
            parameters: BTreeMap::from([
                ("candidate-cap".to_owned(), cap.to_string()),
                ("bound".to_owned(), "admissible-lower-bound".to_owned()),
            ]),
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
    // `reserve` is a real floor on `elapsed`, not only a selector nudge: the exploratory sweep may
    // spend at most `search_elapsed()`, so `reserve` nanoseconds of the caller's deadline are still
    // unspent when the sweep ends. Before this was enforced here, `reserve` was read exactly once
    // (`choose_strategy_with_policy`) and the loop below could consume the whole deadline, so the
    // "confirmation_available" half of the documented two-phase allocation protected nothing.
    let search_elapsed = budget.search_elapsed();
    let mut usage = BudgetUsage::default();
    let mut evaluated = Vec::new();
    for candidate in &search.selected {
        if usage.evaluations >= budget.evaluations {
            break;
        }
        // The baseline is element zero and is always evaluated: a reserve large enough to swallow
        // the whole deadline must still not strip the run of the baseline the spec requires
        // ("Every optimization SHALL include the current default plan as a baseline").
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
        // Every selected candidate was evaluated, but the *measured* cost of the last one breached a
        // budget dimension — only `evaluations` is pre-checked; `elapsed`/`build`/`memory`/
        // `confirmation` are known after the evaluator returns. Without this arm a run whose final
        // candidate blew the deadline reported `Complete`, claiming it stayed inside a bound it had
        // already exceeded. The candidate-count deficit above cannot catch it: the overrun happens
        // on the last selected candidate, so no candidate is left unevaluated.
        //
        // TERMINATION ONLY, never `quality`, and that distinction is load-bearing rather than
        // stylistic. `SearchQuality` answers "did the search look at everything it selected?" and
        // `Termination` answers "why did it stop?". In THIS arm the first answer is yes — the
        // deficit branch above owns the case where it is no — so the two answers genuinely differ,
        // and only the second one changed.
        //
        // Downgrading `quality` here as well produced a report that could not be written AT ALL.
        // [`crate::recipe_report::RecipeOptimizationReport::validate`] refuses `Approximate` with
        // `unexplored == 0` ("approximate search must quantify unexplored space"), and `unexplored` is zero by
        // construction on this path — every selected candidate was evaluated. So the child exited 1
        // with no `report.json`, and `write_supervisor_failure_report` never ran either (it fires
        // only on a deadline/memory KILL, not on a non-zero exit), which means an entire run's
        // banked candidates were reachable only through `progress.jsonl`. Reproduced end to end on
        // `recipe-strata-generic` with `--confirmation-work` set one unit below the corpus's total
        // confirmation work; pinned by
        // `pg-cli/tests/recipe_optimize_continuation.rs::a_final_candidate_that_overruns_an_aggregate_bound_still_writes_a_report`.
        search.termination = Termination::BudgetExhausted;
    }
    let ranking: Vec<(String, Certification, Score)> = evaluated
        .iter()
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
mod tests {
    use super::*;

    fn candidate(
        id: &str,
        family: &str,
        signature: &str,
        bound: u64,
        exact: Option<u64>,
        baseline: bool,
    ) -> CandidateState {
        CandidateState {
            id: id.to_owned(),
            family: family.to_owned(),
            signature: signature.to_owned(),
            lower_bound: bound,
            exact_objective: exact,
            baseline,
        }
    }

    #[test]
    fn exact_half_budget_rule_and_adaptive_policy() {
        let budget = Budget {
            elapsed: 80,
            reserve: 0,
            ..Budget::default()
        };
        assert!(exhaustive_admitted(4, 10, budget.search_elapsed()));
        assert!(!exhaustive_admitted(4, 11, budget.search_elapsed()));
        assert_eq!(
            choose_strategy(
                4,
                PilotCosts { p50: 5, p95: 10 },
                budget,
                ConstraintTopology {
                    strong_pruning: false,
                    compositional: false
                }
            ),
            Strategy::Exhaustive
        );
    }

    #[test]
    fn measured_costs_and_topology_change_strategy() {
        let budget = Budget {
            elapsed: 100,
            ..Budget::default()
        };
        assert_eq!(
            choose_strategy(
                20,
                PilotCosts { p50: 4, p95: 4 },
                budget,
                ConstraintTopology {
                    strong_pruning: false,
                    compositional: false
                }
            ),
            Strategy::DiverseBeam
        );
        assert_eq!(
            choose_strategy(
                20,
                PilotCosts { p50: 4, p95: 4 },
                budget,
                ConstraintTopology {
                    strong_pruning: true,
                    compositional: false
                }
            ),
            Strategy::BranchAndBound
        );
        assert_eq!(
            choose_strategy(
                2,
                PilotCosts { p50: 4, p95: 4 },
                budget,
                ConstraintTopology {
                    strong_pruning: false,
                    compositional: false
                }
            ),
            Strategy::Exhaustive
        );
    }

    #[test]
    fn beam_preserves_baseline_and_diversity_and_seed_replays() {
        let candidates = vec![
            candidate("z", "baseline", "base", 9, None, true),
            candidate("a", "one", "same", 1, None, false),
            candidate("b", "one", "same", 1, None, false),
            candidate("c", "two", "different", 2, None, false),
        ];
        let budget = Budget {
            candidates: 3,
            evaluations: 3,
            ..Budget::default()
        };
        let first = DiverseBeam { width: 3 }.search(&candidates, budget, 7);
        let replay = DiverseBeam { width: 3 }.search(&candidates, budget, 7);
        assert_eq!(first, replay);
        assert!(first.selected[0].baseline);
        assert!(first.selected.iter().any(|c| c.family == "two"));
    }

    #[test]
    fn branch_and_bound_prunes_only_from_exact_incumbent_and_preserves_optimum() {
        let candidates = vec![
            candidate("baseline", "base", "base", 0, Some(10), true),
            candidate("winner", "f", "a", 1, Some(3), false),
            candidate("pruned", "g", "b", 4, Some(4), false),
        ];
        let result = BranchAndBound.search(&candidates, Budget::default(), 1);
        assert_eq!(result.pruned, 1);
        assert!(result
            .selected
            .iter()
            .any(|candidate| candidate.id == "winner"));
        assert_eq!(result.quality, SearchQuality::Exact);
    }

    /// Pins `SearchAccounting.pruned`'s structural inertness (recipe-pipeline-hygiene D7): the
    /// only production caller of `BranchAndBound`, `pg-cli/src/recipe_optimize.rs`, builds every
    /// `CandidateState` with `exact_objective: None` -- neither call site there ever runs a cheap
    /// evaluation before search, so no candidate can ever populate the incumbent, and
    /// `incumbent` (initialized to `u64::MAX`) never drops below it. `lower_bound > incumbent` is
    /// then never true regardless of how the bounds are spread, so `pruned` is always `0`. This
    /// test builds candidates the same production-shaped way (varied `lower_bound`, `baseline`
    /// flag, always `exact_objective: None`) and pins that `pruned == 0` -- if a future change
    /// wires a real bound (populating `exact_objective` from an actual completed evaluation) this
    /// test's premise changes and it should be revisited, not "fixed" back to zero.
    #[test]
    fn pruned_is_structurally_zero_in_production_shaped_run() {
        use crate::recipe_registry::FAMILY_ORDERED_MORPHOPHONOLOGY;
        let candidates = vec![
            candidate(
                "baseline",
                FAMILY_ORDERED_MORPHOPHONOLOGY,
                "baseline",
                0,
                None,
                true,
            ),
            candidate("a", "one", "sig-a", 1, None, false),
            candidate("b", "two", "sig-b", 5, None, false),
            candidate("c", "three", "sig-c", 100, None, false),
        ];
        let result = BranchAndBound.search(&candidates, Budget::default(), 1);
        assert_eq!(
            result.pruned, 0,
            "no CandidateState in production ever carries exact_objective: Some(_), so the \
             incumbent can never leave u64::MAX and nothing can ever be pruned -- a nonzero \
             result here means either this test stopped being production-shaped or the field \
             stopped being structurally inert, either of which needs a human to look"
        );
    }

    #[test]
    fn evaluation_budget_exhaustion_downgrades_exact_search() {
        struct ConfirmingEvaluator;
        impl CandidateEvaluator for ConfirmingEvaluator {
            fn evaluate(
                &mut self,
                _candidate: &CandidateState,
                _remaining: Budget,
            ) -> ConfirmationEvidence {
                ConfirmationEvidence {
                    certification: Certification::FullHcConfirmed {
                        words: 1,
                        corpus_hash: "h".into(),
                    },
                    score: Some(Score {
                        states: 1,
                        arcs: 1,
                        build: 1,
                        apply: 1,
                        proposals: 1,
                        confirmation: 1,
                        confirmation_steps: 1,
                        raw_paths: 0,
                    }),
                    usage: BudgetUsage {
                        evaluations: 1,
                        ..BudgetUsage::default()
                    },
                }
            }
        }
        let candidates = vec![
            candidate("baseline", "base", "base", 1, Some(1), true),
            candidate("other", "other", "other", 2, Some(2), false),
        ];
        let search_budget = Budget {
            candidates: 2,
            evaluations: 2,
            ..Budget::default()
        };
        let evaluation_budget = Budget {
            candidates: 2,
            evaluations: 1,
            ..Budget::default()
        };
        let selected = Exhaustive.search(&candidates, search_budget, 1);
        assert_eq!(selected.quality, SearchQuality::Exact);
        let mut evaluator = ConfirmingEvaluator;
        let outcome = optimize_with_evaluator(
            &selected.selected,
            evaluation_budget,
            1,
            &Exhaustive,
            &mut evaluator,
        );
        assert_eq!(outcome.search.quality, SearchQuality::Approximate);
        assert_eq!(outcome.search.termination, Termination::BudgetExhausted);
        assert_eq!(outcome.search.explored, 1);
        assert_eq!(outcome.search.unexplored, 1);
    }

    /// The measured-overrun sibling of the test above. `evaluations` is the *only* dimension checked
    /// before the evaluator runs; `elapsed`/`build`/`memory`/`confirmation` are known only after it
    /// returns. When the breach happens on the LAST selected candidate, no candidate is left
    /// unevaluated, so the candidate-count deficit branch cannot fire — and the run used to report
    /// `Complete` while having already spent more than the caller's deadline.
    ///
    /// `quality` must nonetheless stay `Exact`, and that is not cosmetic: an `Approximate` result
    /// with `unexplored == 0` is a combination
    /// [`crate::recipe_report::RecipeOptimizationReport::validate`] REFUSES, so the
    /// fix for the termination label used to make the whole report unwritable. See the arm's own
    /// comment in `optimize_with_evaluator`.
    #[test]
    fn measured_overrun_on_the_final_candidate_still_reports_budget_exhausted() {
        struct ExpensiveEvaluator;
        impl CandidateEvaluator for ExpensiveEvaluator {
            fn evaluate(
                &mut self,
                _candidate: &CandidateState,
                _remaining: Budget,
            ) -> ConfirmationEvidence {
                ConfirmationEvidence {
                    certification: Certification::FullHcConfirmed {
                        words: 1,
                        corpus_hash: "h".into(),
                    },
                    score: Some(Score {
                        states: 1,
                        arcs: 1,
                        build: 1,
                        apply: 1,
                        proposals: 1,
                        confirmation: 1,
                        confirmation_steps: 1,
                        raw_paths: 0,
                    }),
                    usage: BudgetUsage {
                        evaluations: 1,
                        elapsed: 60,
                        ..BudgetUsage::default()
                    },
                }
            }
        }
        let candidates = vec![
            candidate("baseline", "base", "base", 1, Some(1), true),
            candidate("other", "other", "other", 2, Some(2), false),
        ];
        // 100ns deadline, no reserve: both candidates are selected and started (the second begins at
        // usage.elapsed 60 < 100), but together they measure 120ns.
        let budget = Budget {
            candidates: 2,
            evaluations: 2,
            elapsed: 100,
            ..Budget::default()
        };
        let selected = Exhaustive.search(&candidates, budget, 1);
        assert_eq!(selected.quality, SearchQuality::Exact);
        assert_eq!(selected.selected.len(), 2);
        let outcome = optimize_with_evaluator(
            &selected.selected,
            budget,
            1,
            &Exhaustive,
            &mut ExpensiveEvaluator,
        );
        assert_eq!(
            outcome.evaluated.len(),
            2,
            "no candidate was left unevaluated"
        );
        assert_eq!(outcome.usage.elapsed, 120);
        assert!(
            !budget.admits(outcome.usage),
            "the deadline was really breached"
        );
        assert_eq!(outcome.search.termination, Termination::BudgetExhausted);
        // Nothing was left unexplored, so nothing may claim otherwise -- and the pair
        // (`Approximate`, `unexplored == 0`) is exactly what
        // [`crate::recipe_report::RecipeOptimizationReport::validate`] rejects.
        assert_eq!(outcome.search.unexplored, 0);
        assert_eq!(
            outcome.search.quality,
            SearchQuality::Exact,
            "every selected candidate WAS evaluated; only the reason for stopping changed, and \
             downgrading quality here makes the report unwritable"
        );
    }

    /// `reserve` must leave real unspent `elapsed` behind, not merely bias strategy selection.
    #[test]
    fn reserve_stops_the_sweep_early_yet_never_strips_the_baseline() {
        struct FixedCostEvaluator;
        impl CandidateEvaluator for FixedCostEvaluator {
            fn evaluate(
                &mut self,
                _candidate: &CandidateState,
                _remaining: Budget,
            ) -> ConfirmationEvidence {
                ConfirmationEvidence {
                    certification: Certification::FullHcConfirmed {
                        words: 1,
                        corpus_hash: "h".into(),
                    },
                    score: Some(Score {
                        states: 1,
                        arcs: 1,
                        build: 1,
                        apply: 1,
                        proposals: 1,
                        confirmation: 1,
                        confirmation_steps: 1,
                        raw_paths: 0,
                    }),
                    usage: BudgetUsage {
                        evaluations: 1,
                        elapsed: 40,
                        ..BudgetUsage::default()
                    },
                }
            }
        }
        let candidates = vec![
            candidate("baseline", "base", "base", 1, Some(1), true),
            candidate("second", "second", "second", 2, Some(2), false),
            candidate("third", "third", "third", 3, Some(3), false),
        ];
        // 200ns deadline with a 120ns reserve leaves an 80ns sweep: two 40ns candidates fit, the
        // third is never started, and 120ns of the deadline is still unspent afterwards.
        let budget = Budget {
            elapsed: 200,
            reserve: 120,
            ..Budget::default()
        };
        let selected = Exhaustive.search(&candidates, budget, 1);
        let outcome = optimize_with_evaluator(
            &selected.selected,
            budget,
            1,
            &Exhaustive,
            &mut FixedCostEvaluator,
        );
        assert_eq!(outcome.evaluated.len(), 2);
        assert_eq!(outcome.usage.elapsed, 80);
        assert_eq!(budget.elapsed - outcome.usage.elapsed, budget.reserve);
        assert_eq!(outcome.search.termination, Termination::BudgetExhausted);

        // A reserve that swallows the entire deadline still evaluates the baseline: an optimization
        // with no baseline would violate the "baseline is always included" requirement outright.
        let starved = Budget {
            elapsed: 200,
            reserve: 200,
            ..Budget::default()
        };
        let outcome = optimize_with_evaluator(
            &selected.selected,
            starved,
            1,
            &Exhaustive,
            &mut FixedCostEvaluator,
        );
        assert_eq!(outcome.evaluated.len(), 1);
        assert!(outcome.evaluated[0].candidate.baseline);
    }

    #[test]
    fn only_full_hc_confirmed_candidates_enter_frontier_or_win() {
        let score = Score {
            states: 1,
            arcs: 1,
            build: 1,
            apply: 1,
            proposals: 1,
            confirmation: 1,
            confirmation_steps: 1,
            raw_paths: 0,
        };
        let failures = vec![
            Certification::EstimateOnly,
            Certification::BuildFailed {
                reason: "x".to_owned(),
            },
            Certification::Timeout {
                stage: "confirm".to_owned(),
            },
            Certification::Truncated {
                stage: "corpus".to_owned(),
                corpus: None,
            },
            Certification::Unsupported {
                reason: "x".to_owned(),
            },
            Certification::ResourceBreach {
                dimension: "rss".to_owned(),
                value: 2,
                limit: 1,
            },
            Certification::IdentityMismatch {
                word: "a".to_owned(),
                detail: "x".to_owned(),
            },
            Certification::MultiplicityMismatch {
                word: "a".to_owned(),
                expected: 2,
                actual: 1,
            },
        ];
        let items: Vec<_> = failures
            .into_iter()
            .enumerate()
            .map(|(i, certification)| (format!("f{i}"), certification, score))
            .collect();
        assert_eq!(select_confirmed(&items), None);
        assert!(pareto_frontier(&items).is_empty());
    }

    #[test]
    fn legacy_truncated_certification_json_deserializes_without_transitional_evidence() {
        let legacy = r#"{"status":"truncated","stage":"corpus"}"#;
        let certification: Certification =
            serde_json::from_str(legacy).expect("legacy truncated JSON must still parse");
        assert_eq!(
            certification,
            Certification::Truncated {
                stage: "corpus".to_owned(),
                corpus: None,
            }
        );
        assert_eq!(
            serde_json::to_string(&certification).expect("legacy shape must remain serializable"),
            legacy
        );
    }

    #[test]
    fn corpus_evidence_keeps_duplicate_occurrences_and_binds_reason_to_ledger_hash() {
        let requested = vec!["same".to_owned(), "same".to_owned(), "other".to_owned()];
        let included = vec!["same".to_owned()];
        let exclusions = vec![
            CorpusExclusion {
                requested_ordinal: 1,
                word: "same".to_owned(),
                reason: "corpus-row-not-prepared".to_owned(),
            },
            CorpusExclusion {
                requested_ordinal: 2,
                word: "other".to_owned(),
                reason: "oracle-timeout".to_owned(),
            },
        ];
        let evidence = CorpusCompletenessEvidence::from_selection(
            &requested,
            &included,
            exclusions.clone(),
            test_oracle_config(),
        );
        assert_eq!(
            (evidence.requested, evidence.included, evidence.excluded),
            (3, 1, 2)
        );
        assert_eq!(evidence.exclusions, exclusions);

        let changed_reason = CorpusCompletenessEvidence::from_selection(
            &requested,
            &included,
            vec![
                CorpusExclusion {
                    requested_ordinal: 1,
                    word: "same".to_owned(),
                    reason: "oracle-timeout".to_owned(),
                },
                CorpusExclusion {
                    requested_ordinal: 2,
                    word: "other".to_owned(),
                    reason: "oracle-timeout".to_owned(),
                },
            ],
            test_oracle_config(),
        );
        assert_ne!(evidence.excluded_hash, changed_reason.excluded_hash);

        // The generating CONFIGURATION is part of the ledger hash too: same words, same exclusions,
        // a different oracle step cap must not hash the same, or two runs at different caps produce
        // indistinguishable evidence -- the defect this field exists to close.
        let changed_cap = CorpusCompletenessEvidence::from_selection(
            &requested,
            &included,
            exclusions.clone(),
            OracleEligibilityConfig {
                step_cap: 40_000,
                ..test_oracle_config()
            },
        );
        assert_ne!(evidence.excluded_hash, changed_cap.excluded_hash);
    }

    #[test]
    #[should_panic(expected = "corpus evidence must account for every requested occurrence")]
    fn corpus_evidence_constructor_rejects_unaccounted_occurrences() {
        CorpusCompletenessEvidence::from_selection(
            &["a".to_owned(), "b".to_owned()],
            &[],
            vec![CorpusExclusion {
                requested_ordinal: 1,
                word: "b".to_owned(),
                reason: "missing".to_owned(),
            }],
            test_oracle_config(),
        );
    }

    #[test]
    #[should_panic(expected = "corpus exclusions must be in strictly increasing requested order")]
    fn corpus_evidence_constructor_rejects_non_deterministic_exclusion_order() {
        CorpusCompletenessEvidence::from_selection(
            &["a".to_owned(), "b".to_owned()],
            &[],
            vec![
                CorpusExclusion {
                    requested_ordinal: 1,
                    word: "b".to_owned(),
                    reason: "missing".to_owned(),
                },
                CorpusExclusion {
                    requested_ordinal: 0,
                    word: "a".to_owned(),
                    reason: "missing".to_owned(),
                },
            ],
            test_oracle_config(),
        );
    }

    /// The oracle configuration these constructor tests declare. Any value works -- what matters is
    /// that the constructor now REQUIRES one, so no evidence can exist without stating the bounds it
    /// was derived under.
    fn test_oracle_config() -> OracleEligibilityConfig {
        OracleEligibilityConfig {
            step_cap: 20_000,
            memory_ceiling_bytes: 12 * 1024 * 1024 * 1024,
            liveness_net_ns: 300_000_000_000,
        }
    }

    #[test]
    fn pareto_frontier_and_lexicographic_winner_are_deterministic() {
        let confirmed = Certification::FullHcConfirmed {
            words: 4,
            corpus_hash: "h".to_owned(),
        };
        let items = vec![
            (
                "large-fast".to_owned(),
                confirmed.clone(),
                Score {
                    states: 10,
                    arcs: 10,
                    build: 1,
                    apply: 1,
                    proposals: 1,
                    confirmation: 1,
                    confirmation_steps: 1,
                    raw_paths: 0,
                },
            ),
            (
                "small-slow".to_owned(),
                confirmed,
                Score {
                    states: 2,
                    arcs: 2,
                    build: 9,
                    apply: 9,
                    proposals: 9,
                    confirmation: 9,
                    confirmation_steps: 9,
                    raw_paths: 0,
                },
            ),
        ];
        // Both stay on the frontier: neither dominates the other, since one is smaller and the other
        // does less work. That is unchanged by the ranking policy.
        assert_eq!(pareto_frontier(&items), vec!["large-fast", "small-slow"]);
        // `large-fast`, and this assertion REVERSED deliberately -- it is the clearest statement of
        // what the objective now optimizes. These two fixtures are built to disagree: `small-slow` has
        // a 5x smaller network (4 vs 20) but does 9x the confirmation work (9 calls vs 1).
        //
        // The old key ranked `states + arcs` first and so chose `small-slow` -- the smaller net,
        // regardless of what it costs to use. That preference is wrong for this project twice over.
        // First, a smaller FST is not a better one: the one documented case where a candidate's net
        // shrank sharply, it had shrunk because it was MISSING the material that makes the grammar
        // work. Second, and measured: on a marker-free fixture two candidates compiled to identical
        // 11 states / 13 arcs while doing 2 and 4 confirmation calls respectively, and a size-first
        // key tied them and fell through to build time -- naming the candidate that does twice the
        // work. Confirmation work is the cost that dominates propose->confirm, so it ranks first now
        // and size is only a tiebreak beneath it.
        assert_eq!(select_confirmed(&items), Some("large-fast".to_owned()));
    }

    /// The motivating case, pinned as a synthetic fixture: the measured Sena shape where the
    /// plan-composed candidate proposes several-fold more (higher `raw_paths`) for a marginally
    /// LOWER confirm-step count, and the old steps-only key picked it on that alone. Measured
    /// four-corpus numbers: plan-composed 575 proposals / 42 confirmation calls /
    /// 1192 confirmation_steps, vs hand-spun 127 proposals / 17 calls / 1252 steps. `raw_paths`
    /// (pre-dedup traversal, necessarily >= the post-dedup `proposals` count) is not one of the
    /// measured columns, so this fixture invents illustrative values consistent with "several-fold
    /// more propose-side work" and asserts the new key reverses the old steps-only preference.
    #[test]
    fn sena_shaped_lower_total_work_wins_despite_higher_confirmation_steps() {
        let confirmed = Certification::FullHcConfirmed {
            words: 20,
            corpus_hash: "sena-shape".to_owned(),
        };
        let plan_composed = Score {
            states: 100,
            arcs: 200,
            build: 1,
            apply: 1,
            proposals: 575,
            confirmation: 42,
            confirmation_steps: 1192,
            // Several-fold more propose-side traversal than the hand-spun candidate below -- the
            // cost chunk fusion hides from `confirmation_steps` alone.
            raw_paths: 2000,
        };
        let hand_spun = Score {
            states: 100,
            arcs: 200,
            build: 1,
            apply: 1,
            proposals: 127,
            confirmation: 17,
            confirmation_steps: 1252,
            raw_paths: 400,
        };
        // Under the OLD key (confirmation_steps alone, ascending) `plan_composed` would win: 1192 <
        // 1252. Confirm that fact directly, so this test also documents the regression it guards.
        assert!(plan_composed.confirmation_steps < hand_spun.confirmation_steps);
        // Under the NEW key the combined leading term reverses that: 1192 + 2000 = 3192 for
        // `plan_composed` against 1252 + 400 = 1652 for `hand_spun` -- lower total work wins.
        let items = vec![
            ("plan-composed".to_owned(), confirmed.clone(), plan_composed),
            ("hand-spun".to_owned(), confirmed, hand_spun),
        ];
        assert_eq!(select_confirmed(&items), Some("hand-spun".to_owned()));
    }

    /// The measured Indonesian shape: one candidate is better-or-equal on every deterministic work
    /// metric (states+arcs, proposals, confirmation calls, confirmation_steps, and raw_paths) and
    /// strictly better on at least one. A dominant winner must be selected regardless of the new
    /// `raw_paths` term -- adding it must never flip an outcome that was already unambiguous.
    #[test]
    fn dominant_on_every_metric_still_wins_with_raw_paths_in_the_key() {
        let confirmed = Certification::FullHcConfirmed {
            words: 10,
            corpus_hash: "indonesian-shape".to_owned(),
        };
        let dominant = Score {
            states: 50,
            arcs: 100,
            build: 1,
            apply: 1,
            proposals: 200,
            confirmation: 10,
            confirmation_steps: 300,
            raw_paths: 500,
        };
        let dominated = Score {
            states: 60,
            arcs: 120,
            build: 1,
            apply: 1,
            proposals: 250,
            confirmation: 12,
            confirmation_steps: 350,
            raw_paths: 600,
        };
        let items = vec![
            ("dominant".to_owned(), confirmed.clone(), dominant),
            ("dominated".to_owned(), confirmed, dominated),
        ];
        assert_eq!(select_confirmed(&items), Some("dominant".to_owned()));
        // A dominated candidate is never on the frontier either.
        assert_eq!(pareto_frontier(&items), vec!["dominant".to_owned()]);
    }

    #[test]
    fn pareto_dominance_counts_confirmation_steps() {
        let confirmed = Certification::FullHcConfirmed {
            words: 1,
            corpus_hash: "d4-steps".to_owned(),
        };
        let lower_step_work = Score {
            states: 10,
            arcs: 10,
            build: 99,
            apply: 99,
            proposals: 10,
            confirmation: 10,
            confirmation_steps: 10,
            raw_paths: 10,
        };
        let higher_step_work = Score {
            confirmation_steps: 11,
            ..lower_step_work
        };
        let items = vec![
            (
                "lower-step-work".to_owned(),
                confirmed.clone(),
                lower_step_work,
            ),
            ("higher-step-work".to_owned(), confirmed, higher_step_work),
        ];

        assert_eq!(pareto_frontier(&items), vec!["lower-step-work".to_owned()]);
    }

    #[test]
    fn pareto_dominance_counts_raw_proposer_paths() {
        let confirmed = Certification::FullHcConfirmed {
            words: 1,
            corpus_hash: "d4-raw-paths".to_owned(),
        };
        let fewer_raw_paths = Score {
            states: 10,
            arcs: 10,
            build: 99,
            apply: 99,
            proposals: 10,
            confirmation: 10,
            confirmation_steps: 10,
            raw_paths: 10,
        };
        let more_raw_paths = Score {
            raw_paths: 11,
            ..fewer_raw_paths
        };
        let items = vec![
            (
                "fewer-raw-paths".to_owned(),
                confirmed.clone(),
                fewer_raw_paths,
            ),
            ("more-raw-paths".to_owned(), confirmed, more_raw_paths),
        ];

        assert_eq!(pareto_frontier(&items), vec!["fewer-raw-paths".to_owned()]);
    }

    #[test]
    fn pareto_dominance_uses_each_deterministic_coordinate_componentwise() {
        let confirmed = Certification::FullHcConfirmed {
            words: 1,
            corpus_hash: "d4-componentwise".to_owned(),
        };
        let lower = Score {
            states: 10,
            arcs: 10,
            build: 999,
            apply: 999,
            proposals: 10,
            confirmation: 10,
            confirmation_steps: 10,
            raw_paths: 10,
        };
        let cases = [
            (
                "confirmation-steps",
                Score {
                    confirmation_steps: 11,
                    build: 1,
                    apply: 1,
                    ..lower
                },
            ),
            (
                "raw-paths",
                Score {
                    raw_paths: 11,
                    build: 1,
                    apply: 1,
                    ..lower
                },
            ),
            (
                "confirmation",
                Score {
                    confirmation: 11,
                    build: 1,
                    apply: 1,
                    ..lower
                },
            ),
            (
                "proposals",
                Score {
                    proposals: 11,
                    build: 1,
                    apply: 1,
                    ..lower
                },
            ),
            (
                "states",
                Score {
                    states: 11,
                    build: 1,
                    apply: 1,
                    ..lower
                },
            ),
            (
                "arcs",
                Score {
                    arcs: 11,
                    build: 1,
                    apply: 1,
                    ..lower
                },
            ),
        ];

        for (coordinate, higher) in cases {
            let items = vec![
                ("lower".to_owned(), confirmed.clone(), lower),
                (format!("higher-{coordinate}"), confirmed.clone(), higher),
            ];
            assert_eq!(
                pareto_frontier(&items),
                vec!["lower".to_owned()],
                "coordinate {coordinate} must participate in componentwise dominance"
            );
        }
    }

    #[test]
    fn pareto_dominance_excludes_build_and_apply_timing() {
        let confirmed = Certification::FullHcConfirmed {
            words: 1,
            corpus_hash: "d4-time".to_owned(),
        };
        let slower = Score {
            states: 10,
            arcs: 10,
            // These two fields are wall-clock diagnostics. They must not create dominance when
            // every deterministic coordinate is tied.
            build: 999,
            apply: 999,
            proposals: 10,
            confirmation: 10,
            confirmation_steps: 10,
            raw_paths: 10,
        };
        let faster = Score {
            states: 10,
            arcs: 10,
            build: 1,
            apply: 1,
            proposals: 10,
            confirmation: 10,
            confirmation_steps: 10,
            raw_paths: 10,
        };
        let items = vec![
            ("slower".to_owned(), confirmed.clone(), slower),
            ("faster".to_owned(), confirmed, faster),
        ];

        assert_eq!(
            pareto_frontier(&items),
            vec!["faster".to_owned(), "slower".to_owned()]
        );
    }

    #[test]
    fn uncertified_candidate_cannot_dominate_a_certified_frontier_member() {
        let certified = Certification::FullHcConfirmed {
            words: 1,
            corpus_hash: "d4-certified".to_owned(),
        };
        let certified_score = Score {
            states: 10,
            arcs: 10,
            build: 10,
            apply: 10,
            proposals: 10,
            confirmation: 10,
            confirmation_steps: 10,
            raw_paths: 10,
        };
        let cheaper_but_uncertified = Score {
            states: 1,
            arcs: 1,
            build: 1,
            apply: 1,
            proposals: 1,
            confirmation: 1,
            confirmation_steps: 1,
            raw_paths: 1,
        };
        let items = vec![
            ("certified".to_owned(), certified, certified_score),
            (
                "estimate-only".to_owned(),
                Certification::EstimateOnly,
                cheaper_but_uncertified,
            ),
        ];

        assert_eq!(pareto_frontier(&items), vec!["certified".to_owned()]);
        assert_eq!(select_confirmed(&items), Some("certified".to_owned()));
    }

    /// Backward compatibility: a report written before `raw_paths` existed has no such key in its
    /// JSON at all. `#[serde(default)]` must resolve that absence to `0` -- "not measured", not a
    /// deserialization failure -- mirroring the same convention already documented on
    /// `confirmation_steps`.
    #[test]
    fn score_without_raw_paths_deserializes_with_zero_default() {
        let json = r#"{
            "states": 11,
            "arcs": 13,
            "build": 5,
            "apply": 7,
            "proposals": 3,
            "confirmation": 2,
            "confirmation_steps": 9
        }"#;
        let score: Score = serde_json::from_str(json).expect("legacy Score JSON must still parse");
        assert_eq!(score.raw_paths, 0);
        assert_eq!(
            score,
            Score {
                states: 11,
                arcs: 13,
                build: 5,
                apply: 7,
                proposals: 3,
                confirmation: 2,
                confirmation_steps: 9,
                raw_paths: 0,
            }
        );
    }
}
