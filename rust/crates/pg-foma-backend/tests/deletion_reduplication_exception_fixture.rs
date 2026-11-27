use pg_conformance_fixtures::{assert_matches_oracle, require_fixture};

fn fixture() -> pg_conformance_fixtures::FixtureRef {
    require_fixture("edge-cases", "deletion-reduplication-exception-composite")
}

#[test]
fn fixture_loads_replays_and_exercises_required_grammar_facts() {
    let f = fixture();
    let xml = f.load_grammar_xml();
    let g = pg_grammar::load(&xml).expect("fixture grammar must load");
    let yaml = f.load_words_yaml();
    assert_matches_oracle(&f.label(), &yaml, &pg_parse::Morpher::new(&g, usize::MAX));
    assert!(
        g.mrules.iter().any(
            |r| r.affix_allomorphs().is_some_and(|as_| as_.iter().any(|a| a
                .rhs
                .iter()
                .filter(|x| matches!(x, pg_grammar::model::OutputAction::Copy(_)))
                .count()
                > a.lhs.len()))
        ),
        "reduplication fact missing"
    );
    assert!(g.prules.iter().any(|r| matches!(r, pg_grammar::model::PhonRuleDef::Rewrite(w) if w.subrules.iter().any(|s| s.rhs.nodes.is_empty()))), "deletion fact missing");
    assert!(!g.mpr_features.is_empty(), "gated/exception fact missing");
    assert!(
        g.entries.iter().any(|e| !e.mpr.is_empty()),
        "lexical exception fact missing"
    );
}
