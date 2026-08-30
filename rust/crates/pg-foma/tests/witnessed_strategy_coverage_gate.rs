//! The wired-up edge of `pg_foma::witnessed_coverage`: compiles every discovered conformance fixture with every backend the selector permits, states the denominator, and prints the completeness account -- asserting NON-VACUITY only, with the gap inventory reported rather than gated (see `REQUIREMENT`).

use pg_conformance_fixtures::{claimed_scope, discover, SCOPE_ENV};
use pg_foma::capability::CharacteristicKind;
use pg_foma::coverage_seam::collect_observations;
use pg_foma::enumerate::EmissionStrategy;
use pg_foma::witnessed_coverage::{
    build_report, observe_grammar, observe_grammar_with, BackendOutcome, CompletenessReport,
    CompletenessRequirement, GrammarObservation,
};

/// THE PLACE THE GATE BECOMES STRICT: swap to `CompletenessRequirement::NoGaps` once the printed inventory reaches zero.
const REQUIREMENT: CompletenessRequirement = CompletenessRequirement::NonVacuity;

/// Walks every discovered fixture once via the shared `crate::coverage_seam` walk (a compiler contract violation is recorded as a failed compile, never a witness, via `observe_grammar`'s own internal `catch_unwind`).
fn collect() -> (usize, Vec<GrammarObservation>) {
    let fixtures = discover();
    collect_observations(
        &fixtures,
        |fixture| pg_grammar::load(&fixture.load_grammar_xml()).ok(),
        |fixture, grammar| observe_grammar(&fixture.label(), grammar),
    )
}

fn report() -> CompletenessReport {
    let (discovered, observations) = collect();
    build_report(claimed_scope().label(), discovered, &observations)
}

#[test]
fn report_witnessed_strategy_coverage() {
    let report = report();
    println!("{}", report.render());

    if let Err(violations) = report.check(REQUIREMENT) {
        panic!(
            "the witnessed-coverage collection measured nothing usable ({SCOPE_ENV}={}): {:#?}",
            report.scope, violations
        );
    }

    // Non-vacuity's third clause is about DISTINCT backends; this pins that the non-default ones are among them, since a sweep carried entirely by the shipping analyzer is the inheritance trap this whole account exists to expose.
    for strategy in [
        EmissionStrategy::PlanComposed,
        EmissionStrategy::TemplatedUnderlyingTokens,
    ] {
        assert!(
            report.backends_compiling.contains(&strategy),
            "{strategy:?} compiled no discovered fixture at all, so every one of its rows would be \
             a gap by construction rather than by measurement"
        );
    }
}

/// A witness must come from a compile, so forcing one backend's compile to fail must remove exactly that backend's witnesses and nothing else.
#[test]
fn forcing_a_backend_to_fail_removes_exactly_its_witnesses() {
    let (discovered, honest) = collect();
    let honest_report = build_report(claimed_scope().label(), discovered, &honest);
    let sabotaged_backend = EmissionStrategy::TunedSurfaceProbed;
    assert!(
        !honest_report.witnessed_for(sabotaged_backend).is_empty(),
        "the falsification is vacuous unless the backend has witnesses to lose"
    );

    let fixtures = discover();
    let (_, sabotaged) = collect_observations(
        &fixtures,
        |fixture| pg_grammar::load(&fixture.load_grammar_xml()).ok(),
        |fixture, grammar| {
            observe_grammar_with(&fixture.label(), grammar, &|g, strategy| {
                if strategy == sabotaged_backend {
                    Err("forced failure".to_string())
                } else {
                    pg_foma::witnessed_coverage::compile_with_backend(g, strategy)
                }
            })
        },
    );
    let sabotaged_report = build_report(claimed_scope().label(), discovered, &sabotaged);
    println!(
        "falsification: {sabotaged_backend:?} witnessed {} -> {}; gaps {} -> {}",
        honest_report.witnessed_for(sabotaged_backend).len(),
        sabotaged_report.witnessed_for(sabotaged_backend).len(),
        honest_report.gaps.len(),
        sabotaged_report.gaps.len()
    );

    assert!(
        sabotaged_report.witnessed_for(sabotaged_backend).is_empty(),
        "a backend whose compile always fails must witness nothing, yet it kept {:?}",
        sabotaged_report.witnessed_for(sabotaged_backend)
    );
    assert!(
        !sabotaged_report
            .backends_compiling
            .contains(&sabotaged_backend),
        "a backend that never compiled must not be listed as exercised"
    );
    for other in [
        EmissionStrategy::PlanComposed,
        EmissionStrategy::TemplatedUnderlyingTokens,
    ] {
        assert_eq!(
            sabotaged_report.witnessed_for(other),
            honest_report.witnessed_for(other),
            "sabotaging {sabotaged_backend:?} must not disturb {other:?}'s evidence"
        );
    }
    assert_eq!(
        sabotaged_report.gaps.len(),
        honest_report.gaps.len() + honest_report.witnessed_for(sabotaged_backend).len(),
        "every witness the sabotaged backend lost must reappear as a gap"
    );
}

/// The other direction: a construct no discovered fixture contains can never be reported witnessed, however many backends compiled.
#[test]
fn a_construct_no_fixture_exhibits_is_never_witnessed() {
    let report = report();
    let unexhibited: Vec<CharacteristicKind> = CharacteristicKind::ALL
        .iter()
        .copied()
        .filter(|kind| !report.kinds_exhibited.contains(kind))
        .collect();
    println!(
        "constructs no discovered fixture exhibits: {:?}",
        unexhibited
    );
    for kind in &unexhibited {
        assert!(
            !report.witnessed.iter().any(|(k, _)| k == kind),
            "{kind:?} is exhibited by no discovered fixture, so no compile can have witnessed it"
        );
    }
    assert!(
        !report.kinds_exhibited.is_empty(),
        "sanity: the sweep must exhibit at least one construct"
    );
}

/// Every recorded outcome must be one a real run produced: a selector refusal names a backend the selection layer actually excluded, and a compile failure carries its reason.
#[test]
fn every_recorded_outcome_is_attributable() {
    let (_, observations) = collect();
    assert!(!observations.is_empty());
    for observation in &observations {
        assert_eq!(
            observation.outcomes.len(),
            pg_foma::strategy_coverage::ALL_STRATEGIES.len(),
            "{} must record an outcome for every backend",
            observation.label
        );
        for (strategy, outcome) in &observation.outcomes {
            if let BackendOutcome::CompileFailed(reason) = outcome {
                assert!(
                    !reason.is_empty(),
                    "{} x {strategy:?} failed with no reason recorded",
                    observation.label
                );
            }
        }
    }
}
