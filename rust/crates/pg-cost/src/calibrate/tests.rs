use super::*;

fn samples(weights: [f64; 4]) -> Vec<Sample> {
    (1..40u64)
        .map(|i| {
            let counts = Counts {
                work: 1000 + i * 700,
                bytes: 90_000 + (i * i * 913) % 400_000,
                registers: 500 + (i * i * i * 131) % 4_000,
                allocations: 800 + (i * 7_919) % 3_001,
            };
            let measured = counts.work as f64 * weights[0]
                + counts.bytes as f64 * weights[1]
                + counts.registers as f64 * weights[2]
                + counts.allocations as f64 * weights[3];
            Sample { counts, measured }
        })
        .collect()
}

/// Weights that every term carries, so the bounded fit can move each one.
fn all_terms() -> [f64; 4] {
    [200.0, 1.25, 20.0, 150.0]
}

#[test]
fn the_weights_in_use_are_instruction_sized() {
    assert_eq!(current_weights(), [200.0, 1.25, 0.0, 150.0]);
}

#[test]
fn the_fit_leaves_weights_that_describe_the_counts_alone() {
    let fit = fit(&samples(all_terms()), None).unwrap();
    assert!((fit.byte_factor - 1.0).abs() < 0.1, "{fit:?}");
    assert!((fit.allocation_factor - 1.0).abs() < 0.1, "{fit:?}");
    assert!(fit.speed > 0.0, "{fit:?}");
}

#[test]
fn machine_speed_does_not_move_a_weight() {
    let slower = fit(&samples(all_terms().map(|w| w * 0.37)), None).unwrap();
    let same = fit(&samples(all_terms()), None).unwrap();
    assert!(
        (slower.speed / same.speed - 0.37).abs() < 0.01,
        "{slower:?}"
    );
}

#[test]
fn a_real_shift_moves_only_that_weight() {
    let mut weights = current_weights();
    weights[3] *= 1.3;
    let fit = fit(&samples(weights), None).unwrap();
    assert!(fit.allocation_factor > fit.work_factor * 1.1, "{fit:?}");
    assert!(fit.allocation_factor > fit.byte_factor * 1.1, "{fit:?}");
}

#[test]
fn a_wild_measurement_stops_at_the_bound() {
    let mut weights = current_weights();
    weights[3] *= 50.0;
    let fit = fit(&samples(weights), None).unwrap();
    assert_eq!(fit.allocation_factor, MAX_FACTOR);
}

#[test]
fn a_busy_machine_is_refused_and_instructions_never_are() {
    let measured = samples(current_weights());
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
            allocations: 10,
        },
        measured: 1.0,
    }; 50];
    assert_eq!(fit(&tiny, None), Err(Refusal::TooFewWords { usable: 0 }));
}

#[test]
fn the_exact_fit_recovers_the_weights_behind_the_counts() {
    let solution = solve(&samples(all_terms())).unwrap();
    assert!((solution.free.work - 200.0).abs() < 1e-6, "{solution:?}");
    assert!((solution.free.byte - 1.25).abs() < 1e-9, "{solution:?}");
    assert!((solution.free.register - 20.0).abs() < 1e-6, "{solution:?}");
    assert!(
        (solution.free.allocation - 150.0).abs() < 1e-6,
        "{solution:?}"
    );
    assert!(solution.free.error < 1e-6, "{solution:?}");
    assert_eq!(solution.words, 39);
}

#[test]
fn leaving_out_a_term_the_words_carry_costs_the_fit() {
    let solution = solve(&samples(all_terms())).unwrap();
    for (term, error) in solution.error_without.iter().enumerate() {
        assert!(*error > 0.002, "term {term}: {solution:?}");
    }
}

#[test]
fn a_weight_is_never_negative() {
    let data: Vec<Sample> = samples([300.0, 2.0, 0.0, 150.0])
        .into_iter()
        .map(|sample| Sample {
            measured: sample.measured - 10.0 * sample.counts.registers as f64,
            ..sample
        })
        .collect();
    let solution = solve(&data).unwrap();
    assert_eq!(solution.free.register, 0.0, "{solution:?}");
    assert!(
        solution.free.work > 0.0 && solution.free.byte > 0.0 && solution.free.allocation > 0.0,
        "{solution:?}"
    );
    assert_eq!(solution.error_without[2], solution.free.error);
}

#[test]
fn the_exact_fit_refuses_too_few_words() {
    let few: Vec<Sample> = samples(all_terms()).into_iter().take(5).collect();
    assert_eq!(solve(&few), Err(Refusal::TooFewWords { usable: 5 }));
}
