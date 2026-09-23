use super::*;

fn observation(
    label: &str,
    kinds: &[CharacteristicKind],
    outcomes: &[(EmissionStrategy, BackendOutcome)],
) -> GrammarObservation {
    GrammarObservation {
        label: label.to_string(),
        kinds: kinds.to_vec(),
        outcomes: outcomes.to_vec(),
    }
}

/// A refused backend and a failed compile must both credit nothing, or the collector is just the declarative table with extra steps.
#[test]
fn only_a_successful_compile_witnesses_anything() {
    let report = build_report(
        "all",
        1,
        &[observation(
            "synthetic",
            &[CharacteristicKind::Affixation],
            &[
                (EmissionStrategy::PlanComposed, BackendOutcome::Compiled),
                (
                    EmissionStrategy::TunedSurfaceProbed,
                    BackendOutcome::CompileFailed("forced".to_string()),
                ),
                (
                    EmissionStrategy::TemplatedUnderlyingTokens,
                    BackendOutcome::RefusedBySelector,
                ),
            ],
        )],
    );
    assert_eq!(
        report.witnessed,
        vec![(
            CharacteristicKind::Affixation,
            EmissionStrategy::PlanComposed
        )]
    );
    assert_eq!(
        report.backends_compiling,
        vec![EmissionStrategy::PlanComposed]
    );
    assert_eq!(report.compile_failures.len(), 1);
    assert_eq!(report.selector_refusals.len(), 1);
}

/// A kind no observed grammar contains can never be witnessed, however many backends compiled.
#[test]
fn a_kind_no_grammar_exhibits_is_never_witnessed() {
    let report = build_report(
        "all",
        1,
        &[observation(
            "synthetic",
            &[CharacteristicKind::Affixation],
            &ALL_STRATEGIES
                .iter()
                .map(|&s| (s, BackendOutcome::Compiled))
                .collect::<Vec<_>>(),
        )],
    );
    for &kind in CharacteristicKind::ALL {
        if kind == CharacteristicKind::Affixation {
            continue;
        }
        assert!(
            !report.witnessed.iter().any(|(k, _)| *k == kind),
            "{kind:?} was witnessed by a grammar that does not contain it"
        );
    }
    assert_eq!(report.kinds_exhibited, vec![CharacteristicKind::Affixation]);
}

/// The three classes plus the declared/witnessed overlap must account for every pair exactly once.
#[test]
fn every_pair_is_witnessed_declared_or_a_gap() {
    let report = build_report(
        "all",
        1,
        &[observation(
            "synthetic",
            CharacteristicKind::ALL,
            &ALL_STRATEGIES
                .iter()
                .map(|&s| (s, BackendOutcome::Compiled))
                .collect::<Vec<_>>(),
        )],
    );
    let union = report.witnessed.len() + report.declared_cannot_represent.len()
        - report.contradictions.len();
    assert_eq!(union + report.gaps.len(), CompletenessReport::total_pairs());
}

/// Every gap must carry its own reason, and the two reasons must be told apart: an unexhibited construct is a fixture-set limit, a refused one is a backend limit.
#[test]
fn each_gap_names_why_it_is_one() {
    let report = build_report(
        "all",
        1,
        &[observation(
            "synthetic",
            &[CharacteristicKind::Affixation],
            &[
                (EmissionStrategy::PlanComposed, BackendOutcome::Compiled),
                (
                    EmissionStrategy::TunedSurfaceProbed,
                    BackendOutcome::RefusedBySelector,
                ),
                (
                    EmissionStrategy::TemplatedUnderlyingTokens,
                    BackendOutcome::Compiled,
                ),
            ],
        )],
    );
    assert_eq!(report.gaps.len(), report.gap_attributions.len());
    let affixation = report
        .gap_attributions
        .iter()
        .find(|(kind, strategy, _)| {
            *kind == CharacteristicKind::Affixation
                && *strategy == EmissionStrategy::TunedSurfaceProbed
        })
        .expect("the refused backend must have an Affixation gap");
    assert!(
        affixation.2.contains("refused by the selector"),
        "{affixation:?}"
    );
    let unexhibited = report
        .gap_attributions
        .iter()
        .find(|(kind, _, _)| *kind == CharacteristicKind::Metathesis)
        .expect("a construct the fixture lacks must be a gap");
    assert!(
        unexhibited.2.contains("no observed grammar exhibits"),
        "{unexhibited:?}"
    );
}

/// An empty run must fail non-vacuity rather than report a clean sheet.
#[test]
fn a_vacuous_run_is_refused_by_the_non_vacuity_requirement() {
    let report = build_report("all", 0, &[]);
    let violations = report
        .check(CompletenessRequirement::NonVacuity)
        .expect_err("an empty collection must not pass");
    assert_eq!(violations.len(), 3, "{violations:?}");
    assert_eq!(report.gaps.len(), 69, "every representable pair is a gap");
}

/// The strict requirement is live code, not a comment: it must reject exactly what the lenient one tolerates.
#[test]
fn the_strict_requirement_rejects_a_gap_the_lenient_one_reports() {
    let mut outcomes: Vec<(EmissionStrategy, BackendOutcome)> = ALL_STRATEGIES
        .iter()
        .map(|&s| (s, BackendOutcome::Compiled))
        .collect();
    outcomes.pop();
    let report = build_report(
        "all",
        1,
        &[observation("synthetic", CharacteristicKind::ALL, &outcomes)],
    );
    assert!(!report.gaps.is_empty());
    assert!(report.check(CompletenessRequirement::NonVacuity).is_ok());
    let violations = report
        .check(CompletenessRequirement::NoGaps)
        .expect_err("a non-empty gap inventory must fail the strict requirement");
    assert!(violations.iter().any(|v| v.contains("neither witnessed")));
}
