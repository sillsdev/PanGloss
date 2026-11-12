use super::*;

fn samples(weights: [f64; 3]) -> Vec<Sample> {
    (1..40u64)
        .map(|i| {
            let counts = Counts {
                work: 1000 + i * 700,
                bytes: 90_000 + (i * i * 913) % 400_000,
                registers: 500 + (i * 311) % 3_000,
            };
            let measured = counts.work as f64 * weights[0]
                + counts.bytes as f64 * weights[1]
                + counts.registers as f64 * weights[2];
            Sample { counts, measured }
        })
        .collect()
}

#[test]
fn the_current_weights_fit_themselves() {
    let fit = fit(
        &samples([
            WORK_WEIGHT as f64,
            BYTE_WEIGHT as f64,
            REGISTER_WEIGHT as f64,
        ]),
        None,
    )
    .unwrap();
    assert!((fit.work_factor - 1.0).abs() < 0.01, "{fit:?}");
    assert!((fit.byte_factor - 1.0).abs() < 0.01, "{fit:?}");
    assert!((fit.register_factor - 1.0).abs() < 0.01, "{fit:?}");
    assert!((fit.speed - 1.0).abs() < 0.01, "{fit:?}");
}

#[test]
fn machine_speed_does_not_move_a_weight() {
    let weights = [
        WORK_WEIGHT as f64,
        BYTE_WEIGHT as f64,
        REGISTER_WEIGHT as f64,
    ];
    let slower = fit(&samples(weights.map(|w| w * 0.37)), None).unwrap();
    assert!((slower.work_factor - 1.0).abs() < 0.01, "{slower:?}");
    assert!((slower.speed - 0.37).abs() < 0.01, "{slower:?}");
}

#[test]
fn a_real_shift_moves_only_that_weight() {
    let fit = fit(
        &samples([
            WORK_WEIGHT as f64,
            BYTE_WEIGHT as f64 * 1.3,
            REGISTER_WEIGHT as f64,
        ]),
        None,
    )
    .unwrap();
    assert!(fit.byte_factor > fit.work_factor * 1.15, "{fit:?}");
    assert!(fit.byte_factor > fit.register_factor * 1.15, "{fit:?}");
}

#[test]
fn a_wild_measurement_stops_at_the_bound() {
    let fit = fit(
        &samples([
            WORK_WEIGHT as f64,
            BYTE_WEIGHT as f64,
            REGISTER_WEIGHT as f64 * 50.0,
        ]),
        None,
    )
    .unwrap();
    assert_eq!(fit.register_factor, MAX_FACTOR);
}

#[test]
fn a_busy_machine_is_refused_and_instructions_never_are() {
    let measured = samples([
        WORK_WEIGHT as f64,
        BYTE_WEIGHT as f64,
        REGISTER_WEIGHT as f64,
    ]);
    assert_eq!(
        fit(&measured, Some(0.6)),
        Err(Refusal::MachineBusy { cpu_share: 0.6 })
    );
    assert!(fit(&measured, Some(0.97)).is_ok());
    assert!(fit(&measured, None).is_ok());
}

#[test]
fn too_few_usable_words_are_refused() {
    let tiny = [Sample {
        counts: Counts {
            work: 10,
            bytes: 10,
            registers: 10,
        },
        measured: 1.0,
    }; 50];
    assert_eq!(fit(&tiny, None), Err(Refusal::TooFewWords { usable: 0 }));
}

#[test]
fn the_exact_fit_recovers_the_weights_behind_the_counts() {
    let truth = [200.0, 2.0, 100.0];
    let solution = solve(&samples(truth), current_weights()).unwrap();
    assert!((solution.free.work - 200.0).abs() < 1e-6, "{solution:?}");
    assert!((solution.free.byte - 2.0).abs() < 1e-9, "{solution:?}");
    assert!(
        (solution.free.register - 100.0).abs() < 1e-6,
        "{solution:?}"
    );
    assert!(solution.free.error < 1e-9, "{solution:?}");
    assert_eq!(solution.words, 39);
}

#[test]
fn leaving_out_a_term_the_words_carry_costs_the_fit() {
    let solution = solve(&samples([200.0, 2.0, 100.0]), current_weights()).unwrap();
    for (term, error) in solution.error_without.iter().enumerate() {
        assert!(*error > 0.01, "term {term}: {solution:?}");
    }
}

#[test]
fn a_weight_is_never_negative() {
    let data: Vec<Sample> = samples([300.0, 2.0, 0.0])
        .into_iter()
        .map(|sample| Sample {
            measured: sample.measured - 10.0 * sample.counts.registers as f64,
            ..sample
        })
        .collect();
    let solution = solve(&data, current_weights()).unwrap();
    assert_eq!(solution.free.register, 0.0, "{solution:?}");
    assert!(
        solution.free.work > 0.0 && solution.free.byte > 0.0,
        "{solution:?}"
    );
    assert_eq!(solution.error_without[2], solution.free.error);
}

fn registers_follow_work(weights: [f64; 3]) -> Vec<Sample> {
    samples(weights)
        .into_iter()
        .map(|sample| {
            let counts = Counts {
                registers: sample.counts.work * 3,
                ..sample.counts
            };
            let measured = counts.work as f64 * weights[0]
                + counts.bytes as f64 * weights[1]
                + counts.registers as f64 * weights[2];
            Sample { counts, measured }
        })
        .collect()
}

#[test]
fn words_that_cannot_split_work_from_registers_say_so_and_the_tied_fit_still_fits() {
    let solution = solve(
        &registers_follow_work([200.0, 2.0, 100.0]),
        [676.0, 2.0, 268.0],
    )
    .unwrap();
    assert!(solution.free.error < 1e-6, "{solution:?}");
    assert!(
        solution.error_without[0] < 1e-6 && solution.error_without[2] < 1e-6,
        "either of the pair can be left out at no cost: {solution:?}"
    );
    assert!(
        solution.error_without[1] > 0.01,
        "bytes are still carried: {solution:?}"
    );
    assert!(
        solution.work_register_tradeoff.is_none_or(|t| t < -0.99),
        "{solution:?}"
    );
    assert!(solution.tied.error < 1e-6, "{solution:?}");
    let ratio = solution.tied.work / solution.tied.register;
    assert!((ratio - 676.0 / 268.0).abs() < 1e-9, "{solution:?}");
    let per_work = solution.tied.work + 3.0 * solution.tied.register;
    assert!((per_work - 500.0).abs() < 1e-3, "{solution:?}");
}

#[test]
fn the_exact_fit_refuses_too_few_words() {
    let few: Vec<Sample> = samples([200.0, 2.0, 100.0]).into_iter().take(5).collect();
    assert_eq!(
        solve(&few, current_weights()),
        Err(Refusal::TooFewWords { usable: 5 })
    );
}
