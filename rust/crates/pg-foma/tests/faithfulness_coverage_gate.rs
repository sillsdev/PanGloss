//! The wired-up edge of `pg_foma::faithfulness_coverage`: runs the real propose+confirm pipeline over every discovered fixture with every backend the selector permits, checks proposal containment against full Rust HermitCrab, states the denominator, and prints the account -- asserting NON-VACUITY only, with the failure inventory reported rather than gated (see `REQUIREMENT`).

use std::panic::{self, AssertUnwindSafe};

use pg_conformance_fixtures::{claimed_scope, discover, SCOPE_ENV};
use pg_foma::enumerate::EmissionStrategy;
use pg_foma::faithfulness_coverage::{
    build_report, containment_outcome_for_evidence, observe_fixture_containment,
    unobservable_fixture, ContainmentOutcome, FaithfulnessReport, FaithfulnessRequirement,
    FixtureContainmentObservation,
};

/// THE PLACE THIS ACCOUNT BECOMES STRICT: swap to `FaithfulnessRequirement::NoFailures` once the printed failure inventory reaches zero.
const REQUIREMENT: FaithfulnessRequirement = FaithfulnessRequirement::NonVacuity;

/// A fixture that fails to load, is `skip_in_generic_replay`, or panics mid-evaluation contributes an `unobservable_fixture` row rather than aborting the sweep.
fn collect() -> (usize, Vec<FixtureContainmentObservation>) {
    let fixtures = discover();
    let discovered = fixtures.len();

    let default_hook = panic::take_hook();
    panic::set_hook(Box::new(|_| {}));
    let mut observations = Vec::new();
    for fixture in fixtures {
        let label = fixture.label();
        let Ok(grammar) = pg_grammar::load(&fixture.load_grammar_xml()) else {
            continue;
        };
        let words_yaml = fixture.load_words_yaml();
        if let Some(reason) = words_yaml.skip_in_generic_replay() {
            observations.push(unobservable_fixture(&label, Vec::new(), reason.to_string()));
            continue;
        }
        let words: Vec<String> = words_yaml.words.into_iter().map(|w| w.word).collect();
        let observation = panic::catch_unwind(AssertUnwindSafe(|| {
            observe_fixture_containment(&label, &grammar, &words)
        }))
        .unwrap_or_else(|payload| {
            let reason = payload
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| payload.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "<non-string panic payload>".to_string());
            unobservable_fixture(&label, Vec::new(), format!("panicked: {reason}"))
        });
        observations.push(observation);
    }
    panic::set_hook(default_hook);
    (discovered, observations)
}

fn report() -> FaithfulnessReport {
    let (discovered, observations) = collect();
    build_report(claimed_scope().label(), discovered, &observations)
}

#[test]
fn report_faithfulness_coverage() {
    let report = report();
    println!("{}", report.render());

    if let Err(violations) = report.check(REQUIREMENT) {
        panic!(
            "the faithfulness-coverage collection measured nothing usable ({SCOPE_ENV}={}): {:#?}",
            report.scope, violations
        );
    }

    // Pins that the non-default backends are among the exercised ones, not just the shipping one.
    for strategy in [
        EmissionStrategy::PlanComposed,
        EmissionStrategy::TemplatedUnderlyingTokens,
    ] {
        assert!(
            report.backends_exercised.contains(&strategy),
            "{strategy:?} had no containment comparison attempted on any discovered fixture, so \
             every one of its rows would be not-attempted by construction rather than by measurement"
        );
    }
}

/// Every real containment FAILURE this run finds is printed loudly, never smoothed into the totals.
#[test]
fn any_containment_failure_is_printed_with_its_missing_analysis() {
    let report = report();
    if report.failed.is_empty() {
        println!("faithfulness-coverage: no containment failures on this run");
        return;
    }
    println!(
        "faithfulness-coverage: {} (construct, backend) pair(s) FAILED containment:",
        report.failed.len()
    );
    for (kind, strategy, fixture, detail) in &report.failure_examples {
        println!(
            "  FAILED {kind:?} x {} -- {fixture}: {detail}",
            strategy.label()
        );
    }
}

/// FALSIFICATION: dropping a real oracle-required candidate from one backend's evidence must fail containment for exactly that backend and no other.
#[test]
fn dropping_a_candidate_fails_containment_for_exactly_that_backends_evidence() {
    use pg_conformance_fixtures::Root;
    use pg_foma::backend_runtime::{
        evaluate_plans_observed_with_cache, RunEvaluationCache, RuntimeBudget,
    };
    use pg_foma::enumerate::{enumerate_default, CandidateRole, LoweredCandidate};
    use pg_foma::junctions::PhonologyProbe;
    use pg_foma::lowering_adapter::LoweringAdapter;
    use pg_foma::replace::SegAlphabet;

    const FIXTURE: &str = "template-category-sharing";
    const STRATEGIES: [EmissionStrategy; 3] = [
        EmissionStrategy::PlanComposed,
        EmissionStrategy::TunedSurfaceProbed,
        EmissionStrategy::TemplatedUnderlyingTokens,
    ];

    let fixture = discover()
        .into_iter()
        .find(|fixture| fixture.root == Root::Staging && fixture.name == FIXTURE)
        .unwrap_or_else(|| panic!("missing pinned synthetic fixture {FIXTURE}"));
    let grammar = pg_grammar::load(&fixture.load_grammar_xml()).expect("fixture must load");
    let words: Vec<String> = fixture
        .load_words_yaml()
        .words
        .into_iter()
        .map(|w| w.word)
        .collect();
    assert!(
        words.iter().any(|w| w == "pakolosa"),
        "the falsification needs the fixture's known-positive word"
    );

    let alphabet = SegAlphabet::new(&grammar.char_tables[0]);
    let prules: Vec<&pg_grammar::model::PhonRuleDef> = grammar
        .strata
        .iter()
        .flat_map(|stratum| {
            stratum
                .prules
                .iter()
                .map(|id| &grammar.prules[id.0 as usize])
        })
        .collect();
    let baseline_plan = enumerate_default(
        &grammar,
        &alphabet,
        &prules,
        PhonologyProbe::new(&grammar).as_ref(),
    );
    let plans: Vec<LoweredCandidate> = STRATEGIES
        .iter()
        .map(|&strategy| LoweredCandidate {
            label: "faithfulness-falsification",
            plan: baseline_plan.clone(),
            adapter: LoweringAdapter::for_strategy(strategy),
            role: if strategy == EmissionStrategy::PlanComposed {
                CandidateRole::Baseline
            } else {
                CandidateRole::Alternative
            },
        })
        .collect();

    let mut cache = RunEvaluationCache::prepare(&grammar, &words, RuntimeBudget::default())
        .expect("oracle preparation must succeed for this fixture");
    let observed = evaluate_plans_observed_with_cache(
        &grammar,
        &plans,
        &words,
        RuntimeBudget::default(),
        &mut cache,
    );

    // Honest baseline, checked before any sabotage so the falsification below cannot be vacuous.
    let mut honest_evidence: Vec<(
        EmissionStrategy,
        Vec<pg_foma::backend_runtime::WordEvidence>,
    )> = Vec::new();
    for (plan, observation) in plans.iter().zip(&observed) {
        let evidence = observation
            .words
            .clone()
            .unwrap_or_else(|| panic!("{:?} evaluation failed outright", plan.strategy()));
        assert_eq!(
            containment_outcome_for_evidence(&evidence),
            ContainmentOutcome::Held,
            "{:?} must hold containment before sabotage, or this falsification is vacuous",
            plan.strategy()
        );
        honest_evidence.push((plan.strategy(), evidence));
    }

    let sabotaged_backend = EmissionStrategy::TunedSurfaceProbed;
    let mut sabotaged_word = None;
    let sabotaged_evidence: Vec<(
        EmissionStrategy,
        Vec<pg_foma::backend_runtime::WordEvidence>,
    )> = honest_evidence
        .iter()
        .map(|(strategy, evidence)| {
            let mut evidence = evidence.clone();
            if *strategy == sabotaged_backend {
                let word = evidence
                    .iter_mut()
                    .find(|word| !word.expected.is_empty())
                    .expect("the fixture must have at least one word with oracle analyses");
                assert!(
                    !word.proposals.is_empty(),
                    "the word being sabotaged must have started with real proposals"
                );
                sabotaged_word = Some(word.word.clone());
                // Simulates an emitter silently skipping this occurrence's construct material.
                word.proposals.clear();
            }
            (*strategy, evidence)
        })
        .collect();

    for (strategy, evidence) in &sabotaged_evidence {
        let outcome = containment_outcome_for_evidence(evidence);
        if *strategy == sabotaged_backend {
            match &outcome {
                ContainmentOutcome::Failed { word, detail } => {
                    assert_eq!(Some(word.clone()), sabotaged_word);
                    println!(
                        "falsification: {strategy:?} containment now FAILS as expected -- {detail}"
                    );
                }
                other => panic!(
                    "sabotaging {strategy:?}'s proposals must fail containment, got {other:?}"
                ),
            }
        } else {
            assert_eq!(
                outcome,
                ContainmentOutcome::Held,
                "sabotaging {sabotaged_backend:?} must not disturb {strategy:?}'s containment"
            );
        }
    }
}
