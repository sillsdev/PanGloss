use super::*;

#[test]
fn probing_a_mixed_rule_keeps_deleted_nodes_and_matches_synthesis() {
    let xml = pg_conformance_fixtures::require_fixture("edge-cases", "cross-subrule-feeding")
        .load_grammar_xml();
    let mut g = pg_grammar::load(&xml).unwrap();
    let pid = PRuleId(0);
    let pg_grammar_model::model::PhonRuleDef::Rewrite(rule) = &mut g.prules[0] else {
        panic!("rewrite rule")
    };
    rule.subrules[0].rhs = Pattern::default();
    let cache = crate::cache::RuleCache::build(&g);
    let pg_grammar_model::model::PhonRuleDef::Rewrite(rule) = &g.prules[0] else {
        panic!("rewrite rule")
    };
    let input = crate::shape_feat::segment_with_features(&g, &g.char_tables[0], "uuu").unwrap();
    let expected = synthesize(&g, rule, &input).pop().unwrap();
    let mut ms = MutShape::for_rewrite(&g, &g.char_tables[0], &input);
    assert!(matches!(
        probe_apply_rule_cached(
            &g,
            pid,
            rule,
            &mut ms,
            &cache,
            &FeatureStruct::EMPTY,
            MprSet::EMPTY
        ),
        ProbeOutcome::Applied
    ));
    assert_eq!(ms.nodes.iter().filter(|n| n.deleted).count(), 1);
    let actual = ms.to_shape();
    assert_eq!(actual.len(), expected.len());
    for i in 0..actual.len() {
        assert_eq!(actual.kind(i), expected.kind(i));
        assert_eq!(actual.node_lanes(i), expected.node_lanes(i));
    }
}

#[test]
fn feeding_uses_the_same_result_for_plain_cached_and_traced_synthesis() {
    use crate::trace::{TraceSink, TreeTraceSink};
    let xml = pg_conformance_fixtures::require_fixture("edge-cases", "cross-subrule-feeding")
        .load_grammar_xml();
    let g = pg_grammar::load(&xml).unwrap();
    let pid = PRuleId(0);
    let pg_grammar_model::model::PhonRuleDef::Rewrite(rule) = &g.prules[0] else {
        panic!("rewrite rule")
    };
    let input = crate::shape_feat::segment_with_features(&g, &g.char_tables[0], "uuu").unwrap();
    let cache = crate::cache::RuleCache::build(&g);
    let plain = synthesize(&g, rule, &input);
    let cached = synthesize_with_mpr_cached(
        &g,
        pid,
        rule,
        &input,
        &FeatureStruct::EMPTY,
        MprSet::EMPTY,
        &cache,
    );
    let sink = TreeTraceSink::new();
    let parent = sink.generate_words();
    let traced = synthesize_with_mpr_traced(
        &g,
        pid,
        rule,
        &input,
        &FeatureStruct::EMPTY,
        MprSet::EMPTY,
        &sink,
        parent,
    );
    let word = Word::new(input, StratumId(0));
    let cached_traced =
        synthesize_with_mpr_cached_traced(&g, pid, rule, &word, &cache, &sink, parent);
    assert_eq!(plain.len(), 1);
    assert_eq!(plain, cached);
    assert_eq!(plain, traced);
    assert_eq!(plain, cached_traced);
}

#[test]
fn rtl_epenthesis_resumes_before_the_inserted_sequence_in_every_route() {
    use crate::trace::{TraceSink, TreeTraceSink};
    let xml =
        pg_conformance_fixtures::require_fixture("edge-cases", "rtl-epenthesis-cursor-after-apply")
            .load_grammar_xml();
    // The final case makes both internal gaps eligible, so skipping one changes the output.
    for (width, internal_feed, expected) in
        [(1, false, "uyu"), (3, false, "uyyyu"), (3, true, "uyiyiyu")]
    {
        let mut g = pg_grammar::load(&xml).unwrap();
        let pg_grammar_model::model::PhonRuleDef::Rewrite(rule) = &mut g.prules[0] else {
            panic!("rewrite rule")
        };
        if internal_feed {
            rule.subrules[0].right_env = rule.subrules[0].left_env.clone();
        }
        let rhs = &mut rule.subrules[1].rhs.nodes;
        rhs.extend(std::iter::repeat_n(rhs[0].clone(), width - 1));
        let cache = crate::cache::RuleCache::build(&g);
        let pg_grammar_model::model::PhonRuleDef::Rewrite(rule) = &g.prules[0] else {
            panic!("rewrite rule")
        };
        let input = crate::shape_feat::segment_with_features(&g, &g.char_tables[0], "uu").unwrap();
        let expected =
            crate::shape_feat::segment_with_features(&g, &g.char_tables[0], expected).unwrap();
        let sink = TreeTraceSink::new();
        let parent = sink.generate_words();
        let word = Word::new(input.clone(), StratumId(0));
        let routes = [
            synthesize(&g, rule, &input),
            synthesize_with_mpr_cached(
                &g,
                PRuleId(0),
                rule,
                &input,
                &FeatureStruct::EMPTY,
                MprSet::EMPTY,
                &cache,
            ),
            synthesize_with_mpr_traced(
                &g,
                PRuleId(0),
                rule,
                &input,
                &FeatureStruct::EMPTY,
                MprSet::EMPTY,
                &sink,
                parent,
            ),
            synthesize_with_mpr_cached_traced(&g, PRuleId(0), rule, &word, &cache, &sink, parent),
        ];
        for (route, result) in routes.iter().enumerate() {
            assert_eq!(result.len(), 1, "width {width}, route {route}");
            let actual = &result[0];
            assert_eq!(actual.len(), expected.len(), "width {width}, route {route}");
            for i in 0..actual.len() {
                assert_eq!(
                    actual.kind(i),
                    expected.kind(i),
                    "width {width}, route {route}"
                );
                assert_eq!(
                    &actual.node_lanes(i)[..g.phon_features.len()],
                    &expected.node_lanes(i)[..g.phon_features.len()],
                    "width {width}, route {route}"
                );
            }
        }
    }
}
