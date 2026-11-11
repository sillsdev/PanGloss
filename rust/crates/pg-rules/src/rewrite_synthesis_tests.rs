use super::*;

#[test]
fn ambiguous_intermediate_context_uses_subsumption_in_synthesis_and_unification_in_analysis() {
    use pg_grammar_model::model::{Dir, PhonRuleDef};
    let xml =
        pg_conformance_fixtures::require_fixture("edge-cases", "permissive-intermediate-class")
            .load_grammar_xml();
    for dir in [Dir::LeftToRight, Dir::RightToLeft] {
        let mut g = pg_grammar::load(&xml).unwrap();
        for rule in &mut g.prules {
            if let PhonRuleDef::Rewrite(rule) = rule {
                rule.dir = dir;
            }
        }
        let PhonRuleDef::Rewrite(make_ambiguous) = &g.prules[0] else {
            panic!("rewrite")
        };
        let PhonRuleDef::Rewrite(flank) = &g.prules[1] else {
            panic!("rewrite")
        };
        let PhonRuleDef::Rewrite(other_flank) = &g.prules[2] else {
            panic!("rewrite")
        };
        let table = &g.char_tables[0];
        let input = crate::shape_feat::segment_with_features(&g, table, "aua").unwrap();
        let ambiguous = synthesize(&g, make_ambiguous, &input).pop().unwrap();
        assert!(
            synthesize(&g, flank, &ambiguous).is_empty(),
            "{dir:?}: [y|u] is not wholly front"
        );
        assert!(
            synthesize(&g, other_flank, &ambiguous).is_empty(),
            "{dir:?}: [y|u] is not wholly back"
        );
        assert!(
            synthesize(&g, make_ambiguous, &ambiguous).is_empty(),
            "{dir:?}: [y|u] is not wholly u"
        );

        for (rule, word, expected) in [(flank, "aya", "iya"), (other_flank, "aua", "auy")] {
            let input = crate::shape_feat::segment_with_features(&g, table, word).unwrap();
            let expected = crate::shape_feat::segment_with_features(&g, table, expected).unwrap();
            let actual = synthesize(&g, rule, &input);
            assert_eq!(actual.len(), 1, "{dir:?} {word}");
            for pos in 0..expected.len() {
                assert_eq!(
                    &actual[0].node_lanes(pos)[..g.phon_features.len()],
                    &expected.node_lanes(pos)[..g.phon_features.len()],
                    "{dir:?} {word} position {pos}"
                );
            }
        }

        let surface = crate::shape_feat::segment_with_features(&g, table, "iua").unwrap();
        let ambiguous_surface = synthesize(&g, make_ambiguous, &surface).pop().unwrap();
        let analyzed = analyze(&g, flank, &ambiguous_surface, None);
        assert_eq!(analyzed.len(), 1, "{dir:?}: [y|u] unifies with front");
        let first = (0..analyzed[0].len())
            .find(|&i| analyzed[0].kind(i) == NodeKind::Segment)
            .unwrap();
        let i = table.lookup_nfd("i").unwrap();
        let a = table.lookup_nfd("a").unwrap();
        for f in 0..g.phon_features.len() {
            let expected = table.get(i).feature_lanes()[f] | table.get(a).feature_lanes()[f];
            assert_eq!(
                analyzed[0].node_lanes(first)[f],
                expected,
                "{dir:?} feature {f}"
            );
        }
        let surface = crate::shape_feat::segment_with_features(&g, table, "auy").unwrap();
        let ambiguous_surface = synthesize(&g, make_ambiguous, &surface).pop().unwrap();
        let analyzed = analyze(&g, other_flank, &ambiguous_surface, None);
        assert_eq!(analyzed.len(), 1, "{dir:?}: [y|u] unifies with back");
        let last = (0..analyzed[0].len())
            .rev()
            .find(|&i| analyzed[0].kind(i) == NodeKind::Segment)
            .unwrap();
        let y = table.lookup_nfd("y").unwrap();
        for f in 0..g.phon_features.len() {
            let expected = table.get(y).feature_lanes()[f] | table.get(a).feature_lanes()[f];
            assert_eq!(
                analyzed[0].node_lanes(last)[f],
                expected,
                "{dir:?} feature {f}"
            );
        }
    }
}

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

#[test]
fn generated_full_intermediate_remains_present_between_rules_in_both_directions() {
    use pg_grammar_model::model::{Dir, PhonRuleDef};
    let xml = pg_conformance_fixtures::require_fixture("edge-cases", "generated-full-intermediate")
        .load_grammar_xml();
    for dir in [Dir::LeftToRight, Dir::RightToLeft] {
        let mut g = pg_grammar::load(&xml).unwrap();
        for rule in &mut g.prules {
            if let PhonRuleDef::Rewrite(rule) = rule {
                rule.dir = dir;
            }
        }
        let PhonRuleDef::Rewrite(merge) = &g.prules[0] else {
            panic!("rewrite")
        };
        let PhonRuleDef::Rewrite(front) = &g.prules[1] else {
            panic!("rewrite")
        };
        let PhonRuleDef::Rewrite(back) = &g.prules[2] else {
            panic!("rewrite")
        };
        let table = &g.char_tables[0];
        let input = crate::shape_feat::segment_with_features(&g, table, "auua").unwrap();
        let middle = synthesize(&g, merge, &input).pop().unwrap();
        assert_eq!(middle.generated_full_features(2).count(), 2, "{dir:?}");
        assert!(synthesize(&g, front, &middle).is_empty(), "{dir:?} front");
        assert!(synthesize(&g, back, &middle).is_empty(), "{dir:?} back");
        let input = crate::shape_feat::segment_with_features(&g, table, "iuua").unwrap();
        let middle = synthesize(&g, merge, &input).pop().unwrap();
        assert!(
            !analyze(&g, front, &middle, None).is_empty(),
            "{dir:?} analysis unifies"
        );
    }
}

#[test]
fn bound_alpha_subsumes_in_synthesis_and_overlaps_in_analysis_in_both_walks() {
    let xml =
        pg_conformance_fixtures::require_fixture("edge-cases", "intermediate-alpha-subsumption")
            .load_grammar_xml();
    let g = pg_grammar::load(&xml).unwrap();
    let feature = 0;
    let plus = VarOccur {
        feature,
        var: 0,
        plus: true,
    };
    let minus = VarOccur {
        feature,
        var: 0,
        plus: false,
    };
    let table = &g.char_tables[0];
    let a = table.get(table.lookup_nfd("a").unwrap()).feature_lanes()[feature];
    let y = table.get(table.lookup_nfd("y").unwrap()).feature_lanes()[feature];
    let ambiguous = a | y;
    for occ in [&plus, &minus] {
        let mut binding = Bindings::default();
        assert!(bind_or_check(&g, &mut binding, occ, a, true, false));
        assert!(!bind_or_check(
            &g,
            &mut binding.clone(),
            occ,
            ambiguous,
            true,
            false
        ));
        assert!(bind_or_check(
            &g,
            &mut binding.clone(),
            occ,
            ambiguous,
            false,
            false
        ));
        let mut lanes: Vec<_> = (0..pg_grammar_model::membership::width(&g.phon_features))
            .map(|f| full_mask(&g, f))
            .collect();
        lanes[feature] = ambiguous;
        let segment = Segment::new(lanes);
        let constraint = AgreementNode::Constraint {
            lanes: vec![],
            vars: vec![occ.clone()],
        };
        for reverse in [false, true] {
            for (synthesis, expected) in [(true, false), (false, true)] {
                let result = walk_binding_environment(
                    &g,
                    &[&segment],
                    reverse,
                    false,
                    1,
                    synthesis,
                    vec![BindingTask::Node(&constraint)],
                    0,
                    binding.clone(),
                );
                assert_eq!(
                    result.is_some(),
                    expected,
                    "reverse={reverse} synthesis={synthesis}"
                );
            }
        }
    }
}

#[test]
fn target_alpha_bindings_follow_the_match_direction() {
    use pg_grammar_model::model::PhonRuleDef;
    let xml = pg_conformance_fixtures::require_fixture("edge-cases", "intermediate-alpha-target")
        .load_grammar_xml();
    let g = pg_grammar::load(&xml).unwrap();
    let PhonRuleDef::Rewrite(make_ambiguous) = &g.prules[0] else {
        panic!("rewrite")
    };
    let PhonRuleDef::Rewrite(target) = &g.prules[1] else {
        panic!("rewrite")
    };
    let table = &g.char_tables[0];
    let input = crate::shape_feat::segment_with_features(&g, table, "aua").unwrap();
    let intermediate = synthesize(&g, make_ambiguous, &input).pop().unwrap();
    let ms = MutShape::for_rewrite(&g, table, &intermediate);
    let (segs, node_of) = ms.segs(true);
    let vars = pattern_var_occurrences(&target.lhs);
    let nodes = &node_of[..2];
    let bindings = |reverse| {
        resolve_bindings(
            &g, &ms, &node_of, nodes, &segs, 0, 2, &vars, &None, &None, &None, &None, true, reverse,
        )
    };
    assert!(
        bindings(false).is_none(),
        "singleton binding cannot subsume the ambiguous node"
    );
    assert!(
        bindings(true).is_some(),
        "ambiguous binding subsumes the singleton node"
    );
}

#[test]
fn alpha_output_keeps_absent_and_generated_full_values_distinct() {
    let xml =
        pg_conformance_fixtures::require_fixture("edge-cases", "intermediate-alpha-subsumption")
            .load_grammar_xml();
    let g = pg_grammar::load(&xml).unwrap();
    let table = &g.char_tables[0];
    let shape = crate::shape_feat::segment_with_features(&g, table, "a").unwrap();
    let ms = MutShape::for_rewrite(&g, table, &shape);
    let occ = VarOccur {
        feature: 0,
        var: 0,
        plus: true,
    };
    let mask = full_mask(&g, occ.feature);
    let narrow = table.get(table.lookup_nfd("a").unwrap()).feature_lanes()[occ.feature];
    let mut wanted = vec![u64::MAX; ms.w];
    wanted[occ.feature] = narrow;
    for (absent, expected_match) in [(true, true), (false, false)] {
        let mut bindings = Bindings::default();
        assert!(bind_or_check(&g, &mut bindings, &occ, mask, true, absent));
        let mut node = ms.nodes[1].clone();
        apply_variable_bindings(&g, &mut node, std::slice::from_ref(&occ), &bindings);
        let segment =
            Segment::new(node.lanes.clone()).with_explicit_full(segment_full_markers(&node));
        assert_eq!(
            synthesis_constraint_matches(&g, &segment, &wanted),
            expected_match
        );
    }
}

fn env_with_defaults(defaults: Vec<(usize, u64, u64)>) -> EnvFst {
    EnvFst {
        fst: pg_fst::CompileInput::new(Vec::new()).compile_with_direction(Direction::LeftToRight),
        anchor_start: false,
        anchor_end: false,
        only_anchor: false,
        node_vars: Vec::new(),
        group_names: Vec::new(),
        agreement: None,
        binding_nodes: None,
        defaults,
        synthesis: false,
    }
}

/// `env_input` borrows the caller's segments when the environment has no `UseDefaults` features, and otherwise returns an owned copy where a lane equal to its feature's full mask becomes the default unless the segment authored it explicitly; the caller's segments are never touched.
#[test]
fn env_input_borrows_without_defaults_and_copies_with_them() {
    let segs = vec![
        Segment::new(vec![0b11, 0b1]),
        Segment::new(vec![0b01, 0b1]),
        {
            let mut s = Segment::new(vec![0b11, 0b1]);
            s.explicit_full = vec![0];
            s
        },
    ];

    let plain = env_with_defaults(Vec::new());
    let borrowed = env_input(&plain, &segs);
    assert!(matches!(borrowed, std::borrow::Cow::Borrowed(_)));
    assert!(std::ptr::eq(borrowed.as_ptr(), segs.as_ptr()), "no copy");

    let with_defaults = env_with_defaults(vec![(0, 0b11, 0b10)]);
    let owned = env_input(&with_defaults, &segs);
    assert!(matches!(owned, std::borrow::Cow::Owned(_)));
    assert_eq!(owned[0].lanes, vec![0b10, 0b1], "full mask defaulted");
    assert_eq!(owned[1].lanes, vec![0b01, 0b1], "not the full mask: kept");
    assert_eq!(owned[2].lanes, vec![0b11, 0b1], "explicit full: kept");
    assert_eq!(
        segs[0].lanes,
        vec![0b11, 0b1],
        "caller's segments untouched"
    );
}
