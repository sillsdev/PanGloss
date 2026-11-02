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
