use super::*;

fn synthetic() -> Manifest {
    Manifest {
        schema_version: 1,
        corpus_root: "samples/data".into(),
        corpora: vec![CorpusEntry {
            logical_name: "synthetic".into(),
            purpose: "a synthetic corpus used only to prove fail-closed behaviour".into(),
            files: vec![
                CorpusFile {
                    path: "present.txt".into(),
                    role: "corpus".into(),
                    required: true,
                    word_list: None,
                },
                CorpusFile {
                    path: "absent.txt".into(),
                    role: "grammar".into(),
                    required: true,
                    word_list: None,
                },
            ],
            requiring_tests: vec!["synthetic-suite".into()],
        }],
    }
}

#[test]
fn the_committed_manifest_parses_and_validates() {
    let m = load_manifest().expect("committed manifest must parse");
    let problems = validate_manifest(&m);
    assert!(problems.is_empty(), "manifest problems: {problems:?}");
    for expected in ["indonesian", "sena", "amharic", "aweti", "mbugwe"] {
        assert!(
            m.corpora.iter().any(|c| c.logical_name == expected),
            "manifest must declare the {expected} corpus"
        );
    }
}

#[test]
fn every_requiring_test_in_the_committed_manifest_names_a_test_that_exists() {
    let m = load_manifest().expect("committed manifest must parse");
    let problems = unresolvable_requiring_tests(&m, &crates_root());
    assert!(
        problems.is_empty(),
        "requiring_tests naming tests that do not exist: {problems:#?}"
    );
    // A manifest that declared no gates at all would pass the loop above vacuously.
    let declared: usize = m.corpora.iter().map(|c| c.requiring_tests.len()).sum();
    assert!(declared >= 5, "only {declared} requiring_tests resolved");
}

/// Falsification: a synthetic tree, so the check cannot pass by accident of the real crates.
#[test]
fn a_phantom_requiring_test_is_reported_against_a_synthetic_tree() {
    let dir = std::env::temp_dir().join(format!("pg-req-tests-{}", std::process::id()));
    let tests = dir.join("pg-foma").join("tests");
    std::fs::create_dir_all(&tests).unwrap();
    std::fs::write(tests.join("real_gate.rs"), "#[test]\nfn a_real_case() {}\n").unwrap();
    std::fs::create_dir_all(dir.join("pg-foma").join("src")).unwrap();
    std::fs::write(
        dir.join("pg-foma").join("src").join("lib.rs"),
        "fn a_real_unit_case() {}\n",
    )
    .unwrap();

    let resolves = |spec: &str| {
        let mut m = synthetic();
        m.corpora[0].requiring_tests = vec![spec.to_owned()];
        unresolvable_requiring_tests(&m, &dir)
    };

    assert!(resolves("pg-foma --test real_gate").is_empty());
    assert!(resolves("pg-foma --test real_gate a_real_case").is_empty());
    assert!(resolves("pg-foma (lib) some_mod::a_real_unit_case").is_empty());

    // The exact shape that shipped: a target name nothing defines.
    let problems = resolves("pg-foma --test compose_recall_aweti_gate");
    assert_eq!(problems.len(), 1, "{problems:#?}");
    assert!(
        problems[0].contains("names no such test target"),
        "{problems:#?}"
    );

    // A real target with a test function it does not define.
    let problems = resolves("pg-foma --test real_gate a_case_that_is_not_there");
    assert!(problems[0].contains("absent from"), "{problems:#?}");

    // A shape nothing can resolve is an error, never a silent pass.
    for bad in [
        "synthetic-suite",
        "pg-foma --tests real_gate",
        "pg-foma (lib)",
    ] {
        assert!(!resolves(bad).is_empty(), "{bad:?} must not resolve");
    }

    let _ = std::fs::remove_dir_all(&dir);
}

/// CI proves fail-closed with a synthetic manifest and intentionally missing files, needing no private corpus.
#[test]
fn a_missing_required_file_is_reported_against_a_synthetic_manifest() {
    let dir = std::env::temp_dir().join(format!("pg-corpus-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("present.txt"), b"x").unwrap();
    let _ = std::fs::remove_file(dir.join("absent.txt"));

    let missing = missing_required_under(&synthetic(), &dir);
    assert_eq!(missing, vec!["synthetic:absent.txt".to_owned()]);

    std::fs::write(dir.join("absent.txt"), b"y").unwrap();
    assert!(missing_required_under(&synthetic(), &dir).is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn validation_rejects_the_shapes_that_would_defeat_the_gate() {
    let mut m = synthetic();
    m.corpora[0]
        .files
        .iter_mut()
        .for_each(|f| f.required = false);
    assert!(validate_manifest(&m)
        .iter()
        .any(|p| p.contains("cannot fail closed")));

    let mut m = synthetic();
    m.corpora[0].requiring_tests.clear();
    assert!(validate_manifest(&m)
        .iter()
        .any(|p| p.contains("no requiring_tests")));

    let mut m = synthetic();
    m.corpora[0].files[0].path = "../escape.txt".into();
    assert!(validate_manifest(&m)
        .iter()
        .any(|p| p.contains("escape it")));

    let mut m = synthetic();
    m.corpora.push(m.corpora[0].clone());
    assert!(validate_manifest(&m)
        .iter()
        .any(|p| p.contains("duplicate logical_name")));
}

#[test]
fn word_list_metadata_is_validated() {
    // Bad line_ending value.
    let mut m = synthetic();
    m.corpora[0].files[0].word_list = Some(WordList {
        line_ending: "CR".into(),
        skip_leading_lines: 0,
        notes: String::new(),
    });
    assert!(validate_manifest(&m)
        .iter()
        .any(|p| p.contains("line_ending must be LF, CRLF, or mixed")));

    // skip_leading_lines > 0 with no explanation.
    let mut m = synthetic();
    m.corpora[0].files[0].word_list = Some(WordList {
        line_ending: "CRLF".into(),
        skip_leading_lines: 4,
        notes: String::new(),
    });
    assert!(validate_manifest(&m)
        .iter()
        .any(|p| p.contains("no notes explaining why")));

    // Valid word_list on a role: "corpus" file passes cleanly.
    let mut m = synthetic();
    m.corpora[0].files[0].word_list = Some(WordList {
        line_ending: "CRLF".into(),
        skip_leading_lines: 4,
        notes: "first 4 lines are an English-gloss header, not surface words".into(),
    });
    assert!(validate_manifest(&m).is_empty());

    // word_list on a non-"corpus"-role file is rejected.
    let mut m = synthetic();
    m.corpora[0].files[1].word_list = Some(WordList {
        line_ending: "LF".into(),
        skip_leading_lines: 0,
        notes: String::new(),
    });
    assert!(validate_manifest(&m)
        .iter()
        .any(|p| p.contains("has a word_list but role is")));
}

#[test]
fn required_mode_reads_the_env_var_in_both_directions() {
    // Serialized within one test, not split across two: cargo runs tests in one process with shared env, so two tests toggling the same var would race.
    let restore = std::env::var(REQUIRED_ENV).ok();
    for (value, expected) in [
        ("1", true),
        ("true", true),
        ("yes", true),
        ("0", false),
        ("false", false),
        ("", false),
    ] {
        std::env::set_var(REQUIRED_ENV, value);
        assert_eq!(required(), expected, "value {value:?}");
    }
    std::env::remove_var(REQUIRED_ENV);
    assert!(!required());
    if let Some(v) = restore {
        std::env::set_var(REQUIRED_ENV, v);
    }
}
