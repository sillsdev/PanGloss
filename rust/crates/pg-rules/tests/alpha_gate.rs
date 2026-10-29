//! Alpha-variable agreement in targets and environments.

#[path = "common/mod.rs"]
mod common;

use common::*;
use pg_grammar_model::chardef::CharDefId;
use pg_grammar_model::model::{
    Dir, Grammar, Pattern, PatternNode, RewriteMode, RewriteRuleDef, RewriteSubruleDef,
};
use pg_shape::{NodeFlags, NodeKind, Shape, ShapeBuilder};

fn subrule(rhs: Pattern, left: Option<Pattern>, right: Option<Pattern>) -> RewriteSubruleDef {
    RewriteSubruleDef {
        required_pos: None,
        required_mpr: pg_grammar_model::model::MprSet::EMPTY,
        excluded_mpr: pg_grammar_model::model::MprSet::EMPTY,
        rhs,
        left_env: left,
        right_env: right,
        self_opaquing: false, // every rule this file builds is Iterative-mode
    }
}

fn rule(lhs: Pattern, sr: RewriteSubruleDef) -> RewriteRuleDef {
    RewriteRuleDef {
        xml_id: "t".into(),
        name: None,
        mode: RewriteMode::Iterative,
        dir: Dir::LeftToRight,
        vars: Default::default(),
        lhs,
        subrules: vec![sr],
    }
}

fn pat(node: PatternNode) -> Pattern {
    Pattern { nodes: vec![node] }
}
fn pat_char(cd: CharDefId) -> Pattern {
    pat(PatternNode::CharDef(cd))
}

fn interior(s: &Shape) -> Vec<(u32, Vec<u64>)> {
    (0..s.len())
        .filter(|&i| !matches!(s.kind(i), NodeKind::LeftAnchor | NodeKind::RightAnchor))
        .map(|i| (s.char_def(i), s.node_lanes(i).to_vec()))
        .collect()
}

fn seg(g: &Grammar, w: &str) -> Shape {
    pg_rules::shape_feat::segment_with_features(g, table(g), w).unwrap()
}

// poa lane values in the alpha grammar: lab = 0b01, vel = 0b10 (FlatIndex 2).
const POA: usize = 2;

// Nasal place assimilation: n -> [poa = alpha_a] / _ [C poa = alpha_a]; right env binds the place, RHS applies it.
fn nasal_assim_rule(g: &Grammar) -> RewriteRuleDef {
    let poa = feat(g, "feat_poa");
    rule(
        pat_char(char_def(g, "char_n")),
        subrule(
            // RHS: any segment, poa = αa (the bound place is written onto the nasal).
            pat(PatternNode::Context(ctx_var(
                nat_class(g, "nc_any"),
                poa,
                0,
                true,
            ))),
            None,
            // Right env: a consonant whose poa binds αa.
            Some(pat(PatternNode::Context(ctx_var(
                nat_class(g, "nc_cons"),
                poa,
                0,
                true,
            )))),
        ),
    )
}

#[test]
fn nasal_assimilation_applies_the_bound_place() {
    let g = load_alpha_grammar();
    let r = nasal_assim_rule(&g);

    // "ng": the following g is velar -> the nasal's poa becomes velar (0b10).
    let out = pg_rules::rewrite::synthesize(&g, &r, &seg(&g, "ng"));
    assert_eq!(out.len(), 1, "n assimilates before g");
    assert_eq!(interior(&out[0])[0].1[POA], 0b10, "n poa -> velar (from g)");

    // "nb": the following b is labial -> the nasal's poa becomes labial (0b01).
    let out2 = pg_rules::rewrite::synthesize(&g, &r, &seg(&g, "nb"));
    assert_eq!(
        interior(&out2[0])[0].1[POA],
        0b01,
        "n poa -> labial (from b)"
    );
}

// Place-agreement voicing: target binds alpha_a from its own place; the following consonant must agree in place, else the candidate is rejected.
fn place_agreement_rule(g: &Grammar) -> RewriteRuleDef {
    let poa = feat(g, "feat_poa");
    rule(
        pat(PatternNode::Context(ctx_var(
            nat_class(g, "nc_cons"),
            poa,
            0,
            true,
        ))),
        subrule(
            pat(PatternNode::Context(ctx(nat_class(g, "nc_voiced")))), // -> [+voice]
            None,
            Some(pat(PatternNode::Context(ctx_var(
                nat_class(g, "nc_cons"),
                poa,
                0,
                true,
            )))),
        ),
    )
}

#[test]
fn place_agreement_applies_when_places_agree() {
    let g = load_alpha_grammar();
    let r = place_agreement_rule(&g);
    // "pb": labial precedes labial, places agree, so p voices (voi lane 0b10 -> 0b01).
    let out = pg_rules::rewrite::synthesize(&g, &r, &seg(&g, "pb"));
    assert_eq!(out.len(), 1, "agreeing places: rule applies");
    assert_eq!(
        interior(&out[0])[0].1[1],
        0b01,
        "p voiced (agreement satisfied)"
    );
}

#[test]
fn place_agreement_rejects_when_places_disagree() {
    let g = load_alpha_grammar();
    let r = place_agreement_rule(&g);
    // "pk": places disagree; the FST alone would wrongly voice p, but the agreement check rejects it.
    let out = pg_rules::rewrite::synthesize(&g, &r, &seg(&g, "pk"));
    assert!(
        out.is_empty(),
        "disagreeing places: candidate rejected, rule does not apply"
    );
}

// Analysis-side agreement: un-applying the place-agreement voicing rule binds alpha_a from the matched voiced consonant's place and rejects if the following consonant's place disagrees.

#[test]
fn analysis_agreement_applies_when_places_agree() {
    let g = load_alpha_grammar();
    let r = place_agreement_rule(&g);
    // Analyze "bb": both labial voiced, so the following b agrees and unapply underspecifies the first b's voice.
    let out = pg_rules::rewrite::analyze(&g, &r, &seg(&g, "bb"), None);
    assert_eq!(out.len(), 1, "agreeing places: unapplication proceeds");
    assert_eq!(
        interior(&out[0])[0].1[1],
        0b11,
        "first b: voice underspecified on unapply"
    );
}

#[test]
fn analysis_agreement_rejects_when_places_disagree() {
    let g = load_alpha_grammar();
    let r = place_agreement_rule(&g);
    // Analyze "bg": the following g's place disagrees, so the candidate is rejected -- the FST alone would over-generate an analysis here.
    let out = pg_rules::rewrite::analyze(&g, &r, &seg(&g, "bg"), None);
    assert!(
        out.is_empty(),
        "disagreeing places: analysis candidate rejected"
    );
}

#[test]
fn analysis_preserves_a_variable_shared_by_input_and_output() {
    let g = load_alpha_grammar();
    let poa = feat(&g, "feat_poa");
    let r = rule(
        pat(PatternNode::Context(ctx_var(
            nat_class(&g, "nc_cons"),
            poa,
            0,
            true,
        ))),
        subrule(
            pat(PatternNode::Context(ctx_var(
                nat_class(&g, "nc_voiced"),
                poa,
                0,
                true,
            ))),
            None,
            Some(pat_char(char_def(&g, "char_b"))),
        ),
    );
    let out = pg_rules::rewrite::analyze(&g, &r, &seg(&g, "bb"), None);
    assert_eq!(out.len(), 1);
    assert_eq!(
        interior(&out[0])[0].1[POA],
        0b01,
        "shared place variable stays labial"
    );
    let opposite = rule(
        pat(PatternNode::Context(ctx_var(
            nat_class(&g, "nc_cons"),
            poa,
            0,
            true,
        ))),
        subrule(
            pat(PatternNode::Context(ctx_var(
                nat_class(&g, "nc_voiced"),
                poa,
                0,
                false,
            ))),
            None,
            Some(pat_char(char_def(&g, "char_b"))),
        ),
    );
    let out = pg_rules::rewrite::analyze(&g, &opposite, &seg(&g, "bb"), None);
    assert_eq!(out.len(), 1);
    assert_eq!(
        interior(&out[0])[0].1[POA],
        0b11,
        "opposite polarity is not cancelled"
    );
}

#[test]
fn analysis_tries_an_optional_agreeing_environment_before_rejecting() {
    let g = load_alpha_grammar();
    let r = place_agreement_rule(&g);
    let source = seg(&g, "bbg");
    let mut builder = ShapeBuilder::from_shape(&source);
    builder.delete(2);
    builder.insert(
        2,
        NodeKind::Segment,
        source.char_def(2),
        NodeFlags(NodeFlags::OPTIONAL),
        source.node_lanes(2),
    );
    let out = pg_rules::rewrite::analyze(&g, &r, &builder.freeze(), None);
    assert_eq!(
        out.len(),
        1,
        "the optional labial agrees even though the following velar does not"
    );
    assert_eq!(interior(&out[0])[0].1[1], 0b11);
}

// A left-environment alpha-variable node separated from the target by an unbounded quantifier, varying the filler count to catch an off-by-N positional-lookup regression.

/// Target binds alpha_a from its own place; the left environment's second alpha_a occurrence must agree with it.
fn quantifier_gap_rule(g: &Grammar) -> RewriteRuleDef {
    let poa = feat(g, "feat_poa");
    rule(
        pat(PatternNode::Context(ctx_var(
            nat_class(g, "nc_cons"),
            poa,
            0,
            true,
        ))),
        subrule(
            pat(PatternNode::Context(ctx(nat_class(g, "nc_voiced")))),
            Some(Pattern {
                nodes: vec![
                    PatternNode::Context(ctx_var(nat_class(g, "nc_cons"), poa, 0, true)),
                    PatternNode::Quantifier {
                        min: 0,
                        max: None,
                        children: vec![PatternNode::CharDef(char_def(g, "char_a"))],
                    },
                    PatternNode::CharDef(char_def(g, "char_b")),
                ],
            }),
            None,
        ),
    )
}

/// `{env_var}{"a" x fillers}b{target}`: env consonant, variable-width filler run, literal `b`, then the rule target.
fn quantifier_gap_word(env_var: char, fillers: usize, target: char) -> String {
    let mut w = String::new();
    w.push(env_var);
    for _ in 0..fillers {
        w.push('a');
    }
    w.push('b');
    w.push(target);
    w
}

#[test]
fn synthesis_left_env_var_across_unbounded_quantifier_rejects_disagreeing_place_at_any_gap_width() {
    let g = load_alpha_grammar();
    let r = quantifier_gap_rule(&g);
    // Velar env var vs labial target: disagree at every filler width, including the widths where the old positional lookup used to get it wrong.
    for fillers in [0usize, 1, 2, 3, 5] {
        let w = quantifier_gap_word('g', fillers, 'p');
        let out = pg_rules::rewrite::synthesize(&g, &r, &seg(&g, &w));
        assert!(
            out.is_empty(),
            "disagreeing places across a {fillers}-segment quantifier gap ({w:?}): must reject, got {} result(s)",
            out.len()
        );
    }
}

#[test]
fn synthesis_left_env_var_across_unbounded_quantifier_accepts_agreeing_place_at_any_gap_width() {
    let g = load_alpha_grammar();
    let r = quantifier_gap_rule(&g);
    // Matching env var and target: agree at every filler width, so the fix must not overcorrect into spurious rejection.
    for fillers in [0usize, 1, 2, 3, 5] {
        let w = quantifier_gap_word('p', fillers, 'p');
        let out = pg_rules::rewrite::synthesize(&g, &r, &seg(&g, &w));
        assert_eq!(
            out.len(),
            1,
            "agreeing places across a {fillers}-segment quantifier gap ({w:?}): must apply"
        );
        let target_lanes = interior(&out[0]).last().unwrap().1.clone();
        assert_eq!(target_lanes[1], 0b01, "target voiced (gap={fillers})");
    }
}

#[test]
fn analysis_left_env_var_across_unbounded_quantifier_rejects_disagreeing_place() {
    let g = load_alpha_grammar();
    let r = quantifier_gap_rule(&g);
    // The analysis target must already be voiced, so use 'b'; the disagreeing env var is still rejected on this analysis call site too.
    let w = quantifier_gap_word('g', 2, 'b');
    let out = pg_rules::rewrite::analyze(&g, &r, &seg(&g, &w), None);
    assert!(
        out.is_empty(),
        "disagreeing places across the quantifier gap ({w:?}): analysis must reject"
    );
}

#[test]
fn left_and_right_environments_share_bindings_across_optional_candidates() {
    let g = load_alpha_grammar();
    let poa = feat(&g, "feat_poa");
    let context = || PatternNode::Context(ctx_var(nat_class(&g, "nc_cons"), poa, 0, true));
    let r = rule(
        pat_char(char_def(&g, "char_a")),
        subrule(
            pat_char(char_def(&g, "char_b")),
            Some(pat(context())),
            Some(pat(context())),
        ),
    );
    let source = seg(&g, "babg");
    let mut builder = ShapeBuilder::from_shape(&source);
    builder.delete(3);
    builder.insert(
        3,
        NodeKind::Segment,
        source.char_def(3),
        NodeFlags(NodeFlags::OPTIONAL),
        source.node_lanes(3),
    );
    let out = pg_rules::rewrite::synthesize(&g, &r, &builder.freeze());
    assert_eq!(
        out.len(),
        1,
        "left labial binding finds the optional agreeing right environment"
    );
    assert!(
        pg_rules::rewrite::synthesize(&g, &r, &seg(&g, "bag")).is_empty(),
        "a mandatory disagreeing right environment must not match"
    );
}

#[test]
fn repeated_environment_variable_can_choose_an_agreeing_optional_segment() {
    let g = load_alpha_grammar();
    let poa = feat(&g, "feat_poa");
    let mut r = place_agreement_rule(&g);
    let context = || PatternNode::Context(ctx_var(nat_class(&g, "nc_cons"), poa, 0, true));
    r.subrules[0].right_env = Some(Pattern {
        nodes: vec![context(), context()],
    });
    let source = seg(&g, "bbbg");
    let mut builder = ShapeBuilder::from_shape(&source);
    builder.delete(2);
    builder.insert(
        2,
        NodeKind::Segment,
        source.char_def(2),
        NodeFlags(NodeFlags::OPTIONAL),
        source.node_lanes(2),
    );
    let out = pg_rules::rewrite::analyze(&g, &r, &builder.freeze(), None);
    assert_eq!(
        out.len(),
        1,
        "both repeated variables agree when the optional labial is consumed"
    );
    assert!(
        pg_rules::rewrite::analyze(&g, &r, &seg(&g, "bbg"), None).is_empty(),
        "a mandatory disagreement remains rejected"
    );
}

#[test]
fn disagreeing_optional_candidates_are_pruned_within_a_bounded_work_budget() {
    let g = load_alpha_grammar();
    let poa = feat(&g, "feat_poa");
    let mut r = place_agreement_rule(&g);
    r.subrules[0].right_env = Some(pat(PatternNode::Quantifier {
        min: 1,
        max: Some(13),
        children: vec![PatternNode::Context(ctx_var(
            nat_class(&g, "nc_cons"),
            poa,
            0,
            true,
        ))],
    }));
    let source = seg(&g, &format!("b{}b", "g".repeat(12)));
    let mut builder = ShapeBuilder::from_shape(&source);
    for index in 2..=13 {
        builder.delete(index);
        builder.insert(
            index,
            NodeKind::Segment,
            source.char_def(index),
            NodeFlags(NodeFlags::OPTIONAL),
            source.node_lanes(index),
        );
    }
    let budget = pg_fst::work::WorkBudget::new(10000);
    let _scope = budget.enter();
    let out = pg_rules::rewrite::analyze(&g, &r, &builder.freeze(), None);
    assert_eq!(out.len(), 1, "the agreeing final segment remains reachable");
    assert!(
        !budget.capped(),
        "disagreeing optional paths must be pruned during matching"
    );
}

#[test]
fn deletion_environments_share_alpha_bindings() {
    let g = load_alpha_grammar();
    let poa = feat(&g, "feat_poa");
    let context = || PatternNode::Context(ctx_var(nat_class(&g, "nc_cons"), poa, 0, true));
    let r = rule(
        pat(context()),
        subrule(
            Pattern { nodes: vec![] },
            Some(pat(context())),
            Some(pat(context())),
        ),
    );
    let out = pg_rules::rewrite::analyze(&g, &r, &seg(&g, "pb"), None);
    assert_eq!(
        out.len(),
        1,
        "agreeing environments allow an optional restored segment"
    );
    assert_eq!(
        interior(&out[0])[1].1[POA],
        0b01,
        "restored segment takes the environment binding"
    );
    assert!(
        pg_rules::rewrite::analyze(&g, &r, &seg(&g, "pg"), None).is_empty(),
        "disagreeing environments cannot restore a segment"
    );
}

#[test]
fn epenthesis_environments_share_alpha_bindings_and_apply_them() {
    let g = load_alpha_grammar();
    let poa = feat(&g, "feat_poa");
    let context = || PatternNode::Context(ctx_var(nat_class(&g, "nc_cons"), poa, 0, true));
    let inserted = PatternNode::Context(ctx_var(nat_class(&g, "nc_any"), poa, 0, true));
    let mut r = rule(
        Pattern { nodes: vec![] },
        subrule(pat(inserted), Some(pat(context())), Some(pat(context()))),
    );
    r.mode = RewriteMode::Simultaneous;
    let out = pg_rules::rewrite::synthesize(&g, &r, &seg(&g, "pb"));
    assert_eq!(out.len(), 1, "agreeing environments allow epenthesis");
    assert_eq!(
        interior(&out[0])[1].1[POA],
        0b01,
        "inserted segment takes the environment binding"
    );
    assert!(
        pg_rules::rewrite::synthesize(&g, &r, &seg(&g, "pg")).is_empty(),
        "disagreeing environments cannot trigger epenthesis"
    );
}

#[test]
fn analysis_epenthesis_checks_target_and_environment_alpha_bindings() {
    let g = load_alpha_grammar();
    let poa = feat(&g, "feat_poa");
    let context = || PatternNode::Context(ctx_var(nat_class(&g, "nc_cons"), poa, 0, true));
    let mut r = rule(
        Pattern { nodes: vec![] },
        subrule(pat(context()), Some(pat(context())), Some(pat(context()))),
    );
    r.mode = RewriteMode::Simultaneous;
    assert_eq!(
        pg_rules::rewrite::analyze(&g, &r, &seg(&g, "pbb"), None).len(),
        1,
        "agreeing target and environments unapply epenthesis"
    );
    assert!(
        pg_rules::rewrite::analyze(&g, &r, &seg(&g, "pbg"), None).is_empty(),
        "a disagreeing right environment must reject the target"
    );
}

#[test]
fn allomorph_environment_checks_repeated_variables() {
    use pg_grammar_model::model::EnvironmentDef;
    let g = load_alpha_grammar();
    let poa = feat(&g, "feat_poa");
    let context = || PatternNode::Context(ctx_var(nat_class(&g, "nc_cons"), poa, 0, true));
    let envs = vec![EnvironmentDef {
        require: true,
        left: None,
        right: Some(Pattern {
            nodes: vec![context(), context()],
        }),
        source: None,
    }];
    assert!(pg_rules::validity::environments_ok(
        &g,
        &envs,
        &seg(&g, "abb"),
        0,
        0
    ));
    assert!(
        !pg_rules::validity::environments_ok(&g, &envs, &seg(&g, "abg"), 0, 0),
        "repeated variables on one side must agree"
    );
}

#[test]
fn left_environment_prefers_the_furthest_successful_match() {
    let g = load_alpha_grammar();
    let poa = feat(&g, "feat_poa");
    let context = || PatternNode::Context(ctx_var(nat_class(&g, "nc_cons"), poa, 0, true));
    let r = rule(
        pat_char(char_def(&g, "char_a")),
        subrule(
            pat_char(char_def(&g, "char_b")),
            Some(pat(context())),
            Some(pat(context())),
        ),
    );
    let source = seg(&g, "bgab");
    let mut builder = ShapeBuilder::from_shape(&source);
    builder.delete(2);
    builder.insert(
        2,
        NodeKind::Segment,
        source.char_def(2),
        NodeFlags(NodeFlags::OPTIONAL),
        source.node_lanes(2),
    );
    let out = pg_rules::rewrite::synthesize(&g, &r, &builder.freeze());
    assert_eq!(
        out.len(),
        1,
        "the furthest left match binds labial and agrees with the right"
    );
}
