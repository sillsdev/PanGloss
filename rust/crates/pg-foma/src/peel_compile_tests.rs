#[test]
fn reduplication_fixture_is_not_structural() {
    let grammar = pg_foma_runtime::peel::minimal_redup_grammar_for_test();
    assert!(!crate::emit::is_structural_rule(
        &grammar,
        pg_grammar::model::MRuleId(0),
    ));
}
