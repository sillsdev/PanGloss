use super::*;

#[test]
fn count_arithmetic_reports_overflow() {
    assert_eq!(Count::product([2, 3]).value, 6);
    assert!(Count::product([u64::MAX, 2]).overflowed);
    assert!(
        Count::sum([
            Count {
                value: u64::MAX,
                overflowed: false
            },
            Count {
                value: 1,
                overflowed: false
            }
        ])
        .overflowed
    );
}

#[test]
fn seeded_sampling_is_deterministic_and_seed_sensitive() {
    assert_eq!(
        deterministic_sample_indices(20, 5, 7),
        deterministic_sample_indices(20, 5, 7)
    );
    assert_ne!(
        deterministic_sample_indices(20, 5, 7),
        deterministic_sample_indices(20, 5, 8)
    );
}

#[test]
fn nearest_rank_quantiles_and_pruning_ratio_are_truthful() {
    let measurements = [
        StageMeasurement {
            materialize: 1,
            capability: 2,
            build: Some(3),
            evaluation: Some(4),
            pruned: true,
        },
        StageMeasurement {
            materialize: 5,
            capability: 6,
            build: Some(7),
            evaluation: Some(8),
            pruned: false,
        },
        StageMeasurement {
            materialize: 9,
            capability: 10,
            build: Some(11),
            evaluation: Some(12),
            pruned: false,
        },
    ];
    let summary = summarize_pilot(&measurements, 42);
    assert_eq!(summary.materialize, Quantiles { p50: 5, p95: 9 });
    assert_eq!(summary.pruning_ratio_ppm, 333_333);
    assert_eq!(summary.seed, 42);
    assert_eq!(summary.executed_samples, 3);
}

/// No fake zero measurements: a refused candidate carries no build/evaluation reading, and quantiles must be taken only over rows that do.
#[test]
fn a_stage_that_never_ran_does_not_contribute_a_zero_to_its_quantiles() {
    let refused = StageMeasurement {
        materialize: 100,
        capability: 200,
        build: None,
        evaluation: None,
        pruned: true,
    };
    let ran = |build: u64| StageMeasurement {
        materialize: 100,
        capability: 200,
        build: Some(build),
        evaluation: Some(build * 2),
        pruned: false,
    };
    let summary = summarize_pilot(&[refused, refused, refused, ran(1_000), ran(3_000)], 1);
    assert_eq!(
        summary.build,
        Quantiles {
            p50: 1_000,
            p95: 3_000
        },
        "build quantiles must be taken over the two rows that were actually built"
    );
    assert_eq!(
        summary.evaluation,
        Quantiles {
            p50: 2_000,
            p95: 6_000
        }
    );
    assert_eq!(
        summary.executed_samples, 2,
        "the honest denominator for the build/evaluation quantiles, not sample_size"
    );
    assert_eq!(
        summary.sample_size, 5,
        "every sampled candidate still counts toward the pilot's size and pruning ratio"
    );
    assert_eq!(summary.pruning_ratio_ppm, 600_000);
    // The two stages a refused candidate genuinely DID pay are still summarized over every row.
    assert_eq!(summary.materialize, Quantiles { p50: 100, p95: 100 });
}
