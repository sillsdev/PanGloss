//! The early root check of the last-analysed stratum changes no word at any work cap: the same analyses in the same order, for the same work.

use pg_conformance_fixtures::discover;
use pg_parse::{Morpher, ParseOptions};
use pg_rules::trace::TreeTraceSink;

#[test]
fn every_conformance_word_parses_the_same_with_and_without_the_early_lookup() {
    let (mut words, mut with_analyses, mut fixtures) = (0, 0, 0);
    for fixture in discover() {
        let yaml = fixture.load_words_yaml();
        if yaml.skip_in_generic_replay().is_some() {
            continue;
        }
        let grammar = fixture.load_grammar_with_settings(&yaml.morpher);
        let early = Morpher::new(&grammar, usize::MAX).with_early_root_check_from(0);
        let late = Morpher::new(&grammar, usize::MAX).with_early_root_lookup(false);
        fixtures += 1;
        for entry in &yaml.words {
            for guess_root in [false, true] {
                let opts = ParseOptions::default().with_guess_root(guess_root);
                let a = early.parse_word_opts(&entry.word, &opts);
                let b = late.parse_word_opts(&entry.word, &opts);
                let at = format!(
                    "{} `{}` guess_root={guess_root}",
                    fixture.label(),
                    entry.word
                );
                assert_eq!(a.analyses, b.analyses, "{at}");
                assert_eq!(a.structured, b.structured, "{at}");
                assert_eq!(a.work_steps, b.work_steps, "{at}");
                assert_eq!(
                    (a.capped, a.invalid_shape, a.guessed, a.candidates_generated),
                    (b.capped, b.invalid_shape, b.guessed, b.candidates_generated),
                    "{at}"
                );
                words += 1;
                with_analyses += usize::from(!a.analyses.is_empty());
            }
            let counted = |morpher: &Morpher| {
                let (outcome, mut rows) =
                    morpher.parse_word_with_stats(&entry.word, &ParseOptions::default());
                rows.iter_mut()
                    .for_each(|row| row.counters.self_time_ns = 0);
                (outcome, rows)
            };
            let ((a, a_rows), (b, b_rows)) = (counted(&early), counted(&late));
            assert_eq!(
                a.analyses,
                b.analyses,
                "{} `{}`",
                fixture.label(),
                entry.word
            );
            assert_eq!(a_rows, b_rows, "{} `{}`", fixture.label(), entry.word);
        }
    }
    assert!(
        fixtures > 100 && words > 1000 && with_analyses > 500,
        "the sweep covered {fixtures} fixtures, {words} parses, {with_analyses} with analyses"
    );
}

#[test]
fn a_capped_word_gives_the_same_partial_result_either_way() {
    let (mut capped, mut finished) = (0, 0);
    for fixture in discover() {
        let yaml = fixture.load_words_yaml();
        if yaml.skip_in_generic_replay().is_some() {
            continue;
        }
        let grammar = fixture.load_grammar_with_settings(&yaml.morpher);
        for entry in yaml.words.iter().take(3) {
            let full = Morpher::new(&grammar, usize::MAX)
                .parse_word(&entry.word)
                .work_steps;
            let near =
                (full.saturating_sub(3)..full + 2).chain([1, 8, 40, 200, full / 2, full * 3 / 4]);
            for cap in near.filter(|&cap| cap > 0) {
                let a = Morpher::new(&grammar, cap)
                    .with_early_root_check_from(0)
                    .parse_word(&entry.word);
                let b = Morpher::new(&grammar, cap)
                    .with_early_root_lookup(false)
                    .parse_word(&entry.word);
                let at = format!("{} `{}` cap {cap}", fixture.label(), entry.word);
                assert_eq!(a.capped, b.capped, "{at}");
                assert_eq!(a.analyses, b.analyses, "{at}");
                assert_eq!(a.work_steps, b.work_steps, "{at}");
                capped += usize::from(a.capped);
                finished += usize::from(!a.capped);
            }
        }
    }
    assert!(
        capped > 100 && finished > 100,
        "{capped} capped and {finished} finished parses were compared"
    );
}

#[test]
fn a_trace_keeps_every_node_whatever_the_setting() {
    let mut nodes = 0;
    for fixture in discover().into_iter().take(40) {
        let yaml = fixture.load_words_yaml();
        if yaml.skip_in_generic_replay().is_some() {
            continue;
        }
        let grammar = fixture.load_grammar_with_settings(&yaml.morpher);
        for entry in yaml.words.iter().take(2) {
            let run = |early: bool| {
                let sink = TreeTraceSink::new();
                let morpher = Morpher::new(&grammar, usize::MAX);
                let morpher = if early {
                    morpher.with_early_root_check_from(0)
                } else {
                    morpher.with_early_root_lookup(false)
                };
                let outcome =
                    morpher.parse_word_traced(&entry.word, &ParseOptions::default(), &sink);
                (outcome.analyses, outcome.work_steps, sink.len())
            };
            let (on, off) = (run(true), run(false));
            assert_eq!(on, off, "{} `{}`", fixture.label(), entry.word);
            nodes += on.2;
        }
    }
    assert!(nodes > 100, "the traces held {nodes} nodes");
}
