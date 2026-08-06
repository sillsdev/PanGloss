//! Rewrite apply/unapply on hand-built rules against hand-reasoned expected shapes, cross-checked against the C# `RewriteRuleTests.cs` method each test cites.

mod common;

use common::*;
use pg_featstruct::FeatureStruct;
use pg_grammar::chardef::CharDefId;
use pg_grammar::model::AnchorSide;
use pg_grammar::model::{
    Dir, NatClassId, Pattern, PatternNode, RewriteMode, RewriteRuleDef, RewriteSubruleDef,
};
use pg_grammar::model::{MprId, MprSet, PRuleId};
use pg_rules::trace::{FailureReason, TraceSink, TraceType, TreeTraceSink};
use pg_shape::{NodeKind, Shape};

// ---- rule builders -----------------------------------------------------------------------------

fn subrule(rhs: Pattern, left: Option<Pattern>, right: Option<Pattern>) -> RewriteSubruleDef {
    RewriteSubruleDef {
        required_pos: None,
        required_mpr: pg_grammar::model::MprSet::EMPTY,
        excluded_mpr: pg_grammar::model::MprSet::EMPTY,
        rhs,
        left_env: left,
        right_env: right,
        self_opaquing: false, // every rule this file builds is Iterative-mode (`rule()`'s default)
    }
}

fn rule(lhs: Pattern, sr: RewriteSubruleDef) -> RewriteRuleDef {
    RewriteRuleDef {
        xml_id: "test".into(),
        name: None,
        mode: RewriteMode::Iterative,
        dir: Dir::LeftToRight,
        vars: Default::default(),
        lhs,
        subrules: vec![sr],
    }
}

/// `rule`'s direction-parameterized sibling, used only by the LtR/RtL pick-order tests below.
fn rule_dir(lhs: Pattern, sr: RewriteSubruleDef, dir: Dir) -> RewriteRuleDef {
    RewriteRuleDef {
        xml_id: "test-dir".into(),
        name: None,
        mode: RewriteMode::Iterative,
        dir,
        vars: Default::default(),
        lhs,
        subrules: vec![sr],
    }
}

/// `rule`'s multi-subrule, mode-parameterized sibling, used only by the Simultaneous multi-subrule disjunction test below.
fn rule_multi(lhs: Pattern, subrules: Vec<RewriteSubruleDef>, mode: RewriteMode) -> RewriteRuleDef {
    RewriteRuleDef {
        xml_id: "test-multi".into(),
        name: None,
        mode,
        dir: Dir::LeftToRight,
        vars: Default::default(),
        lhs,
        subrules,
    }
}

fn pat_ctx(nc: NatClassId) -> Pattern {
    Pattern {
        nodes: vec![PatternNode::Context(ctx(nc))],
    }
}
fn pat_char(cd: CharDefId) -> Pattern {
    Pattern {
        nodes: vec![PatternNode::CharDef(cd)],
    }
}

/// The interior of a shape as `(NodeKind, char_def, lanes, optional)`.
fn interior(s: &Shape) -> Vec<(NodeKind, u32, Vec<u64>, bool)> {
    (0..s.len())
        .filter(|&i| !matches!(s.kind(i), NodeKind::LeftAnchor | NodeKind::RightAnchor))
        .map(|i| {
            (
                s.kind(i),
                s.char_def(i),
                s.node_lanes(i).to_vec(),
                s.flags(i).is_optional(),
            )
        })
        .collect()
}

fn seg(g: &pg_grammar::model::Grammar, word: &str) -> Shape {
    pg_rules::shape_feat::segment_with_features(g, table(g), word).unwrap()
}

// Lane constants for the probe grammar ([cons, voi, Type]); every concrete segment here carries Type=Segment (0b01), never a boundary node's lanes.
const A: [u64; 3] = [0b10, 0b01, 0b01]; // vowel, voiced
const T: [u64; 3] = [0b01, 0b10, 0b01]; // consonant, voiceless
const D: [u64; 3] = [0b01, 0b01, 0b01]; // consonant, voiced

// Feature-change: t -> [+voice] / V _ V (C# FeatureSynthesisRewriteSubruleSpec.ApplyRhs / FeatureAnalysisRewriteRuleSpec.Unapply)

fn voicing_rule(g: &pg_grammar::model::Grammar) -> RewriteRuleDef {
    rule(
        pat_char(char_def(g, "char_t")),
        subrule(
            pat_ctx(nat_class(g, "nc_voi")),
            Some(pat_ctx(nat_class(g, "nc_vowel"))),
            Some(pat_ctx(nat_class(g, "nc_vowel"))),
        ),
    )
}

#[test]
fn feature_change_synthesis_voices_t_between_vowels() {
    let g = load_probe_grammar();
    let r = voicing_rule(&g);
    // "ata": the medial t is voiced to d's lanes; char_def resets to NO_CHAR_DEF since a feature-changed node's literal identity is no longer authoritative, matching C#'s GetMatchingStrReps re-deriving from FeatureStruct rather than trusting a stale identity.
    let out = pg_rules::rewrite::synthesize(&g, &r, &seg(&g, "ata"));
    assert_eq!(out.len(), 1, "rule applied");
    let got = interior(&out[0]);
    assert_eq!(got.len(), 3);
    assert_eq!(got[0].2, A.to_vec(), "left a unchanged");
    assert_eq!(got[2].2, A.to_vec(), "right a unchanged");
    assert_eq!(got[1].2, D.to_vec(), "medial t -> [+voi] == d lanes");
    assert_eq!(
        got[1].1,
        pg_shape::NO_CHAR_DEF,
        "char_def reset: identity now feature-driven, not stale char_t"
    );
}

#[test]
fn feature_change_synthesis_iterates_over_all_targets() {
    let g = load_probe_grammar();
    let r = voicing_rule(&g);
    // "atata": both medial t's are between vowels -> both voiced (iterative application).
    let out = pg_rules::rewrite::synthesize(&g, &r, &seg(&g, "atata"));
    let got = interior(&out[0]);
    let lanes: Vec<Vec<u64>> = got.iter().map(|x| x.2.clone()).collect();
    assert_eq!(
        lanes,
        vec![A.to_vec(), D.to_vec(), A.to_vec(), D.to_vec(), A.to_vec()]
    );
}

#[test]
fn feature_change_synthesis_needs_both_environments() {
    let g = load_probe_grammar();
    let r = voicing_rule(&g);
    // "at": t has a left vowel but no right vowel -> right env fails -> rule does not apply.
    let out = pg_rules::rewrite::synthesize(&g, &r, &seg(&g, "at"));
    assert!(out.is_empty(), "no right-hand vowel: rule must not fire");
}

#[test]
fn feature_change_analysis_underspecifies_voice() {
    let g = load_probe_grammar();
    let r = voicing_rule(&g);
    // Analyze "ada": Unapply underspecifies the changed voice feature to the full mask (0b11) so lexical lookup can match either t or d.
    let out = pg_rules::rewrite::analyze(&g, &r, &seg(&g, "ada"));
    assert_eq!(out.len(), 1, "unapplied");
    let got = interior(&out[0]);
    assert_eq!(
        got[1].2,
        vec![0b01, 0b11, 0b01],
        "d -> [cons+, voi underspecified, Type=Segment]"
    );
}

#[test]
fn feature_change_round_trip_recovers_superset() {
    let g = load_probe_grammar();
    let r = voicing_rule(&g);
    let orig = seg(&g, "ata");
    let synth = pg_rules::rewrite::synthesize(&g, &r, &orig).pop().unwrap();
    let ana = pg_rules::rewrite::analyze(&g, &r, &synth).pop().unwrap();
    // The analyzed medial node must unify with the original t (superset containment).
    let ana_mid = &interior(&ana)[1].2;
    let orig_mid = &interior(&orig)[1].2;
    assert!(
        pg_featstruct::flat_unifiable(ana_mid, orig_mid),
        "analysis {ana_mid:?} must be a superset of original {orig_mid:?}"
    );
}

// Analysis feature-reversal uses C#'s AntiFeatureStruct negation (L ∪ R via mask & !bits), not a blanket full-unconstrain; needs a >=3-symbol feature since a 2-symbol feature's negation always degenerates to full_mask. C# analog: RewriteRuleTests.CommonFeatureRules.

fn place_rule(g: &pg_grammar::model::Grammar) -> RewriteRuleDef {
    // p -> [vel] (place feature), no environment (fires unconditionally).
    rule(
        pat_char(char_def(g, "char_p")),
        subrule(pat_ctx(nat_class(g, "nc_vel")), None, None),
    )
}

#[test]
fn feature_change_analysis_reversal_excludes_the_third_symbol() {
    let g = common::load_anti_fs_grammar();
    let r = place_rule(&g);
    // Analyze "k": reversal sets place to {lab, vel} (L ∪ R), never the full-unconstrained {lab, cor, vel} — "cor" was never a possible value on either side and must stay excluded.
    let out = pg_rules::rewrite::analyze(&g, &r, &seg(&g, "k"));
    assert_eq!(out.len(), 1, "unapplied");
    let place = feat(&g, "feat_place").0 as usize;
    let lab = 0b001u64;
    let cor = 0b010u64;
    let vel = 0b100u64;
    let got = interior(&out[0])[0].2[place];
    assert_eq!(
        got,
        lab | vel,
        "place must be {{lab, vel}} (L ∪ R), excluding the untouched 'cor' symbol"
    );
    assert_ne!(
        got,
        lab | cor | vel,
        "must NOT be the old full-unconstrain bug"
    );
}

// Deletion: t -> 0 / a _ a (C# NarrowSynthesisRewriteSubruleSpec.ApplyRhs / NarrowAnalysisRewriteRuleSpec.Unapply, reapply=Deletion)

fn deletion_rule(g: &pg_grammar::model::Grammar) -> RewriteRuleDef {
    rule(
        pat_char(char_def(g, "char_t")),
        subrule(
            Pattern::default(), // empty RHS => deletion
            Some(pat_ctx(nat_class(g, "nc_vowel"))),
            Some(pat_ctx(nat_class(g, "nc_vowel"))),
        ),
    )
}

#[test]
fn deletion_synthesis_removes_t_between_vowels() {
    let g = load_probe_grammar();
    let r = deletion_rule(&g);
    // "ata" -> "aa": the medial t (RHS empty) is deleted.
    let out = pg_rules::rewrite::synthesize(&g, &r, &seg(&g, "ata"));
    let got = interior(&out[0]);
    assert_eq!(got.len(), 2, "t deleted");
    assert_eq!(
        got.iter().map(|x| x.2.clone()).collect::<Vec<_>>(),
        vec![A.to_vec(), A.to_vec()]
    );
}

#[test]
fn deletion_analysis_reinserts_optional_t() {
    let g = load_probe_grammar();
    let r = deletion_rule(&g);
    // Analyze "aa": NarrowAnalysis re-inserts the deleted t as OPTIONAL, so lexical lookup can recover both "aa" and "ata".
    let out = pg_rules::rewrite::analyze(&g, &r, &seg(&g, "aa"));
    let got = interior(&out[0]);
    assert_eq!(got.len(), 3, "optional t re-inserted");
    assert_eq!(got[1].2, T.to_vec(), "re-inserted segment is t");
    assert!(got[1].3, "re-inserted deletion segment is OPTIONAL");
    assert!(!got[0].3 && !got[2].3, "the vowels stay non-optional");
}

#[test]
fn deletion_round_trip_recovers_original() {
    let g = load_probe_grammar();
    let r = deletion_rule(&g);
    let synth = pg_rules::rewrite::synthesize(&g, &r, &seg(&g, "ata"))
        .pop()
        .unwrap(); // "aa"
    let ana = pg_rules::rewrite::analyze(&g, &r, &synth).pop().unwrap(); // "a(t)a"
                                                                         // Taking the optional t recovers the original interior a t a.
    let got = interior(&ana);
    assert_eq!(got.len(), 3);
    assert_eq!(got[1].2, T.to_vec());
    assert!(got[1].3, "optional");
}

// Word-initial deletion: t -> 0 / # _ a — the word-initial gap must be a matchable analysis-unapply site, not just "after each segment".

fn word_initial_deletion_rule(g: &pg_grammar::model::Grammar) -> RewriteRuleDef {
    rule(
        pat_char(char_def(g, "char_t")),
        subrule(
            Pattern::default(), // empty RHS => deletion
            Some(Pattern {
                nodes: vec![PatternNode::Anchor(AnchorSide::Left)],
            }),
            Some(pat_ctx(nat_class(g, "nc_vowel"))),
        ),
    )
}

#[test]
fn word_initial_deletion_synthesis_removes_leading_t() {
    let g = load_probe_grammar();
    let r = word_initial_deletion_rule(&g);
    // "ta" -> "a": the word-initial t (before the vowel a) is deleted.
    let out = pg_rules::rewrite::synthesize(&g, &r, &seg(&g, "ta"));
    assert_eq!(out.len(), 1, "rule applied");
    let got = interior(&out[0]);
    assert_eq!(got.len(), 1, "t deleted");
    assert_eq!(got[0].2, A.to_vec());
}

#[test]
fn word_initial_deletion_analysis_reinserts_optional_t_at_word_start() {
    let g = load_probe_grammar();
    let r = word_initial_deletion_rule(&g);
    // Analyze "a": ana_narrow treats the shape's own left-anchor node as a legitimate deletion-unapply site, matching C#'s RewriteRuleSpec.MatchSubrule _isTargetEmpty branch.
    let out = pg_rules::rewrite::analyze(&g, &r, &seg(&g, "a"));
    assert_eq!(out.len(), 1, "unapplied");
    let got = interior(&out[0]);
    assert_eq!(got.len(), 2, "optional t re-inserted before the vowel");
    assert_eq!(got[0].2, T.to_vec(), "re-inserted segment is t");
    assert!(got[0].3, "re-inserted deletion segment is OPTIONAL");
    assert_eq!(got[1].2, A.to_vec());
    assert!(!got[1].3, "the vowel stays non-optional");
}

#[test]
fn word_initial_deletion_round_trip_recovers_original() {
    let g = load_probe_grammar();
    let r = word_initial_deletion_rule(&g);
    let synth = pg_rules::rewrite::synthesize(&g, &r, &seg(&g, "ta"))
        .pop()
        .unwrap(); // "a"
    let ana = pg_rules::rewrite::analyze(&g, &r, &synth).pop().unwrap(); // "(t)a"
    let got = interior(&ana);
    assert_eq!(got.len(), 2);
    assert_eq!(got[0].2, T.to_vec());
    assert!(got[0].3, "optional");
}

// Narrowing (RHS non-empty, LHS/RHS node counts differ): tt -> n / a _ a (C# NarrowSynthesisRewriteSubruleSpec.ApplyRhs); the inserted RHS must be non-optional, only dirty, so it can never be treated as skippable downstream.

fn narrowing_rule(g: &pg_grammar::model::Grammar) -> RewriteRuleDef {
    rule(
        Pattern {
            nodes: vec![
                PatternNode::CharDef(char_def(g, "char_t")),
                PatternNode::CharDef(char_def(g, "char_t")),
            ],
        },
        subrule(
            pat_char(char_def(g, "char_n")),
            Some(pat_ctx(nat_class(g, "nc_vowel"))),
            Some(pat_ctx(nat_class(g, "nc_vowel"))),
        ),
    )
}

#[test]
fn narrow_synthesis_replacement_segment_is_not_optional() {
    let g = load_probe_grammar();
    let r = narrowing_rule(&g);
    // "atta" -> "ana": genuine narrowing (2 LHS nodes to 1 RHS node) via syn_narrow's RHS-insert path; the coalesced "n" must not be OPTIONAL, matching C#'s AddAfter/SetDirty split which never conflates the two flags.
    let out = pg_rules::rewrite::synthesize(&g, &r, &seg(&g, "atta"));
    assert_eq!(out.len(), 1, "rule applied");
    let got = interior(&out[0]);
    assert_eq!(got.len(), 3, "tt coalesced to a single n");
    assert_eq!(got[0].2, A.to_vec(), "left a unchanged");
    assert_eq!(got[2].2, A.to_vec(), "right a unchanged");
    assert_eq!(got[1].1, char_def(&g, "char_n").0, "coalesced segment is n");
    assert_eq!(got[1].2, D.to_vec(), "n shares d's [cons+, voi+] lanes");
    assert!(
        !got[1].3,
        "the narrowed RHS segment must NOT be optional (R1)"
    );
}

// Narrowing RHS alpha-variable resolution: syn_narrow's RHS build resolves an alpha variable bound from a merged LHS segment instead of leaving it fully unconstrained. C# analog: RewriteRuleTests.AlphaVariableRules x MergeRules.

fn merge_with_alpha_voice_rule(g: &pg_grammar::model::Grammar) -> RewriteRuleDef {
    let voi = feat(g, "feat_voi");
    // [C, var1=voice] [C] -> [C, var1=voice]: two consonants merge to one whose voice comes from the first LHS node's captured value via alpha var 1.
    rule(
        Pattern {
            nodes: vec![
                PatternNode::Context(ctx_var(nat_class(g, "nc_cons"), voi, 1, true)),
                PatternNode::Context(ctx(nat_class(g, "nc_cons"))),
            ],
        },
        subrule(
            Pattern {
                nodes: vec![PatternNode::Context(ctx_var(
                    nat_class(g, "nc_cons"),
                    voi,
                    1,
                    true,
                ))],
            },
            None,
            None,
        ),
    )
}

#[test]
fn narrow_synthesis_resolves_rhs_alpha_variable_from_lhs() {
    let g = load_probe_grammar();
    let r = merge_with_alpha_voice_rule(&g);
    // "td" -> coalesces to one consonant whose voice comes from the captured first LHS node via alpha var 1, not the unresolved full-mask default nc_cons alone would leave it at.
    let out = pg_rules::rewrite::synthesize(&g, &r, &seg(&g, "td"));
    assert_eq!(out.len(), 1, "rule applied");
    let got = interior(&out[0]);
    assert_eq!(got.len(), 1, "coalesced to a single consonant");
    assert_eq!(
        got[0].2,
        T.to_vec(),
        "voice resolved from the captured LHS var ('t'), not left unconstrained"
    );
}

// Epenthesis: 0 -> t / a _ a (C# EpenthesisSynthesisRewriteSubruleSpec.ApplyRhs / EpenthesisAnalysisRewriteRuleSpec.Unapply)

fn epenthesis_rule(g: &pg_grammar::model::Grammar) -> RewriteRuleDef {
    rule(
        Pattern::default(), // empty LHS => epenthesis
        subrule(
            pat_char(char_def(g, "char_t")),
            Some(pat_ctx(nat_class(g, "nc_vowel"))),
            Some(pat_ctx(nat_class(g, "nc_vowel"))),
        ),
    )
}

#[test]
fn epenthesis_synthesis_inserts_t_between_vowels() {
    let g = load_probe_grammar();
    let r = epenthesis_rule(&g);
    // "aa" -> "ata": a t is inserted in the vowel-vowel gap.
    let out = pg_rules::rewrite::synthesize(&g, &r, &seg(&g, "aa"));
    let got = interior(&out[0]);
    assert_eq!(got.len(), 3, "one segment epenthesized");
    assert_eq!(
        got.iter().map(|x| x.2.clone()).collect::<Vec<_>>(),
        vec![A.to_vec(), T.to_vec(), A.to_vec()]
    );
    assert!(!got[1].3, "synthesized epenthetic segment is not optional");
}

#[test]
fn epenthesis_synthesis_word_initial_site() {
    let g = load_probe_grammar();
    // 0 -> t / # _ a: the word-initial gap is an ordinary application site, since an empty-LHS pattern matches the shape's own left-anchor annotation, matching C#'s RewriteRuleSpec.MatchSubrule _isTargetEmpty branch.
    let r = rule(
        Pattern::default(),
        subrule(
            pat_char(char_def(&g, "char_t")),
            Some(Pattern {
                nodes: vec![PatternNode::Anchor(AnchorSide::Left)],
            }),
            Some(pat_ctx(nat_class(&g, "nc_vowel"))),
        ),
    );
    // "aa" -> "taa": fires only at the word-initial gap; elsewhere the left/right environments can't both hold, so there's no medial double-firing.
    let out = pg_rules::rewrite::synthesize(&g, &r, &seg(&g, "aa"));
    assert_eq!(out.len(), 1, "rule applied");
    let got = interior(&out[0]);
    assert_eq!(
        got.iter().map(|x| x.2.clone()).collect::<Vec<_>>(),
        vec![T.to_vec(), A.to_vec(), A.to_vec()],
        "t inserted word-initially, exactly once"
    );
}

#[test]
fn epenthesis_analysis_marks_epenthetic_segment_optional() {
    let g = load_probe_grammar();
    let r = epenthesis_rule(&g);
    // Analyze "ata": the medial t is marked OPTIONAL (EpenthesisAnalysis.Unapply) rather than deleted.
    let out = pg_rules::rewrite::analyze(&g, &r, &seg(&g, "ata"));
    let got = interior(&out[0]);
    assert_eq!(got.len(), 3);
    assert!(got[1].3, "epenthetic t marked OPTIONAL on unapply");
    assert_eq!(got[1].2, T.to_vec());
}

#[test]
fn epenthesis_analysis_multi_node_target_matches_document_order() {
    let g = load_probe_grammar();
    // 0 -> t d: the analysis matcher runs reversed, but compile_lane_fst reorders so an RtL matcher still matches the pattern's document-order substring ("td"), not its reversal.
    let r = rule(
        Pattern::default(),
        subrule(
            Pattern {
                nodes: vec![
                    PatternNode::CharDef(char_def(&g, "char_t")),
                    PatternNode::CharDef(char_def(&g, "char_d")),
                ],
            },
            None,
            None,
        ),
    );
    // "tda": the document-order substring t,d at (0,2) is marked optional; the final a is not.
    let out = pg_rules::rewrite::analyze(&g, &r, &seg(&g, "tda"));
    assert_eq!(out.len(), 1, "unapply fired on the document-order match");
    let got = interior(&out[0]);
    assert_eq!(
        got.iter().map(|x| x.3).collect::<Vec<_>>(),
        vec![true, true, false],
        "t,d marked optional (document order), a untouched"
    );
    // "dta" contains only the REVERSED sequence d,t — C# would not match it, and neither may we.
    let out = pg_rules::rewrite::analyze(&g, &r, &seg(&g, "dta"));
    assert!(
        out.is_empty(),
        "reversed physical sequence must NOT match the analysis target"
    );
}

#[test]
fn epenthesis_round_trip_recovers_superset() {
    let g = load_probe_grammar();
    let r = epenthesis_rule(&g);
    let synth = pg_rules::rewrite::synthesize(&g, &r, &seg(&g, "aa"))
        .pop()
        .unwrap(); // "ata"
    let ana = pg_rules::rewrite::analyze(&g, &r, &synth).pop().unwrap(); // "a(t)a"
                                                                         // Skipping the optional t recovers the original "aa".
    let got = interior(&ana);
    assert!(got[1].3, "optional t: skipping it recovers the original aa");
}

// Width-mismatch guard: a multi-node LHS abutting a BoundaryMarker can let a nondeterministic FST match transparently skip an Optional segment and report an ENTIRE_MATCH span wider than the compiled pattern; no existing C# test exercises this combination.

/// Re-flags the interior node at interior_idx as OPTIONAL (delete+reinsert, mirroring MutShape::to_shape) — builds the fixture where an Optional real segment, not just a boundary, can widen an ENTIRE_MATCH span.
fn mark_optional(shape: &Shape, interior_idx: usize) -> Shape {
    let idx = interior_idx + 1; // +1 for the left anchor
    let char_def = shape.char_def(idx);
    let lanes = shape.node_lanes(idx).to_vec();
    let mut m = pg_shape::ShapeBuilder::from_shape(shape);
    m.delete(idx);
    m.insert(
        idx,
        NodeKind::Segment,
        char_def,
        pg_shape::NodeFlags(pg_shape::NodeFlags::OPTIONAL),
        &lanes,
    );
    m.freeze()
}

fn double_t_feature_change_rule(g: &pg_grammar::model::Grammar) -> RewriteRuleDef {
    // tt -> [+voice][+voice]: a 2-node LHS/RHS feature-change rule with no environment.
    rule(
        Pattern {
            nodes: vec![
                PatternNode::CharDef(char_def(g, "char_t")),
                PatternNode::CharDef(char_def(g, "char_t")),
            ],
        },
        subrule(
            Pattern {
                nodes: vec![
                    PatternNode::Context(ctx(nat_class(g, "nc_voi"))),
                    PatternNode::Context(ctx(nat_class(g, "nc_voi"))),
                ],
            },
            None,
            None,
        ),
    )
}

#[test]
fn feature_change_synthesis_rejects_an_over_wide_optional_skip_span() {
    let g = load_probe_grammar();
    let r = double_t_feature_change_rule(&g);
    // "tat" with the medial 'a' re-flagged OPTIONAL: pg_fst's Optional-skip mechanism (Transduce::advance) can skip a real Segment-kind Optional node exactly as it would a boundary, so the only span all_spans reports is the over-wide [0,3), with no width-correct "tt" span at all.
    let base = seg(&g, "tat");
    let input = mark_optional(&base, 1);
    let out = pg_rules::rewrite::synthesize(&g, &r, &input);
    // No width-correct "tt" span exists once 'a' is excluded, so the guard rejects the only (over-wide) candidate; the regression check is that this returns instead of panicking.
    assert!(
        out.is_empty(),
        "no width-correct match exists; the over-wide span must be rejected, not applied"
    );
}

fn double_t_narrow_rule(g: &pg_grammar::model::Grammar) -> RewriteRuleDef {
    // tt -> n (narrowing: 2 LHS nodes coalesce to 1 RHS node), no environment.
    rule(
        Pattern {
            nodes: vec![
                PatternNode::CharDef(char_def(g, "char_t")),
                PatternNode::CharDef(char_def(g, "char_t")),
            ],
        },
        subrule(pat_char(char_def(g, "char_n")), None, None),
    )
}

#[test]
fn narrow_synthesis_rejects_an_over_wide_optional_skip_span() {
    let g = load_probe_grammar();
    let r = double_t_narrow_rule(&g);
    // Same fixture as the syn_feature sibling above; without the width guard, syn_narrow's delete-and-splice would swallow the over-wide [0,3) span's extra node ('a') along with the two real targets.
    let base = seg(&g, "tat");
    let input = mark_optional(&base, 1);
    let out = pg_rules::rewrite::synthesize(&g, &r, &input);
    assert!(
        out.is_empty(),
        "no width-correct match exists; the over-wide span must be rejected, not applied"
    );
}

// Direction-aware Iterative pick order: matching C#'s IterativePhonologicalPatternRule.Apply, a LeftToRight rule must find its leftmost remaining candidate first and a RightToLeft rule its rightmost, never both directions picking the same leftmost match.

fn double_t_narrow_rule_dir(g: &pg_grammar::model::Grammar, dir: Dir) -> RewriteRuleDef {
    rule_dir(
        Pattern {
            nodes: vec![
                PatternNode::CharDef(char_def(g, "char_t")),
                PatternNode::CharDef(char_def(g, "char_t")),
            ],
        },
        subrule(pat_char(char_def(g, "char_n")), None, None),
        dir,
    )
}

#[test]
fn narrow_synthesis_pick_order_witness_leftmost_then_rightmost() {
    // tt -> n on "ttt": the raw FST reports two overlapping candidates sharing the middle node, (t0,t1) and (t1,t2), so which one an Iterative loop merges first determines which single t survives unmerged.
    let g = load_probe_grammar();

    let ltr = pg_rules::rewrite::synthesize(
        &g,
        &double_t_narrow_rule_dir(&g, Dir::LeftToRight),
        &seg(&g, "ttt"),
    );
    assert_eq!(ltr.len(), 1, "LtR: rule applied");
    let got = interior(&ltr[0]);
    assert_eq!(got.len(), 2, "one pair merged, one t left over");
    assert_eq!(
        (got[0].1, got[1].1),
        (char_def(&g, "char_n").0, char_def(&g, "char_t").0),
        "LtR merges the LEFTMOST pair (t0,t1) -> n t"
    );

    let rtl = pg_rules::rewrite::synthesize(
        &g,
        &double_t_narrow_rule_dir(&g, Dir::RightToLeft),
        &seg(&g, "ttt"),
    );
    assert_eq!(rtl.len(), 1, "RtL: rule applied");
    let got = interior(&rtl[0]);
    assert_eq!(got.len(), 2, "one pair merged, one t left over");
    assert_eq!(
        (got[0].1, got[1].1),
        (char_def(&g, "char_t").0, char_def(&g, "char_n").0),
        "RtL merges the RIGHTMOST pair (t1,t2) -> t n -- the MIRROR IMAGE of LtR, not the same \
         result under both directions (the pre-fix bug)"
    );
}

#[test]
fn narrow_synthesis_pick_order_with_environment_changes_final_result() {
    // t t -> n / _ t on "tttt": the environment eliminates the (t2,t3) window in either direction, leaving two overlapping env-satisfying candidates, (t0,t1) and (t1,t2), for direction to arbitrate.
    let g = load_probe_grammar();
    let env_rule = |dir: Dir| {
        rule_dir(
            Pattern {
                nodes: vec![
                    PatternNode::CharDef(char_def(&g, "char_t")),
                    PatternNode::CharDef(char_def(&g, "char_t")),
                ],
            },
            subrule(
                pat_char(char_def(&g, "char_n")),
                None,
                Some(pat_char(char_def(&g, "char_t"))),
            ),
            dir,
        )
    };
    let cds = |shape: &Shape| -> Vec<u32> { interior(shape).iter().map(|n| n.1).collect() };

    let ltr = pg_rules::rewrite::synthesize(&g, &env_rule(Dir::LeftToRight), &seg(&g, "tttt"));
    assert_eq!(ltr.len(), 1, "LtR: rule applied");
    assert_eq!(
        cds(&ltr[0]),
        vec![
            char_def(&g, "char_n").0,
            char_def(&g, "char_t").0,
            char_def(&g, "char_t").0,
        ],
        "LtR merges the LEFTMOST env-satisfying pair (t0,t1) -> n t t"
    );

    let rtl = pg_rules::rewrite::synthesize(&g, &env_rule(Dir::RightToLeft), &seg(&g, "tttt"));
    assert_eq!(rtl.len(), 1, "RtL: rule applied");
    assert_eq!(
        cds(&rtl[0]),
        vec![
            char_def(&g, "char_t").0,
            char_def(&g, "char_n").0,
            char_def(&g, "char_t").0,
        ],
        "RtL merges the RIGHTMOST env-satisfying pair (t1,t2) -> t n t -- a DIFFERENT final surface \
         from LtR, driven jointly by direction and a real (non-vacuous) environment"
    );
}

fn double_t_feature_change_rule_dir(g: &pg_grammar::model::Grammar, dir: Dir) -> RewriteRuleDef {
    rule_dir(
        Pattern {
            nodes: vec![
                PatternNode::CharDef(char_def(g, "char_t")),
                PatternNode::CharDef(char_def(g, "char_t")),
            ],
        },
        subrule(
            Pattern {
                nodes: vec![
                    PatternNode::Context(ctx(nat_class(g, "nc_voi"))),
                    PatternNode::Context(ctx(nat_class(g, "nc_voi"))),
                ],
            },
            None,
            None,
        ),
        dir,
    )
}

#[test]
fn feature_change_analysis_pick_order_depends_on_direction() {
    // The analysis counterpart (ana_feature): AnalysisRewriteRule's own matcher direction is the OPPOSITE of rule.dir, so un-application is the mirror image of application, not a copy of it — a LeftToRight-declared rule's analysis scans right-to-left and vice versa.
    let g = load_probe_grammar();
    let unconstrained_voi = vec![0b01u64, 0b11, 0b01]; // cons+ (from d), voi now full-mask (t|d)

    let out_ltr = pg_rules::rewrite::analyze(
        &g,
        &double_t_feature_change_rule_dir(&g, Dir::LeftToRight),
        &seg(&g, "ddd"),
    );
    assert_eq!(out_ltr.len(), 1, "LtR-declared rule: unapplied");
    let got = interior(&out_ltr[0]);
    assert_eq!(got.len(), 3);
    assert_eq!(
        got[0].2,
        D.to_vec(),
        "leftmost d untouched -- a LtR-declared rule's analysis scans RtL"
    );
    assert_eq!(
        got[1].2, unconstrained_voi,
        "middle d unapplied (part of the rightmost pair)"
    );
    assert_eq!(
        got[2].2, unconstrained_voi,
        "rightmost d unapplied (part of the rightmost pair)"
    );

    let out_rtl = pg_rules::rewrite::analyze(
        &g,
        &double_t_feature_change_rule_dir(&g, Dir::RightToLeft),
        &seg(&g, "ddd"),
    );
    assert_eq!(out_rtl.len(), 1, "RtL-declared rule: unapplied");
    let got = interior(&out_rtl[0]);
    assert_eq!(got.len(), 3);
    assert_eq!(
        got[0].2, unconstrained_voi,
        "leftmost d unapplied (part of the leftmost pair)"
    );
    assert_eq!(
        got[1].2, unconstrained_voi,
        "middle d unapplied (part of the leftmost pair)"
    );
    assert_eq!(
        got[2].2,
        D.to_vec(),
        "rightmost d untouched -- a RtL-declared rule's analysis scans LtR"
    );
}

// Anchor environment: t -> [+voice] / a _ # (C# AnchorRules — a word-boundary right environment)

#[test]
fn feature_change_word_final_anchor_environment() {
    let g = load_probe_grammar();
    // t -> [+voi] / a _ #  (voice a t only word-finally).
    let r = rule(
        pat_char(char_def(&g, "char_t")),
        subrule(
            pat_ctx(nat_class(&g, "nc_voi")),
            Some(pat_ctx(nat_class(&g, "nc_vowel"))),
            Some(Pattern {
                nodes: vec![PatternNode::Anchor(AnchorSide::Right)],
            }),
        ),
    );
    // "at": t is word-final and preceded by a vowel -> voiced.
    let out = pg_rules::rewrite::synthesize(&g, &r, &seg(&g, "at"));
    assert_eq!(out.len(), 1, "word-final t voiced");
    assert_eq!(interior(&out[0])[1].2, D.to_vec());

    // "ata": the t is NOT word-final (an a follows) -> rule must not fire.
    let out2 = pg_rules::rewrite::synthesize(&g, &r, &seg(&g, "ata"));
    assert!(
        out2.is_empty(),
        "medial t is not word-final: no application"
    );
}

// Multi-subrule Simultaneous disjunction: Rust dispatches sim_feature/sim_narrow per-subrule rather than C#'s one collect-then-apply pass with per-position first-applicable dispatch, so this test pins which subrule wins when two Simultaneous subrules' environments both hold at the same position.

/// Two Simultaneous subrules over nc_cons: subrule 1 voices before another consonant, subrule 2 is an unconditional catch-all — on "td", both environments hold at position 0, a genuine same-position overlap; only subrule 2 can apply at position 1.
fn disjunctive_simultaneous_rule(g: &pg_grammar::model::Grammar) -> RewriteRuleDef {
    rule_multi(
        pat_ctx(nat_class(g, "nc_cons")),
        vec![
            subrule(
                pat_ctx(nat_class(g, "nc_voi")),
                None,
                Some(pat_ctx(nat_class(g, "nc_cons"))),
            ),
            subrule(pat_ctx(nat_class(g, "nc_t")), None, None),
        ],
        RewriteMode::Simultaneous,
    )
}

#[test]
fn simultaneous_multi_subrule_disjunction_first_subrule_wins_at_overlapping_position() {
    let g = load_probe_grammar();
    let r = disjunctive_simultaneous_rule(&g);
    // "td": both subrules' environments hold at position 0, but subrule 1 runs first and marks the node dirty before subrule 2's snapshot, so subrule 2 correctly skips it — matching C#'s first-applicable-subrule-wins semantics.
    let out = pg_rules::rewrite::synthesize(&g, &r, &seg(&g, "td"));
    assert_eq!(out.len(), 1, "rule applied");
    let got = interior(&out[0]);
    assert_eq!(
        got.len(),
        2,
        "no nodes added/removed (Feature-kind rewrite)"
    );
    assert_eq!(
        got[0].2,
        D.to_vec(),
        "position 0: subrule 1 (voicing, RightEnvironment=Cons) wins over subrule 2's catch-all, \
         even though subrule 2's own vacuous environment ALSO holds there -- confirming Rust's \
         per-subrule-sequential dirty-carryover reproduces C#'s first-applicable-subrule-wins \
         semantics for this overlapping-position case, not just for the non-overlapping cases every \
         reference-grammar rule and every other fixture in this pass happens to have"
    );
    assert_eq!(
        got[1].2,
        T.to_vec(),
        "position 1: only subrule 2's catch-all applies (subrule 1's RightEnvironment fails at \
         word end), forcing 'd' to t's exact (voiceless) lanes"
    );
}

// sim_narrow coverage: its splice-then-delete-descending transform is genuinely new code with no oracle fixture exercising a Simultaneous Narrow/Expansion synthesis rule, unlike sim_feature and Simultaneous Epenthesis (both covered elsewhere).

fn simultaneous_narrowing_rule(g: &pg_grammar::model::Grammar) -> RewriteRuleDef {
    // tt -> n / V _ V, tagged Simultaneous — the Iterative form of this same rule is narrowing_rule, tested elsewhere via a single site.
    rule_multi(
        Pattern {
            nodes: vec![
                PatternNode::CharDef(char_def(g, "char_t")),
                PatternNode::CharDef(char_def(g, "char_t")),
            ],
        },
        vec![subrule(
            pat_char(char_def(g, "char_n")),
            Some(pat_ctx(nat_class(g, "nc_vowel"))),
            Some(pat_ctx(nat_class(g, "nc_vowel"))),
        )],
        RewriteMode::Simultaneous,
    )
}

#[test]
fn simultaneous_narrow_synthesis_merges_two_non_overlapping_sites_in_one_pass() {
    let g = load_probe_grammar();
    let r = simultaneous_narrowing_rule(&g);
    // "attatta": two separate "tt" sites; sim_narrow must collect both against one pristine snapshot and apply them descending so the first splice/delete doesn't shift the second site's captured indices.
    let out = pg_rules::rewrite::synthesize(&g, &r, &seg(&g, "attatta"));
    assert_eq!(out.len(), 1, "rule applied");
    let got = interior(&out[0]);
    assert_eq!(
        got.len(),
        5,
        "both tt pairs coalesced to a single n each (7 nodes -> 5)"
    );
    assert_eq!(
        got.iter().map(|x| x.2.clone()).collect::<Vec<_>>(),
        vec![A.to_vec(), D.to_vec(), A.to_vec(), D.to_vec(), A.to_vec()],
        "\"anana\": both non-overlapping tt sites narrowed in one Simultaneous pass"
    );
    for (i, node) in got.iter().enumerate() {
        if i % 2 == 1 {
            assert_eq!(
                node.1,
                char_def(&g, "char_n").0,
                "narrowed segment at index {i} is n"
            );
            assert!(
                !node.3,
                "the narrowed RHS segment must NOT be optional (same R1 invariant syn_narrow has)"
            );
        }
    }
}

// Phonological rule tracing: synthesize_with_mpr_traced/analyze_traced's subrule outcome side channel, Pattern fallback, the three gate reasons, and the multi-subrule readout order/early-stop, matching C#'s SynthesisRewriteRule.Apply.

fn children_of(
    sink: &TreeTraceSink,
    h: pg_rules::trace::TraceHandle,
) -> Vec<pg_rules::trace::TraceHandle> {
    sink.node(h).children.clone()
}

#[test]
fn traced_synthesis_pattern_fallback_when_nothing_matches() {
    let g = load_probe_grammar();
    let r = voicing_rule(&g);
    let sink = TreeTraceSink::new();
    let root = sink.generate_words();
    // "aaa": no "t" anywhere, so subrule 0's LHS never matches — the generic Pattern fallback, since this rule declares no MPR/POS restriction.
    let out = pg_rules::rewrite::synthesize_with_mpr_traced(
        &g,
        PRuleId(0),
        &r,
        &seg(&g, "aaa"),
        &FeatureStruct::EMPTY,
        MprSet::EMPTY,
        &sink,
        root,
    );
    assert!(out.is_empty(), "rule doesn't apply to an all-vowel input");
    let children = children_of(&sink, root);
    assert_eq!(
        children.len(),
        1,
        "exactly one trace event, for the rule's one subrule"
    );
    let ev = sink.node(children[0]);
    assert_eq!(ev.type_, TraceType::PhonologicalRuleSynthesis);
    assert_eq!(ev.subrule_index, Some(0));
    assert_eq!(ev.failure_reason, Some(FailureReason::Pattern));
}

#[test]
fn traced_synthesis_applied_carries_no_reason_and_the_rewritten_output() {
    let g = load_probe_grammar();
    let r = voicing_rule(&g);
    let sink = TreeTraceSink::new();
    let root = sink.generate_words();
    let out = pg_rules::rewrite::synthesize_with_mpr_traced(
        &g,
        PRuleId(0),
        &r,
        &seg(&g, "ata"),
        &FeatureStruct::EMPTY,
        MprSet::EMPTY,
        &sink,
        root,
    );
    assert_eq!(out.len(), 1, "rule applies");
    let children = children_of(&sink, root);
    assert_eq!(children.len(), 1);
    let ev = sink.node(children[0]);
    assert_eq!(ev.type_, TraceType::PhonologicalRuleSynthesis);
    assert_eq!(ev.subrule_index, Some(0));
    assert_eq!(
        ev.failure_reason, None,
        "Applied carries no FailureReason (TraceManager.cs:174-184)"
    );
    let out_word = ev.output.expect("Applied records the output word");
    assert_eq!(
        interior(&out_word.shape),
        interior(&out[0]),
        "trace output matches the returned shape"
    );
}

/// The RequiredMprFeatures gate is checked, and its reason reported, before the pattern is ever tried — matching C#'s IsApplicable-before-MatchSubrule order.
#[test]
fn traced_synthesis_required_mpr_gate_reports_reason_before_the_pattern_is_tried() {
    let g = load_probe_grammar();
    let mut sr = subrule(
        pat_ctx(nat_class(&g, "nc_voi")),
        Some(pat_ctx(nat_class(&g, "nc_vowel"))),
        Some(pat_ctx(nat_class(&g, "nc_vowel"))),
    );
    let mut required = MprSet::EMPTY;
    required.insert(MprId(0));
    sr.required_mpr = required;
    let r = rule(pat_char(char_def(&g, "char_t")), sr);
    let sink = TreeTraceSink::new();
    let root = sink.generate_words();
    let out = pg_rules::rewrite::synthesize_with_mpr_traced(
        &g,
        PRuleId(1),
        &r,
        &seg(&g, "ata"),
        &FeatureStruct::EMPTY,
        MprSet::EMPTY, // the synthesizing word has NONE of the required MPR features
        &sink,
        root,
    );
    assert!(
        out.is_empty(),
        "gate fails, so the rule cannot apply regardless of the pattern"
    );
    let ev = sink.node(children_of(&sink, root)[0]);
    assert_eq!(ev.failure_reason, Some(FailureReason::RequiredMprFeatures));
}

/// The dual gate (`ExcludedMprFeatures`, C# `SynthesisRewriteSubruleSpec.cs:61-74`).
#[test]
fn traced_synthesis_excluded_mpr_gate_reports_reason() {
    let g = load_probe_grammar();
    let mut sr = subrule(
        pat_ctx(nat_class(&g, "nc_voi")),
        Some(pat_ctx(nat_class(&g, "nc_vowel"))),
        Some(pat_ctx(nat_class(&g, "nc_vowel"))),
    );
    let mut excluded = MprSet::EMPTY;
    excluded.insert(MprId(0));
    sr.excluded_mpr = excluded;
    let r = rule(pat_char(char_def(&g, "char_t")), sr);
    let sink = TreeTraceSink::new();
    let root = sink.generate_words();
    let mut have = MprSet::EMPTY;
    have.insert(MprId(0)); // the synthesizing word HAS the excluded feature
    let out = pg_rules::rewrite::synthesize_with_mpr_traced(
        &g,
        PRuleId(2),
        &r,
        &seg(&g, "ata"),
        &FeatureStruct::EMPTY,
        have,
        &sink,
        root,
    );
    assert!(out.is_empty());
    let ev = sink.node(children_of(&sink, root)[0]);
    assert_eq!(ev.failure_reason, Some(FailureReason::ExcludedMprFeatures));
}

/// Two subrules sharing one LHS with different environments: both trace events fire, in subrule-index order, and the readout stops at the first Applied — matching C#'s SynthesisRewriteRule readout discipline.
#[test]
fn traced_synthesis_multi_subrule_readout_reports_failure_then_applied_in_order() {
    let g = load_probe_grammar();
    let r = rule_multi(
        pat_char(char_def(&g, "char_t")),
        vec![
            // subrule 0: requires a preceding consonant, never true in "ata" — Pattern fallback.
            subrule(
                pat_ctx(nat_class(&g, "nc_voi")),
                Some(pat_ctx(nat_class(&g, "nc_cons"))),
                None,
            ),
            // subrule 1: the ordinary intervocalic voicing rule -- matches.
            subrule(
                pat_ctx(nat_class(&g, "nc_voi")),
                Some(pat_ctx(nat_class(&g, "nc_vowel"))),
                Some(pat_ctx(nat_class(&g, "nc_vowel"))),
            ),
        ],
        RewriteMode::Iterative,
    );
    let sink = TreeTraceSink::new();
    let root = sink.generate_words();
    let out = pg_rules::rewrite::synthesize_with_mpr_traced(
        &g,
        PRuleId(3),
        &r,
        &seg(&g, "ata"),
        &FeatureStruct::EMPTY,
        MprSet::EMPTY,
        &sink,
        root,
    );
    assert_eq!(out.len(), 1, "subrule 1 applies");
    let children = children_of(&sink, root);
    assert_eq!(
        children.len(),
        2,
        "subrule 0 (failed) then subrule 1 (applied) -- both reported"
    );
    let ev0 = sink.node(children[0]);
    assert_eq!(ev0.subrule_index, Some(0));
    assert_eq!(ev0.failure_reason, Some(FailureReason::Pattern));
    let ev1 = sink.node(children[1]);
    assert_eq!(ev1.subrule_index, Some(1));
    assert_eq!(
        ev1.failure_reason, None,
        "the first Applied index -- readout stops here"
    );
}

/// Analysis side (analyze_traced): no gate and no FailureReason either way, just Unapplied/NotUnapplied fired inline per subrule.
#[test]
fn traced_analysis_reports_unapplied_and_not_unapplied() {
    let g = load_probe_grammar();
    let r = deletion_rule(&g);

    // "aa": the deleted t round-trips (analysis re-inserts it optionally) — Unapplied.
    let sink = TreeTraceSink::new();
    let root = sink.generate_words();
    let out = pg_rules::rewrite::analyze_traced(&g, PRuleId(4), &r, &seg(&g, "aa"), &sink, root);
    assert_eq!(out.len(), 1);
    let children = children_of(&sink, root);
    assert_eq!(children.len(), 1);
    let ev = sink.node(children[0]);
    assert_eq!(ev.type_, TraceType::PhonologicalRuleAnalysis);
    assert_eq!(ev.subrule_index, Some(0));
    assert_eq!(
        ev.failure_reason, None,
        "analysis events never carry a FailureReason (ITraceManager.cs:42-43)"
    );

    // "at": no vowel-t-vowel deletion site exists at all -- NotUnapplied.
    let sink2 = TreeTraceSink::new();
    let root2 = sink2.generate_words();
    let out2 = pg_rules::rewrite::analyze_traced(&g, PRuleId(4), &r, &seg(&g, "at"), &sink2, root2);
    assert!(out2.is_empty());
    let ev2 = sink2.node(children_of(&sink2, root2)[0]);
    assert_eq!(ev2.type_, TraceType::PhonologicalRuleAnalysis);
    assert_eq!(ev2.subrule_index, Some(0));
    assert_eq!(ev2.failure_reason, None);
}

/// analyze_cached_traced's only exercise: confirms the RuleCache-backed dispatch path fires identical events to its uncached sibling.
#[test]
fn traced_analysis_cached_matches_uncached() {
    let mut g = load_probe_grammar();
    // deletion_rule builds a fresh RewriteRuleDef each call; one copy is registered for RuleCache::build and a second is held here, matching the real (pid, rule) contract that rule must describe what pid indexes.
    let for_cache = deletion_rule(&g);
    g.prules
        .push(pg_grammar::model::PhonRuleDef::Rewrite(for_cache));
    let cache = pg_rules::cache::RuleCache::build(&g);
    let r = deletion_rule(&g);

    let sink = TreeTraceSink::new();
    let root = sink.generate_words();
    let out = pg_rules::rewrite::analyze_cached_traced(
        &g,
        PRuleId(0),
        &r,
        &seg(&g, "aa"),
        &cache,
        &sink,
        root,
    );
    assert_eq!(out.len(), 1);
    let ev = sink.node(children_of(&sink, root)[0]);
    assert_eq!(ev.type_, TraceType::PhonologicalRuleAnalysis);
    assert_eq!(ev.subrule_index, Some(0));
    assert_eq!(ev.failure_reason, None);
}

// A bounded Quantifier occupying the whole LHS/RHS has no defined C# behavior (it crashes there); this pins the current safe behavior instead — the quantifier's own multiplicity is invisible, since every individual occurrence of its child pattern independently satisfies min=1 as its own width-1 site.

#[test]
fn quantifier_as_whole_lhs_ignores_its_own_multiplicity_but_never_crashes_or_misgroups() {
    let g = load_probe_grammar();
    // LHS = (a){1,2} as the entire LHS pattern, RHS a single fixed segment: classify sees l == r == 1 => Kind::Feature, same dispatch a plain a -> t rule gets — the quantifier envelope changes only what the FST structurally matches, not which spec function runs.
    let lhs = Pattern {
        nodes: vec![PatternNode::Quantifier {
            min: 1,
            max: Some(2),
            children: vec![PatternNode::CharDef(char_def(&g, "char_a"))],
        }],
    };
    let r = rule(lhs, subrule(pat_char(char_def(&g, "char_t")), None, None));

    // One, two, and three a's: every single a is independently rewritten to t — the quantifier's 1..2 bound is never enforced as a group.
    for (word, want_len) in [("a", 1), ("aa", 2), ("aaa", 3)] {
        let out = pg_rules::rewrite::synthesize(&g, &r, &seg(&g, word));
        assert_eq!(
            out.len(),
            1,
            "{word:?}: rule applies (no crash, no silent no-op)"
        );
        let got = interior(&out[0]);
        assert_eq!(
            got.len(),
            want_len,
            "{word:?}: node count preserved (no over-wide consumption)"
        );
        for (i, node) in got.iter().enumerate() {
            assert_eq!(
                node.2,
                T.to_vec(),
                "{word:?}: position {i} independently rewritten to t's lanes"
            );
        }
    }
}

// Pins natural-class (PatternNode::Context) RHS epenthesis with a two-sided environment round-tripping in both directions, distinct from the epenthesis_* gates above, which all use a concrete CharDef RHS.

fn ctx_epenthesis_rule(g: &pg_grammar::model::Grammar, dir: Dir) -> RewriteRuleDef {
    rule_dir(
        Pattern::default(), // empty LHS => epenthesis
        subrule(
            pat_ctx(nat_class(g, "nc_n")), // RHS: a natural-class reference, not a concrete segment
            Some(pat_ctx(nat_class(g, "nc_vowel"))), // left env: a
            Some(pat_ctx(nat_class(g, "nc_t"))), // right env: t (excludes the inserted n itself)
        ),
        dir,
    )
}

#[test]
fn epenthesis_natural_class_rhs_round_trips_with_environment() {
    let g = load_probe_grammar();
    for dir in [Dir::LeftToRight, Dir::RightToLeft] {
        let r = ctx_epenthesis_rule(&g, dir);

        // "at" -> "ant": obligatory insertion of the nc_n-class segment between the vowel and t.
        let synth = pg_rules::rewrite::synthesize(&g, &r, &seg(&g, "at"));
        assert_eq!(synth.len(), 1, "{dir:?}: epenthesis must fire (env holds)");
        let got = interior(&synth[0]);
        assert_eq!(got.len(), 3, "{dir:?}: one segment epenthesized");
        assert_eq!(got[0].2, A.to_vec(), "{dir:?}: left a unchanged");
        assert_eq!(got[2].2, T.to_vec(), "{dir:?}: right t unchanged");
        // The inserted segment carries nc_n's own lanes (same bundle as d) but char_def is NO_CHAR_DEF, since a Context RHS has no single concrete identity — the shape this test targets, distinct from the concrete-CharDef epenthesis gates above.
        assert_eq!(
            got[1].2,
            D.to_vec(),
            "{dir:?}: inserted segment carries nc_n's own lanes"
        );
        assert_eq!(
            got[1].1,
            pg_shape::NO_CHAR_DEF,
            "{dir:?}: Context RHS has no concrete char-def identity"
        );
        assert!(
            !got[1].3,
            "{dir:?}: synthesized epenthetic segment is not optional"
        );

        // Analysis must recover the pre-insertion form: the medial inserted segment is marked OPTIONAL (never deleted), flanking a/t untouched, round-tripping correctly in both directions.
        let ana = pg_rules::rewrite::analyze(&g, &r, &synth[0]);
        assert_eq!(
            ana.len(),
            1,
            "{dir:?}: unapplication must fire (nonvacuous: the segment is not yet optional)"
        );
        let got = interior(&ana[0]);
        assert_eq!(
            got.iter().map(|x| x.3).collect::<Vec<_>>(),
            vec![false, true, false],
            "{dir:?}: only the epenthetic medial segment is marked optional"
        );
    }
}
