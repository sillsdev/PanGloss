use super::*;

// Every strategy `None`; `observation_with_soundness` is the one that exercises the new axis.
fn observation(
    label: &str,
    kinds: &[CharacteristicKind],
    outcomes: &[(EmissionStrategy, ContainmentOutcome)],
) -> FixtureContainmentObservation {
    observation_with_soundness(label, kinds, outcomes, &[])
}

fn observation_with_soundness(
    label: &str,
    kinds: &[CharacteristicKind],
    outcomes: &[(EmissionStrategy, ContainmentOutcome)],
    soundness: &[(EmissionStrategy, u64)],
) -> FixtureContainmentObservation {
    FixtureContainmentObservation {
        label: label.to_string(),
        kinds: kinds.to_vec(),
        outcomes: outcomes.to_vec(),
        soundness: ALL_STRATEGIES
            .iter()
            .map(|&strategy| {
                (
                    strategy,
                    soundness
                        .iter()
                        .find(|(s, _)| *s == strategy)
                        .map(|(_, count)| *count),
                )
            })
            .collect(),
    }
}

/// A single held observation must classify as held, never as not-attempted or failed.
#[test]
fn a_held_pair_is_classified_held() {
    let report = build_report(
        "all",
        1,
        &[observation(
            "synthetic",
            &[CharacteristicKind::Affixation],
            &[
                (EmissionStrategy::PlanComposed, ContainmentOutcome::Held),
                (
                    EmissionStrategy::TunedSurfaceProbed,
                    ContainmentOutcome::Held,
                ),
                (
                    EmissionStrategy::TemplatedUnderlyingTokens,
                    ContainmentOutcome::NotAttempted {
                        reason: NotAttemptedReason::RefusedBySelector,
                    },
                ),
            ],
        )],
    );
    assert_eq!(
        report.held,
        vec![
            (
                CharacteristicKind::Affixation,
                EmissionStrategy::PlanComposed
            ),
            (
                CharacteristicKind::Affixation,
                EmissionStrategy::TunedSurfaceProbed
            ),
        ]
    );
    assert!(report.failed.is_empty());
    assert_eq!(
        report.not_attempted,
        vec![(
            CharacteristicKind::Affixation,
            EmissionStrategy::TemplatedUnderlyingTokens
        )]
    );
}

/// A failure among exhibiting fixtures must never be diluted by a passing neighbor.
#[test]
fn any_failure_among_exhibiting_fixtures_wins_over_held() {
    let report = build_report(
        "all",
        2,
        &[
            observation(
                "held-fixture",
                &[CharacteristicKind::Affixation],
                &[(EmissionStrategy::PlanComposed, ContainmentOutcome::Held)],
            ),
            observation(
                "failing-fixture",
                &[CharacteristicKind::Affixation],
                &[(
                    EmissionStrategy::PlanComposed,
                    ContainmentOutcome::Failed {
                        word: "kolo".to_string(),
                        detail: "missing identity".to_string(),
                    },
                )],
            ),
        ],
    );
    assert_eq!(
        report.failed,
        vec![(
            CharacteristicKind::Affixation,
            EmissionStrategy::PlanComposed
        )]
    );
    assert!(report.held.is_empty());
    assert_eq!(report.failure_examples.len(), 1);
    assert_eq!(report.failure_examples[0].2, "failing-fixture");
}

/// A kind no observed fixture exhibits must not appear in held, failed, or not_attempted.
#[test]
fn an_unexhibited_kind_is_not_in_any_bucket() {
    let report = build_report(
        "all",
        1,
        &[observation(
            "synthetic",
            &[CharacteristicKind::Affixation],
            &[(EmissionStrategy::PlanComposed, ContainmentOutcome::Held)],
        )],
    );
    for &kind in CharacteristicKind::ALL {
        if kind == CharacteristicKind::Affixation {
            continue;
        }
        assert!(!report.held.iter().any(|(k, _)| *k == kind));
        assert!(!report.failed.iter().any(|(k, _)| *k == kind));
        assert!(!report.not_attempted.iter().any(|(k, _)| *k == kind));
    }
}

/// A run with zero observations must fail non-vacuity rather than report a clean sheet.
#[test]
fn a_vacuous_run_is_refused_by_the_non_vacuity_requirement() {
    let report = build_report("all", 0, &[]);
    let violations = report
        .check(FaithfulnessRequirement::NonVacuity)
        .expect_err("an empty collection must not pass");
    assert!(violations.iter().any(|v| v.contains("no fixture")));
    assert!(violations.iter().any(|v| v.contains("not-attempted")));
}

/// The strict requirement must reject exactly the failure inventory the lenient one tolerates.
#[test]
fn the_strict_requirement_rejects_a_failure_the_lenient_one_reports() {
    let report = build_report(
        "all",
        1,
        &[observation(
            "synthetic",
            &[CharacteristicKind::Affixation],
            &[(
                EmissionStrategy::PlanComposed,
                ContainmentOutcome::Failed {
                    word: "kolo".to_string(),
                    detail: "missing identity".to_string(),
                },
            )],
        )],
    );
    // Pins that a single-backend failure still fails the strict requirement outright.
    let violations = report
        .check(FaithfulnessRequirement::NoFailures)
        .expect_err("a non-empty failure inventory must fail the strict requirement");
    assert!(violations
        .iter()
        .any(|v| v.contains("FAILED proposal containment")));
}

/// The ratchet must admit today's count and refuse one more, or it gates nothing.
#[test]
fn the_ratchet_admits_its_own_count_and_refuses_one_more() {
    let report = build_report(
        "all",
        1,
        &[observation(
            "synthetic",
            &[CharacteristicKind::Affixation],
            &[(
                EmissionStrategy::PlanComposed,
                ContainmentOutcome::Failed {
                    word: "kolo".to_string(),
                    detail: "missing identity".to_string(),
                },
            )],
        )],
    );
    // On the ratchet's own violation: one backend trips non-vacuity either way.
    let at_count = report
        .check(FaithfulnessRequirement::NoMoreThan { failures: 1 })
        .err()
        .unwrap_or_default();
    assert!(
        !at_count.iter().any(|v| v.contains("above the ratchet")),
        "a ratchet at the observed count must not fire: {at_count:?}"
    );
    let below = report
        .check(FaithfulnessRequirement::NoMoreThan { failures: 0 })
        .expect_err("one failure above the ratchet must be refused");
    assert!(below.iter().any(|v| v.contains("above the ratchet")));
}

// A HELD (recall) fixture can still be flagged over-generating: the two axes must not conflate.
#[test]
fn over_generation_is_counted_independently_of_containment() {
    let report = build_report(
        "all",
        1,
        &[observation_with_soundness(
            "over-generating-but-held",
            &[CharacteristicKind::Affixation],
            &[(EmissionStrategy::PlanComposed, ContainmentOutcome::Held)],
            &[(EmissionStrategy::PlanComposed, 3)],
        )],
    );
    assert_eq!(
        report.held,
        vec![(
            CharacteristicKind::Affixation,
            EmissionStrategy::PlanComposed
        )],
        "recall containment must still read HELD"
    );
    assert_eq!(
        report.over_generating,
        vec![(
            CharacteristicKind::Affixation,
            EmissionStrategy::PlanComposed
        )],
        "a candidate-only identity must be visible even though recall containment held"
    );
    assert_eq!(report.over_generation_examples.len(), 1);
    assert_eq!(
        report.over_generation_examples[0].2,
        "over-generating-but-held"
    );
    assert_eq!(report.over_generation_examples[0].3, 3);
}

// Neither a zero-count reading nor an uncompared (`None`) one may read as an over-generation.
#[test]
fn zero_and_not_compared_soundness_readings_never_over_generate() {
    let report = build_report(
        "all",
        1,
        &[observation_with_soundness(
            "clean",
            &[CharacteristicKind::Affixation],
            &[(EmissionStrategy::PlanComposed, ContainmentOutcome::Held)],
            &[(EmissionStrategy::PlanComposed, 0)],
        )],
    );
    assert!(report.over_generating.is_empty());

    // `soundness: &[]` (no entry) makes every strategy `None` via `observation_with_soundness`'s own fill -- "not compared", not "clean".
    let uncompared = build_report(
        "all",
        1,
        &[observation(
            "uncompared",
            &[CharacteristicKind::Affixation],
            &[(EmissionStrategy::PlanComposed, ContainmentOutcome::Held)],
        )],
    );
    assert!(uncompared.over_generating.is_empty());
}

// FALSIFICATION: a forced over-generation must trip `check_soundness`; the ratchet at that exact count must not.
#[test]
fn the_soundness_ratchet_fires_on_a_forced_over_generation_and_admits_its_own_count() {
    let clean = build_report(
        "all",
        1,
        &[observation_with_soundness(
            "synthetic",
            &[CharacteristicKind::Affixation],
            &[(EmissionStrategy::PlanComposed, ContainmentOutcome::Held)],
            &[(EmissionStrategy::PlanComposed, 0)],
        )],
    );
    assert!(
        clean
            .check_soundness(SoundnessRequirement::NoMoreThan {
                over_generations: 0
            })
            .is_ok(),
        "a clean run must not trip a zero ratchet"
    );
    assert!(
        clean
            .check_soundness(SoundnessRequirement::NoOverGeneration)
            .is_ok(),
        "NoOverGeneration must agree with NoMoreThan{{0}} on a clean run"
    );

    // Forces the trigger the gate exists to catch: a candidate-only identity that survived confirmation.
    let sabotaged = build_report(
        "all",
        1,
        &[observation_with_soundness(
            "synthetic",
            &[CharacteristicKind::Affixation],
            &[(EmissionStrategy::PlanComposed, ContainmentOutcome::Held)],
            &[(EmissionStrategy::PlanComposed, 1)],
        )],
    );
    let violations = sabotaged
        .check_soundness(SoundnessRequirement::NoMoreThan {
            over_generations: 0,
        })
        .expect_err("a forced over-generation must be caught by the ratchet");
    assert!(violations.iter().any(|v| v.contains("CANDIDATE-ONLY")));
    assert!(
        sabotaged
            .check_soundness(SoundnessRequirement::NoOverGeneration)
            .is_err(),
        "NoOverGeneration must also catch the same forced over-generation"
    );

    // The ratchet set to the observed count must admit it -- a ratchet that never admits its own count gates nothing either.
    assert!(sabotaged
        .check_soundness(SoundnessRequirement::NoMoreThan {
            over_generations: 1
        })
        .is_ok());
}
