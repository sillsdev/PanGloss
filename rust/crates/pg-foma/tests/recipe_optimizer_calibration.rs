use pg_foma::recipe_optimizer::*;
use std::collections::BTreeMap;

#[derive(Clone)]
struct SyntheticSpace {
    name: &'static str,
    candidates: Vec<CandidateState>,
    tractable: bool,
    topology: ConstraintTopology,
    pilot: PilotCosts,
}

struct SyntheticEvaluator {
    objective: BTreeMap<String, u64>,
    evaluations: Vec<String>,
}

impl CandidateEvaluator for SyntheticEvaluator {
    fn evaluate(&mut self, candidate: &CandidateState, _remaining: Budget) -> ConfirmationEvidence {
        self.evaluations.push(candidate.id.clone());
        let objective = self.objective[&candidate.id];
        ConfirmationEvidence {
            certification: Certification::FullHcConfirmed {
                words: 1,
                corpus_hash: "synthetic".into(),
            },
            score: Some(Score {
                states: objective / 2,
                arcs: objective - objective / 2,
                build: 1,
                apply: 1,
                proposals: 1,
                confirmation: 1,
            }),
            usage: BudgetUsage {
                candidates: 1,
                evaluations: 1,
                elapsed: candidate.lower_bound,
                ..BudgetUsage::default()
            },
        }
    }
}

fn candidate(
    id: String,
    family: String,
    bound: u64,
    objective: u64,
    baseline: bool,
) -> CandidateState {
    CandidateState {
        signature: family.clone(),
        id,
        family,
        lower_bound: bound,
        exact_objective: Some(objective),
        baseline,
    }
}

fn space(
    name: &'static str,
    count: usize,
    winner: usize,
    tractable: bool,
    topology: ConstraintTopology,
    pilot: PilotCosts,
) -> SyntheticSpace {
    let candidates = (0..count)
        .map(|i| {
            let objective = if i == winner {
                2
            } else {
                20 + ((i * 7) % 31) as u64
            };
            candidate(
                format!("{name}-{i:04}"),
                format!("family-{}", i % 8),
                objective.min(10),
                objective,
                i == 0,
            )
        })
        .collect();
    SyntheticSpace {
        name,
        candidates,
        tractable,
        topology,
        pilot,
    }
}

fn spaces() -> Vec<SyntheticSpace> {
    vec![
        space(
            "small",
            12,
            7,
            true,
            ConstraintTopology {
                strong_pruning: false,
                compositional: false,
            },
            PilotCosts { p50: 2, p95: 2 },
        ),
        space(
            "medium",
            80,
            61,
            true,
            ConstraintTopology {
                strong_pruning: false,
                compositional: false,
            },
            PilotCosts { p50: 2, p95: 2 },
        ),
        space(
            "huge-raw-small-static-pruned-large",
            500,
            477,
            false,
            ConstraintTopology {
                strong_pruning: true,
                compositional: true,
            },
            PilotCosts { p50: 4, p95: 4 },
        ),
        space(
            "weak-pruning",
            120,
            91,
            false,
            ConstraintTopology {
                strong_pruning: false,
                compositional: false,
            },
            PilotCosts { p50: 3, p95: 3 },
        ),
        space(
            "strong-pruning",
            120,
            111,
            false,
            ConstraintTopology {
                strong_pruning: true,
                compositional: false,
            },
            PilotCosts { p50: 3, p95: 3 },
        ),
        space(
            "misleading-cheap-fidelity",
            120,
            109,
            false,
            ConstraintTopology {
                strong_pruning: false,
                compositional: false,
            },
            PilotCosts { p50: 1, p95: 1 },
        ),
    ]
}

fn objective(outcome: &OptimizationOutcome) -> Option<u64> {
    outcome.winner.as_ref().map(|id| {
        outcome
            .evaluated
            .iter()
            .find(|x| &x.candidate.id == id)
            .unwrap()
            .evidence
            .score
            .unwrap()
            .scalar_objective()
    })
}

#[test]
fn calibration_is_deterministic_and_reports_metrics_without_successive_halving() {
    for space in spaces() {
        let budget = Budget {
            candidates: if space.tractable { u64::MAX } else { 24 },
            evaluations: if space.tractable { u64::MAX } else { 24 },
            elapsed: 100,
            ..Budget::default()
        };
        let strategy = choose_strategy(
            space.candidates.len() as u64,
            space.pilot,
            budget,
            space.topology,
        );
        let oracle = if space.tractable {
            let mut evaluator = SyntheticEvaluator {
                objective: space
                    .candidates
                    .iter()
                    .map(|c| (c.id.clone(), c.exact_objective.unwrap()))
                    .collect(),
                evaluations: Vec::new(),
            };
            Some(optimize_with_evaluator(
                &space.candidates,
                Budget::default(),
                41,
                &Exhaustive,
                &mut evaluator,
            ))
        } else {
            None
        };
        let run = |seed| {
            let mut evaluator = SyntheticEvaluator {
                objective: space
                    .candidates
                    .iter()
                    .map(|c| (c.id.clone(), c.exact_objective.unwrap()))
                    .collect(),
                evaluations: Vec::new(),
            };
            let strategy_impl = DefaultStrategyRegistry::default().get(strategy).unwrap();
            (
                optimize_with_evaluator(
                    &space.candidates,
                    budget,
                    seed,
                    strategy_impl.as_ref(),
                    &mut evaluator,
                ),
                evaluator.evaluations,
            )
        };
        let (first, first_evals) = run(41);
        let (replay, replay_evals) = run(41);
        assert_eq!(first, replay, "{name} replay changed", name = space.name);
        assert_eq!(first_evals, replay_evals);
        assert!(first.usage.evaluations <= budget.evaluations);
        if let Some(ref oracle) = oracle {
            assert_eq!(
                objective(&first),
                objective(&oracle),
                "{name} lost tractable optimum",
                name = space.name
            );
        }
        let coverage = first.winner.is_some() as u64;
        let regret = match (objective(&first), oracle.as_ref().and_then(objective)) {
            (Some(actual), Some(best)) => actual.saturating_sub(best),
            _ => 0,
        };
        println!("CALIBRATION name={} strategy={:?} coverage={} regret={} evaluations={} pruned={} termination={:?} operations={} successive_halving=disabled", space.name, strategy, coverage, regret, first.usage.evaluations, first.search.pruned, first.search.termination, first.search.generated + first.search.expanded + first.search.pruned);
    }
}
