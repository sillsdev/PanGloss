//! Part-2 acceptance gate: rewrite apply/unapply on **hand-built** rules + input shapes with
//! hand-reasoned expected output shapes, at the rule level (the full Morpher is a later milestone).
//! Modeled on `tests/SIL.Machine.Morphology.HermitCrab.Tests/PhonologicalRules/RewriteRuleTests.cs`
//! (SimpleRules / AnchorRules / MultipleDeletionRules), reduced to single rules.
//!
//! Each test cites the C# spec method it exercises and states the expected shape derived by hand
//! from HermitCrab semantics.

mod common;

use common::*;
use hc_featstruct::FeatureStruct;
use hc_grammar::chardef::CharDefId;
use hc_grammar::model::AnchorSide;
use hc_grammar::model::{
    Dir, NatClassId, Pattern, PatternNode, RewriteMode, RewriteRuleDef, RewriteSubruleDef,
};
use hc_grammar::model::{MprId, MprSet, PRuleId};
use hc_rules::trace::{FailureReason, TraceSink, TraceType, TreeTraceSink};
use hc_shape::{NodeKind, Shape};

// ---- rule builders -----------------------------------------------------------------------------

fn subrule(rhs: Pattern, left: Option<Pattern>, right: Option<Pattern>) -> RewriteSubruleDef {
    RewriteSubruleDef {
        required_pos: None,
        required_mpr: hc_grammar::model::MprSet::EMPTY,
        excluded_mpr: hc_grammar::model::MprSet::EMPTY,
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

/// [`rule`]'s multi-subrule, mode-parameterized sibling — needed only by the P13 multi-subrule
/// Simultaneous-disjunction gate test below (§4.1's warning / §7 open question 1), which is the
/// only test in this file with more than one subrule on a single rule.
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

fn seg(g: &hc_grammar::model::Grammar, word: &str) -> Shape {
    hc_rules::shape_feat::segment_with_features(g, table(g), word).unwrap()
}

// Lane constants for the probe grammar ([cons, voi, Type]; masks 0b11 each on cons/voi, 0b01/0b10
// on the always-appended synthetic Type feature — plan §13.1 Tier-1 #1). Every concrete segment
// node here carries Type=Segment (0b01); nothing in this file exercises a boundary node's lanes.
const A: [u64; 3] = [0b10, 0b01, 0b01]; // vowel, voiced
const T: [u64; 3] = [0b01, 0b10, 0b01]; // consonant, voiceless
const D: [u64; 3] = [0b01, 0b01, 0b01]; // consonant, voiced

// =================================================================================================
// Feature-change: t -> [+voice] / V _ V   (C# FeatureSynthesisRewriteSubruleSpec.ApplyRhs /
// FeatureAnalysisRewriteRuleSpec.Unapply)
// =================================================================================================

fn voicing_rule(g: &hc_grammar::model::Grammar) -> RewriteRuleDef {
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
    // "ata": the medial t is voiced to d's feature bundle. `char_def` is reset to `NO_CHAR_DEF`
    // (not left stale at char_t): once a feature-change rule rewrites a node's lanes, its original
    // literal identity is no longer authoritative for rendering/lexical-matching purposes — C#'s
    // `CharacterDefinitionTable.GetMatchingStrReps` has no notion of a node's "original identity" at
    // all, it always re-derives matching representations from the node's *current* FeatureStruct
    // against the whole table. Confirmed via the Indonesian `meN-` prefix: `hc_shape::Shape::
    // node_cd_set`'s "concrete char_def == singleton identity" shortcut, if left stale after a
    // rewrite, made a real assimilated nasal (rewritten from the archiphoneme's own char_def) match
    // only the archiphoneme's own (now feature-incompatible) representation set, silently rendering
    // as nothing instead of "m" and breaking that whole word family's synthesis round-trip.
    let out = hc_rules::rewrite::synthesize(&g, &r, &seg(&g, "ata"));
    assert_eq!(out.len(), 1, "rule applied");
    let got = interior(&out[0]);
    assert_eq!(got.len(), 3);
    assert_eq!(got[0].2, A.to_vec(), "left a unchanged");
    assert_eq!(got[2].2, A.to_vec(), "right a unchanged");
    assert_eq!(got[1].2, D.to_vec(), "medial t -> [+voi] == d lanes");
    assert_eq!(
        got[1].1,
        hc_shape::NO_CHAR_DEF,
        "char_def reset: identity now feature-driven, not stale char_t"
    );
}

#[test]
fn feature_change_synthesis_iterates_over_all_targets() {
    let g = load_probe_grammar();
    let r = voicing_rule(&g);
    // "atata": both medial t's are between vowels -> both voiced (iterative application).
    let out = hc_rules::rewrite::synthesize(&g, &r, &seg(&g, "atata"));
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
    let out = hc_rules::rewrite::synthesize(&g, &r, &seg(&g, "at"));
    assert!(out.is_empty(), "no right-hand vowel: rule must not fire");
}

#[test]
fn feature_change_analysis_underspecifies_voice() {
    let g = load_probe_grammar();
    let r = voicing_rule(&g);
    // Analyze "ada": the medial d matches the analysis target (lhs t priority-union rhs [+voi] =
    // [cons+,voi+] = d), and Unapply makes the *changed* feature (voice) underspecified -> voi lane
    // becomes the full mask 0b11 (so lexical lookup can match either t or d).
    let out = hc_rules::rewrite::analyze(&g, &r, &seg(&g, "ada"));
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
    let synth = hc_rules::rewrite::synthesize(&g, &r, &orig).pop().unwrap();
    let ana = hc_rules::rewrite::analyze(&g, &r, &synth).pop().unwrap();
    // The analyzed medial node must unify with the original t (superset containment).
    let ana_mid = &interior(&ana)[1].2;
    let orig_mid = &interior(&orig)[1].2;
    assert!(
        hc_featstruct::flat_unifiable(ana_mid, orig_mid),
        "analysis {ana_mid:?} must be a superset of original {orig_mid:?}"
    );
}

// =================================================================================================
// Tier-2 #11 (plan §6 item 4 / W1.2): analysis feature-reversal must use C#'s `AntiFeatureStruct`
// negation (`L ∪ R`, via the `mask & !bits` idiom `bind_or_check` already uses elsewhere in this
// file for alpha-variable disagreement), not a blanket full-unconstrain. Needs a >=3-symbol feature
// to distinguish the two — a 2-symbol feature's negation always degenerates to `full_mask`, which
// is why none of the 3 reference grammars (nor this file's own 2-symbol `voicing_rule` test above)
// can tell the two formulas apart. C# analog: `RewriteRuleTests.CommonFeatureRules`, adapted to a
// 3-way place-of-articulation-style feature.
// =================================================================================================

fn place_rule(g: &hc_grammar::model::Grammar) -> RewriteRuleDef {
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
    // Analyze "k" (place=vel): the analysis target is LHS⊕RHS = place=vel exactly, so "k" matches.
    // Reversal must set place to {lab, vel} (L ∪ R, LHS's "p"=lab unioned with the matched "k"=vel)
    // -- NOT full-unconstrained {lab, cor, vel}: "cor" was never a possible value on either side of
    // this rule and must stay excluded. Before the Tier-2 #11 fix this asserted false (the old code
    // set the lane to `full_mask`, including "cor").
    let out = hc_rules::rewrite::analyze(&g, &r, &seg(&g, "k"));
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

// =================================================================================================
// Deletion: t -> 0 / a _ a   (C# NarrowSynthesisRewriteSubruleSpec.ApplyRhs /
// NarrowAnalysisRewriteRuleSpec.Unapply, reapply=Deletion)
// =================================================================================================

fn deletion_rule(g: &hc_grammar::model::Grammar) -> RewriteRuleDef {
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
    let out = hc_rules::rewrite::synthesize(&g, &r, &seg(&g, "ata"));
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
    // Analyze "aa": NarrowAnalysis re-inserts the deleted LHS segment (t) as OPTIONAL at the site
    // between the two vowels, so lexical lookup can recover both "aa" and "ata".
    let out = hc_rules::rewrite::analyze(&g, &r, &seg(&g, "aa"));
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
    let synth = hc_rules::rewrite::synthesize(&g, &r, &seg(&g, "ata"))
        .pop()
        .unwrap(); // "aa"
    let ana = hc_rules::rewrite::analyze(&g, &r, &synth).pop().unwrap(); // "a(t)a"
                                                                         // Taking the optional t recovers the original interior a t a.
    let got = interior(&ana);
    assert_eq!(got.len(), 3);
    assert_eq!(got[1].2, T.to_vec());
    assert!(got[1].3, "optional");
}

// =================================================================================================
// Word-initial deletion: t -> 0 / # _ a   (Tier-1 R2: the word-initial gap, before the very first
// segment, must be a matchable analysis-unapply site, not just "after each segment")
// =================================================================================================

fn word_initial_deletion_rule(g: &hc_grammar::model::Grammar) -> RewriteRuleDef {
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
    let out = hc_rules::rewrite::synthesize(&g, &r, &seg(&g, "ta"));
    assert_eq!(out.len(), 1, "rule applied");
    let got = interior(&out[0]);
    assert_eq!(got.len(), 1, "t deleted");
    assert_eq!(got[0].2, A.to_vec());
}

#[test]
fn word_initial_deletion_analysis_reinserts_optional_t_at_word_start() {
    let g = load_probe_grammar();
    let r = word_initial_deletion_rule(&g);
    // Analyze "a": before the R2 fix, `ana_narrow` only enumerated "gap after segment i" sites
    // (never "before the very first segment"), so a word-initial deletion could never be
    // un-applied at all — C#'s `RewriteRuleSpec.MatchSubrule` `_isTargetEmpty` branch matches the
    // shape's own left-anchor node as a legitimate site (`RewriteRuleSpec.cs:55-77`,
    // `NarrowAnalysisRewriteRuleSpec.cs:24-31`).
    let out = hc_rules::rewrite::analyze(&g, &r, &seg(&g, "a"));
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
    let synth = hc_rules::rewrite::synthesize(&g, &r, &seg(&g, "ta"))
        .pop()
        .unwrap(); // "a"
    let ana = hc_rules::rewrite::analyze(&g, &r, &synth).pop().unwrap(); // "(t)a"
    let got = interior(&ana);
    assert_eq!(got.len(), 2);
    assert_eq!(got[0].2, T.to_vec());
    assert!(got[0].3, "optional");
}

// =================================================================================================
// Narrowing (RHS non-empty, LHS/RHS node counts differ): tt -> n / a _ a   (C#
// `NarrowSynthesisRewriteSubruleSpec.ApplyRhs` with a real replacement segment, not a pure
// deletion — Tier-1 R1: the inserted RHS must be non-optional, only `dirty`, so it can never be
// treated as skippable downstream)
// =================================================================================================

fn narrowing_rule(g: &hc_grammar::model::Grammar) -> RewriteRuleDef {
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
    // "atta" -> "ana": the doubled t coalesces to a single n between vowels via a genuine
    // narrowing (LHS has 2 nodes, RHS has 1 — not a pure deletion, so this exercises
    // `syn_narrow`'s RHS-insert path, not `ana_narrow`'s deletion-only re-insert path).
    // Before the R1 fix, `syn_narrow` inserted the RHS node via `new_seg_node(..., true)`,
    // abusing the `optional` parameter to also get `dirty=true` as a side effect
    // (`optional` and `dirty` were coupled in `new_seg_node`) — making the coalesced "n"
    // spuriously OPTIONAL, so downstream matching could accept a surface missing it entirely.
    // C# `NarrowSynthesisRewriteSubruleSpec.ApplyRhs` (cs:31-45) calls `Shape.AddAfter` (which
    // never sets `Optional`) and only conditionally calls the separate `SetDirty(true)`.
    let out = hc_rules::rewrite::synthesize(&g, &r, &seg(&g, "atta"));
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

// =================================================================================================
// Narrowing RHS alpha-variable resolution (plan §6 item 3 / W1.3): `syn_narrow`'s RHS build had no
// `rhs_vars` step at all (unlike `syn_feature`) — a narrowing RHS natural class carrying an alpha
// variable bound from a merged LHS segment was left fully unconstrained instead of resolved to the
// captured value. C# analog: `RewriteRuleTests.AlphaVariableRules` x `MergeRules`.
// =================================================================================================

fn merge_with_alpha_voice_rule(g: &hc_grammar::model::Grammar) -> RewriteRuleDef {
    let voi = feat(g, "feat_voi");
    // [C, var1=voice] [C] -> [C, var1=voice] : two consonants merge to one whose voice comes from
    // the FIRST LHS node's own (captured) voice value, via alpha var 1 (agree).
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
    // "td" -> coalesce to one consonant whose voice must come from the captured FIRST LHS node
    // ('t', voiceless) via alpha var 1, NOT stay unconstrained. Before this fix `syn_narrow` never
    // computed bindings at all, so the RHS's voice lane stayed at its unresolved full-mask default
    // (nc_cons itself only pins `cons+`, leaving `voi` unconstrained absent the var resolution).
    let out = hc_rules::rewrite::synthesize(&g, &r, &seg(&g, "td"));
    assert_eq!(out.len(), 1, "rule applied");
    let got = interior(&out[0]);
    assert_eq!(got.len(), 1, "coalesced to a single consonant");
    assert_eq!(
        got[0].2,
        T.to_vec(),
        "voice resolved from the captured LHS var ('t'), not left unconstrained"
    );
}

// =================================================================================================
// Epenthesis: 0 -> t / a _ a   (C# EpenthesisSynthesisRewriteSubruleSpec.ApplyRhs /
// EpenthesisAnalysisRewriteRuleSpec.Unapply)
// =================================================================================================

fn epenthesis_rule(g: &hc_grammar::model::Grammar) -> RewriteRuleDef {
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
    let out = hc_rules::rewrite::synthesize(&g, &r, &seg(&g, "aa"));
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
    // 0 -> t / # _ a : the P1 fix — C# `SynthesisRewriteRuleSpec`'s empty-LHS pattern matches the
    // left-anchor annotation itself (`Segment|Anchor` constraint), so the word-initial gap is an
    // ordinary application site (`RewriteRuleSpec.MatchSubrule`'s `_isTargetEmpty` branch inserts
    // `AddAfter(rangeStart)` = right after the anchor).
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
    // "aa" -> "taa": fires ONLY at the word-initial gap (after seg 0 the anchor-only left env
    // fails; after seg 1 the vowel right env has nothing to match) — no medial double-firing.
    let out = hc_rules::rewrite::synthesize(&g, &r, &seg(&g, "aa"));
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
    // Analyze "ata": the medial t (matching the epenthesis RHS, between two vowels) is marked
    // OPTIONAL (EpenthesisAnalysis.Unapply) rather than deleted.
    let out = hc_rules::rewrite::analyze(&g, &r, &seg(&g, "ata"));
    let got = interior(&out[0]);
    assert_eq!(got.len(), 3);
    assert!(got[1].3, "epenthetic t marked OPTIONAL on unapply");
    assert_eq!(got[1].2, T.to_vec());
}

#[test]
fn epenthesis_analysis_multi_node_target_matches_document_order() {
    let g = load_probe_grammar();
    // 0 -> t d (2-node RHS, no envs). The analysis matcher runs in the REVERSED direction (C#
    // `AnalysisRewriteRule`'s `MatcherSettings.Direction`), but C#'s `PatternNode.GenerateNfa`
    // enumerates pattern children in `fsa.Direction` order (PatternNode.cs:55), so an RtL matcher
    // still matches the pattern's DOCUMENT-order physical substring ("td"), not its reversal.
    // `compile_lane_fst` performs that document->traversal reorder; before the P1 fix a 2-node
    // analysis target silently matched the physically-reversed sequence instead.
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
    let out = hc_rules::rewrite::analyze(&g, &r, &seg(&g, "tda"));
    assert_eq!(out.len(), 1, "unapply fired on the document-order match");
    let got = interior(&out[0]);
    assert_eq!(
        got.iter().map(|x| x.3).collect::<Vec<_>>(),
        vec![true, true, false],
        "t,d marked optional (document order), a untouched"
    );
    // "dta" contains only the REVERSED sequence d,t — C# would not match it, and neither may we.
    let out = hc_rules::rewrite::analyze(&g, &r, &seg(&g, "dta"));
    assert!(
        out.is_empty(),
        "reversed physical sequence must NOT match the analysis target"
    );
}

#[test]
fn epenthesis_round_trip_recovers_superset() {
    let g = load_probe_grammar();
    let r = epenthesis_rule(&g);
    let synth = hc_rules::rewrite::synthesize(&g, &r, &seg(&g, "aa"))
        .pop()
        .unwrap(); // "ata"
    let ana = hc_rules::rewrite::analyze(&g, &r, &synth).pop().unwrap(); // "a(t)a"
                                                                         // Skipping the optional t recovers the original "aa".
    let got = interior(&ana);
    assert!(got[1].3, "optional t: skipping it recovers the original aa");
}

// =================================================================================================
// Width-mismatch guard (plan §6 item 1 / W1.1): a multi-node LHS abutting a `BoundaryMarker`, so a
// nondeterministic FST match can transparently skip the boundary (an Optional segment in
// `MutShape::segs`) and report an `ENTIRE_MATCH` span *wider* than the compiled 2-node pattern. No
// existing C# test exercises this combination (`RewriteRuleTests.BoundaryRules` and
// `MultipleSegmentRules` each test one half). Before the fix: `syn_feature` panicked indexing
// `rhs_pins[k]` out of bounds; `syn_narrow` silently deleted the boundary node too (one node more
// than the LHS pattern matched).
// =================================================================================================

/// Re-flag the interior node at `interior_idx` (0-based, post-left-anchor) as `OPTIONAL`, mirroring
/// `MutShape::to_shape`'s own delete+reinsert technique for materializing an Optional flag onto a
/// frozen `Shape`. Used to build the "an Optional real segment, not just a boundary, can widen an
/// `ENTIRE_MATCH` span" fixture (see this module's `MutNode::segs` gap doc): a plain word-medial
/// segment marked Optional this way is exactly what `ana_narrow`'s deletion-unapply produces, which
/// can then feed a *later* synthesis pass in a real pipeline.
fn mark_optional(shape: &Shape, interior_idx: usize) -> Shape {
    let idx = interior_idx + 1; // +1 for the left anchor
    let char_def = shape.char_def(idx);
    let lanes = shape.node_lanes(idx).to_vec();
    let mut m = hc_shape::ShapeBuilder::from_shape(shape);
    m.delete(idx);
    m.insert(
        idx,
        NodeKind::Segment,
        char_def,
        hc_shape::NodeFlags(hc_shape::NodeFlags::OPTIONAL),
        &lanes,
    );
    m.freeze()
}

fn double_t_feature_change_rule(g: &hc_grammar::model::Grammar) -> RewriteRuleDef {
    // tt -> [+voice][+voice] (both LHS nodes voiced) — a 2-node LHS/RHS feature-change rule, no
    // environment (the boundary sits *inside* the matched span, not in an environment).
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
    // "tat" with the medial 'a' re-flagged OPTIONAL (exactly what `ana_narrow`'s deletion-unapply
    // produces, which can then feed a later synthesis pass in a real pipeline). `hc_fst`'s
    // Optional-skip mechanism (`Transduce::advance`, which lets a pattern transparently pass over
    // any Optional segment — boundary or not, per this module's own doc) fires here exactly as it
    // would for a literal `BoundaryMarker`, but the medial 'a' does NOT itself match the LHS's
    // second `t` node, so no width-correct "tt" span exists at all — `all_spans` reports only the
    // single over-wide `[0,3)` skip-through span (empirically confirmed while developing this fix:
    // raw FST results were exactly `[(0,3)]`, none width-correct). Because 'a' is `Segment`-kind
    // (not `Boundary`), it also passes the pre-existing `kind != NodeKind::Segment` filter that
    // incidentally protects a literal boundary from ever reaching the panicking line below — this
    // is the genuine, previously-unguarded crash site. Before the width guard: `syn_feature`
    // walked this 3-node span against a 2-element `rhs_pins`, indexing `rhs_pins[2]` and panicking
    // with "index out of bounds: the len is 2 but the index is 2" (confirmed via direct repro
    // during this fix's development, at the exact line the guard now short-circuits).
    let base = seg(&g, "tat");
    let input = mark_optional(&base, 1);
    let out = hc_rules::rewrite::synthesize(&g, &r, &input);
    // No width-correct "tt" span exists once 'a' is excluded from consideration, so the guard
    // correctly rejects the only (over-wide) candidate: the rule must not apply. The load-bearing
    // regression check is simply that this call returns instead of panicking.
    assert!(
        out.is_empty(),
        "no width-correct match exists; the over-wide span must be rejected, not applied"
    );
}

fn double_t_narrow_rule(g: &hc_grammar::model::Grammar) -> RewriteRuleDef {
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
    // Same fixture as the `syn_feature` sibling above ("tat", medial 'a' marked Optional). Before
    // the width guard, `syn_narrow` doesn't index a per-node array positionally (no panic risk
    // there), but it deletes every node in `target_nodes` and splices the RHS in after the last
    // one — so the over-wide `[0,3)` span would delete THREE nodes (t, a, t) and insert "n" after
    // them, silently swallowing 'a' along with the two real targets: one physical node more than
    // the 2-node LHS pattern actually specifies.
    let base = seg(&g, "tat");
    let input = mark_optional(&base, 1);
    let out = hc_rules::rewrite::synthesize(&g, &r, &input);
    assert!(
        out.is_empty(),
        "no width-correct match exists; the over-wide span must be rejected, not applied"
    );
}

// =================================================================================================
// Anchor environment: t -> [+voice] / a _ #   (C# AnchorRules — a word-boundary right environment)
// =================================================================================================

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
    let out = hc_rules::rewrite::synthesize(&g, &r, &seg(&g, "at"));
    assert_eq!(out.len(), 1, "word-final t voiced");
    assert_eq!(interior(&out[0])[1].2, D.to_vec());

    // "ata": the t is NOT word-final (an a follows) -> rule must not fire.
    let out2 = hc_rules::rewrite::synthesize(&g, &r, &seg(&g, "ata"));
    assert!(
        out2.is_empty(),
        "medial t is not word-final: no application"
    );
}

// =================================================================================================
// P13 (`rust/docs/p13-simultaneous-design.md` §4.1's warning / §7 open question 1): multi-subrule
// Simultaneous disjunction. `sim_feature`/`sim_narrow` are dispatched per subrule (Rust's existing
// per-subrule-outer-loop architecture, `synthesize_with_mpr`'s `for sr in &rule.subrules` sharing
// one `MutShape` across subrules) rather than as one collect-then-apply pass across the WHOLE rule
// with per-position first-applicable-subrule dispatch (C#'s actual `RewriteRuleSpec.MatchSubrule`
// mechanism, §1.2). The design doc flags this as an UNEXERCISED risk, not a known bug: a rule with
// two Simultaneous subrules whose environments can BOTH hold at the SAME target position might
// (a) apply only the first (correct, matching C#), (b) apply both (wrong), or (c) apply the second
// because the first's own snapshot-based pass hadn't marked anything dirty before the second's
// snapshot was taken (wrong). This test constructs exactly that overlapping-position case and
// records which of (a)/(b)/(c) Rust's actual per-subrule-sequential design produces.
// =================================================================================================

/// Two Simultaneous subrules over `nc_cons` (matches t/d/n): subrule 1 voices a consonant that is
/// followed by ANOTHER consonant (`RightEnvironment = nc_cons`); subrule 2 is an unconditional
/// catch-all (no environment at all) that devoices to `t`'s exact lanes. On word "td" (t=cons+,
/// voi-; d=cons+,voi+), LHS `nc_cons` matches BOTH positions (single-node target). At position 0
/// ("t"), subrule 1's environment holds (followed by "d", a consonant) AND subrule 2's environment
/// holds too (vacuously, everywhere) — a genuine overlap at the SAME position, the exact shape §4.1
/// warns about. At position 1 ("d", word-final), subrule 1's environment fails (nothing follows);
/// only subrule 2's catch-all can apply there.
fn disjunctive_simultaneous_rule(g: &hc_grammar::model::Grammar) -> RewriteRuleDef {
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
    // "td": position 0 ("t") satisfies BOTH subrules' environments simultaneously (subrule 1's
    // RightEnvironment=Cons holds because "d" follows; subrule 2's catch-all always holds) --
    // Rust's per-subrule-sequential architecture applies subrule 1 first (voicing "t" to
    // cons+,voi+ = D's lanes) and marks that node dirty BEFORE subrule 2's own `sim_feature` call
    // takes its snapshot, so subrule 2 correctly skips the already-touched position 0 — matching
    // C#'s first-applicable-subrule-wins semantics (RewriteRuleSpec.MatchSubrule's inner loop),
    // even though this is "one collect-then-apply pass per subrule", not "one pass across the
    // whole rule with per-position dispatch" (§4.1's distinction). Position 1 ("d", word-final) is
    // untouched by subrule 1 (no following consonant) and is caught by subrule 2's catch-all,
    // forcing it to T's exact (voiceless) lanes.
    let out = hc_rules::rewrite::synthesize(&g, &r, &seg(&g, "td"));
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

// =================================================================================================
// P13: `sim_narrow` coverage. `sim_feature` is exercised by the disjunction test above (and, at the
// full-pipeline level, by `hc-parse/tests/simultaneous_conformance.rs`'s oracle fixtures); Epenthesis
// under Simultaneous mode reuses `syn_epenthesis` (already covered by the oracle fixtures too). But
// `sim_narrow` — genuinely new code (the design doc's §4.1 splice-then-delete-descending transform
// of `syn_narrow`) — had no dedicated test anywhere: no oracle fixture exercises a Simultaneous
// Narrow/Expansion synthesis rule (§6.4 deliberately built none, reasoning that the ANALYSIS side
// needs no new code — true, but orthogonal to `sim_narrow`, which is synthesis-only and genuinely
// new). This closes that gap directly.
// =================================================================================================

fn simultaneous_narrowing_rule(g: &hc_grammar::model::Grammar) -> RewriteRuleDef {
    // tt -> n / V _ V, tagged Simultaneous (the Iterative form of this same rule is
    // `narrowing_rule`, tested elsewhere in this file via "atta" -> "ana", a single site).
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
    // "attatta" (a,t,t,a,t,t,a): TWO separate "tt" sites, both flanked by vowels, at node indices
    // (1,2) and (4,5) (1-indexed after the left anchor). `sim_narrow` must collect BOTH accepted
    // spans against one pristine snapshot and apply them (descending, so the first splice/delete
    // doesn't shift the second site's own captured indices — see `sim_narrow`'s doc), giving
    // "anana": both merges present in a single `synthesize` call, not just one.
    let out = hc_rules::rewrite::synthesize(&g, &r, &seg(&g, "attatta"));
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

// =================================================================================================
// P12 chunk 6: phonological rule tracing. `synthesize_with_mpr_traced`/`analyze_traced`'s subrule
// outcome side channel (C#'s `Word.CurrentRuleResults`), the `Pattern` fallback, the three gate
// reasons (`RequiredSyntacticFeatureStruct`/`RequiredMprFeatures`/`ExcludedMprFeatures`), and the
// multi-subrule readout order/early-stop C#'s `SynthesisRewriteRule.Apply` (cs:65-85) uses.
// =================================================================================================

fn children_of(
    sink: &TreeTraceSink,
    h: hc_rules::trace::TraceHandle,
) -> Vec<hc_rules::trace::TraceHandle> {
    sink.node(h).children.clone()
}

#[test]
fn traced_synthesis_pattern_fallback_when_nothing_matches() {
    let g = load_probe_grammar();
    let r = voicing_rule(&g);
    let sink = TreeTraceSink::new();
    let root = sink.generate_words();
    // "aaa": no "t" anywhere, so subrule 0's LHS never matches at all -- the generic `Pattern`
    // fallback (no gate to have failed; this rule declares no MPR/POS restriction).
    let out = hc_rules::rewrite::synthesize_with_mpr_traced(
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
    let out = hc_rules::rewrite::synthesize_with_mpr_traced(
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

/// [`SynthesisRewriteSubruleSpec.IsApplicable`]'s first gate (`RequiredMprFeatures`, C#
/// `SynthesisRewriteSubruleSpec.cs:46-59`): reported even though the LHS pattern (medial "t" in
/// "ata") WOULD otherwise match -- confirming the gate is checked, and its specific reason reported,
/// BEFORE the pattern is ever tried, matching C#'s `IsApplicable`-before-`MatchSubrule` order.
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
    let out = hc_rules::rewrite::synthesize_with_mpr_traced(
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
    let out = hc_rules::rewrite::synthesize_with_mpr_traced(
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

/// Two subrules sharing one LHS but different environments: subrule 0's environment never holds in
/// "ata" (Pattern fallback), subrule 1's does (Applied). Confirms C#'s exact readout discipline
/// (`SynthesisRewriteRule.cs:65-83`): BOTH events fire, in subrule-index order, and the readout
/// stops at the FIRST `Applied` -- there is no third subrule here to prove the `break` against, but
/// the two-event shape (not one, not zero) already confirms the loop doesn't stop at the first
/// failure either.
#[test]
fn traced_synthesis_multi_subrule_readout_reports_failure_then_applied_in_order() {
    let g = load_probe_grammar();
    let r = rule_multi(
        pat_char(char_def(&g, "char_t")),
        vec![
            // subrule 0: requires a preceding consonant -- never holds in "ata" (t is preceded by a
            // vowel) -- Pattern fallback.
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
    let out = hc_rules::rewrite::synthesize_with_mpr_traced(
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

/// Analysis side (`analyze_traced`): no gate, no `FailureReason` at all either way -- just
/// `Unapplied`/`NotUnapplied` fired inline per subrule (`AnalysisRewriteRule.cs:178-187`).
#[test]
fn traced_analysis_reports_unapplied_and_not_unapplied() {
    let g = load_probe_grammar();
    let r = deletion_rule(&g);

    // "aa": the deleted "t" between the two vowels round-trips (analysis re-inserts it optionally)
    // -- Unapplied.
    let sink = TreeTraceSink::new();
    let root = sink.generate_words();
    let out = hc_rules::rewrite::analyze_traced(&g, PRuleId(4), &r, &seg(&g, "aa"), &sink, root);
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
    let out2 = hc_rules::rewrite::analyze_traced(&g, PRuleId(4), &r, &seg(&g, "at"), &sink2, root2);
    assert!(out2.is_empty());
    let ev2 = sink2.node(children_of(&sink2, root2)[0]);
    assert_eq!(ev2.type_, TraceType::PhonologicalRuleAnalysis);
    assert_eq!(ev2.subrule_index, Some(0));
    assert_eq!(ev2.failure_reason, None);
}

/// `analyze_cached_traced`'s only exercise (see its doc: not yet called from live code) -- confirms
/// the `RuleCache`-backed dispatch path fires the identical events the uncached sibling does.
#[test]
fn traced_analysis_cached_matches_uncached() {
    let mut g = load_probe_grammar();
    // `deletion_rule` builds a fresh, independent `RewriteRuleDef` each call (no `Clone` on that
    // type) -- one copy registered into `g.prules` for `RuleCache::build`, a second held by this
    // test to pass alongside the cache (matching every real `_cached`/`_cached_traced` call site's
    // own `(pid, rule)` contract: `rule` must describe the same rule `pid` indexes).
    let for_cache = deletion_rule(&g);
    g.prules
        .push(hc_grammar::model::PhonRuleDef::Rewrite(for_cache));
    let cache = hc_rules::cache::RuleCache::build(&g);
    let r = deletion_rule(&g);

    let sink = TreeTraceSink::new();
    let root = sink.generate_words();
    let out = hc_rules::rewrite::analyze_cached_traced(
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
