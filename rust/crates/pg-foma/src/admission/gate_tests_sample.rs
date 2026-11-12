use super::decide::{calibrate, calibration_candidates, sample_positions, Ctx};
use super::tests::{affix_grammar, batch, gated_signatures, plain_signatures, ON};
use super::{
    calibration_words, decide, parse_cost, restricted_parse, CALIBRATION_DIVISOR, K_MAX, K_MIN,
    MIN_BATCH, WARMUP,
};
use crate::admission::{build_rule_admission_within, UNLIMITED_UNITS};
use foma::options::FomaOptions;
use pg_parse::{Morpher, ParseOptions};

#[test]
fn the_sample_is_evenly_spaced_distinct_and_split_into_warm_up_and_candidates() {
    for n in [MIN_BATCH, MIN_BATCH + 1, 1000, ON, 1_000_003] {
        let (warm, candidates) = sample_positions(n);
        assert_eq!((warm.len(), candidates.len()), (WARMUP, K_MAX), "n {n}");
        let mut all: Vec<usize> = warm.iter().chain(&candidates).copied().collect();
        all.sort_unstable();
        assert!(all.windows(2).all(|w| w[0] < w[1]), "distinct, n {n}");
        let expected: Vec<usize> = (0..WARMUP + K_MAX).map(|m| m * n / 96).collect();
        assert_eq!(all, expected, "n {n}");
        assert!(candidates.windows(2).all(|w| w[0] < w[1]));
        assert_eq!(candidates[0], n / 96, "the first candidate is m = 1");
        assert_eq!(all[0], 0);
        assert!(all[95] < n);
    }
}

#[test]
fn a_batch_ordered_cheap_first_gets_the_decision_of_the_same_words_mixed() {
    let g = affix_grammar();
    let morpher = Morpher::new(&g, 2_000_000);
    let opts = ParseOptions::default();
    let mixed = batch(ON);
    let mut ordered = mixed.clone();
    ordered.sort_by_key(|w| (parse_cost(&morpher.parse_word_opts(w, &opts)), w.clone()));
    assert_ne!(ordered, mixed);
    let plain = plain_signatures(&morpher, &ordered, &opts);
    let (_, from_mixed) = gated_signatures(&g, &morpher, &mixed, (opts, 1));
    for threads in [1, 4] {
        let (gated, from_ordered) = gated_signatures(&g, &morpher, &ordered, (opts, threads));
        let (a, b) = (&from_mixed.report, &from_ordered.report);
        assert!(a.admission.is_on() && b.admission.is_on(), "{a}\n{b}");
        assert_eq!(a.tiled.is_on(), b.tiled.is_on(), "{a}\n{b}");
        assert_eq!(gated, plain, "threads {threads}: outcomes at their indices");
    }
}

#[test]
fn a_batch_of_the_shortest_length_samples_and_is_gated_or_declined_with_plain_outcomes() {
    let g = affix_grammar();
    let morpher = Morpher::new(&g, 2_000_000);
    let opts = ParseOptions::default();
    for count in [MIN_BATCH - 1, MIN_BATCH] {
        let words = batch(count);
        let (gated, decided) = gated_signatures(&g, &morpher, &words, (opts, 1));
        assert_eq!(gated, plain_signatures(&morpher, &words, &opts), "{count}");
        assert_eq!(decided.outcomes.is_empty(), count < MIN_BATCH, "{count}");
    }
    let words = batch(MIN_BATCH - 1);
    let decided = decide(&g, &morpher, &opts, &words).expect("sound");
    assert!(decided.report.to_string().contains("sample=even"));
}

#[test]
fn the_calibration_is_a_sixty_fourth_of_the_batch_between_the_minimum_and_the_maximum() {
    assert_eq!(MIN_BATCH, 512);
    assert_eq!(calibration_words(MIN_BATCH), K_MIN);
    for (n, k) in [
        (MIN_BATCH, 8),
        (1000, 15),
        (1600, 25),
        (2048, 32),
        (1_000_003, 32),
    ] {
        assert_eq!(calibration_words(n), k, "n {n}");
        assert!(
            k * CALIBRATION_DIVISOR <= n.max(K_MAX * CALIBRATION_DIVISOR),
            "n {n}"
        );
    }
}

#[test]
fn fewer_candidates_are_spread_over_the_batch_not_taken_from_its_head() {
    let (_, candidates) = sample_positions(1000);
    assert_eq!(calibration_candidates(&candidates, K_MAX), candidates);
    for k in [K_MIN, 15, 25] {
        let chosen = calibration_candidates(&candidates, k);
        assert_eq!(chosen.len(), k);
        assert!(chosen.windows(2).all(|w| w[0] < w[1]), "distinct, in order");
        assert!(chosen.iter().all(|c| candidates.contains(c)));
        assert!(
            *chosen.last().unwrap() > candidates[candidates.len() / 2],
            "k {k}"
        );
    }
}

#[test]
fn a_mid_sized_batch_calibrates_on_no_more_than_its_share_and_equals_plain() {
    let g = affix_grammar();
    let morpher = Morpher::new(&g, 2_000_000);
    let opts = ParseOptions::default();
    let admission =
        build_rule_admission_within(&FomaOptions::default(), &g, UNLIMITED_UNITS).expect("builds");
    for n in [MIN_BATCH, 1000, 3000] {
        let words = batch(n);
        let ctx = Ctx {
            grammar: &g,
            morpher: &morpher,
            options: &opts,
            stage: opts.with_guess_root(false),
        };
        let (_, all) = sample_positions(n);
        let chosen = calibration_candidates(&all, calibration_words(n));
        let mut outcomes = Vec::new();
        let cal = calibrate(
            &ctx,
            &admission,
            (&words, &chosen),
            &mut outcomes,
            &restricted_parse,
        )
        .expect("sound");
        assert!(cal.sample.len() <= calibration_words(n), "n {n}");
        assert_eq!(outcomes.len(), cal.sample.len(), "n {n}");
        let (gated, _) = gated_signatures(&g, &morpher, &words, (opts, 1));
        assert_eq!(gated, plain_signatures(&morpher, &words, &opts), "n {n}");
    }
}
