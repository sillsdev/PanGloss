//! Each staged ad hoc prohibition case carries a LibLCM-authored `fieldworks/project.fwdata`; importing it and compiling it must give the analysis counts `words.yaml` records from C# `hc.dll` on the case's `grammar.xml`, because the FieldWorks compile step is where the ad hoc prohibition copies were lost (#99, #100).

use pg_conformance_fixtures::require_fixture;
use pg_parse::{Morpher, ParseOptions};

/// Imports the case's FieldWorks witness and returns `(word, analysis count, expected count)` for every word in `words.yaml`.
fn witness_counts(case: &str) -> Vec<(String, usize, usize)> {
    let fixture = require_fixture("edge-cases", case);
    let project = fixture.dir.join("fieldworks").join("project.fwdata");
    assert!(project.is_file(), "{case}: missing {}", project.display());
    let (snapshot, _report) = pg_fwdata::import_file(&project).unwrap();
    let (grammar, _warnings) = pg_grammar::compile_project(&snapshot).unwrap();
    let morpher = Morpher::new(&grammar, usize::MAX);
    fixture
        .load_words_yaml()
        .words
        .iter()
        .map(|entry| {
            let out = morpher.parse_word_opts(&entry.word, &ParseOptions::default());
            assert!(
                !out.invalid_shape && !out.capped && !out.timed_out && !out.guessed,
                "{case}/{}: parse did not complete cleanly",
                entry.word
            );
            (entry.word.clone(), out.structured.len(), entry.parses.len())
        })
        .collect()
}

fn assert_witness_agrees(case: &str) {
    let counts = witness_counts(case);
    assert!(
        counts.iter().any(|(_, _, expected)| *expected == 0)
            && counts.iter().any(|(_, _, expected)| *expected > 0),
        "{case}: a witness needs both prohibited words and controls"
    );
    let wrong: Vec<String> = counts
        .iter()
        .filter(|(_, got, expected)| got != expected)
        .map(|(word, got, expected)| format!("{word}: {got} analyses, C# has {expected}"))
        .collect();
    assert!(
        wrong.is_empty(),
        "{case}: the FieldWorks project disagrees with grammar.xml on {}",
        wrong.join("; ")
    );
}

#[test]
fn fieldworks_witness_of_stem_msa_copies_matches_the_oracle() {
    assert_witness_agrees("adhoc-prohibition-stem-msa-copies");
}

#[test]
fn fieldworks_witness_of_affix_msa_copies_matches_the_oracle() {
    assert_witness_agrees("adhoc-prohibition-affix-msa-copies");
}

#[test]
fn fieldworks_witness_of_variant_msa_copies_matches_the_oracle() {
    assert_witness_agrees("adhoc-prohibition-variant-msa-copies");
}

#[test]
fn fieldworks_witness_of_affix_environment_copies_matches_the_oracle() {
    assert_witness_agrees("adhoc-prohibition-affix-environment-copies");
}

#[test]
fn fieldworks_witness_of_circumfix_pairings_matches_the_oracle() {
    assert_witness_agrees("adhoc-prohibition-circumfix-pairings");
}

#[test]
fn fieldworks_witness_of_variant_enclitic_first_matches_the_oracle() {
    assert_witness_agrees("adhoc-prohibition-variant-enclitic-first");
}

#[test]
fn fieldworks_witness_of_variant_main_first_enclitic_matches_the_oracle() {
    assert_witness_agrees("adhoc-prohibition-variant-main-first-enclitic");
}

#[test]
fn fieldworks_witness_of_allomorph_variant_control_matches_the_oracle() {
    assert_witness_agrees("adhoc-prohibition-allomorph-variant-control");
}

#[test]
fn fieldworks_witness_of_do_not_use_for_parsing_matches_the_unflagged_oracle() {
    // The project flags the `ta-` and `la` entries; grammar.xml is the unflagged oracle grammar (#154).
    let project = std::fs::read_to_string(
        require_fixture("edge-cases", "do-not-use-for-parsing-flag-ignored")
            .dir
            .join("fieldworks")
            .join("project.fwdata"),
    )
    .unwrap();
    assert_eq!(
        project
            .matches(r#"<DoNotUseForParsing val="True" />"#)
            .count(),
        2
    );
    assert_witness_agrees("do-not-use-for-parsing-flag-ignored");
}

#[test]
fn fieldworks_witness_of_complex_form_ref_variant_matches_the_oracle() {
    assert_witness_agrees("variant-complex-form-ref-senseless-entry");
}

#[test]
fn fieldworks_witness_of_clitic_typed_variant_matches_the_oracle() {
    assert_witness_agrees("variant-clitic-typed-form");
}

#[test]
fn fieldworks_witness_of_empty_required_template_slot_parses_like_the_oracle() {
    let case = "empty-required-template-slot";
    const BUNDLED_PROJECT: &[u8] =
        include_bytes!("data/empty-required-template-slot/project.fwdata");
    let has_suite_override = std::env::var_os(pg_conformance_fixtures::SUITE_ENV)
        .filter(|path| !path.is_empty())
        .is_some();
    let temp = if has_suite_override {
        None
    } else {
        Some(tempfile::tempdir().expect("temporary project directory"))
    };
    let fixture = has_suite_override.then(|| require_fixture("edge-cases", case));
    let project = if let Some(fixture) = &fixture {
        assert!(matches!(
            fixture.root,
            pg_conformance_fixtures::Root::Machine
        ));
        let override_root = pg_conformance_fixtures::suite_root();
        assert!(fixture.dir.starts_with(&override_root));
        assert!(fixture.dir.join("grammar.xml").is_file());
        assert!(fixture.dir.join("words.yaml").is_file());
        let external_project = fixture.dir.join("fieldworks/project.fwdata");
        let external_bytes = std::fs::read(&external_project).unwrap_or_else(|error| {
            panic!(
                "{case}: HC_CONFORMANCE_SUITE override is missing {}: {error}",
                external_project.display()
            )
        });
        assert_eq!(
            external_bytes, BUNDLED_PROJECT,
            "{case}: HC_CONFORMANCE_SUITE witness differs from the bundled regression witness"
        );
        external_project
    } else {
        let project = temp.as_ref().unwrap().path().join("project.fwdata");
        std::fs::write(&project, BUNDLED_PROJECT).expect("write bundled FieldWorks witness");
        project
    };

    let (snapshot, _report) = pg_fwdata::import_file(&project).unwrap();
    let (grammar, warnings) = pg_grammar::compile_project(&snapshot).unwrap();
    let morpher = Morpher::new(&grammar, usize::MAX);
    let actual: std::collections::BTreeMap<_, _> = ["amonu", "monu", "monua"]
        .into_iter()
        .map(|word| {
            let out = morpher.parse_word_opts(word, &ParseOptions::default());
            assert!(
                !out.invalid_shape && !out.capped && !out.timed_out && !out.guessed,
                "{case}/{word}: parse did not complete cleanly"
            );
            (word.to_string(), (out.structured.len(), out.signature()))
        })
        .collect();
    assert_eq!(
        actual
            .iter()
            .map(|(word, (count, _))| (word.clone(), *count))
            .collect::<std::collections::BTreeMap<_, _>>(),
        [
            ("amonu".to_string(), 0),
            ("monu".to_string(), 1),
            ("monua".to_string(), 1)
        ]
        .into_iter()
        .collect(),
        "{case}: unexpected parse counts from the FieldWorks project"
    );
    println!("{case} actual counts/signatures: {actual:#?}");

    let incomplete_slot = warnings
        .iter()
        .find(|warning| warning.code == "grammar.template.slot-no-rules")
        .unwrap_or_else(|| panic!("{case}: missing grammar.template.slot-no-rules warning"));
    let title = pg_snapshot::ImportWarningCode::ALL
        .iter()
        .find(|code| code.wire() == incomplete_slot.code)
        .map(|code| pg_snapshot::import_warning_metadata((*code).clone()).group_name);
    assert_eq!(title, Some("Incomplete affix slot"));
    println!(
        "{case} warning: title={}, full={incomplete_slot:#?}",
        title.unwrap(),
    );
    assert_eq!(
        incomplete_slot.message,
        "Incomplete affix slot 'slotEmptyRequired' in affix template 'probeTemplate': it has no loaded inflectional affixes, so parsing skips it."
    );

    if let Some(fixture) = fixture {
        let words = fixture.load_words_yaml();
        let expected_counts: std::collections::BTreeMap<_, _> = words
            .words
            .iter()
            .map(|word| (word.word.clone(), word.parses.len()))
            .collect();
        assert_eq!(
            expected_counts,
            [
                ("amonu".to_string(), 0),
                ("monu".to_string(), 1),
                ("monua".to_string(), 1)
            ]
            .into_iter()
            .collect(),
            "{case}: C# fixture rows or counts differ from the required regression"
        );
        for word in &words.words {
            if let Some((_, signature)) = actual.get(&word.word) {
                assert!(
                    pg_conformance_fixtures::signatures_equal(
                        signature,
                        &word.expected_signature()
                    ),
                    "{case}/{}: PanGloss signature {signature:?} differs from C# fixture signature {:?}",
                    word.word,
                    word.expected_signature()
                );
            }
        }
        println!(
            "{case} C# fixture signatures: {:#?}",
            words
                .words
                .iter()
                .map(|word| (&word.word, word.expected_signature()))
                .collect::<Vec<_>>()
        );
    }
}
