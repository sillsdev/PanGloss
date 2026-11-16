use super::*;
use pg_parse::{Morpher, ParseOptions};

#[test]
fn partials_bounds_are_checked_before_loading_files() {
    for value in ["0", "4", "-1", "256", "bad", ""] {
        for flags in [vec!["--partials", value], vec![]] {
            let mut args = vec![
                "unused.xml".into(),
                "unused.txt".into(),
                "unused.tsv".into(),
            ];
            if flags.is_empty() {
                args.push(format!("--partials={value}"));
            } else {
                args.extend(flags.into_iter().map(str::to_string));
            }
            assert!(run_batch(&args)
                .unwrap_err()
                .contains("--partials must be between 1 and 3"));
        }
    }
}

#[test]
fn no_root_and_root_gate_report_the_actual_rejector_without_affecting_parses_or_work() {
    for (xml, word, kind, object) in [
        (MINI_GRAMMAR_XML.to_owned(), "tat", "no-root", "stratum:0"),
        (
            MINI_GRAMMAR_XML.replace("id=\"le1-1\"", "id=\"le1-1\" isBound=\"true\""),
            "kat",
            "root-gate",
            "allomorph:0",
        ),
    ] {
        let g = pg_grammar::load(&xml).unwrap();
        let m = Morpher::new(&g, usize::MAX);
        let off = m.parse_word(word);
        let on = m.parse_word_opts(word, &ParseOptions::default().with_partials(3));
        assert!(on.analyses.is_empty());
        assert_eq!(off.structured, on.structured);
        assert_eq!(off.work_steps, on.work_steps);
        assert_eq!(on.partials[0].failure_kind, kind);
        assert_eq!(on.partials[0].rejecting_object_id, object);
        assert_eq!(on.partials[0].residue, word);
        assert!(on.partials[0].morpheme_ids.is_empty());
        assert!(off.partials.is_empty());
        let bounded = Morpher::new(&g, off.work_steps);
        let bounded_off = bounded.parse_word(word);
        let bounded_on = bounded.parse_word_opts(word, &ParseOptions::default().with_partials(3));
        assert_eq!(bounded_off.capped, off.capped);
        assert_eq!(bounded_on.capped, off.capped);
        assert_eq!(bounded_on.work_steps, off.work_steps);
    }
}

#[test]
fn stripped_affix_ids_and_residue_survive_lookup_and_affix_gate_rejections() {
    let base = homophonous_suffix_grammar_xml(1);
    // The analysis peels d; its stem-name requirement rejects the real root at replay/synthesis.
    let gated = base.replace("<NaturalClasses>", "<StemNames><StemName id=\"sn\"><Name>S</Name><Regions><Region partOfSpeech=\"posV\" /></Regions></StemName></StemNames><NaturalClasses>")
        .replace("<MorphologicalRule id=\"mrD0\"", "<MorphologicalRule requiredStemName=\"sn\" id=\"mrD0\"");
    for (xml, word, kind) in [(&base, "kakd", "no-root"), (&gated, "kadd", "affix-gate")] {
        let g = pg_grammar::load(xml).unwrap();
        let m = Morpher::new(&g, usize::MAX);
        let off = m.parse_word(word);
        let on = m.parse_word_opts(word, &ParseOptions::default().with_partials(3));
        assert_eq!(off.structured, on.structured);
        assert_eq!(off.work_steps, on.work_steps);
        assert!(on.analyses.is_empty());
        assert!(
            on.partials.iter().any(|p| p.failure_kind == kind
                && p.residue == word[..3]
                && p.morpheme_ids == ["mrD0"]),
            "{kind}: {:?}",
            on.partials
        );
    }
}

#[test]
fn batch_partials_column_is_opt_in_in_both_thread_modes_and_guesses_suppress_it() {
    for threads in ["1", "2"] {
        for k in ["1", "2", "3"] {
            let rows = run_batch_tsv_custom(
                "partials",
                MINI_GRAMMAR_XML,
                "tat\nkat\n",
                &["--threads", threads, "--partials", k, "--work-columns"],
            );
            let results: Vec<_> = rows
                .iter()
                .map(|r| r.split('\t').collect::<Vec<_>>())
                .filter(|f| f.len() > 3)
                .collect();
            assert_eq!(results.len(), 2);
            assert_eq!(results[0].len(), 7);
            let partials: serde_json::Value = serde_json::from_str(results[0][5]).unwrap();
            assert_eq!(partials[0]["failure_kind"], "no-root");
            assert_eq!(results[1][5], "[]");
        }
    }
    let f = pg_conformance_fixtures::require_fixture("edge-cases", "guesser-pattern-root-fallback");
    let g = pg_grammar::load(&f.load_grammar_xml()).unwrap();
    let m = Morpher::new(&g, usize::MAX);
    let off = m.parse_word_opts("gagd", &ParseOptions::default().with_guess_root(true));
    let on = m.parse_word_opts(
        "gagd",
        &ParseOptions::default()
            .with_guess_root(true)
            .with_partials(3),
    );
    assert!(on.guessed);
    assert_eq!(off.structured, on.structured);
    assert_eq!(off.work_steps, on.work_steps);
    assert!(on.partials.is_empty());
}

#[test]
fn default_writer_remains_byte_identical_with_fixed_elapsed() {
    let g = pg_grammar::load(MINI_GRAMMAR_XML).unwrap();
    let outcome = Morpher::new(&g, usize::MAX)
        .parse_word_opts("tat", &ParseOptions::default().with_partials(3));
    assert!(!outcome.partials.is_empty());
    for guess in [false, true] {
        let mut output = Vec::new();
        crate::write_batch_row(
            &mut output,
            0,
            "tat",
            7,
            "ok",
            "-",
            crate::BatchRowTail::new(
                crate::BatchRowFlags {
                    partials: false,
                    guess,
                    work_columns: false,
                    cost_column: false,
                },
                &outcome,
            ),
        )
        .unwrap();
        assert_eq!(
            output,
            if guess {
                b"0\ttat\t7\tok\t-\tfalse\n".as_slice()
            } else {
                b"0\ttat\t7\tok\t-\n".as_slice()
            }
        );
    }
}

#[test]
fn partials_preserve_cap_and_segmentation_stops() {
    let g = pg_grammar::load(&homophonous_suffix_grammar_xml(3)).unwrap();
    for cap in [1, 500] {
        let m = Morpher::new(&g, cap);
        for word in ["kadddd", "zzz"] {
            let off = m.parse_word(word);
            for k in 1..=3 {
                let on = m.parse_word_opts(word, &ParseOptions::default().with_partials(k));
                assert_eq!(off.structured, on.structured);
                assert_eq!(off.work_steps, on.work_steps);
                assert_eq!(off.capped, on.capped);
                assert_eq!(off.invalid_shape, on.invalid_shape);
                assert!(on.partials.len() <= k as usize);
            }
        }
    }
    let rows = run_batch_tsv_custom(
        "partials-equals",
        MINI_GRAMMAR_XML,
        "tat\n",
        &["--partials=1", "--guess"],
    );
    let row = rows.last().unwrap().split('\t').collect::<Vec<_>>();
    assert_eq!(row.len(), 7);
    assert_eq!(row[5], "false");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(row[6]).unwrap()[0]["failure_kind"],
        "no-root"
    );
}
