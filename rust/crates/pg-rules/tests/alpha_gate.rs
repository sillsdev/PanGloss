//! Alpha-variable agreement gate: the FST over-approximates variable-governed lanes, so pg-rules
//! runs the real agreement check (C# `SimpleFeatureValue.cs:52-77` variable arms +
//! `VariableBindings`) after a candidate span is found — binding on first occurrence, verifying on
//! later ones, and REJECTING candidates that violate a binding (the over-generation the bridge alone
//! would produce). Modeled on the C# `RewriteRuleTests.AlphaVariableRules` nasal place-assimilation
//! (`mbindiŋg`) and place/voicing agreement patterns.

mod common;

use common::*;
use pg_grammar::chardef::CharDefId;
use pg_grammar::model::{
    Dir, Grammar, Pattern, PatternNode, RewriteMode, RewriteRuleDef, RewriteSubruleDef,
};
use pg_shape::{NodeKind, Shape};

fn subrule(rhs: Pattern, left: Option<Pattern>, right: Option<Pattern>) -> RewriteSubruleDef {
    RewriteSubruleDef {
        required_pos: None,
        required_mpr: pg_grammar::model::MprSet::EMPTY,
        excluded_mpr: pg_grammar::model::MprSet::EMPTY,
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

// =================================================================================================
// Nasal place assimilation: n -> [poa = αa] / _ [C poa = αa]   (C# AlphaVariableRules, "mbindiŋg":
// RHS binds nothing, right env binds αa = following consonant's poa, RHS applies it.)
// =================================================================================================

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

// =================================================================================================
// Place-agreement voicing: C[poa = αa] -> [+voice] / _ C[poa = αa]   (target binds αa from its own
// place; the following consonant must AGREE in place, else the candidate is rejected).
// =================================================================================================

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
    // "pb": p (labial) precedes b (labial) -> places agree -> p voices ([+voice]).
    // voi lane (FlatIndex 1): p is 0b10 (-), becomes 0b01 (+).
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
    // "pk": p (labial) precedes k (velar) -> places DISAGREE. The FST alone would match (both are
    // consonants) and wrongly voice p; the agreement check binds a=labial from p, then finds k's
    // poa=velar does not overlap -> REJECT. No application.
    let out = pg_rules::rewrite::synthesize(&g, &r, &seg(&g, "pk"));
    assert!(
        out.is_empty(),
        "disagreeing places: candidate rejected, rule does not apply"
    );
}

// =================================================================================================
// Analysis-side agreement (nondeterministic, underspecified). Un-applying the place-agreement
// voicing rule: analysis target = LHS(C, poa=αa) priority-union RHS([+voice]) = [C, +voice, poa=αa];
// it binds αa from the matched voiced consonant's place and rejects if the following consonant's
// place disagrees.
// =================================================================================================

#[test]
fn analysis_agreement_applies_when_places_agree() {
    let g = load_alpha_grammar();
    let r = place_agreement_rule(&g);
    // Analyze "bb": both labial voiced -> the first b matches (voiced C), binds a=labial, the
    // following b agrees -> unapply makes the first b's voice underspecified (full mask 0b11).
    let out = pg_rules::rewrite::analyze(&g, &r, &seg(&g, "bb"));
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
    // Analyze "bg": b (labial) then g (velar) -> the first b binds a=labial, the following g's
    // place (velar) disagrees -> candidate rejected; g itself has no following consonant. No
    // unapplication (the FST alone would over-generate an analysis here).
    let out = pg_rules::rewrite::analyze(&g, &r, &seg(&g, "bg"));
    assert!(
        out.is_empty(),
        "disagreeing places: analysis candidate rejected"
    );
}

// =================================================================================================
// Tier-2 #12: a LEFT-environment α-variable node separated from the target by an unbounded
// quantifier — Indonesian `prule3`'s exact shape: `LeftEnvironment = [nc10(α), (nc6)*, char17]`
// (`indonesian-hc.xml` prule3; the real `nc3`/`char29` filler nodes ahead of the var are omitted
// here as immaterial to the bug). Before the Tier-2 #12 fix, `resolve_bindings` located the env's
// α-bearing node by `s - env.node_vars.len() + k` — a positional guess that is only correct when
// the quantifier happens to consume exactly one segment. This grammar's `nc_any`-typed filler
// ('a', poa unconstrained) makes the old off-by-N lookup land on a segment whose place is a full
// bitmask, which trivially "agrees" with anything -- so the old code silently accepted candidates
// it should have rejected whenever the quantifier consumed a segment count other than 1.
// =================================================================================================

/// Target: any consonant, binding αa from its own place (matches Indonesian prule3's LHS `nc8(α)`
/// binding first, C# `RewriteRuleSpec.MatchSubrule` target-then-environments order). RHS: `[+voice]`
/// only (an observable side effect uninvolved in the α-mechanism, mirroring `place_agreement_rule`).
/// Left environment: `[Context(nc_cons, αa), Quantifier{0,None}(CharDef('a')), CharDef('b')]` — the
/// env's own occurrence of αa (a SECOND occurrence of the same variable) must AGREE with the
/// target's bound place, exactly like prule3's `nc10(α)` checking against `nc8(α)`'s binding.
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

/// `{env_var}{"a" x fillers}b{target}` — the env's var-bearing consonant, a variable-width run of
/// filler vowels (the quantifier body), the fixed anchor-adjacent literal `b`, then the rule target.
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
    // env var 'g' (velar) vs target 'p' (labial): disagree, at every filler width. Width 1 is the
    // width the old positional code implicitly assumed (`env.node_vars.len() == 3` total nodes,
    // coincidentally matching a 3-segment left context) and happened to get right; 0, 2, 3, 5 are
    // exactly the widths that exposed the bug (confirmed by reverting the fix — see the module
    // report).
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
    // env var 'p' (labial) == target 'p' (labial): agree at every filler width — the fix must not
    // overcorrect into spuriously rejecting a genuinely agreeing candidate.
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
    // Analysis target is LHS ⊕ RHS = [cons+, voi+, poa=any], so the target segment must already be
    // voiced; use 'b' (voiced labial) so the ana target matches, then check the same disagreeing
    // env var ('g', velar) across a 2-segment quantifier gap is still rejected on the analysis side
    // (the `ana_feature` call site uses the identical `resolve_bindings` fix).
    let w = quantifier_gap_word('g', 2, 'b');
    let out = pg_rules::rewrite::analyze(&g, &r, &seg(&g, &w));
    assert!(
        out.is_empty(),
        "disagreeing places across the quantifier gap ({w:?}): analysis must reject"
    );
}
