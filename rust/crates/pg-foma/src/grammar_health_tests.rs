use pg_grammar::grammar_health::{GrammarHealthCode, GrammarHealthReport};

#[test]
fn retained_reduplication_refusal_uses_grammar_health() {
    let fixture = pg_conformance_fixtures::require_fixture(
        "edge-cases",
        "realizational-reduplication-no-proposal-route",
    );
    let grammar = pg_grammar::load(&fixture.load_grammar_xml()).unwrap();
    let diagnostics = crate::grammar_health::check_fst_diagnostics(&grammar).unwrap();
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].code.wire(), "fst.reduplication-no-route");
    assert!(diagnostics[0].blocks_fst);
    assert!(diagnostics[0].fst_reason.is_some());
    let report = GrammarHealthReport::new(diagnostics)
        .unwrap()
        .with_fst_checked();
    assert!(report.fst_checked());
    assert_eq!(report.fst_blocking_count(), 1);
}

#[test]
fn alpha_compiler_refusal_has_no_blocking_label() {
    let grammar = pg_grammar::load(include_str!(
        "../../pg-parse/tests/data/alpha-optional-environment.xml"
    ))
    .unwrap();
    let mut diagnostics = Vec::new();
    let incomplete = crate::grammar_health::extend_diagnostics(&grammar, &mut diagnostics).unwrap();
    assert!(incomplete.unwrap().contains("#177"));
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert!(!GrammarHealthCode::ALL
        .iter()
        .any(|code| code.wire() == "fst.alpha-environment"));
}
