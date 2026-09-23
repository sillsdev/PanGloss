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
                    confirmation_steps: 1,
                    raw_paths: 0,
                }),
                usage: BudgetUsage {
                    evaluations: 1,
                    ..BudgetUsage::default()
                },
                production_blocks_publication: false,
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

/// The measured-overrun sibling of the test above: when the breach happens on the last selected candidate, `quality` must stay `Exact` (see docs/research/pg-foma-recipe-optimizer-design-notes.md for why `Approximate` here made the report unwritable).
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
                production_blocks_publication: false,
            }
        }
    }
    let candidates = vec![
        candidate("baseline", "base", "base", 1, Some(1), true),
        candidate("other", "other", "other", 2, Some(2), false),
    ];
    // 100ns deadline, no reserve: both candidates start (the second at usage.elapsed 60 < 100), but together measure 120ns.
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
    // Nothing was left unexplored: (Approximate, unexplored == 0) is the combination BackendOptimizationReport::validate returns Err for.
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
                production_blocks_publication: false,
            }
        }
    }
    let candidates = vec![
        candidate("baseline", "base", "base", 1, Some(1), true),
        candidate("second", "second", "second", 2, Some(2), false),
        candidate("third", "third", "third", 3, Some(3), false),
    ];
    // 200ns deadline with a 120ns reserve leaves an 80ns sweep: two 40ns candidates fit, the third never starts, and 120ns stays unspent.
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

    // A reserve that swallows the entire deadline still evaluates the baseline; an optimization with no baseline would violate that requirement outright.
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
            direction: crate::parity::IdentityMismatchDirection::Both,
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

    // The generating configuration is part of the ledger hash too: same words and exclusions but a different oracle step cap must not hash the same.
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

/// The oracle configuration these constructor tests declare; any value works, what matters is that the constructor requires one.
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
    // Both stay on the frontier: neither dominates, one is smaller and the other does less work.
    assert_eq!(pareto_frontier(&items), vec!["large-fast", "small-slow"]);
    // `large-fast` wins deliberately over the smaller `small-slow`: confirmation work dominates propose->confirm cost, so it ranks first and size is only a tiebreak beneath it (a smaller FST is not a better one -- the old size-first key could tie two candidates and fall through to build time, naming whichever did twice the confirmation work).
    assert_eq!(select_confirmed(&items), Some("large-fast".to_owned()));
}

/// The motivating Sena-shaped case: a plan-composed candidate proposes several-fold more (higher `raw_paths`) for a marginally lower confirm-step count, and the old steps-only key picked it on that alone; asserts the new key reverses that preference.
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
        // Several-fold more propose-side traversal than hand-spun below -- a cost chunk fusion hides from confirmation_steps alone.
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
    // Under the old key (confirmation_steps alone) plan_composed would win: 1192 < 1252.
    assert!(plan_composed.confirmation_steps < hand_spun.confirmation_steps);
    // Under the new combined key that reverses: 3192 for plan_composed vs 1652 for hand_spun -- lower total work wins.
    let items = vec![
        ("plan-composed".to_owned(), confirmed.clone(), plan_composed),
        ("hand-spun".to_owned(), confirmed, hand_spun),
    ];
    assert_eq!(select_confirmed(&items), Some("hand-spun".to_owned()));
}

/// The Indonesian-shaped case: one candidate is better-or-equal on every deterministic work metric and strictly better on at least one; adding `raw_paths` to the key must never flip an outcome that was already unambiguous.
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
        // These two fields are wall-clock diagnostics and must not create dominance when every deterministic coordinate is tied.
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

/// A `selectable()` candidate whose production health blocks publication must win nothing and reach no frontier, unlike an identically-scored clean one.
#[test]
fn a_production_blocked_candidate_cannot_win_or_reach_the_frontier() {
    struct MixedHealthEvaluator;
    impl CandidateEvaluator for MixedHealthEvaluator {
        fn evaluate(
            &mut self,
            candidate: &CandidateState,
            _remaining: Budget,
        ) -> ConfirmationEvidence {
            let score = Score {
                states: 10,
                arcs: 10,
                build: 1,
                apply: 1,
                proposals: 1,
                confirmation: 1,
                confirmation_steps: 1,
                raw_paths: 0,
            };
            ConfirmationEvidence {
                certification: Certification::FullHcConfirmed {
                    words: 1,
                    corpus_hash: "h".into(),
                },
                score: Some(score),
                usage: BudgetUsage {
                    evaluations: 1,
                    ..BudgetUsage::default()
                },
                // "baseline" is production-blocked; "other" is otherwise identical and clean.
                production_blocks_publication: candidate.baseline,
            }
        }
    }
    let candidates = vec![
        candidate("baseline", "base", "base", 1, Some(1), true),
        candidate("other", "other", "other", 1, Some(1), false),
    ];
    let budget = Budget {
        candidates: 2,
        evaluations: 2,
        ..Budget::default()
    };
    let selected = Exhaustive.search(&candidates, budget, 1);
    let outcome = optimize_with_evaluator(
        &selected.selected,
        budget,
        1,
        &Exhaustive,
        &mut MixedHealthEvaluator,
    );
    assert_eq!(outcome.evaluated.len(), 2, "both candidates were evaluated");
    assert_eq!(
        outcome.winner,
        Some("other".to_owned()),
        "the production-blocked baseline must never win despite being scored and certified"
    );
    assert_eq!(
        outcome.frontier,
        vec!["other".to_owned()],
        "the production-blocked baseline must not reach the frontier either"
    );
}
