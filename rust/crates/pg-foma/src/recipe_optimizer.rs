//! Deterministic, budgeted search and confirmed-only ranking for compilation recipes.
//! Candidate construction and HC execution are injected through [`CandidateEvaluator`], while this
//! module owns search policy, budget enforcement, certification boundaries, Pareto ranking, and
//! replay semantics.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

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
}

impl Score {
    pub fn key(&self, id: &str) -> (u64, u64, u64, u64, u64, String) {
        (
            self.states.saturating_add(self.arcs),
            self.build,
            self.apply,
            self.proposals,
            self.confirmation,
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
    let mut usage = BudgetUsage::default();
    let mut evaluated = Vec::new();
    for candidate in &search.selected {
        if usage.evaluations >= budget.evaluations {
            break;
        }
        let remaining = Budget {
            candidates: budget.candidates.saturating_sub(usage.candidates),
            evaluations: budget.evaluations.saturating_sub(usage.evaluations),
            elapsed: budget.elapsed.saturating_sub(usage.elapsed),
            build: budget.build.saturating_sub(usage.build),
            memory: budget.memory.saturating_sub(usage.memory_peak),
            confirmation: budget.confirmation.saturating_sub(usage.confirmation),
            reserve: budget
                .reserve
                .min(budget.elapsed.saturating_sub(usage.elapsed)),
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
    let left = [
        left.states,
        left.arcs,
        left.build,
        left.apply,
        left.proposals,
        left.confirmation,
    ];
    let right = [
        right.states,
        right.arcs,
        right.build,
        right.apply,
        right.proposals,
        right.confirmation,
    ];
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

    #[test]
    fn only_full_hc_confirmed_candidates_enter_frontier_or_win() {
        let score = Score {
            states: 1,
            arcs: 1,
            build: 1,
            apply: 1,
            proposals: 1,
            confirmation: 1,
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
                },
            ),
        ];
        assert_eq!(pareto_frontier(&items), vec!["large-fast", "small-slow"]);
        assert_eq!(select_confirmed(&items), Some("small-slow".to_owned()));
    }
}
