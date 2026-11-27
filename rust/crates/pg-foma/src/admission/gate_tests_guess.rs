use super::tests::{affix_grammar, batch, gated_signatures, plain_signatures, LETTERS, ON};
use super::{decide, restricted_parse, staged, Prefilter, Stages};
use crate::admission::tests::{load, prefix, spec, suffix};
use crate::admission::{build_rule_admission, build_tiled};
use foma::options::FomaOptions;
use pg_grammar::model::Grammar;
use pg_parse::{Morpher, ParseOptions, ParseOutcome};

#[test]
fn a_guess_only_parse_is_off() {
    let g = affix_grammar();
    let morpher = Morpher::new(&g, 2_000_000);
    let words = batch(ON);
    let opts = ParseOptions::default().with_guess_only(true);
    let decided = decide(&g, &morpher, &opts, &words).expect("sound");
    assert!(!decided.prefilter.admission_on());
    assert!(decided.report.to_string().contains("guess-only"));
}

#[test]
fn guessing_without_a_lexical_pattern_counts_as_not_guessing() {
    let g = affix_grammar();
    let morpher = Morpher::new(&g, 2_000_000);
    let words = batch(ON);
    let opts = ParseOptions::default().with_guess_root(true);
    let plain = plain_signatures(&morpher, &words, &opts);
    for threads in [1, 4] {
        let (gated, decided) = gated_signatures(&g, &morpher, &words, (opts, threads));
        let line = decided.report.to_string();
        assert!(decided.report.admission.is_on(), "{line}");
        assert!(line.contains("guess=no-patterns"), "{line}");
        assert_eq!(gated, plain, "threads {threads}: {line}");
    }
}

/// The helpful grammar plus a lexical pattern: words starting with `d` guess, the others parse or fail.
fn guessing_grammar() -> Grammar {
    let mut rules = Vec::new();
    for (i, lit) in [
        "ba", "bi", "bo", "da", "di", "do", "ga", "gi", "go", "ka", "ki", "ko",
    ]
    .iter()
    .enumerate()
    {
        rules.push(suffix(i, lit));
    }
    for (i, lit) in ["na", "ni", "no"].iter().enumerate() {
        rules.push(prefix(12 + i, lit));
    }
    let roots = ["pat", "pit", "put", "lat", "lit", "d[Any]*"];
    load(&spec(LETTERS, rules, &roots))
}

#[test]
fn guessing_with_a_lexical_pattern_gives_the_plain_outcome_for_every_kind_of_word() {
    let g = guessing_grammar();
    let morpher = Morpher::new(&g, 2_000_000);
    let words = batch(ON);
    let opts = ParseOptions::default().with_guess_root(true);
    let outcomes: Vec<ParseOutcome> = words
        .iter()
        .map(|w| morpher.parse_word_opts(w, &opts))
        .collect();
    let plain: Vec<String> = outcomes.iter().map(ParseOutcome::signature).collect();
    assert!(outcomes.iter().any(|o| o.guessed), "some word guesses");
    assert!(
        outcomes
            .iter()
            .any(|o| !o.guessed && !o.analyses.is_empty()),
        "some word parses"
    );
    assert!(
        outcomes.iter().any(|o| o.analyses.is_empty()),
        "some word has no analysis"
    );
    for threads in [1, 4] {
        let (gated, decided) = gated_signatures(&g, &morpher, &words, (opts, threads));
        let line = decided.report.to_string();
        assert!(line.contains("guess=orchestrated"), "{line}");
        assert!(decided.report.admission.is_on(), "{line}");
        assert_eq!(gated, plain, "threads {threads}: {line}");
    }
}

#[test]
fn a_rejected_word_that_misses_goes_straight_to_the_guess_and_gives_the_plain_outcome() {
    let g = guessing_grammar();
    let morpher = Morpher::new(&g, 2_000_000);
    let opts = ParseOptions::default().with_guess_root(true);
    let admission = build_rule_admission(&FomaOptions::default(), &g).expect("admission");
    let mut misses = 0;
    for word in batch(240) {
        let plain = morpher.parse_word_opts(&word, &opts);
        if plain.analyses.is_empty() || plain.guessed {
            misses += 1;
            let admitted = admission.admit(&word);
            let run = staged(&morpher, &word, (&admitted, true), &opts, &restricted_parse);
            assert_eq!(run.outcome.signature(), plain.signature(), "{word}");
            assert_eq!(run.outcome.guessed, plain.guessed, "{word}");
            assert_eq!(run.first_cost, 0);
        }
    }
    assert!(misses > 0);
}

#[test]
fn the_tiled_stage_in_a_guessing_pipeline_keeps_the_plain_outcome() {
    let g = guessing_grammar();
    let morpher = Morpher::new(&g, 2_000_000);
    let opts = ParseOptions::default().with_guess_root(true);
    let admission = build_rule_admission(&FomaOptions::default(), &g).expect("admission");
    let tiled = build_tiled(&FomaOptions::default(), &g, &admission);
    assert!(tiled.active());
    let prefilter = Prefilter {
        stages: Some(Stages {
            admission,
            tiled: Some(tiled),
        }),
    };
    assert!(prefilter.tiled_on());
    let stage = opts.with_guess_root(false);
    let stages = prefilter.stages.as_ref().expect("stages");
    let mut rejected = 0;
    for word in batch(240) {
        let plain = morpher.parse_word_opts(&word, &opts);
        let parsed = prefilter.parse(&morpher, &word, &opts);
        assert_eq!(parsed.signature(), plain.signature(), "{word}");
        assert_eq!(parsed.guessed, plain.guessed, "{word}");
        let admitted = stages.admission.admit(&word);
        let tiled = stages.tiled.as_ref().expect("tiled");
        rejected += usize::from(tiled.rejects(&word, &admitted, &stage));
    }
    assert!(rejected > 0, "the tiled stage rejected words");
}

#[test]
fn a_word_that_does_not_segment_keeps_its_invalid_shape_through_the_guessing_stages() {
    let g = guessing_grammar();
    let morpher = Morpher::new(&g, 2_000_000);
    let opts = ParseOptions::default().with_guess_root(true);
    let admission = build_rule_admission(&FomaOptions::default(), &g).expect("admission");
    let tiled = build_tiled(&FomaOptions::default(), &g, &admission);
    let prefilter = Prefilter {
        stages: Some(Stages {
            admission,
            tiled: Some(tiled),
        }),
    };
    let stages = prefilter.stages.as_ref().expect("stages");
    let mut invalid = 0;
    for word in ["zzz", "patz", "dzz", "zpat", "nazit"] {
        let plain = morpher.parse_word_opts(word, &opts);
        invalid += usize::from(plain.invalid_shape);
        let parsed = prefilter.parse(&morpher, word, &opts);
        assert_eq!(parsed.invalid_shape, plain.invalid_shape, "{word}");
        assert_eq!(parsed.signature(), plain.signature(), "{word}");
        assert_eq!(parsed.guessed, plain.guessed, "{word}");
        let admitted = stages.admission.admit(word);
        let misses = plain.analyses.is_empty() || plain.guessed;
        for rejected in [false, true].into_iter().filter(|r| !r || misses) {
            let run = staged(
                &morpher,
                word,
                (&admitted, rejected),
                &opts,
                &restricted_parse,
            );
            assert_eq!(
                run.outcome.invalid_shape, plain.invalid_shape,
                "{word} {rejected}"
            );
            assert_eq!(
                run.outcome.signature(),
                plain.signature(),
                "{word} {rejected}"
            );
        }
    }
    assert!(invalid > 0, "some word does not segment");
}
