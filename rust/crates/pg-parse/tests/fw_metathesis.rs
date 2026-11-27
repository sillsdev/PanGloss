//! A FieldWorks `PhMetathesisRule` is imported and run as HCLoader loads it (data/fw-metathesis/README.md).

use pg_parse::{Morpher, ParseOptions};

const PROJECT: &str = include_str!("data/fw-metathesis/project.fwdata");

/// `(word, analysis count hc.dll gives for the live HCLoader language of this project)`.
const HC_DLL_COUNTS: [(&str, usize); 11] = [
    ("akta", 1),
    ("atka", 0),
    ("aktas", 1),
    ("atkas", 0),
    ("itka", 1),
    ("ikta", 0),
    ("itkas", 1),
    ("kita", 0),
    ("tika", 1),
    ("tikas", 1),
    ("kitas", 0),
];

#[test]
fn imported_metathesis_rule_matches_hc_dll_on_the_same_project() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("project.fwdata");
    std::fs::write(&path, PROJECT).unwrap();
    let (snapshot, report) = pg_fwdata::import_file(&path).unwrap();
    assert!(
        report.warnings.is_empty(),
        "a well-formed metathesis rule imports without warnings: {:?}",
        report.warnings
    );
    let (grammar, warnings) = pg_grammar::compile_project(&snapshot).unwrap();
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
    assert_eq!(
        grammar.prules.len(),
        1,
        "the metathesis rule must be in the grammar"
    );
    let morpher = Morpher::new(&grammar, usize::MAX);
    for (word, expected) in HC_DLL_COUNTS {
        let out = morpher.parse_word_opts(word, &ParseOptions::default());
        assert!(!out.invalid_shape && !out.capped && !out.timed_out && !out.guessed);
        assert_eq!(out.structured.len(), expected, "{word}");
    }
}
