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
    parses:
      - signature: "M1|foo"
        rules: []
  - word: bar
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
        "language: Test\n{front_matter}\nwords:\n  - word: foo\n    expect_fail: true\n"
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
            format!("language: Test\n{front_matter}\nwords: []\n"),
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
