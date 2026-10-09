use pg_grammar_model::model::PhonRuleDef;

fn records() -> Vec<serde_json::Value> {
    let root = pg_conformance_fixtures::staging_root();
    let evidence = root
        .parent()
        .unwrap()
        .join("docs/divergences/evidence/069-overridden-alpha/sweep-records.json");
    let records: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(evidence).unwrap()).unwrap();
    let records = records.as_array().unwrap().clone();
    assert_eq!(records.len(), 144);
    records
}

#[test]
fn effective_analysis_target_parses_match_recorded_csharp_sweep() {
    let mut checked = 0;
    for record in records() {
        let name = record["name"].as_str().unwrap();
        let grammar = pg_grammar::load(record["xml"].as_str().unwrap()).unwrap();
        let expected = serde_json::from_value(record["expected"].clone()).unwrap();
        let morpher = pg_parse::Morpher::new(&grammar, usize::MAX);
        checked += pg_conformance_fixtures::assert_matches_oracle(name, &expected, &morpher);
    }
    assert_eq!(checked, 2304);
}

#[test]
fn effective_analysis_target_synthesis_matches_recorded_csharp_traces() {
    let mut checked = 0;
    for record in records() {
        let name = record["name"].as_str().unwrap();
        let grammar = pg_grammar::load(record["xml"].as_str().unwrap()).unwrap();
        let PhonRuleDef::Rewrite(rule) = &grammar.prules[0] else {
            panic!("{name}: missing rewrite rule");
        };
        let table = &grammar.char_tables[0];
        let pairs = record["synthesis"].as_array().unwrap();
        assert_eq!(
            pairs.len(),
            16,
            "{name}: all sixteen roots must be observed"
        );
        for pair in pairs {
            let word = pair[0].as_str().unwrap();
            let expected = pair[1].as_str().unwrap();
            let input = pg_rules::shape_feat::segment_with_features(&grammar, table, word).unwrap();
            let output = pg_rules::rewrite::synthesize(&grammar, rule, &input);
            assert!(output.len() <= 1, "{name}: nondeterministic synthesis");
            let surface =
                pg_rules::surface_probe::render_shape(table, output.first().unwrap_or(&input));
            assert_eq!(surface.as_deref(), Some(expected), "{name}: root {word}");
            checked += 1;
        }
    }
    assert_eq!(checked, 2304);
}
