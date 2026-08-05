//! G5 acceptance gate: `SynthesisAffixProcessRule.Apply`'s gate ORDER
//! (`SynthesisAffixProcessRule.cs:44-131`) is, in order: `MaxApplicationCount` (not enforced by
//! this port — see `pg_rules::stratum::guided_synth`'s doc), the final-template prohibition, the
//! non-final-template requirement, `RequiredStemName`, and — LAST — the required-syntactic-FS
//! unify. `pg_rules::morph::synth_affix`/`synth_affix_cached` used to check the syn-FS gate
//! FIRST; this pins the fix (the gate moved to after `RequiredStemName`, matching C#) two ways:
//!
//! 1. When BOTH the syn-FS gate and the final-template-prohibition gate would reject the same
//!    candidate, the FIRST `FailureReason` the trace reports is now
//!    `NonPartialRuleProhibitedAfterFinalTemplate` (C#'s answer), not
//!    `RequiredSyntacticFeatureStruct`.
//! 2. The reorder is trace-only: the SET of words a real multi-candidate synthesis run produces
//!    is unchanged (every gate still returns empty/no-match on failure; `synth_syn_fs` is a pure
//!    function of `(g, req, out, word)` with no interaction with the other gates' inputs).

mod common;

use common::load_alpha_grammar;
use pg_featstruct::{FeatId, FeatureStruct, FeatureStructBuilder, FeatureValue, FsId, SymbolBits};
use pg_grammar::model::{
    AffixAllomorphDef, AffixProcessRuleDef, AllomorphId, AllomorphOwner, Grammar, MRuleId,
    MorphRuleDef, MorphRuleOrder, MorphemeId, MprSet, OutputAction, PartRef, Pattern, PatternNode,
    ReduplicationHint, SimpleContext, StratumDef, StratumId, TableId, VarTable,
};
use pg_rules::cache::RuleCache;
use pg_rules::stratum::{synthesize_stratum, synthesize_stratum_traced, StepBudget};
use pg_rules::trace::{
    FailureReason, TraceHandle, TraceSink, TraceSource, TraceType, TreeTraceSink,
};
use pg_rules::Word;
use pg_shape::{NodeKind, Shape, ShapeBuilder};

// ---- shared harness (mirrors `template_partial_gate.rs`) ----------------------------------------

fn shape_with_lanes(g: &Grammar, text: &str) -> Shape {
    let t = &g.char_tables[0];
    let seg = pg_grammar::segment::segment(t, text).expect("segments");
    let w = g.phon_features.len() as u32;
    let mut b = ShapeBuilder::with_features_capacity(w, seg.len());
    for (_, kind, cd, _) in seg.interior() {
        let mut lanes = vec![u64::MAX; w as usize];
        for (i, &l) in t
            .get(pg_grammar::chardef::CharDefId(cd))
            .feature_lanes()
            .iter()
            .enumerate()
        {
            lanes[i] = l;
        }
        match kind {
            NodeKind::Segment => b.push_segment_with_lanes(cd, &lanes),
            NodeKind::Boundary => b.push_boundary_with_lanes(cd, &lanes),
            _ => {}
        }
    }
    b.finish()
}

fn word(g: &Grammar, text: &str, stratum: StratumId) -> Word {
    Word::new(shape_with_lanes(g, text), stratum)
}

fn ctx(nc: &str, g: &Grammar) -> SimpleContext {
    pg_grammar::model::SimpleContext {
        nat_class: common::nat_class(g, nc),
        vars: vec![],
    }
}

fn one_or_more(nc: &str, g: &Grammar) -> Pattern {
    Pattern {
        nodes: vec![PatternNode::Quantifier {
            min: 1,
            max: None,
            children: vec![PatternNode::Context(ctx(nc, g))],
        }],
    }
}

fn insert_segments(g: &Grammar, text: &str) -> OutputAction {
    let shape = pg_grammar::segment::segment(&g.char_tables[0], text).expect("segments");
    OutputAction::InsertSegments {
        table: TableId(0),
        shape: pg_grammar::model::SegmentedText {
            text: text.to_string(),
            shape,
        },
    }
}

/// A one-feature `FeatureStruct`: `FeatId(0) = Symbolic(bits)`. Bypasses the loader's syntactic
/// feature system entirely — `pg_featstruct::ops::{is_unifiable, unify}` only need structurally
/// valid `(FeatId, FeatureValue)` pairs, never `g.syn_features` itself (see `synth_syn_fs`'s own
/// callers: neither reads the feature system for the unify itself, only `add`/`ana_syn_fs` mask
/// against it on the analysis side). Disjoint `bits` between two calls are guaranteed
/// non-unifiable (`SymbolBits::overlaps` on disjoint sets is false).
fn one_feature_fs(bits: u64) -> FeatureStruct {
    let mut b = FeatureStructBuilder::new();
    b.add(FeatId(0), FeatureValue::Symbolic(SymbolBits(bits)));
    b.build()
}

/// Push a single-allomorph, non-template, non-partial `AffixProcess` suffix rule with a caller-
/// supplied `required_syn_fs` (`push_suffix_rule`'s sibling in `template_partial_gate.rs` hardcodes
/// `FsId(0)` / EMPTY; this test needs a real, mismatchable value). Registers the allomorph in
/// `g.allomorph_owners` the way `pg_grammar::load` would, so the CACHED production path
/// (`RuleCache`) can resolve it.
fn push_suffix_rule_with_syn_fs(
    g: &mut Grammar,
    morpheme: u32,
    seg: &str,
    required_syn_fs: FsId,
) -> MRuleId {
    let mrule_id = MRuleId(g.mrules.len() as u32);
    let allo_id = AllomorphId(g.allomorph_owners.len() as u32);
    g.allomorph_owners.push(AllomorphOwner::Affix(mrule_id, 0));
    let rule = MorphRuleDef::AffixProcess(AffixProcessRuleDef {
        morpheme: MorphemeId(morpheme),
        name: None,
        blockable: false,
        partial: false,
        max_apps: 1,
        required_syn_fs,
        out_syn_fs: FsId(0),
        obligatory_features: vec![],
        required_stem_name: None,
        is_template_rule: false,
        allomorphs: vec![AffixAllomorphDef {
            id: allo_id,
            environments: vec![],
            co_occurrence: vec![],
            required_syn_fs: FsId(0),
            vars: VarTable::default(),
            required_mpr: MprSet::EMPTY,
            excluded_mpr: MprSet::EMPTY,
            out_mpr: MprSet::EMPTY,
            redup_hint: ReduplicationHint::Suffix,
            lhs: vec![one_or_more("nc_any", g)],
            rhs: vec![
                OutputAction::Copy(PartRef::Input(0)),
                insert_segments(g, seg),
            ],
            properties: vec![],
        }],
    });
    g.mrules.push(rule);
    mrule_id
}

fn push_stratum(g: &mut Grammar, mrules: Vec<MRuleId>) -> StratumId {
    let id = StratumId(g.strata.len() as u8);
    g.strata.push(StratumDef {
        name: None,
        table: TableId(0),
        mrule_order: MorphRuleOrder::Linear,
        prules: vec![],
        mrules,
        templates: vec![],
        entries: vec![],
    });
    id
}

/// Build the shared fixture: a rule that is NOT a template rule, NOT partial, requires a
/// syntactic FS that will NOT unify with the test word's own — and a word that is ALSO, on its
/// own, disqualified by the final-template-prohibition gate (`is_last_applied_rule_final ==
/// Some(true)`, not partial, rule not partial/not-template). Both gates reject; the only question
/// this file's tests answer is which `FailureReason` the trace reports FIRST.
fn build_fixture() -> (Grammar, StratumId, MRuleId, Word, RuleCache) {
    let mut g = load_alpha_grammar();
    // Disjoint single-bit values on the same (fabricated) `FeatId(0)` -- guaranteed non-unifiable
    // (`SymbolBits::overlaps` is false for disjoint sets), independent of any real POS/head feature
    // system (this hand-built grammar declares none).
    let req_fs = g.fs_interner.intern(one_feature_fs(0b01));
    let r = push_suffix_rule_with_syn_fs(&mut g, 200, "p", req_fs);
    let s = push_stratum(&mut g, vec![r]);
    let cache = RuleCache::build(&g);

    let mut input = word(&g, "a", s);
    input.syn_fs = one_feature_fs(0b10); // disjoint from `req_fs` => the syn-FS gate fails too.
    input.flags.is_last_applied_rule_final = Some(true); // => the final-template gate fails.
    input.flags.is_partial = false;
    input.mrule_apps = vec![Some(r)];
    input.mrule_app_index = 0;

    (g, s, r, input, cache)
}

#[test]
fn both_gates_reject_first_reported_reason_is_the_template_prohibition() {
    let (g, s, r, input, cache) = build_fixture();
    let budget = StepBudget::new(10_000);
    let sink = TreeTraceSink::new();
    let root = sink.generate_words();

    synthesize_stratum_traced(&g, s, input, 10_000, &cache, &budget, &sink, root);

    // Find every `MorphologicalRuleSynthesis` node sourced from `r`'s rule-level gate
    // (`subrule_index == Some(-1)`, `SynthesisAffixProcessRule.cs`'s four rule-level gates) --
    // there must be exactly one (the FIRST gate that actually rejects short-circuits the rest, so
    // the trace records at most one rule-level `MorphologicalRuleNotApplied` node per call --
    // asserted below), and its reason must be the template prohibition, not the syn-FS one.
    let mut rule_level_reasons = Vec::new();
    fn walk(sink: &TreeTraceSink, h: TraceHandle, r: MRuleId, out: &mut Vec<FailureReason>) {
        let n = sink.node(h);
        if n.type_ == TraceType::MorphologicalRuleSynthesis
            && n.source == TraceSource::MorphRule(r)
            && n.subrule_index == Some(-1)
        {
            if let Some(reason) = n.failure_reason {
                out.push(reason);
            }
        }
        for &c in &n.children {
            walk(sink, c, r, out);
        }
    }
    walk(&sink, root, r, &mut rule_level_reasons);

    assert_eq!(
        rule_level_reasons,
        vec![FailureReason::NonPartialRuleProhibitedAfterFinalTemplate],
        "both the syn-FS gate and the final-template-prohibition gate reject this candidate; \
         C# (`SynthesisAffixProcessRule.cs:44-131`) checks the template prohibition BEFORE the \
         syn-FS unify (which is LAST), so that must be the (only) reason reported -- got \
         {rule_level_reasons:?}"
    );
}

#[test]
fn reorder_does_not_change_the_surviving_word_set() {
    // Same fixture, but through the UNTRACED entry point real callers use -- confirms the reorder
    // is trace-only: both gates still reject (empty/None on failure, no side effects consumed
    // between the old and new position -- see `synth_affix_cached`'s doc), so the surviving-word
    // set a full stratum synthesis produces is identical to what it was before G5's reorder. This
    // candidate contributes nothing either way (both gates always failed it, before AND after the
    // fix); what's being pinned is that moving WHEN the syn-FS gate runs doesn't change WHETHER it
    // (or anything else) accepts.
    let (g, s, _r, input, cache) = build_fixture();
    let out = synthesize_stratum(&g, s, input, 10_000, &cache);
    assert!(
        out.is_empty(),
        "the rule must still be rejected (by whichever gate fires first) after the reorder; got \
         {} surviving word(s)",
        out.len()
    );
}

#[test]
fn syn_fs_gate_still_applies_when_every_gate_actually_passes() {
    // The positive-path half of the invariance argument: a candidate where the syn-FS gate is now
    // the LAST check (moved to the end of the function) must still SUCCEED when it actually
    // unifies and no earlier gate rejects -- proving the reorder didn't silently break the
    // success path (e.g. by consuming `new_syn` before it's computed, or skipping the allomorph
    // loop). Same rule/stratum shape as `build_fixture`, but `word.syn_fs` now unifies with
    // `req_fs` (identical single-bit value, not disjoint) and `is_last_applied_rule_final` is
    // `None` (no prior template at all -- neither template gate applies).
    let mut g = load_alpha_grammar();
    let req_fs = g.fs_interner.intern(one_feature_fs(0b01));
    let r = push_suffix_rule_with_syn_fs(&mut g, 201, "p", req_fs);
    let s = push_stratum(&mut g, vec![r]);
    let cache = RuleCache::build(&g);

    let mut input = word(&g, "a", s);
    input.syn_fs = one_feature_fs(0b01); // unifies with `req_fs` (same bit).
    input.mrule_apps = vec![Some(r)];
    input.mrule_app_index = 0;

    let out = synthesize_stratum(&g, s, input, 10_000, &cache);
    assert_eq!(
        out.len(),
        1,
        "the rule must still apply when every gate (including the now-last syn-FS one) passes"
    );
    let chars: Vec<u32> = out[0].shape.interior().map(|(_, _, cd, _)| cd).collect();
    let expected: Vec<u32> = shape_with_lanes(&g, "ap")
        .interior()
        .map(|(_, _, cd, _)| cd)
        .collect();
    assert_eq!(
        chars, expected,
        "the suffix must still be produced correctly"
    );
}
