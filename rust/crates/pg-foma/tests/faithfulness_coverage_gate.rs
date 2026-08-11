//! Runs the full faithfulness-coverage pipeline and requires zero containment failures.

use std::collections::BTreeSet;
use std::panic::{self, AssertUnwindSafe};

use pg_conformance_fixtures::{claimed_scope, discover, SCOPE_ENV};
use pg_foma::enumerate::EmissionStrategy;
use pg_foma::faithfulness_coverage::{
    build_report, check_ratchet, containment_outcome_for_evidence, failed_triples,
    observe_fixture_containment, unobservable_fixture, ContainmentOutcome, FaithfulnessReport,
    FaithfulnessRequirement, FixtureContainmentObservation,
};

const REQUIREMENT: FaithfulnessRequirement = FaithfulnessRequirement::NoFailures;

/// Loads the grammar, or returns a visible `unobservable_fixture` row naming the load error.
fn load_or_unobservable(
    label: &str,
    grammar_xml: &str,
) -> Result<pg_grammar::model::Grammar, FixtureContainmentObservation> {
    pg_grammar::load(grammar_xml).map_err(|err| {
        unobservable_fixture(label, Vec::new(), format!("grammar failed to load: {err}"))
    })
}

/// A fixture that fails to load, is `skip_in_generic_replay`, or panics mid-evaluation contributes an `unobservable_fixture` row rather than aborting the sweep.
fn collect() -> (usize, Vec<String>, Vec<FixtureContainmentObservation>) {
    let fixtures = discover();
    let discovered = fixtures.len();
    let discovered_labels: Vec<String> = fixtures.iter().map(|f| f.label()).collect();

    let default_hook = panic::take_hook();
    panic::set_hook(Box::new(|_| {}));
    let mut observations = Vec::new();
    for fixture in fixtures {
        let label = fixture.label();
        let grammar = match load_or_unobservable(&label, &fixture.load_grammar_xml()) {
            Ok(grammar) => grammar,
            Err(observation) => {
                observations.push(observation);
                continue;
            }
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
    (discovered, discovered_labels, observations)
}

fn report() -> FaithfulnessReport {
    let (discovered, _, observations) = collect();
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

/// A grammar that fails to load must still produce a visible row, never a silent drop.
#[test]
fn an_unloadable_fixture_produces_a_visible_row_not_a_silent_drop() {
    let result = load_or_unobservable("synthetic-unloadable", "not valid grammar xml at all");
    let observation = result.expect_err("garbage XML must not load");
    assert_eq!(observation.label, "synthetic-unloadable");
    assert!(observation.kinds.is_empty());
    assert!(
        observation.outcomes.iter().all(|(_, outcome)| matches!(
            outcome,
            ContainmentOutcome::NotAttempted { reason } if reason.contains("grammar failed to load")
        )),
        "every strategy's outcome must be a NotAttempted row naming the load error, got {:?}",
        observation.outcomes
    );
}

/// Every fixture `discover()` finds must end up in `collect()`'s observations, by label.
#[test]
fn every_discovered_fixture_produces_an_observation_row() {
    let (discovered, discovered_labels, observations) = collect();
    assert_eq!(
        observations.len(),
        discovered,
        "collect() must produce exactly one observation per discovered fixture"
    );
    let observed_labels: BTreeSet<&str> = observations.iter().map(|o| o.label.as_str()).collect();
    let missing: Vec<&str> = discovered_labels
        .iter()
        .map(String::as_str)
        .filter(|label| !observed_labels.contains(label))
        .collect();
    assert!(
        missing.is_empty(),
        "fixture(s) discovered but silently absent from the faithfulness sweep: {missing:?}"
    );
}

/// The retired ratchet accepts only an empty observed failure set.
#[test]
fn containment_failures_are_empty_after_ratchet_retirement() {
    let (_, _, observations) = collect();
    let observed = failed_triples(&observations);

    if let Err(violations) = check_ratchet(&observed, &[]) {
        panic!(
            "faithfulness-containment zero-failure check violated:\n{}",
            violations.join("\n")
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
