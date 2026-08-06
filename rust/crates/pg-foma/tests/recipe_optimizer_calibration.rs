use pg_foma::recipe_optimizer::*;
use pg_foma::recipe_space::deterministic_sample_indices;
use std::collections::BTreeMap;

#[derive(Clone)]
struct ReducedFixture {
    name: &'static str,
    candidates: Vec<CandidateState>,
}

struct SyntheticEvaluator {
    objective: BTreeMap<String, u64>,
}

fn beam_threshold_fixture() -> ReducedFixture {
    let candidates = (0..32)
        .map(|i| {
            let winner = i == 23;
            CandidateState {
                id: format!("threshold-32-{i:04}"),
                family: format!("threshold-family-{i:04}"),
                signature: format!("threshold-signature-{i:04}"),
                lower_bound: if i < 14 {
                    1
                } else if winner {
                    2
                } else {
                    10
                },
                exact_objective: Some(if winner { 2 } else { 20 + i as u64 }),
                baseline: i == 0,
            }
        })
        .collect();
    ReducedFixture {
        name: "threshold-32",
        candidates,
    }
}

impl CandidateEvaluator for SyntheticEvaluator {
    fn evaluate(&mut self, candidate: &CandidateState, _remaining: Budget) -> ConfirmationEvidence {
        let objective = self.objective[&candidate.id];
        ConfirmationEvidence {
            certification: Certification::FullHcConfirmed {
                words: 1,
                corpus_hash: "calibration-ground-truth".into(),
            },
            score: Some(Score {
                states: objective / 2,
                arcs: objective - objective / 2,
                build: 1,
                apply: 1,
                proposals: 1,
                confirmation: 1,
                confirmation_steps: 1,
                raw_paths: 0,
            }),
            usage: BudgetUsage {
                candidates: 1,
                evaluations: 1,
                elapsed: candidate.lower_bound.max(1),
                ..BudgetUsage::default()
            },
        }
    }
}

fn reduced_fixture(
    name: &'static str,
    count: usize,
    winner: usize,
    misleading_bounds: bool,
) -> ReducedFixture {
    let candidates = (0..count)
        .map(|i| {
            let objective = if i == winner {
                if misleading_bounds {
                    12
                } else {
                    2
                }
            } else {
                20 + ((i * 7) % 31) as u64
            };
            CandidateState {
                id: format!("{name}-{i:04}"),
                family: format!("family-{}", i % 8),
                signature: format!("signature-{}", i % 13),
                lower_bound: if misleading_bounds {
                    if i == winner {
                        10
                    } else {
                        1 + (i % 3) as u64
                    }
                } else {
                    objective.min(10)
                },
                exact_objective: Some(objective),
                baseline: i == 0,
            }
        })
        .collect();
    ReducedFixture { name, candidates }
}

fn evaluate(
    fixture: &ReducedFixture,
    strategy: &dyn SearchStrategy,
    budget: Budget,
    seed: u64,
) -> OptimizationOutcome {
    let mut evaluator = SyntheticEvaluator {
        objective: fixture
            .candidates
            .iter()
            .map(|candidate| (candidate.id.clone(), candidate.exact_objective.unwrap()))
            .collect(),
    };
    optimize_with_evaluator(&fixture.candidates, budget, seed, strategy, &mut evaluator)
}

fn objective(outcome: &OptimizationOutcome) -> Option<u64> {
    outcome.winner.as_ref().map(|id| {
        outcome
            .evaluated
            .iter()
            .find(|item| &item.candidate.id == id)
            .unwrap()
            .evidence
            .score
            .unwrap()
            .scalar_objective()
    })
}

fn operations(outcome: &OptimizationOutcome) -> u64 {
    outcome.search.generated + outcome.search.expanded + outcome.search.pruned
}

fn quantile(values: &[u64], percentile: usize) -> u64 {
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let index = ((sorted.len() - 1) * percentile).div_ceil(100);
    sorted[index]
}

#[test]
fn approximate_strategies_are_compared_with_exhaustive_ground_truth() {
    let fixtures = [
        reduced_fixture("aligned-32", 32, 23, false),
        beam_threshold_fixture(),
        reduced_fixture("weak-signal-32", 32, 29, true),
    ];
    let seeds = [11, 41, 73];
    let widths = [4, 8, 16, 24];

    for fixture in fixtures {
        let oracle = evaluate(&fixture, &Exhaustive, Budget::default(), 41);
        let oracle_id = oracle.winner.clone().unwrap();
        let oracle_objective = objective(&oracle).unwrap();
        assert_eq!(oracle.search.quality, SearchQuality::Exact);

        for width in widths {
            let mut optimum_hits = 0;
            let mut total_regret = 0;
            let mut total_operations = 0;
            let mut total_work = 0;
            for seed in seeds {
                let budget = Budget {
                    candidates: width,
                    evaluations: width,
                    ..Budget::default()
                };
                let run = evaluate(
                    &fixture,
                    &DiverseBeam {
                        width: width as usize,
                    },
                    budget,
                    seed,
                );
                optimum_hits += u64::from(run.winner.as_ref() == Some(&oracle_id));
                total_regret += objective(&run).unwrap().saturating_sub(oracle_objective);
                total_operations += operations(&run);
                total_work += run.usage.elapsed;
                let replay = evaluate(
                    &fixture,
                    &DiverseBeam {
                        width: width as usize,
                    },
                    budget,
                    seed,
                );
                assert_eq!(run, replay, "seeded replay changed");
            }
            println!(
                "BEAM fixture={} width={} optimum_coverage={}/{} mean_regret={} mean_operations={} mean_work={}",
                fixture.name,
                width,
                optimum_hits,
                seeds.len(),
                total_regret / seeds.len() as u64,
                total_operations / seeds.len() as u64,
                total_work / seeds.len() as u64,
            );

            if fixture.name == "aligned-32" && width == 16 {
                assert_eq!(optimum_hits, seeds.len() as u64);
                assert_eq!(total_regret, 0);
            }
            if fixture.name == "threshold-32" {
                if width < 16 {
                    assert_eq!(
                        optimum_hits, 0,
                        "narrow beam unexpectedly found threshold optimum"
                    );
                    assert!(total_regret > 0);
                } else {
                    assert_eq!(optimum_hits, seeds.len() as u64);
                    assert_eq!(total_regret, 0);
                }
            }
            if fixture.name == "weak-signal-32" {
                assert!(
                    optimum_hits < seeds.len() as u64,
                    "an admissible but weak lower bound must expose approximation risk"
                );
            }
        }

        println!(
            "EXHAUSTIVE fixture={} optimum_coverage=1/1 regret=0 operations={} work={}",
            fixture.name,
            operations(&oracle),
            oracle.usage.elapsed
        );
    }
}

#[test]
fn branch_and_bound_preserves_the_exhaustive_optimum_when_exact_bounds_prune() {
    let fixture = reduced_fixture("strong-pruning-120", 120, 111, false);
    let oracle = evaluate(&fixture, &Exhaustive, Budget::default(), 41);
    let bounded = evaluate(&fixture, &BranchAndBound, Budget::default(), 41);
    assert_eq!(bounded.winner, oracle.winner);
    assert_eq!(objective(&bounded), objective(&oracle));
    assert_eq!(bounded.search.quality, SearchQuality::Exact);
    assert!(bounded.search.pruned >= 100);
    assert!(bounded.usage.evaluations < oracle.usage.evaluations / 4);
    println!(
        "BRANCH fixture={} optimum_coverage=1/1 regret=0 evaluations={}/{} pruned={} operations={}/{} work={}/{}",
        fixture.name,
        bounded.usage.evaluations,
        oracle.usage.evaluations,
        bounded.search.pruned,
        operations(&bounded),
        operations(&oracle),
        bounded.usage.elapsed,
        oracle.usage.elapsed,
    );
}

#[test]
fn pilot_beam_reserve_and_selector_defaults_have_checked_in_calibration() {
    let policy = AdaptivePolicy::default();
    assert_eq!(policy.pilot_candidate_cap, 8);
    assert_eq!(policy.beam_width, 16);
    assert_eq!(
        (
            policy.exhaustive_budget_numerator,
            policy.exhaustive_budget_denominator
        ),
        (1, 2)
    );

    let costs = [
        11, 12, 12, 13, 13, 14, 14, 15, 15, 16, 16, 17, 17, 18, 19, 20, 21, 22, 24, 26, 28, 31, 34,
        38, 43, 49, 56, 64, 73, 83, 94, 106,
    ];
    let population_p95 = quantile(&costs, 95);
    for sample_size in [4, 8, 16] {
        let errors = [11, 41, 73, 101, 149]
            .into_iter()
            .map(|seed| {
                let sample = deterministic_sample_indices(costs.len(), sample_size, seed)
                    .into_iter()
                    .map(|index| costs[index])
                    .collect::<Vec<_>>();
                quantile(&sample, 95).abs_diff(population_p95)
            })
            .collect::<Vec<_>>();
        println!(
            "PILOT size={} population_p95={} median_abs_error={} max_abs_error={}",
            sample_size,
            population_p95,
            quantile(&errors, 50),
            errors.iter().max().unwrap()
        );
    }

    let budget = Budget {
        elapsed: 160,
        ..Budget::default()
    };
    let weak = ConstraintTopology {
        strong_pruning: false,
        compositional: false,
    };
    let strong = ConstraintTopology {
        strong_pruning: true,
        compositional: false,
    };
    assert_eq!(
        choose_strategy_with_policy(32, PilotCosts { p50: 1, p95: 2 }, budget, weak, policy),
        Strategy::Exhaustive
    );
    let reserved = Budget {
        reserve: 40,
        ..budget
    };
    assert_eq!(
        choose_strategy_with_policy(32, PilotCosts { p50: 1, p95: 2 }, reserved, weak, policy),
        Strategy::DiverseBeam
    );
    assert_eq!(
        choose_strategy_with_policy(80, PilotCosts { p50: 2, p95: 2 }, budget, strong, policy),
        Strategy::BranchAndBound
    );
    assert_eq!(
        choose_strategy_with_policy(80, PilotCosts { p50: 2, p95: 2 }, budget, weak, policy),
        Strategy::DiverseBeam
    );
    // Reserve calibration is measured by running the production search loop, not by re-deriving the allocation with local arithmetic.
    let total = 160u64;
    let confirmation_demand = 40u64;
    let sweep: Vec<CandidateState> = (0..13)
        .map(|i| CandidateState {
            id: format!("reserve-{i:04}"),
            family: format!("reserve-family-{i:04}"),
            signature: format!("reserve-signature-{i:04}"),
            lower_bound: 10,
            exact_objective: Some(20 + i as u64),
            baseline: i == 0,
        })
        .collect();
    let search_demand: u64 = sweep.iter().map(|c| c.lower_bound).sum();
    let mut smallest_successful = None;
    for (numerator, denominator) in [(0, 1), (1, 8), (1, 4), (1, 2)] {
        let mut evaluator = SyntheticEvaluator {
            objective: sweep
                .iter()
                .map(|c| (c.id.clone(), c.exact_objective.unwrap()))
                .collect(),
        };
        let outcome = optimize_with_evaluator(
            &sweep,
            Budget {
                elapsed: total,
                reserve: total * numerator / denominator,
                ..Budget::default()
            },
            17,
            &Exhaustive,
            &mut evaluator,
        );
        let search_used = outcome.usage.elapsed;
        let confirmation_available = total.saturating_sub(search_used);
        let confirmed = confirmation_available >= confirmation_demand;
        if confirmed && smallest_successful.is_none() {
            smallest_successful = Some((numerator, denominator));
        }
        // A reserve that genuinely stops the sweep early must say so: budget-limited and approximate, never a clean Exact.
        if search_used < search_demand {
            assert_eq!(outcome.search.quality, SearchQuality::Approximate);
            assert_eq!(outcome.search.termination, Termination::BudgetExhausted);
        } else {
            assert_eq!(outcome.search.quality, SearchQuality::Exact);
            assert_eq!(outcome.search.termination, Termination::Complete);
        }
        // The reserve is never encroached on, and the baseline is always evaluated regardless.
        assert!(search_used <= total * (denominator - numerator) / denominator);
        assert!(outcome.evaluated.iter().any(|e| e.candidate.baseline));
        println!(
            "RESERVE fraction={}/{} search_coverage={}/{} confirmation_available={} confirmation_required={} confirmed={}",
            numerator,
            denominator,
            search_used,
            search_demand,
            confirmation_available,
            confirmation_demand,
            confirmed
        );
    }
    assert_eq!(smallest_successful, Some((1, 4)));

    println!(
        "SELECTOR exhaustive_fraction=1/2 finite_reserve_fraction=1/4 pilot_cap=8 beam_width=16 strong_topology=branch-and-bound weak_topology=diverse-beam successive_halving=disabled"
    );
}
