//! Differential coverage of diagnostic collection across the conformance inventory.

use pg_parse::{Morpher, ParseOptions};

#[test]
fn partials_preserve_complete_analyses_statuses_and_work_across_conformance() {
    let fixtures = pg_conformance_fixtures::discover();
    assert!(!fixtures.is_empty());
    let mut words = 0;
    let mut summaries = 0;
    for fixture in fixtures {
        let cases = fixture.load_words_yaml();
        if cases.skip_in_generic_replay().is_some() {
            continue;
        }
        let grammar = fixture.load_grammar();
        let morpher = Morpher::new(&grammar, usize::MAX);
        let options =
            ParseOptions::default().with_guess_root(cases.morpher.guess_roots.unwrap_or(false));
        for case in cases.words {
            let off = morpher.parse_word_opts(&case.word, &options);
            let on = morpher.parse_word_opts(&case.word, &options.with_partials(3));
            assert_eq!(off.analyses, on.analyses, "{}", fixture.label());
            assert_eq!(off.structured, on.structured, "{}", fixture.label());
            assert_eq!(off.work_steps, on.work_steps, "{}", fixture.label());
            assert_eq!(
                (
                    off.capped,
                    off.capture_capped,
                    off.word_mem_capped,
                    off.invalid_shape,
                    off.timed_out,
                    off.guessed,
                    off.candidates_generated
                ),
                (
                    on.capped,
                    on.capture_capped,
                    on.word_mem_capped,
                    on.invalid_shape,
                    on.timed_out,
                    on.guessed,
                    on.candidates_generated
                ),
                "{}",
                fixture.label()
            );
            assert!(off.partials.is_empty());
            assert!(on.partials.len() <= 3);
            if !on.analyses.is_empty() {
                assert!(on.partials.is_empty());
            }
            let keys: Vec<_> = on
                .partials
                .iter()
                .map(|p| (p.residue.chars().count(), p.work_steps))
                .collect();
            assert!(keys.windows(2).all(|pair| pair[0] <= pair[1]));
            summaries += on.partials.len();
            words += 1;
        }
    }
    assert!(summaries > 0);
    eprintln!("partials preservation: {words} words, {summaries} rejection summaries");
}
