use pg_conformance_fixtures::{assert_matches_oracle, require_fixture};

#[test]
fn promoted_backend_fixtures_replay_and_exercise_their_grammar_facts() {
    let expected = [
        "backend-gated-generic",
        "backend-template-generic",
        "backend-ordered-generic",
        "backend-strata-generic",
    ];
    for name in expected {
        let fixture = require_fixture("edge-cases", name);
        let grammar = pg_grammar::load(&fixture.load_grammar_xml())
            .unwrap_or_else(|e| panic!("{name} failed to load: {e}"));
        let words = fixture.load_words_yaml();
        assert_matches_oracle(
            &fixture.label(),
            &words,
            &pg_parse::Morpher::new(&grammar, usize::MAX),
        );

        match name {
            "backend-gated-generic" => {
                assert!(!grammar.mpr_features.is_empty());
                assert!(grammar.prules.len() >= 2);
            }
            "backend-template-generic" => assert!(!grammar.templates.is_empty()),
            "backend-ordered-generic" => {
                assert!(grammar
                    .prules
                    .iter()
                    .any(|r| matches!(r, pg_grammar::model::PhonRuleDef::Metathesis(_))));
            }
            "backend-strata-generic" => assert!(grammar.strata.len() > 1),
            _ => unreachable!(),
        }
    }
}
