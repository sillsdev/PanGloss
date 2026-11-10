//! Direct token-relation checks; the production construction is measured separately.
use super::*;
use foma::apply::apply_init;
use pg_grammar::model::Pattern;
use std::collections::BTreeSet;

fn fixture(name: &str) -> Grammar {
    pg_conformance_fixtures::require_fixture("edge-cases", name).load_grammar()
}

fn rewrite(g: &mut Grammar, index: usize) -> &mut RewriteRuleDef {
    let PhonRuleDef::Rewrite(rule) = &mut g.prules[index] else {
        panic!("rewrite fixture")
    };
    rule
}

fn relation(g: &Grammar, positions: &[usize]) -> Result<Fsm, PhonologyLoweringError> {
    let rules: Vec<_> = positions.iter().map(|i| &g.prules[*i]).collect();
    compile_and_compose_rules_recall_safe(&FomaOptions::default(), g, &rules, &mut Vec::new())
}

fn encoded(g: &Grammar, text: &str) -> String {
    let table = &g.char_tables[0];
    SegAlphabet::new(table).encode_shape(&pg_grammar::segment::segment(table, text).unwrap())
}

fn outputs(g: &Grammar, net: &Fsm, input: &str) -> BTreeSet<String> {
    apply_init(net).down(&encoded(g, input)).collect()
}

#[test]
fn expansion_includes_founding_oracle_output() {
    let g = fixture("permissive-multisegment-output");
    let PhonRuleDef::Rewrite(rule) = &g.prules[0] else {
        panic!("rewrite fixture")
    };
    let (obligatory, _) = compile_rewrite_rule(&FomaOptions::default(), &g, rule).unwrap();
    assert!(outputs(&g, &obligatory, "a").contains(&encoded(&g, "ai")));
    let net = relation(&g, &[0]).expect("compiler claims a complete relation");
    let actual = outputs(&g, &net, "a");
    assert!(actual.contains(&encoded(&g, "a")));
    // Forward a -> ai, independently confirmed by pinned C# and HC-Rust.
    assert!(
        actual.contains(&encoded(&g, "ai")),
        "missing oracle-confirmed expansion"
    );
}

fn literal_grammar(
    mode: RewriteMode,
    dir: Dir,
    lhs: &str,
    rhs: &str,
    left: &str,
    right: &str,
) -> Grammar {
    let mut g = fixture("permissive-multisegment-output");
    let [lhs, rhs, left_pattern, right_pattern] = [lhs, rhs, left, right].map(|text| Pattern {
        nodes: text
            .chars()
            .map(|c| PatternNode::CharDef(g.char_tables[0].lookup_nfd(&c.to_string()).unwrap()))
            .collect(),
    });
    let rule = rewrite(&mut g, 0);
    rule.mode = mode;
    rule.dir = dir;
    rule.lhs = lhs;
    rule.subrules[0].rhs = rhs;
    rule.subrules[0].left_env = (!left.is_empty()).then_some(left_pattern);
    rule.subrules[0].right_env = (!right.is_empty()).then_some(right_pattern);
    g
}

fn literal_case(
    mode: RewriteMode,
    dir: Dir,
    lhs: &str,
    rhs: &str,
    left: &str,
    right: &str,
) -> (Grammar, Fsm) {
    let g = literal_grammar(mode, dir, lhs, rhs, left, right);
    let net = relation(&g, &[0]).unwrap();
    (g, net)
}

fn matrix_relation(g: &Grammar, lhs: &str) -> Option<Fsm> {
    let PhonRuleDef::Rewrite(rule) = &g.prules[0] else {
        panic!("rewrite")
    };
    let result = relation(g, &[0]);
    if lhs.is_empty() && rule.mode == RewriteMode::Iterative {
        // Fed insertion makes the relation infinite, so it is checked by inclusion in `insertion_tests`, not enumerated.
        result.expect("iterative empty-focus insertion has a proposing construction");
        None
    } else if lhs.chars().count() > 1 && rule.mode == RewriteMode::Simultaneous {
        assert_eq!(
            result.unwrap_err(),
            PhonologyLoweringError::SimultaneousMultiTokenFocusUnavailable(rule.xml_id.clone()),
            "unproven focus must refuse explicitly"
        );
        None
    } else {
        Some(result.expect("supported matrix construction must be complete"))
    }
}

const MATERIAL_CASES: &[(&str, &str, &str, &[&str])] = &[
    ("a", "ai", "a", &["a", "ai"]),
    ("a", "ai", "aa", &["aa", "aia", "aai", "aiai"]),
    ("a", "ai", "aia", &["aia", "aiia", "aiai", "aiiai"]),
    ("a", "ia", "aa", &["aa", "iaa", "aia", "iaia"]),
    ("a", "ia", "aia", &["aia", "iaia", "aiia", "iaiia"]),
    ("a", "aii", "aa", &["aa", "aiia", "aaii", "aiiaii"]),
    ("a", "iia", "aa", &["aa", "iiaa", "aiia", "iiaiia"]),
    ("a", "iai", "aa", &["aa", "iaia", "aiai", "iaiiai"]),
    ("a", "aa", "aa", &["aa", "aaa", "aaaa"]),
    ("aa", "i", "aaa", &["aaa", "ia", "ai"]),
    ("aa", "i", "aaaa", &["aaaa", "iaa", "aia", "aai", "ii"]),
    (
        "aa",
        "aii",
        "aaaa",
        &["aaaa", "aiiaa", "aaiia", "aaaii", "aiiaii"],
    ),
    ("ai", "i", "aiai", &["aiai", "iai", "aii", "ii"]),
    ("ai", "iai", "aiai", &["aiai", "iaiai", "aiiai", "iaiiai"]),
    ("a", "", "aaa", &["aaa", "aa", "a", ""]),
    ("aa", "", "aaa", &["aaa", "a"]),
    ("ai", "", "aiai", &["aiai", "ai", ""]),
    ("a", "a", "aa", &["aa"]),
];

#[test]
fn literal_operand_shapes_keep_every_independent_site_choice() {
    // Explicit forward substitutions, not expectations obtained from either engine.
    let mut misses = Vec::new();
    for mode in [RewriteMode::Iterative, RewriteMode::Simultaneous] {
        for dir in [Dir::LeftToRight, Dir::RightToLeft] {
            for &(lhs, rhs, input, expected) in MATERIAL_CASES {
                let g = literal_grammar(mode, dir, lhs, rhs, "", "");
                let Some(net) = matrix_relation(&g, lhs) else {
                    continue;
                };
                let actual = outputs(&g, &net, input);
                for word in expected {
                    if !actual.contains(&encoded(&g, word)) {
                        misses.push(format!("{mode:?}/{dir:?} {lhs}->{rhs}: {input}->{word}"));
                    }
                }
            }
        }
    }
    assert!(
        misses.is_empty(),
        "missing independent choices: {misses:#?}"
    );
}

#[test]
fn insertion_operand_shapes_keep_choices_on_both_sides_of_targets() {
    let cases: &[(&str, &str, &str, &str, &[&str])] = &[
        ("i", "", "", "", &["", "i"]),
        ("ai", "", "", "a", &["a", "aia", "aai", "aiaai"]),
        ("ia", "", "", "a", &["a", "iaa", "aia", "iaaia"]),
        ("ii", "", "", "a", &["a", "iia", "aii", "iiaii"]),
        ("i", "a", "", "aa", &["aa", "aia", "aai", "aiai"]),
        ("i", "", "a", "aa", &["aa", "iaa", "aia", "iaia"]),
        ("i", "a", "a", "aa", &["aa", "aia"]),
        ("", "", "", "a", &["a"]),
    ];
    let mut misses = Vec::new();
    for mode in [RewriteMode::Iterative, RewriteMode::Simultaneous] {
        for dir in [Dir::LeftToRight, Dir::RightToLeft] {
            for &(rhs, left, right, input, expected) in cases {
                let g = literal_grammar(mode, dir, "", rhs, left, right);
                let Some(net) = matrix_relation(&g, "") else {
                    continue;
                };
                let actual = outputs(&g, &net, input);
                let alphabet = encoded(&g, "ai");
                assert!(actual
                    .iter()
                    .all(|s| s.chars().all(|c| alphabet.contains(c))));
                for word in expected {
                    if !actual.contains(&encoded(&g, word)) {
                        misses.push(format!(
                            "{mode:?}/{dir:?} 0->{rhs}/{left}_{right}: {input}->{word}"
                        ));
                    }
                }
            }
        }
    }
    assert!(misses.is_empty(), "missing insertion choices: {misses:#?}");
}

#[test]
fn unchanged_insertions_do_not_obscure_processed_side_contexts() {
    for mode in [RewriteMode::Iterative, RewriteMode::Simultaneous] {
        for dir in [Dir::LeftToRight, Dir::RightToLeft] {
            for (rhs, left, right, wanted, both, opaque_dir) in [
                (
                    "ai",
                    "aa",
                    "",
                    ["aaa", "aaaia", "aaaai"],
                    "aaaiaai",
                    Dir::LeftToRight,
                ),
                (
                    "ia",
                    "",
                    "aa",
                    ["aaa", "iaaaa", "aiaaa"],
                    "iaaiaaa",
                    Dir::RightToLeft,
                ),
            ] {
                let g = literal_grammar(mode, dir, "", rhs, left, right);
                let Some(net) = matrix_relation(&g, "") else {
                    continue;
                };
                let actual = outputs(&g, &net, "aaa");
                for word in wanted {
                    assert!(
                        actual.contains(&encoded(&g, word)),
                        "{mode:?}/{dir:?}: {word}"
                    );
                }
                assert_eq!(
                    actual.contains(&encoded(&g, both)),
                    mode != RewriteMode::Iterative || dir != opaque_dir,
                    "contexts must observe the correct side: {mode:?}/{dir:?} {left}_{right}",
                );
            }
        }
    }
}

#[test]
fn mixed_alpha_environment_sites_keep_independent_identity_choices() {
    let g = fixture("per-site-alpha-rewrite");
    let net = relation(&g, &[0]).unwrap();
    let actual = outputs(&g, &net, "uuu");
    for word in ["uuu", "uiu", "uui", "uii"] {
        assert!(actual.contains(&encoded(&g, word)), "missing {word}");
    }
}

/// The one-site outputs synthesis gives `word`, as token strings.
fn synthesized(g: &Grammar, word: &str) -> Vec<String> {
    let PhonRuleDef::Rewrite(rule) = &g.prules[0] else {
        panic!("rewrite fixture")
    };
    let table = &g.char_tables[0];
    let shape = pg_grammar::segment::segment(table, word).unwrap();
    let alphabet = SegAlphabet::new(table);
    pg_rules::rewrite::synthesize(g, rule, &shape)
        .iter()
        .map(|output| alphabet.encode_shape(output))
        .collect()
}

#[test]
fn an_unbound_alpha_output_relates_each_row_only_to_what_synthesis_makes_of_it() {
    let mut g = fixture("overridden-alpha-plain-ltr-left");
    let rule = rewrite(&mut g, 0);
    let material = rule.lhs.clone();
    rule.lhs = rule.subrules[0].rhs.clone();
    rule.subrules[0].rhs = material;
    rule.subrules[0].left_env = None;
    let net = relation(&g, &[0]).unwrap();
    let actual = outputs(&g, &net, "ii");
    assert_eq!(actual, BTreeSet::from([encoded(&g, "ii")]));
    for expected in synthesized(&g, "ii") {
        assert!(actual.contains(&expected));
    }
}

#[test]
fn simultaneous_epenthesis_preserves_overlapping_sites_and_cascade_feeding() {
    let mut g = fixture("iterative-epenthesis-cascade");
    rewrite(&mut g, 0).mode = RewriteMode::Simultaneous;
    rewrite(&mut g, 1).mode = RewriteMode::Simultaneous;
    let net = relation(&g, &[0, 1]).unwrap();
    for (input, output) in [
        ("uaa", "uotaa"),
        ("uaaa", "uotaata"),
        ("uaa", "uata"),
        ("uaa", "uaa"),
    ] {
        assert!(
            outputs(&g, &net, input).contains(&encoded(&g, output)),
            "{input} -> {output}"
        );
    }
}

#[test]
fn deletion_is_optional_at_each_site() {
    let mut g = fixture("iterative-epenthesis-cascade");
    let target = rewrite(&mut g, 1).subrules[0].rhs.clone();
    let rule = rewrite(&mut g, 0);
    rule.lhs = target;
    rule.subrules[0].rhs = Pattern::default();
    rule.subrules[0].left_env = None;
    rule.subrules[0].right_env = None;
    let net = relation(&g, &[0]).unwrap();
    let actual = outputs(&g, &net, "tt");
    for word in ["tt", "t", ""] {
        assert!(
            actual.contains(&encoded(&g, word)),
            "missing deletion output {word:?}"
        );
    }
}

#[test]
fn default_overmatch_does_not_force_all_sites_to_change() {
    // Existing defaults support is retained; this does not add a production capability.
    let g = fixture("rewrite-default-environment");
    let net = relation(&g, &[0]).unwrap();
    assert!(outputs(&g, &net, "batdt").contains(&encoded(&g, "batd")));
}

#[test]
fn word_anchors_survive_widening_in_both_directions() {
    for dir in [Dir::LeftToRight, Dir::RightToLeft] {
        let mut g = fixture("overridden-alpha-plain-ltr-left");
        rewrite(&mut g, 0).dir = dir;
        let net = relation(&g, &[0]).unwrap();
        let actual = outputs(&g, &net, "iuu");
        assert!(actual.contains(&encoded(&g, "iiu")));
        assert!(
            !actual.contains(&encoded(&g, "iii")),
            "anchor must restrict the matching site"
        );
    }
}

#[test]
fn morpheme_boundary_targets_keep_identity_at_other_sites() {
    let mut g = fixture("boundary-first-in-narrowing-lhs");
    let boundary = g.char_tables[0].lookup_nfd("+").unwrap();
    let m = g.char_tables[0].lookup_nfd("m").unwrap();
    let rule = rewrite(&mut g, 0);
    rule.lhs.nodes = vec![PatternNode::CharDef(boundary)];
    rule.subrules.truncate(1);
    rule.subrules[0].rhs.nodes = vec![PatternNode::CharDef(m)];
    rule.subrules[0].left_env = None;
    rule.subrules[0].right_env = None;
    let net = relation(&g, &[0]).unwrap();
    let actual = outputs(&g, &net, "kan+ban+bo");
    for expected in [
        "kan+ban+bo",
        "kanmban+bo",
        "kan+banmbo",
        "kanmbanmbo",
        "kanbanbo",
    ] {
        assert!(
            actual.contains(&encoded(&g, expected)),
            "missing {expected}"
        );
    }
}

#[test]
fn an_empty_cascade_is_a_complete_identity_relation() {
    let g = fixture("iterative-epenthesis-cascade");
    let net = relation(&g, &[]).expect("empty cascade is identity");
    assert_eq!(
        outputs(&g, &net, "uaa"),
        BTreeSet::from([encoded(&g, "uaa")])
    );
}

#[test]
fn unsupported_later_rule_cannot_return_an_earlier_partial_cascade() {
    let mut g = fixture("iterative-epenthesis-cascade");
    rewrite(&mut g, 0).mode = RewriteMode::Simultaneous;
    let rule = rewrite(&mut g, 1);
    rule.lhs.nodes = vec![PatternNode::Quantifier {
        min: 1,
        max: Some(2),
        children: rule.subrules[0].rhs.nodes.clone(),
    }];
    assert!(
        matches!(relation(&g, &[0, 1]), Err(PhonologyLoweringError::UnsupportedRewrite(ref rule)) if rule == "prRule2"),
        "unsupported lowering must reject the entire cascade"
    );
}

#[test]
fn a_segment_class_output_keeps_the_lanes_it_does_not_pin() {
    let mut g = fixture("overridden-alpha-plain-ltr-left");
    let partial = pg_grammar::model::NatClassId(
        g.natural_classes
            .iter()
            .position(|class| class.xml_id == "ncPartial")
            .unwrap() as u32,
    );
    let rule = rewrite(&mut g, 0);
    rule.lhs = rule.subrules[0].rhs.clone();
    let PatternNode::Context(mut context) =
        rule.subrules[0].left_env.as_ref().unwrap().nodes[1].clone()
    else {
        panic!("fixture context")
    };
    context.nat_class = partial;
    context.vars.clear();
    rule.subrules[0].rhs.nodes = vec![PatternNode::Context(context)];
    rule.subrules[0].left_env = None;
    let net = relation(&g, &[0]).unwrap();
    // i/y/a span both values of each lane, so the class pins nothing and synthesis keeps the row.
    let actual = outputs(&g, &net, "ii");
    assert_eq!(actual, BTreeSet::from([encoded(&g, "ii")]));
    for expected in synthesized(&g, "ii") {
        assert!(actual.contains(&expected));
    }
}

#[test]
fn a_failed_cascade_keeps_no_partial_reports() {
    let mut g = fixture("iterative-epenthesis-cascade");
    rewrite(&mut g, 0).mode = RewriteMode::Simultaneous;
    let rule = rewrite(&mut g, 1);
    rule.lhs.nodes = vec![PatternNode::Quantifier {
        min: 1,
        max: Some(2),
        children: rule.subrules[0].rhs.nodes.clone(),
    }];
    let rules: Vec<_> = g.prules.iter().collect();
    let mut reports = Vec::new();
    let result =
        compile_and_compose_rules_recall_safe(&FomaOptions::default(), &g, &rules, &mut reports);
    assert!(
        matches!(result, Err(PhonologyLoweringError::UnsupportedRewrite(ref id)) if id == "prRule2")
    );
    assert!(reports.is_empty());
}

#[test]
fn permissive_metathesis_refuses_until_it_has_a_per_site_construction() {
    let g = fixture("metathesis-featureless-phonemes");
    assert!(matches!(
        relation(&g, &[0]),
        Err(PhonologyLoweringError::PermissiveMetathesisUnavailable(_))
    ));
}

#[test]
fn literal_and_subrule_feeding_keep_later_sites() {
    let mut misses = Vec::new();
    for dir in [Dir::LeftToRight, Dir::RightToLeft] {
        let mut g = fixture("per-site-alpha-rewrite");
        if dir == Dir::RightToLeft {
            for index in [1, 2] {
                let rule = rewrite(&mut g, index);
                rule.dir = dir;
                for subrule in &mut rule.subrules {
                    subrule.right_env = subrule.left_env.take();
                }
            }
        }
        let pairs = if dir == Dir::LeftToRight {
            [(1, "iuu", "iii"), (2, "uuu", "uyi")]
        } else {
            [(1, "uui", "iii"), (2, "uuu", "iyu")]
        };
        for (index, input, output) in pairs {
            let net = relation(&g, &[index]).unwrap();
            if !outputs(&g, &net, input).contains(&encoded(&g, output)) {
                misses.push(format!("{dir:?} rule {index}: {input} -> {output}"));
            }
        }
    }
    assert!(misses.is_empty(), "missing feeding outputs: {misses:?}");
}

#[test]
fn final_word_anchors_survive_widening_in_both_directions() {
    for dir in [Dir::LeftToRight, Dir::RightToLeft] {
        let mut g = fixture("overridden-alpha-plain-ltr-left");
        let rule = rewrite(&mut g, 0);
        rule.dir = dir;
        let mut env = rule.subrules[0].left_env.take().unwrap();
        env.nodes.reverse();
        rule.subrules[0].right_env = Some(env);
        let net = relation(&g, &[0]).unwrap();
        let actual = outputs(&g, &net, "uui");
        assert!(actual.contains(&encoded(&g, "uii")));
        assert!(!actual.contains(&encoded(&g, "iii")));
    }
}

#[test]
fn simultaneous_contexts_observe_the_input_shape() {
    let mut g = fixture("per-site-alpha-rewrite");
    rewrite(&mut g, 1).mode = RewriteMode::Simultaneous;
    let net = relation(&g, &[1]).unwrap();
    let actual = outputs(&g, &net, "iuu");
    assert!(actual.contains(&encoded(&g, "iiu")));
    assert!(!actual.contains(&encoded(&g, "iii")));
}

#[test]
fn epenthesis_can_insert_at_initial_and_final_word_boundaries() {
    use pg_grammar::model::AnchorSide;
    for dir in [Dir::LeftToRight, Dir::RightToLeft] {
        for side in [AnchorSide::Left, AnchorSide::Right] {
            let mut g = fixture("iterative-epenthesis-cascade");
            let rule = rewrite(&mut g, 0);
            rule.dir = dir;
            rule.mode = RewriteMode::Simultaneous;
            let anchor = Pattern {
                nodes: vec![PatternNode::Anchor(side)],
            };
            rule.subrules[0].left_env = (side == AnchorSide::Left).then_some(anchor.clone());
            rule.subrules[0].right_env = (side == AnchorSide::Right).then_some(anchor);
            let net = relation(&g, &[0]).unwrap();
            let actual = outputs(&g, &net, "uaa");
            let expected = if side == AnchorSide::Left {
                "ouaa"
            } else {
                "uaao"
            };
            assert!(actual.contains(&encoded(&g, expected)), "{dir:?} {side:?}");
            assert!(actual.contains(&encoded(&g, "uaa")));
            assert!(!actual.contains(&encoded(&g, "uoaa")));
        }
    }
}

#[test]
fn optional_morpheme_boundaries_can_be_crossed_and_preserved() {
    let g = fixture("per-site-alpha-rewrite");
    let net = relation(&g, &[3]).unwrap();
    let actual = outputs(&g, &net, "a+ua");
    assert!(actual.contains(&encoded(&g, "a+ia")));
    assert!(actual.contains(&encoded(&g, "a+ua")));
}

#[test]
fn widened_earlier_subrules_cannot_preempt_later_material_outputs() {
    let mut g = fixture("per-site-alpha-rewrite");
    let y = g.char_tables[0].lookup_nfd("y").unwrap();
    let mut fallback_source = fixture("per-site-alpha-rewrite");
    let mut fallback = rewrite(&mut fallback_source, 0).subrules.remove(0);
    let rule = rewrite(&mut g, 0);
    fallback.rhs.nodes = vec![PatternNode::CharDef(y)];
    for node in &mut fallback.left_env.as_mut().unwrap().nodes {
        if let PatternNode::Context(context) = node {
            context.vars.clear();
        }
    }
    rule.subrules.push(fallback);
    let net = relation(&g, &[0]).unwrap();
    let actual = outputs(&g, &net, "uuu");
    for left in ["u", "i", "y"] {
        for right in ["u", "i", "y"] {
            let word = format!("u{left}{right}");
            assert!(actual.contains(&encoded(&g, &word)), "missing {word}");
        }
    }
}

#[test]
fn rtl_context_expansions_include_founding_oracle_outputs() {
    let mut misses = Vec::new();
    for (name, input, output) in [
        ("rtl-expansion-context", "au", "aau"),
        ("rtl-expansion-final-anchor", "a", "aa"),
    ] {
        let g = fixture(name);
        let net = relation(&g, &[0]).expect("compiler claims a complete relation");
        let actual = outputs(&g, &net, input);
        assert!(actual.contains(&encoded(&g, input)));
        if !actual.contains(&encoded(&g, output)) {
            misses.push(format!("{name}: {input}->{output}"));
        }
    }
    assert!(
        misses.is_empty(),
        "missing C#-confirmed expansions: {misses:?}"
    );
}

fn literal_pattern(g: &Grammar, text: &str) -> Pattern {
    Pattern {
        nodes: text
            .chars()
            .map(|c| PatternNode::CharDef(g.char_tables[0].lookup_nfd(&c.to_string()).unwrap()))
            .collect(),
    }
}

#[test]
fn nonempty_operand_shapes_keep_choices_in_environments() {
    use pg_grammar::model::AnchorSide;
    let mut misses = Vec::new();
    for mode in [RewriteMode::Iterative, RewriteMode::Simultaneous] {
        for dir in [Dir::LeftToRight, Dir::RightToLeft] {
            for &(lhs, rhs, _, _) in MATERIAL_CASES {
                for alpha in [false, true] {
                    for side in [0, 1, 2] {
                        let mut g = fixture("overridden-alpha-plain-ltr-left");
                        let mut focus = literal_pattern(&g, lhs);
                        let material = literal_pattern(&g, rhs);
                        let flank = if alpha { "i" } else { "u" };
                        let mut environment = literal_pattern(&g, flank);
                        if alpha {
                            environment = opposing_alpha_environment(&mut g, &mut focus);
                        }
                        let rule = rewrite(&mut g, 0);
                        rule.mode = mode;
                        rule.dir = dir;
                        rule.lhs = focus;
                        rule.subrules[0].rhs = material;
                        rule.subrules[0].left_env = (side != 1).then_some(environment.clone());
                        rule.subrules[0].right_env = (side != 0).then_some(environment);
                        let prefix = if side != 1 { flank } else { "" };
                        let suffix = if side != 0 { flank } else { "" };
                        let input = format!("{prefix}{lhs}{flank}{lhs}{suffix}");
                        let Some(net) = matrix_relation(&g, lhs) else {
                            continue;
                        };
                        let actual = outputs(&g, &net, &input);
                        // The unchanged delimiter preserves both sites' environments for all four choices.
                        for first in [lhs, rhs] {
                            for second in [lhs, rhs] {
                                let word = format!("{prefix}{first}{flank}{second}{suffix}");
                                if !actual.contains(&encoded(&g, &word)) {
                                    misses.push(format!("{mode:?}/{dir:?} alpha={alpha} side={side} {lhs}->{rhs}: {input}->{word}"));
                                }
                            }
                        }
                    }
                }
                for anchored in 0..5 {
                    let mut g = fixture("overridden-alpha-plain-ltr-left");
                    let focus = literal_pattern(&g, lhs);
                    let material = literal_pattern(&g, rhs);
                    let mut left = literal_pattern(&g, if anchored == 3 { "u" } else { "" });
                    let mut right = literal_pattern(&g, if anchored == 4 { "u" } else { "" });
                    if matches!(anchored, 0 | 2 | 3) {
                        left.nodes.insert(0, PatternNode::Anchor(AnchorSide::Left));
                    }
                    if matches!(anchored, 1 | 2 | 4) {
                        right.nodes.push(PatternNode::Anchor(AnchorSide::Right));
                    }
                    let (input, changed) = match anchored {
                        0 => (format!("{lhs}u{lhs}"), format!("{rhs}u{lhs}")),
                        1 => (format!("{lhs}u{lhs}"), format!("{lhs}u{rhs}")),
                        2 => (lhs.to_string(), rhs.to_string()),
                        3 => (format!("u{lhs}"), format!("u{rhs}")),
                        4 => (format!("{lhs}u"), format!("{rhs}u")),
                        _ => unreachable!(),
                    };
                    let rule = rewrite(&mut g, 0);
                    rule.mode = mode;
                    rule.dir = dir;
                    rule.lhs = focus;
                    rule.subrules[0].rhs = material;
                    rule.subrules[0].left_env = (!left.nodes.is_empty()).then_some(left);
                    rule.subrules[0].right_env = (!right.nodes.is_empty()).then_some(right);
                    let Some(net) = matrix_relation(&g, lhs) else {
                        continue;
                    };
                    let actual = outputs(&g, &net, &input);
                    for word in [&input, &changed] {
                        if !actual.contains(&encoded(&g, word)) {
                            misses.push(format!(
                                "{mode:?}/{dir:?} anchor={anchored} {lhs}->{rhs}: {input}->{word}"
                            ));
                        }
                    }
                }
            }
        }
    }
    assert!(misses.is_empty(), "missing contextual choices: {misses:#?}");
}

#[test]
fn expansions_preserve_directional_feeding_and_bleeding() {
    for mode in [RewriteMode::Iterative, RewriteMode::Simultaneous] {
        for (dir, rhs, left, right, input, changed, both, feeding) in [
            (
                Dir::LeftToRight,
                "ai",
                "i",
                "",
                "iaa",
                "iaia",
                "iaiai",
                true,
            ),
            (
                Dir::RightToLeft,
                "ia",
                "",
                "i",
                "aai",
                "aiai",
                "iaiai",
                true,
            ),
            (
                Dir::LeftToRight,
                "ai",
                "a",
                "",
                "aaa",
                "aaia",
                "aaiai",
                false,
            ),
            (
                Dir::RightToLeft,
                "ia",
                "",
                "a",
                "aaa",
                "aiaa",
                "iaiaa",
                false,
            ),
        ] {
            let (g, net) = literal_case(mode, dir, "a", rhs, left, right);
            let actual = outputs(&g, &net, input);
            assert!(actual.contains(&encoded(&g, input)));
            assert!(
                actual.contains(&encoded(&g, changed)),
                "{mode:?}/{dir:?}: {changed}"
            );
            assert_eq!(
                actual.contains(&encoded(&g, both)),
                (mode == RewriteMode::Iterative) == feeding,
                "expansion must observe the processed side: {mode:?}/{dir:?} {input}->{both}",
            );
        }
    }
}

#[test]
fn context_bearing_expansions_keep_mixed_sites_and_reject_wrong_flanks() {
    for mode in [RewriteMode::Iterative, RewriteMode::Simultaneous] {
        for dir in [Dir::LeftToRight, Dir::RightToLeft] {
            for rhs in ["aa", "ai", "ia", "aia"] {
                for right_side in [false, true] {
                    let mut g = fixture("overridden-alpha-plain-ltr-left");
                    let focus = literal_pattern(&g, "a");
                    let material = literal_pattern(&g, rhs);
                    let environment = literal_pattern(&g, "u");
                    let rule = rewrite(&mut g, 0);
                    rule.mode = mode;
                    rule.dir = dir;
                    rule.lhs = focus;
                    rule.subrules[0].rhs = material;
                    rule.subrules[0].left_env = (!right_side).then_some(environment.clone());
                    rule.subrules[0].right_env = right_side.then_some(environment);
                    let net = relation(&g, &[0]).unwrap();
                    let (input, changed) = if right_side {
                        ("auai", format!("{rhs}uai"))
                    } else {
                        ("iaua", format!("iau{rhs}"))
                    };
                    assert_eq!(
                        outputs(&g, &net, input),
                        BTreeSet::from([encoded(&g, input), encoded(&g, &changed)]),
                        "{mode:?}/{dir:?} right={right_side} rhs={rhs}"
                    );
                    assert_eq!(
                        outputs(&g, &net, "iai"),
                        BTreeSet::from([encoded(&g, "iai")])
                    );
                }
            }
        }
    }
}

fn opposing_alpha_environment(g: &mut Grammar, focus: &mut Pattern) -> Pattern {
    use pg_grammar::model::{NatClassId, NaturalClass, NaturalClassKind};
    let PatternNode::Context(mut context) = rewrite(g, 0).lhs.nodes[0].clone() else {
        panic!("alpha focus")
    };
    let environment = rewrite(g, 0).subrules[0].left_env.as_ref().unwrap().nodes[1].clone();
    let a = g.char_tables[0].lookup_nfd("a").unwrap();
    context.nat_class = NatClassId(g.natural_classes.len() as u32);
    g.natural_classes.push(NaturalClass {
        xml_id: "testOnlyA".into(),
        name: None,
        kind: NaturalClassKind::Segments(vec![a]),
    });
    focus.nodes[0] = PatternNode::Context(context);
    Pattern {
        nodes: vec![environment],
    }
}

#[test]
fn alpha_contexts_keep_expansions_at_both_word_edges() {
    use pg_grammar::model::AnchorSide;
    for mode in [RewriteMode::Iterative, RewriteMode::Simultaneous] {
        for dir in [Dir::LeftToRight, Dir::RightToLeft] {
            for &(lhs, rhs, _, _) in MATERIAL_CASES {
                for side in [AnchorSide::Left, AnchorSide::Right] {
                    let mut g = fixture("overridden-alpha-plain-ltr-left");
                    let mut focus = literal_pattern(&g, lhs);
                    let material = literal_pattern(&g, rhs);
                    let mut environment = opposing_alpha_environment(&mut g, &mut focus);
                    let (input, changed) = if side == AnchorSide::Left {
                        environment.nodes.insert(0, PatternNode::Anchor(side));
                        (format!("i{lhs}"), format!("i{rhs}"))
                    } else {
                        environment.nodes.push(PatternNode::Anchor(side));
                        (format!("{lhs}i"), format!("{rhs}i"))
                    };
                    let rule = rewrite(&mut g, 0);
                    rule.mode = mode;
                    rule.dir = dir;
                    rule.lhs = focus;
                    rule.subrules[0].rhs = material;
                    rule.subrules[0].left_env =
                        (side == AnchorSide::Left).then_some(environment.clone());
                    rule.subrules[0].right_env = (side == AnchorSide::Right).then_some(environment);
                    let Some(net) = matrix_relation(&g, lhs) else {
                        continue;
                    };
                    let actual = outputs(&g, &net, &input);
                    for word in [&input, &changed] {
                        assert!(
                            actual.contains(&encoded(&g, word)),
                            "{mode:?}/{dir:?} {side:?}: {input}->{word}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn mirrored_cascade_refuses_an_unsupported_successor_atomically() {
    let mut g = fixture("iterative-epenthesis-cascade");
    let target = rewrite(&mut g, 1).subrules[0].rhs.clone();
    let rule = rewrite(&mut g, 0);
    rule.dir = Dir::RightToLeft;
    rule.lhs = target;
    rule.subrules[0].left_env = None;
    rule.subrules[0].right_env = None;
    assert!(relation(&g, &[0]).is_ok());
    let rule = rewrite(&mut g, 1);
    rule.lhs.nodes = vec![PatternNode::Quantifier {
        min: 1,
        max: Some(2),
        children: rule.subrules[0].rhs.nodes.clone(),
    }];
    let rules: Vec<_> = g.prules.iter().collect();
    let mut reports = vec![("retained".to_string(), Vec::new())];
    assert!(matches!(
        compile_and_compose_rules_recall_safe(&FomaOptions::default(), &g, &rules, &mut reports),
        Err(PhonologyLoweringError::UnsupportedRewrite(ref id)) if id == "prRule2"
    ));
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].0, "retained");
    assert!(reports[0].1.is_empty());
}

#[test]
fn asymmetric_multisegment_contexts_preserve_order_and_independent_sites() {
    for mode in [RewriteMode::Iterative, RewriteMode::Simultaneous] {
        for dir in [Dir::LeftToRight, Dir::RightToLeft] {
            for lhs in ["a", "aa", "ai"] {
                for rhs in ["aa", "ai", "ia", "aia"] {
                    for side in [0, 1, 2] {
                        let mut g = fixture("overridden-alpha-plain-ltr-left");
                        let focus = literal_pattern(&g, lhs);
                        let material = literal_pattern(&g, rhs);
                        let left = literal_pattern(&g, "iu");
                        let right = literal_pattern(&g, "ui");
                        let prefix = if side != 1 { "iu" } else { "" };
                        let suffix = if side != 0 { "ui" } else { "" };
                        let between = format!("{suffix}{prefix}");
                        let rule = rewrite(&mut g, 0);
                        rule.mode = mode;
                        rule.dir = dir;
                        rule.lhs = focus;
                        rule.subrules[0].rhs = material;
                        rule.subrules[0].left_env = (side != 1).then_some(left);
                        rule.subrules[0].right_env = (side != 0).then_some(right);
                        let input = format!("{prefix}{lhs}{between}{lhs}{suffix}");
                        let Some(net) = matrix_relation(&g, lhs) else {
                            continue;
                        };
                        let actual = outputs(&g, &net, &input);
                        for first in [lhs, rhs] {
                            for second in [lhs, rhs] {
                                let word = format!("{prefix}{first}{between}{second}{suffix}");
                                assert!(
                                    actual.contains(&encoded(&g, &word)),
                                    "{mode:?}/{dir:?} {lhs}->{rhs} side={side}: {input}->{word}"
                                );
                            }
                        }
                        // The block relation ignores environments, so only the site construction is exact here.
                        if mode == RewriteMode::Iterative && lhs.chars().count() > 1 {
                            continue;
                        }
                        let wrong = match side {
                            0 => format!("ui{lhs}"),
                            1 => format!("{lhs}iu"),
                            2 => format!("iu{lhs}iu"),
                            _ => unreachable!(),
                        };
                        assert_eq!(
                            outputs(&g, &net, &wrong),
                            BTreeSet::from([encoded(&g, &wrong)])
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn simultaneous_multi_token_refusal_cannot_leave_a_partial_cascade() {
    let mut g = fixture("per-site-alpha-rewrite");
    let a = g.char_tables[0].lookup_nfd("a").unwrap();
    let i = g.char_tables[0].lookup_nfd("i").unwrap();
    for index in [0, 1] {
        let rule = rewrite(&mut g, index);
        rule.lhs.nodes = vec![PatternNode::CharDef(a)];
        rule.subrules.truncate(1);
        rule.subrules[0].rhs.nodes = vec![PatternNode::CharDef(i)];
        rule.subrules[0].left_env = None;
        rule.subrules[0].right_env = None;
    }
    rewrite(&mut g, 0).dir = Dir::RightToLeft;
    assert!(relation(&g, &[0]).is_ok());
    let rule = rewrite(&mut g, 1);
    rule.mode = RewriteMode::Simultaneous;
    rule.lhs.nodes.push(PatternNode::CharDef(a));
    assert!(matches!(relation(&g, &[0, 1]),
        Err(PhonologyLoweringError::SimultaneousMultiTokenFocusUnavailable(ref id)) if id == "prLiteralFeed"
    ));
    let mut reports = Vec::new();
    let error = compile_and_compose_rules_recall_safe(
        &FomaOptions::default(),
        &g,
        &[&g.prules[0], &g.prules[1]],
        &mut reports,
    )
    .unwrap_err();
    assert!(reports.is_empty());
    assert!(error.to_string().contains("simultaneous multi-token focus"));
}

#[test]
fn unattested_intermediate_bundle_includes_founding_oracle_outputs() {
    for (name, input, expected) in [
        ("unattested-intermediate-bundle", "a", "e"),
        ("unattested-intermediate-environments", "xa", "ze"),
    ] {
        for mode in [RewriteMode::Iterative, RewriteMode::Simultaneous] {
            for dir in [Dir::LeftToRight, Dir::RightToLeft] {
                let mut g = fixture(name);
                for index in 0..g.prules.len() {
                    let rule = rewrite(&mut g, index);
                    rule.mode = mode;
                    rule.dir = dir;
                }
                let positions: Vec<_> = (0..g.prules.len()).collect();
                let net = relation(&g, &positions).expect("complete intermediate cascade");
                let actual = outputs(&g, &net, input);
                assert!(
                    actual.contains(&encoded(&g, expected)),
                    "{name}/{mode:?}/{dir:?}: missing independently derived {input} -> {expected}"
                );
                assert!(
                    actual.iter().all(|output| output
                        .chars()
                        .all(|token| (token as u32) < PUA_BASE + g.char_tables[0].len() as u32)),
                    "private intermediate token escaped"
                );
            }
        }
    }
}

#[test]
fn every_pattern_kind_is_included_or_refused_at_its_actual_compile_seam() {
    use pg_grammar::model::AnchorSide;
    let base = fixture("per-site-alpha-rewrite");
    let PhonRuleDef::Rewrite(rule) = &base.prules[0] else {
        panic!("rewrite")
    };
    let alpha = rule.subrules[0].left_env.as_ref().unwrap().nodes[0].clone();
    let PatternNode::Context(mut plain) = alpha.clone() else {
        panic!("context")
    };
    plain.vars.clear();
    let cd = base.char_tables[0].lookup_nfd("u").unwrap();
    let literal = PatternNode::CharDef(cd);
    let grouped = PatternNode::Segments {
        table: TableId(0),
        shape: pg_grammar::model::SegmentedText {
            shape: pg_grammar::segment::segment(&base.char_tables[0], "u").unwrap(),
            text: "u".into(),
        },
    };
    let nodes = [
        literal.clone(),
        PatternNode::Context(plain),
        alpha,
        PatternNode::Quantifier {
            min: 1,
            max: Some(2),
            children: vec![literal.clone()],
        },
        PatternNode::Quantifier {
            min: 0,
            max: None,
            children: vec![literal.clone()],
        },
        PatternNode::Anchor(AnchorSide::Left),
        PatternNode::Anchor(AnchorSide::Right),
        grouped,
    ];
    for node in nodes {
        for position in 0..4 {
            let covered = match &node {
                PatternNode::CharDef(_) | PatternNode::Context(_) => true,
                PatternNode::Quantifier { .. }
                | PatternNode::Anchor(_)
                | PatternNode::Segments { .. } => position >= 2,
            };
            for mode in [RewriteMode::Iterative, RewriteMode::Simultaneous] {
                for dir in [Dir::LeftToRight, Dir::RightToLeft] {
                    let mut g = fixture("per-site-alpha-rewrite");
                    let material = g.char_tables[0].lookup_nfd("i").unwrap();
                    let rule = rewrite(&mut g, 0);
                    rule.mode = mode;
                    rule.dir = dir;
                    rule.lhs.nodes = vec![literal.clone()];
                    rule.subrules.truncate(1);
                    rule.subrules[0].rhs.nodes = vec![PatternNode::CharDef(material)];
                    rule.subrules[0].left_env = None;
                    rule.subrules[0].right_env = None;
                    let pattern = Pattern {
                        nodes: vec![node.clone()],
                    };
                    match position {
                        0 => rule.lhs = pattern,
                        1 => rule.subrules[0].rhs = pattern,
                        2 => rule.subrules[0].left_env = Some(pattern),
                        3 => rule.subrules[0].right_env = Some(pattern),
                        _ => unreachable!(),
                    }
                    let result = relation(&g, &[0]);
                    if covered {
                        result.unwrap_or_else(|error| {
                            panic!("covered {node:?} position={position} {mode:?}/{dir:?}: {error}")
                        });
                    } else {
                        assert!(
                            matches!(result, Err(PhonologyLoweringError::UnsupportedRewrite(_))),
                            "uncovered {node:?} position={position} must refuse atomically"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn intermediate_closure_exhaustion_refuses_the_whole_cascade_and_reports() {
    let features: String = (0..13).map(|i| format!("<SymbolicFeature id='f{i}'><Name>feature {i}</Name><Symbols><Symbol id='p{i}'>+</Symbol><Symbol id='m{i}'>-</Symbol></Symbols></SymbolicFeature>")).collect();
    let values: String = (0..13)
        .map(|i| format!("<FeatureValue feature='f{i}' symbolValues='m{i}'/>"))
        .collect();
    let classes: String = (0..13).map(|i| format!("<FeatureNaturalClass id='n{i}'><Name>class {i}</Name><FeatureValue feature='f{i}' symbolValues='p{i}'/></FeatureNaturalClass>")).collect();
    let rules: String = (0..13).map(|i| format!("<PhonologicalRule id='r{i}' multipleApplicationOrder='leftToRightIterative'><Name>rule {i}</Name><PhoneticInput><PhoneticSequence><Segment segment='cA'/></PhoneticSequence></PhoneticInput><PhonologicalSubrules><PhonologicalSubrule><PhoneticOutput><PhoneticSequence><SimpleContext naturalClass='n{i}'/><SimpleContext naturalClass='n{i}'/></PhoneticSequence></PhoneticOutput></PhonologicalSubrule></PhonologicalSubrules></PhonologicalRule>")).collect();
    let order = (0..13)
        .map(|i| format!("r{i}"))
        .collect::<Vec<_>>()
        .join(" ");
    let xml = format!("<HermitCrabInput><Language><Name>Closure bound</Name><PartsOfSpeech><PartOfSpeech id='n'><Name>n</Name></PartOfSpeech></PartsOfSpeech><PhonologicalFeatureSystem>{features}</PhonologicalFeatureSystem><CharacterDefinitionTable id='t'><Name>table</Name><SegmentDefinitions><SegmentDefinition id='cA'><Representations><Representation>a</Representation></Representations>{values}</SegmentDefinition></SegmentDefinitions></CharacterDefinitionTable><NaturalClasses>{classes}</NaturalClasses><PhonologicalRuleDefinitions>{rules}</PhonologicalRuleDefinitions><Strata><Stratum characterDefinitionTable='t' phonologicalRules='{order}'><Name>Main</Name><LexicalEntries/></Stratum></Strata></Language></HermitCrabInput>");
    let g = pg_grammar::load(&xml).unwrap();
    let rules: Vec<_> = g.prules.iter().collect();
    let mut reports = vec![("untouched".into(), Vec::new())];
    let error =
        compile_and_compose_rules_recall_safe(&FomaOptions::default(), &g, &rules, &mut reports)
            .unwrap_err();
    assert!(matches!(
        error,
        PhonologyLoweringError::IntermediateBundleClosureUnavailable(_)
    ));
    assert!(error.to_string().contains("65536-bundle / 100 MiB"));
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].0, "untouched");
}

#[test]
fn a_multi_table_grammar_refuses_without_guessing_a_rule_owner() {
    let mut g = fixture("per-site-alpha-rewrite");
    let mut other = fixture("per-site-alpha-rewrite");
    g.char_tables.push(other.char_tables.remove(0));
    let mut stratum = other.strata.remove(0);
    stratum.table = TableId(1);
    stratum.prules = vec![PRuleId(1)];
    g.strata[0].prules = vec![PRuleId(0)];
    g.strata.push(stratum);
    assert!(
        relation(&g, &[]).is_ok(),
        "an empty cascade needs no table choice"
    );
    assert!(
        matches!(
            relation(&g, &[0]),
            Err(PhonologyLoweringError::MultipleCascadeTablesUnavailable)
        ),
        "even a single rule must not guess among grammar tables"
    );
    g.strata[1].prules.push(PRuleId(0));
    assert!(
        matches!(
            relation(&g, &[0]),
            Err(PhonologyLoweringError::MultipleCascadeTablesUnavailable)
        ),
        "a reused rule must not select its first table and claim completeness"
    );
    let mut reports = vec![("untouched".into(), Vec::new())];
    assert_eq!(
        compile_and_compose_rules_recall_safe(
            &FomaOptions::default(),
            &g,
            &[&g.prules[0], &g.prules[1]],
            &mut reports
        )
        .unwrap_err(),
        PhonologyLoweringError::MultipleCascadeTablesUnavailable
    );
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].0, "untouched");
}

#[test]
fn iterative_overlapping_focus_uses_the_block_relation() {
    for dir in [Dir::LeftToRight, Dir::RightToLeft] {
        let g = literal_grammar(RewriteMode::Iterative, dir, "aa", "iii", "", "");
        let mut reports = Vec::new();
        let (net, evidence) =
            compile_proposing_cascade(&FomaOptions::default(), &g, &[&g.prules[0]], &mut reports)
                .expect("block relation");
        assert_eq!(evidence.rules[0].construction, RuleConstruction::Block);
        assert_eq!(reports.len(), 1);
        let actual = outputs(&g, &net, "aaa");
        for word in ["aaa", "iiia", "aiii"] {
            assert!(actual.contains(&encoded(&g, word)), "{word}");
        }
    }
}

#[test]
fn an_empty_class_is_not_an_epenthesis_center() {
    let g = fixture("unattested-intermediate-bundle");
    let net = relation(&g, &[1]).expect("empty target is a complete no-match relation");
    assert_eq!(outputs(&g, &net, "a"), BTreeSet::from([encoded(&g, "a")]));
}

#[test]
fn bounded_iterative_epenthesis_keeps_the_fed_output() {
    let g = fixture("bounded-iterative-epenthesis");
    // Forward e -> ae -> aae is independently confirmed by pinned C#.
    let mut reports = vec![("retained".to_owned(), Vec::new())];
    let net = compile_and_compose_rules_recall_safe(
        &FomaOptions::default(),
        &g,
        &[&g.prules[0]],
        &mut reports,
    )
    .expect("fed insertion has a proposing construction");
    assert_eq!(reports.len(), 2);
    for word in ["e", "ae", "aae"] {
        assert!(
            super::insertion_tests::relates(&net, &encoded(&g, "e"), &encoded(&g, word)),
            "{word}"
        );
    }
}

#[test]
fn iterative_empty_focus_inserts_in_both_directions_with_any_environment() {
    for dir in [Dir::LeftToRight, Dir::RightToLeft] {
        // Right-to-left synthesis places the replacement reversed.
        let reversed = dir == Dir::RightToLeft;
        for (rhs, left, right, input, inserted) in [
            ("i", "", "", "a", Some("ia")),
            (
                "ai",
                "a",
                "",
                "a",
                Some(if reversed { "aia" } else { "aai" }),
            ),
            (
                "ia",
                "",
                "a",
                "a",
                Some(if reversed { "aia" } else { "iaa" }),
            ),
            ("", "a", "a", "aa", None),
        ] {
            let g = literal_grammar(RewriteMode::Iterative, dir, "", rhs, left, right);
            let net = relation(&g, &[0]).expect("insertion has a proposing construction");
            let relates = |output: &str| {
                super::insertion_tests::relates(&net, &encoded(&g, input), &encoded(&g, output))
            };
            assert!(relates(input), "{dir:?} {rhs}: identity");
            match inserted {
                Some(word) => assert!(relates(word), "{dir:?} 0->{rhs}/{left}_{right}: {word}"),
                None => assert!(
                    !relates("iaa"),
                    "{dir:?}: an empty replacement inserts nothing"
                ),
            }
        }
    }
}

#[test]
fn an_iterative_insertion_successor_joins_the_cascade() {
    for dir in [Dir::LeftToRight, Dir::RightToLeft] {
        let mut g = literal_grammar(RewriteMode::Iterative, dir, "", "i", "", "");
        let mut first = literal_grammar(RewriteMode::Iterative, dir, "i", "i", "", "")
            .prules
            .remove(0);
        let PhonRuleDef::Rewrite(rule) = &mut first else {
            unreachable!()
        };
        rule.xml_id = "supported-prefix".to_owned();
        rule.lhs = rule.subrules[0].rhs.clone();
        g.prules.insert(0, first);
        g.strata[0].prules.push(pg_grammar::model::PRuleId(1));
        assert!(relation(&g, &[0]).is_ok());
        let mut reports = Vec::new();
        let net = compile_and_compose_rules_recall_safe(
            &FomaOptions::default(),
            &g,
            &[&g.prules[0], &g.prules[1]],
            &mut reports,
        )
        .expect("a cascade with an iterative insertion compiles");
        assert_eq!(reports.len(), 2);
        assert!(super::insertion_tests::relates(
            &net,
            &encoded(&g, "a"),
            &encoded(&g, "iia")
        ));
    }
}
