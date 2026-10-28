use super::*;

// These exercise `parse`/`discover_scoped` directly; `claimed_scope`'s env var is process-wide.

#[test]
fn scope_parse_accepts_exactly_the_two_claims() {
    assert_eq!(
        ConformanceScope::parse("local"),
        Ok(ConformanceScope::Local)
    );
    assert_eq!(ConformanceScope::parse("all"), Ok(ConformanceScope::All));
    assert_eq!(
        ConformanceScope::parse("  all  "),
        Ok(ConformanceScope::All),
        "a claim passed through a shell should survive incidental whitespace"
    );
}

#[test]
fn scope_parse_refuses_anything_else_including_the_tempting_ones() {
    // "" is a set-but-empty env var; the rest are near-misses that must not resolve to a scope.
    for value in [
        "", "  ", "both", "ALL", "Local", "staging", "machine", "true", "1",
    ] {
        assert!(
            ConformanceScope::parse(value).is_err(),
            "{value:?} must not parse as a scope"
        );
    }
}

#[test]
fn an_absent_claim_is_refused_rather_than_defaulted() {
    // Falsify by making this return a scope: no claim must mean NO scope, never a quiet one.
    let refusal = scope_from_env_value(None).expect_err("an absent claim must not resolve");
    assert!(
        refusal.contains(SCOPE_ENV) && refusal.contains("conformance-test"),
        "the refusal must name the variable and how to claim it, got: {refusal}"
    );
}

#[test]
fn a_present_claim_is_honoured_and_a_bogus_one_refused() {
    assert_eq!(
        scope_from_env_value(Some("local")),
        Ok(ConformanceScope::Local)
    );
    assert_eq!(scope_from_env_value(Some("all")), Ok(ConformanceScope::All));
    assert!(scope_from_env_value(Some("")).is_err());
    assert!(scope_from_env_value(Some("both")).is_err());
}

#[test]
fn scope_labels_round_trip_through_parse() {
    for scope in [ConformanceScope::Local, ConformanceScope::All] {
        assert_eq!(ConformanceScope::parse(scope.label()), Ok(scope));
    }
}

#[test]
fn local_scope_reaches_no_upstream_fixture() {
    // A green local run must never borrow credit from an upstream fixture.
    assert!(
        discover_scoped(ConformanceScope::Local)
            .iter()
            .all(|f| f.root == Root::Staging),
        "local scope must yield staged fixtures only"
    );
}

#[test]
fn require_fixture_finds_an_upstream_fixture_whatever_the_claimed_scope_is() {
    // A named pin makes no coverage claim, so scope must not hide a healthy upstream fixture.
    let found = require_fixture("edge-cases", "loader-isactive");
    assert_eq!(found.root, Root::Machine);
    assert!(found.grammar_path().is_file());
}

#[test]
#[should_panic(expected = "no conformance fixture `edge-cases/definitely-not-a-fixture`")]
fn require_fixture_panics_rather_than_returning_nothing() {
    // The whole point: 32 legacy guard sites returned early instead, and skipped for months.
    let _ = require_fixture("edge-cases", "definitely-not-a-fixture");
}

#[test]
fn all_scope_is_a_superset_of_local_scope() {
    let local = discover_scoped(ConformanceScope::Local);
    let all = discover_scoped(ConformanceScope::All);
    assert!(
        all.len() >= local.len(),
        "all scope ({}) must cover at least what local does ({})",
        all.len(),
        local.len()
    );
    for fixture in &local {
        assert!(
            all.iter().any(|f| f.dir == fixture.dir),
            "{} is in local scope but missing from all scope",
            fixture.label()
        );
    }
}

#[test]
fn discover_tolerates_absent_roots() {
    // Must not panic even if neither root exists on disk, exercised here with a nonexistent directory.
    let mut out = Vec::new();
    scan_one_root(
        Path::new("/definitely/does/not/exist"),
        Root::Machine,
        &mut out,
    );
    assert!(out.is_empty());
}

#[test]
fn graduation_guard_flags_same_key_both_roots() {
    let fixtures = vec![
        FixtureRef {
            root: Root::Machine,
            category: "edge-cases".into(),
            name: "foo".into(),
            dir: PathBuf::from("machine/conformance/edge-cases/foo"),
        },
        FixtureRef {
            root: Root::Staging,
            category: "edge-cases".into(),
            name: "foo".into(),
            dir: PathBuf::from("conformance-staging/edge-cases/foo"),
        },
        FixtureRef {
            root: Root::Staging,
            category: "edge-cases".into(),
            name: "bar".into(),
            dir: PathBuf::from("conformance-staging/edge-cases/bar"),
        },
    ];
    let violations = graduation_guard_violations(&fixtures);
    assert_eq!(
        violations,
        vec![("edge-cases".to_string(), "foo".to_string())]
    );
}

#[test]
fn words_yaml_parses_minimal_doc() {
    let doc = r#"
language: Test
requires: [phonology]
words:
  - word: foo
    note: A test word.
    parses:
      - signature: "M1|foo"
        rules: []
  - word: bar
    note: A word expected to fail.
    expect_fail: true
"#;
    let parsed: WordsYaml = serde_yaml::from_str(doc).unwrap();
    assert_eq!(parsed.language, "Test");
    assert_eq!(parsed.requires, vec!["phonology".to_string()]);
    assert_eq!(parsed.words.len(), 2);
    assert_eq!(parsed.words[0].expected_signature(), "M1|foo");
    assert_eq!(parsed.words[1].expected_signature(), "-");
    assert_eq!(
        parsed.fieldworks_producible,
        FieldworksProducibility::Unmarked
    );
}

fn minimal_doc_with(front_matter: &str) -> Result<WordsYaml, serde_yaml::Error> {
    serde_yaml::from_str(&format!(
        "language: Test\n{front_matter}\nwords:\n  - word: foo\n    note: A test word.\n    expect_fail: true\n"
    ))
}

#[test]
fn fieldworks_producible_true_parses() {
    let parsed = minimal_doc_with("fieldworks_producible: true").unwrap();
    assert_eq!(
        parsed.fieldworks_producible,
        FieldworksProducibility::Producible
    );
}

#[test]
fn fieldworks_producible_false_with_notes_parses() {
    let parsed = minimal_doc_with(
        "fieldworks_producible: false\nfieldworks_producible_notes: RealizationalRule never constructed by HCLoader",
    )
    .unwrap();
    assert_eq!(
        parsed.fieldworks_producible,
        FieldworksProducibility::EngineOnly {
            notes: "RealizationalRule never constructed by HCLoader".to_string()
        }
    );
}

#[test]
fn fieldworks_producible_false_without_notes_is_a_parse_error_naming_the_field() {
    let error = minimal_doc_with("fieldworks_producible: false")
        .expect_err("false with no notes must not silently parse");
    assert!(
        error.to_string().contains("fieldworks_producible_notes"),
        "error must name the missing field, got: {error}"
    );
}

#[test]
fn fieldworks_producible_false_with_empty_notes_is_a_parse_error() {
    let error = minimal_doc_with("fieldworks_producible: false\nfieldworks_producible_notes: \"\"")
        .expect_err("empty notes must not satisfy the non-empty requirement");
    assert!(error.to_string().contains("fieldworks_producible_notes"));
}

#[test]
fn fieldworks_producible_garbage_value_is_a_parse_error() {
    let error = minimal_doc_with("fieldworks_producible: sometimes")
        .expect_err("a non-boolean value must not resolve to a producibility");
    assert!(error.to_string().contains("fieldworks_producible"));
}

#[test]
fn producibility_census_sorts_and_partitions() {
    let base = std::env::temp_dir().join(format!(
        "pg-conformance-fixtures-census-test-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&base);
    let write_fixture = |name: &str, front_matter: &str| -> FixtureRef {
        let dir = base.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("grammar.xml"), "<HermitCrabInput/>").unwrap();
        std::fs::write(
            dir.join("words.yaml"),
            format!(
                "language: Test\n{front_matter}\nwords:\n  - word: test\n    note: Census fixture word.\n    expect_fail: true\n"
            ),
        )
        .unwrap();
        FixtureRef {
            root: Root::Staging,
            category: "edge-cases".to_string(),
            name: name.to_string(),
            dir,
        }
    };

    let fixtures = vec![
        write_fixture("z-fixture", "fieldworks_producible: true"),
        write_fixture("a-fixture", "fieldworks_producible: true"),
        write_fixture(
            "m-fixture",
            "fieldworks_producible: false\nfieldworks_producible_notes: x",
        ),
        write_fixture("b-fixture", ""),
    ];

    let census = producibility_census(&fixtures);
    std::fs::remove_dir_all(&base).ok();

    assert_eq!(
        census.producible,
        vec![
            "staging:edge-cases/a-fixture".to_string(),
            "staging:edge-cases/z-fixture".to_string(),
        ],
        "producible must be sorted by label"
    );
    assert_eq!(
        census.engine_only,
        vec!["staging:edge-cases/m-fixture".to_string()]
    );
    assert_eq!(
        census.unmarked,
        vec!["staging:edge-cases/b-fixture".to_string()]
    );
}

#[test]
fn oracle_provenance_marker_parses_both_recognized_values() {
    assert_eq!(
        parse_oracle_provenance_marker(
            "# oracle-provenance: founding-oracle machine-commit=abc\nlanguage: X\n"
        ),
        Some(OracleProvenance::FoundingOracle)
    );
    assert_eq!(
        parse_oracle_provenance_marker("# oracle-provenance: rust-only\nlanguage: X\n"),
        Some(OracleProvenance::RustOnly)
    );
}

#[test]
fn oracle_provenance_marker_is_none_when_absent_or_unrecognized() {
    assert_eq!(
        parse_oracle_provenance_marker("language: X\nwords: []\n"),
        None
    );
    assert_eq!(
        parse_oracle_provenance_marker("# oracle-provenance: something-else\n"),
        None
    );
}

#[test]
fn discover_filter_passes_returns_only_the_staging_filter_passes_category() {
    // Falsify by widening scan_one_category's category list to include edge-cases/languages.
    for f in discover_filter_passes() {
        assert_eq!(f.root, Root::Staging);
        assert_eq!(f.category, "filter-passes");
    }
}

#[test]
fn all_staged_fixtures_is_local_scope_union_filter_passes() {
    let local = discover_scoped(ConformanceScope::Local);
    let filter_passes = discover_filter_passes();
    let combined = all_staged_fixtures();
    assert_eq!(combined.len(), local.len() + filter_passes.len());
    assert!(combined.iter().all(|f| f.root == Root::Staging));
}

fn words_yaml_with(front_matter: &str) -> Result<WordsYaml, serde_yaml::Error> {
    serde_yaml::from_str(&format!(
        "language: T\n{front_matter}words:\n  - word: kat\n    note: A test word.\n    parses:\n      - signature: \"KAT|kat\"\n        rules: []\n"
    ))
}

#[test]
fn an_absent_morpher_block_means_the_defaults() {
    let words = words_yaml_with("").unwrap();
    assert!(words.morpher.is_empty());
}

#[test]
fn every_accepted_morpher_key_is_read() {
    let words = words_yaml_with(
        "requires: [morpher-settings]\nmorpher:\n  DelReapps: 1\n  MaxRoots: 3\n  MergeAnalyses: false\n  MakePartialsFinal: true\n  GuessRoots: true\n",
    )
    .unwrap();
    assert_eq!(
        words.morpher,
        MorpherSettings {
            del_reapps: Some(1),
            max_roots: Some(3),
            merge_analyses: Some(false),
            make_partials_final: Some(true),
            guess_roots: Some(true),
        }
    );
}

#[test]
fn a_morpher_block_the_harness_cannot_apply_is_refused() {
    for (block, expected) in [
        ("morpher:\n  MergeMSAs: true\n", "unsupported"),
        ("morpher:\n  MaxAlternatives: 5\n", "unsupported"),
        ("morpher:\n  DelReapps: one\n", "morpher.DelReapps"),
        ("morpher:\n  DelReapps: -1\n", "morpher.DelReapps"),
        ("morpher:\n  DelReapps: 2147483648\n", "morpher.DelReapps"),
        ("morpher:\n  MaxRoots: 0\n", "morpher.MaxRoots"),
        ("morpher:\n  MaxRoots: 2147483648\n", "morpher.MaxRoots"),
        ("morpher:\n  MergeAnalyses: yes\n", "morpher.MergeAnalyses"),
        ("morpher:\n  GuessRoots: yes\n", "morpher.GuessRoots"),
        ("morpher: {}\n", "empty block"),
        ("morpher: 1\n", "expected a mapping"),
    ] {
        let err = words_yaml_with(&format!("requires: [morpher-settings]\n{block}"))
            .expect_err(block)
            .to_string();
        assert!(err.contains(expected), "{block:?}: {err}");
    }
}

#[test]
fn words_and_parses_require_nonempty_schema_values_and_reject_unknown_fields() {
    for (word_fields, expected) in [
        ("", "missing field `note`"),
        ("    note: \"\"\n", "note must be nonempty"),
        ("    note: valid\n    parses:\n      - signature: \"\"\n        rules: []\n", "signature must be nonempty"),
        ("    note: valid\n    parses:\n      - signature: OK|foo\n", "missing field `rules`"),
        ("    note: valid\n    parses:\n      - signature: OK|foo\n        rules: [\"\"]\n", "rules[0] must be nonempty"),
        ("    note: valid\n    unknown_word_field: true\n", "unknown field `unknown_word_field`"),
        ("    note: valid\n    parses:\n      - signature: OK|foo\n        rules: []\n        unknown_parse_field: true\n", "unknown field `unknown_parse_field`"),
        ("    note: valid\n    claimed_cells: [{cell: key, unknown_claim_field: true}]\n", "unknown field `unknown_claim_field`"),
    ] {
        let doc = format!("language: Test\nrequires: [morpher-settings]\nwords:\n  - word: foo\n{word_fields}variants:\n  - name: strict\n    morpher: {{GuessRoots: false}}\n    words:\n      - word: foo\n        note: control\n        expect_fail: true\n");
        let error = serde_yaml::from_str::<WordsYaml>(&doc)
            .expect_err("invalid word/parse schema must be rejected")
            .to_string();
        assert!(error.contains(expected), "{word_fields:?}: {error}");
    }

    let empty_word = "language: Test\nwords:\n  - word: \"\"\n    note: valid\n";
    let error = serde_yaml::from_str::<WordsYaml>(empty_word)
        .expect_err("empty word input must be rejected")
        .to_string();
    assert!(error.contains("word must be nonempty"), "{error}");
}

#[test]
fn legacy_staged_words_can_omit_notes_and_rules_without_variants() {
    let doc = "language: Legacy\nwords:\n  - word: foo\n    parses:\n      - signature: FOO|foo\n";
    let words: WordsYaml = serde_yaml::from_str(doc).unwrap();
    assert!(words.words[0].note.is_none());
    assert!(words.words[0].parses[0].rules.is_empty());
}

#[test]
fn word_statuses_allow_fail_or_skip_but_reject_conflicts_and_inconsistent_metadata() {
    for (fields, expected) in [
        ("    note: valid\n", "requires at least one parse"),
        ("    note: valid\n    expect_fail: true\n    expect_skip: true\n", "both expect_fail and expect_skip"),
        ("    note: valid\n    expect_fail: true\n    parses:\n      - signature: OK|foo\n        rules: []\n", "parses must be empty"),
        ("    note: valid\n    expect_skip: true\n    parses:\n      - signature: OK|foo\n        rules: []\n", "parses must be empty"),
        ("    note: valid\n    blocked_by: [rule]\n", "blocked_by requires expect_fail"),
        ("    note: valid\n    blocked_by: []\n", "blocked_by requires expect_fail"),
    ] {
        let doc = format!("language: Test\nwords:\n  - word: foo\n{fields}");
        let error = serde_yaml::from_str::<WordsYaml>(&doc)
            .expect_err("inconsistent word status must be rejected")
            .to_string();
        assert!(error.contains(expected), "{fields:?}: {error}");
    }

    for status in ["expect_fail: true", "expect_skip: true"] {
        let doc = format!("language: Test\nwords:\n  - word: foo\n    note: valid\n    {status}\n");
        let parsed: WordsYaml = serde_yaml::from_str(&doc).unwrap();
        assert_eq!(
            parsed.words[0].expect_fail,
            status.starts_with("expect_fail")
        );
        assert_eq!(
            parsed.words[0].expect_skip,
            status.starts_with("expect_skip")
        );
    }
}

#[test]
fn top_level_metadata_additions_remain_tolerated() {
    let parsed = words_yaml_with("future_top_level_metadata: true\n").unwrap();
    assert_eq!(parsed.words.len(), 1);
}

#[test]
fn variants_require_valid_unique_names_nonempty_known_settings_and_identical_inputs() {
    let base = "requires: [morpher-settings]\nwords:\n  - word: foo\n    note: A test word.\n    expect_fail: true\n";
    for (extra, expected) in [
        ("variants:\n  - name: Bad\n    morpher: {DelReapps: 0}\n    words:\n      - word: foo\n        note: Variant result.\n        expect_fail: true\n", "invalid name"),
        ("variants:\n  - name: default\n    morpher: {DelReapps: 0}\n    words:\n      - word: foo\n        note: Variant result.\n        expect_fail: true\n", "reserved"),
        ("variants:\n  - name: one\n    morpher: {DelReapps: 0}\n    words:\n      - word: foo\n        note: Variant result.\n        expect_fail: true\n  - name: one\n    morpher: {DelReapps: 1}\n    words:\n      - word: foo\n        note: Variant result.\n        expect_fail: true\n", "duplicate name"),
        ("variants:\n  - name: one\n    words:\n      - word: foo\n        note: Variant result.\n        expect_fail: true\n", "nonempty settings mapping is required"),
        ("variants:\n  - name: one\n    morpher: {GuessRootz: true}\n    words:\n      - word: foo\n        note: Variant result.\n        expect_fail: true\n", "unknown key"),
        ("variants:\n  - name: one\n    morpher: {DelReapps: 0}\n    words:\n      - word: foo\n        expect_fail: true\n", "missing field `note`"),
        ("variants:\n  - name: one\n    morpher: {DelReapps: 0}\n    words:\n      - word: bar\n        note: Variant result.\n        expect_fail: true\n", "exact same ordered word inputs"),
        ("variants:\n  - name: one\n    morpher: {DelReapps: 0}\n    words:\n      - word: foo\n        note: Variant result.\n        expect_fail: true\n        blocked_by: [rule]\n", "may not declare blocked_by"),
        ("variants:\n  - name: one\n    morpher: {DelReapps: 0}\n    words:\n      - word: foo\n        note: Variant result.\n        expect_fail: true\n        neutralizes: [rule]\n", "may not declare blocked_by"),
        ("variants:\n  - name: one\n    morpher: {DelReapps: 0}\n    words:\n      - word: foo\n        note: Variant result.\n        expect_fail: true\n        exercises: [rule]\n", "may not declare blocked_by"),
        ("variants:\n  - name: one\n    morpher: {DelReapps: 0}\n    words:\n      - word: foo\n        note: Variant result.\n        expect_fail: true\n        claimed_cells: [{cell: key}]\n", "may not declare blocked_by"),
        ("variants:\n  - name: one\n    morpher: {DelReapps: 0}\n    words:\n      - word: foo\n        note: Variant result.\n        parses:\n          - signature: FOO|foo\n            rules: []\n            exercises: [rule]\n", "exercises must be empty"),
        ("variants:\n  - name: one\n    morpher: {DelReapps: 0}\n    words:\n      - word: foo\n        note: Variant result.\n        parses:\n          - signature: \"\"\n            rules: []\n", "signature must be nonempty"),
        ("variants:\n  - name: one\n    morpher: {DelReapps: 0}\n    words:\n      - word: foo\n        note: Variant result.\n        parses:\n          - signature: FOO|foo\n", "missing field `rules`"),
        ("variants:\n  - name: one\n    morpher: {DelReapps: 0}\n    words:\n      - word: foo\n        note: Variant result.\n        expect_fail: true\n        parses:\n          - signature: FOO|foo\n            rules: []\n", "parses must be empty"),
        ("variants:\n  - name: one\n    morpher: {DelReapps: 0}\n    words:\n      - word: foo\n        note: Variant result.\n        parses:\n          - signature: FOO|foo\n            rules: []\n            unknown_parse_field: true\n", "unknown field"),
        ("variants:\n  - name: one\n    morpher: {DelReapps: 0}\n    words:\n      - word: foo\n        note: Variant result.\n        unknown_word_field: true\n", "unknown field"),
        ("variants:\n  - name: one\n    morpher: {DelReapps: 0}\n    unexpected: true\n    words:\n      - word: foo\n        note: Variant result.\n        expect_fail: true\n", "unknown field"),
    ] {
        let doc = format!("language: Test\n{base}{extra}");
        let error = serde_yaml::from_str::<WordsYaml>(&doc)
            .expect_err("invalid variant metadata must fail closed")
            .to_string();
        assert!(error.contains(expected), "{extra:?}: {error}");
    }
}

#[test]
fn variants_start_from_only_their_own_settings_and_repeat_the_full_word_list() {
    let parsed = words_yaml_with(
        "requires: [morpher-settings]\nmorpher:\n  DelReapps: 1\nvariants:\n  - name: lower-reapps\n    morpher:\n      DelReapps: 0\n      GuessRoots: false\n    words:\n      - word: kat\n        note: Variant test word.\n        parses:\n          - signature: \"KAT|kat\"\n            rules: []\n",
    )
    .unwrap();
    assert_eq!(
        parsed
            .words
            .iter()
            .map(|w| w.word.as_str())
            .collect::<Vec<_>>(),
        vec!["kat"]
    );
    assert_eq!(parsed.variants.len(), 1);
    let run = parsed.for_variant(&parsed.variants[0]);
    assert_eq!(run.morpher.del_reapps, Some(0));
    assert_eq!(run.morpher.guess_roots, Some(false));
    assert_eq!(run.morpher.max_roots, None, "base settings do not inherit");
    assert!(run.variants.is_empty(), "a variant is one independent run");
}

#[test]
fn guess_roots_is_an_explicit_supported_boolean_and_adapter_json_setting() {
    let settings = MorpherSettings::from_json_str(r#"{"GuessRoots":true,"DelReapps":2}"#).unwrap();
    assert_eq!(settings.guess_roots, Some(true));
    assert_eq!(settings.del_reapps, Some(2));
    assert!(MorpherSettings::from_json_str("{}").unwrap().is_empty());
    for json in [
        r#"{"GuessRoots":"true"}"#,
        r#"{"DelReapps":2147483648}"#,
        r#"{"MaxRoots":2147483648}"#,
        r#"{"MergeMSAs":true}"#,
        r#"{"MaxAlternatives":5}"#,
        r#"{"Typo":true}"#,
    ] {
        assert!(
            MorpherSettings::from_json_str(json).is_err(),
            "unsupported or wrongly typed setting must fail: {json}"
        );
    }
    let duplicate = MorpherSettings::from_json_str(r#"{"GuessRoots":true,"GuessRoots":false}"#)
        .expect_err("duplicate JSON keys must not be silently overwritten");
    assert!(duplicate.contains("duplicate setting key"), "{duplicate}");
}

#[test]
fn a_morpher_block_and_the_morpher_settings_requirement_come_together() {
    let missing_requirement = words_yaml_with("morpher:\n  DelReapps: 1\n").unwrap_err();
    assert!(missing_requirement
        .to_string()
        .contains(MORPHER_SETTINGS_CAPABILITY));
    let missing_block = words_yaml_with("requires: [morpher-settings]\n").unwrap_err();
    assert!(missing_block
        .to_string()
        .contains(MORPHER_SETTINGS_CAPABILITY));
    let missing_variant_requirement = words_yaml_with(
        "variants:\n  - name: explicit\n    morpher: {GuessRoots: false}\n    words:\n      - word: kat\n        note: Variant test word.\n        parses:\n          - signature: \"KAT|kat\"\n            rules: []\n",
    )
    .unwrap_err();
    assert!(missing_variant_requirement
        .to_string()
        .contains(MORPHER_SETTINGS_CAPABILITY));
}

/// A verbatim copy of the draft upstream fixture `edge-cases/deletion-reapplications`, kept outside both discovery roots.
fn deletion_reapplications_fixture() -> FixtureRef {
    FixtureRef {
        root: Root::Staging,
        category: "edge-cases".to_string(),
        name: "deletion-reapplications".to_string(),
        dir: Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/data/edge-cases/deletion-reapplications"),
    }
}

#[test]
fn hc_rust_matches_the_oracle_on_a_fixture_run_with_its_morpher_block() {
    let fixture = deletion_reapplications_fixture();
    let words = fixture.load_words_yaml();
    assert_eq!(words.morpher.del_reapps, Some(1));
    let grammar = fixture.load_grammar();
    assert_eq!(grammar.deletion_reapplications, 1);
    let replay = replay_against_oracle(&words, &pg_parse::Morpher::new(&grammar, usize::MAX));
    assert!(replay.mismatches.is_empty(), "{:?}", replay.mismatches);
    assert_eq!(replay.checked, 3);
}

#[test]
fn a_morpher_that_ignores_the_block_is_reported_not_silently_run_at_defaults() {
    let fixture = deletion_reapplications_fixture();
    let words = fixture.load_words_yaml();
    let defaults = pg_grammar::load(&fixture.load_grammar_xml()).unwrap();
    let replay = replay_against_oracle(&words, &pg_parse::Morpher::new(&defaults, usize::MAX));
    let flagged: Vec<&str> = replay.mismatches.iter().map(|m| m.word.as_str()).collect();
    assert!(flagged.contains(&"<morpher:>"), "{:?}", replay.mismatches);
    assert!(flagged.contains(&"tuki"), "{:?}", replay.mismatches);
}

#[test]
fn wrong_variant_expectations_fail_under_its_named_independent_settings_run() {
    let fixture = deletion_reapplications_fixture();
    let mut words = fixture.load_words_yaml();
    words.variants.push(SettingsVariant {
        name: "del-reapps-zero".to_string(),
        morpher: MorpherSettings {
            del_reapps: Some(0),
            ..MorpherSettings::default()
        },
        // Deliberately retain DelReapps=1 expectations to prove the variant is actually replayed.
        words: words.words.clone(),
    });

    let runs = replay_fixture_runs(&fixture, &words);
    assert_eq!(runs.len(), 2);
    assert!(
        runs[0].replay.mismatches.is_empty(),
        "base run: {:?}",
        runs[0]
    );
    assert!(runs[0].is_base, "base is the coverage run");
    assert!(
        !runs[1].is_base,
        "variant does not count as fixture coverage"
    );
    assert!(runs[1].name.ends_with("#variant=del-reapps-zero"));
    assert!(
        runs[1]
            .replay
            .mismatches
            .iter()
            .any(|mismatch| mismatch.word == "tuki"),
        "wrong variant result must fail on tuki: {:?}",
        runs[1]
    );
}

#[test]
fn guess_roots_matches_the_four_polysynthetic_analyses_and_unapplied_is_reported() {
    let fixture = require_fixture("languages", "polysynthetic-stratal-derivation-chain");
    let mut words = fixture.load_words_yaml();
    let takuvuq = words
        .words
        .iter()
        .find(|word| word.word == "takuvuq")
        .expect("fixture pins the guessed takuvuq word")
        .clone();
    assert!(!takuvuq.adapter_visible_for_guess_roots(false));
    assert!(takuvuq.adapter_visible_for_guess_roots(true));
    words.words = vec![takuvuq];
    words.morpher = MorpherSettings {
        guess_roots: Some(true),
        ..MorpherSettings::default()
    };

    let defaults = pg_grammar::load(&fixture.load_grammar_xml()).unwrap();
    let ignored = replay_against_oracle(&words, &pg_parse::Morpher::new(&defaults, usize::MAX));
    assert!(
        ignored
            .mismatches
            .iter()
            .any(|mismatch| mismatch.word == "<morpher:>"),
        "GuessRoots must fail closed when the morpher did not receive the setting: {:?}",
        ignored.mismatches
    );

    let grammar = fixture.load_grammar_with_settings(&words.morpher);
    let morpher = pg_parse::Morpher::new(&grammar, usize::MAX);
    let guessed = morpher.parse_word_opts(
        "takuvuq",
        &pg_parse::ParseOptions::default().with_guess_root(true),
    );
    assert_eq!(
        pg_parse::result_multiset(&guessed.analyses),
        vec![
            "taku+INFL|takuvuq".to_string(),
            "taku+INFL|takuvuq".to_string(),
            "takuvuq|takuvuq".to_string(),
            "takuvuq|takuvuq".to_string(),
        ],
        "GuessRoots=true must match the C# four-signature multiset"
    );
    let tsv = render_adapter_tsv(
        &fixture.load_grammar_xml(),
        "takuvuq\n!\n",
        r#"{"GuessRoots":true}"#,
    )
    .unwrap();
    let rows: Vec<Vec<&str>> = tsv.lines().map(|line| line.split('\t').collect()).collect();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].len(), 5);
    assert_eq!(rows[0][0], "0");
    assert_eq!(rows[0][1], "takuvuq");
    assert_eq!(rows[0][3], "ok");
    assert_eq!(
        rows[0][4],
        "taku+INFL|takuvuq;taku+INFL|takuvuq;takuvuq|takuvuq;takuvuq|takuvuq"
    );
    assert_eq!(rows[1].len(), 5);
    assert_eq!(rows[1][0], "1");
    assert_eq!(rows[1][1], "!");
    assert_eq!(rows[1][3], "SKIPPED");
    assert_eq!(rows[1][4], "-");
    assert_eq!(
        morpher
            .parse_word_opts(
                "takuvuq",
                &pg_parse::ParseOptions::default().with_guess_root(false)
            )
            .signature(),
        "-",
        "GuessRoots=false has no analysis for the intentionally absent lexicon root"
    );
    let tsv_without_guessing = render_adapter_tsv(
        &fixture.load_grammar_xml(),
        "takuvuq\n",
        r#"{"GuessRoots":false}"#,
    )
    .unwrap();
    assert!(tsv_without_guessing.ends_with("\tok\t-\n"));
    let replay = replay_against_oracle(&words, &morpher);
    assert!(replay.mismatches.is_empty(), "{:?}", replay.mismatches);
    assert_eq!(
        replay.checked, 1,
        "explicit GuessRoots includes guessed words"
    );
}
#[test]
fn incomplete_replay_cannot_pass_as_an_expected_zero_analysis() {
    let fixture = deletion_reapplications_fixture();
    let mut words = fixture.load_words_yaml();
    words.words.truncate(1);
    words.words[0].expect_fail = true;
    words.words[0].parses.clear();
    let grammar = fixture.load_grammar();
    let morpher = pg_parse::Morpher::new(&grammar, 0).with_work_cap(0);
    let outcome = morpher.parse_word(&words.words[0].word);
    assert!(
        outcome.capped,
        "the control must exhaust its zero work budget"
    );
    assert!(outcome.analyses.is_empty());
    let replay = replay_against_oracle(&words, &morpher);
    assert!(
        replay.mismatches.iter().any(|m| m.word == "tuki"),
        "incomplete work must fail even when its empty signature matches: {:?}",
        replay.mismatches
    );
}

#[test]
fn a_variant_with_no_visible_words_is_not_reported_as_exercised() {
    let fixture = deletion_reapplications_fixture();
    let mut words = fixture.load_words_yaml();
    words.words.truncate(1);
    for parse in &mut words.words[0].parses {
        parse.guess = true;
    }
    words.morpher.guess_roots = Some(false);
    words.variants.push(SettingsVariant {
        name: "legacy-guesses".to_string(),
        morpher: MorpherSettings {
            del_reapps: Some(0),
            ..MorpherSettings::default()
        },
        words: words.words.clone(),
    });
    let runs = replay_fixture_runs(&fixture, &words);
    assert_eq!(runs[0].replay.checked, 1);
    assert!(runs[0].replay.mismatches.is_empty());
    assert_eq!(runs[1].replay.checked, 0);
    assert!(runs[1].name.ends_with("#variant=legacy-guesses"));
    assert!(runs[1].replay.mismatches.iter().any(|m| m.word == "<run>"));
}

fn loader_pattern_shapes_fixture() -> FixtureRef {
    FixtureRef {
        root: Root::Staging,
        category: "edge-cases".to_string(),
        name: "loader-pattern-shapes".to_string(),
        dir: Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/data/edge-cases/loader-pattern-shapes"),
    }
}

#[test]
fn replay_treats_bracketed_class_members_as_a_set() {
    let fixture = loader_pattern_shapes_fixture();
    let words = fixture.load_words_yaml();
    let grammar = fixture.load_grammar();
    let replay = replay_against_oracle(&words, &pg_parse::Morpher::new(&grammar, usize::MAX));
    assert!(replay.mismatches.is_empty(), "{:?}", replay.mismatches);
    assert!(replay.checked > 0);
}

#[test]
fn class_order_normalization_sorts_members_and_keeps_negation_and_other_text() {
    assert_eq!(normalize_class_order("A+B|b[ea]t"), "A+B|b[ae]t");
    assert_eq!(normalize_class_order("|b[^ea]t[ou]"), "|b[^ae]t[ou]");
    assert_eq!(normalize_class_order("|bat"), "|bat");
    assert_eq!(normalize_class_order("|b[ea"), "|b[ea");
}

#[test]
fn signatures_equal_ignores_class_member_order_but_not_members_or_negation() {
    assert!(signatures_equal("|b[ae]t", "|b[ea]t"));
    assert!(signatures_equal("X|a;Y|b[ae]t", "Y|b[ea]t;X|a"));
    assert!(signatures_equal("-", "-"));
    assert!(!signatures_equal("|b[ae]t", "|b[ao]t"));
    assert!(!signatures_equal("|b[^ae]t", "|b[ae]t"));
    assert!(!signatures_equal("|b[ae]t", "|b[ae]t;|b[ae]t"));
    assert!(!signatures_equal("-", "|b[ae]t"));
}
