//! Tier-2 #13 acceptance gate — the three template/partial synthesis gates added to
//! `stratum.rs::synth_apply_templates` and `morph.rs::synth_affix{,_cached}`:
//!
//! - **Gate 1** (`SynthesisAffixTemplatesRule.cs:59-77`): when no template produces output, C#
//!   passes the input through (marked final) UNLESS it is non-partial AND some template was
//!   applicable (in which case it is dropped, only traced as `ApplicableTemplatesNotApplied`).
//! - **Gate 2** (`SynthesisAffixTemplatesRule.cs:37-41`): a template only counts as applicable if,
//!   among the other conditions, the word's ROOT MORPHEME is not itself partial
//!   (`input.RootAllomorph.Morpheme.IsPartial`) — distinct from `Word.IsPartial`.
//! - **Gate 3** (`SynthesisAffixProcessRule.cs:86-105`): right after a *non-final* template applied,
//!   a rule may run only if it is itself partial or the input is already partial — i.e. a
//!   *non*-partial input blocks a *partial* rule immediately following a non-final template.
//!
//! Gates 1 and 2 are exercised through the real public entry point `stratum::synthesize_stratum`
//! (the only way to reach the private `synth_apply_templates`), each isolated from the other two by
//! construction (see per-test comments). Gate 3 is exercised the same way but additionally routes
//! through the cached production path (`synthesize_stratum` -> `synth_apply_mrules` ->
//! `guided_synth` -> `synthesize_cached` -> `synth_affix_cached`) by hand-setting the
//! `IsLastAppliedRuleFinal == Some(false)` state a word carries immediately after a non-final
//! template applied — the same state `SynthesisAffixTemplatesRule.cs:44-49` produces — without
//! needing to actually run a template first.

mod common;

use common::load_alpha_grammar;
use hc_grammar::chardef::CharDefId;
use hc_grammar::model::{
    AffixAllomorphDef, AffixProcessRuleDef, AffixTemplateDef, AllomorphId, AllomorphOwner,
    Grammar, LexEntryDef, LexEntryId, MRuleId, MorphRuleDef, MorphRuleOrder, MorphemeId, MprSet,
    OutputAction, PartRef, Pattern, PatternNode, ReduplicationHint, RootAllomorphDef, SegmentedText,
    SimpleContext, SlotDef, StratumDef, StratumId, TableId, TemplateId, VarTable,
};
use hc_rules::cache::RuleCache;
use hc_rules::stratum::synthesize_stratum;
use hc_rules::Word;
use hc_shape::{NodeKind, Shape, ShapeBuilder};

// ---- shared harness (mirrors morph_gate.rs / stratum_gate.rs) -----------------------------------

fn shape_with_lanes(g: &Grammar, text: &str) -> Shape {
    let t = &g.char_tables[0];
    let seg = hc_grammar::segment::segment(t, text).expect("segments");
    let w = g.phon_features.len() as u32;
    let mut b = ShapeBuilder::with_features_capacity(w, seg.len());
    for (_, kind, cd, _) in seg.interior() {
        let mut lanes = vec![u64::MAX; w as usize];
        for (i, &l) in t.get(CharDefId(cd)).feature_lanes().iter().enumerate() {
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

fn char_defs(shape: &Shape) -> Vec<u32> {
    shape.interior().map(|(_, _, cd, _)| cd).collect()
}

fn word(g: &Grammar, text: &str, stratum: StratumId) -> Word {
    Word::new(shape_with_lanes(g, text), stratum)
}

fn ctx(nc: &str, g: &Grammar) -> SimpleContext {
    common::ctx(common::nat_class(g, nc))
}

/// `X+` (one-or-more) over a natural class.
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
    let shape = hc_grammar::segment::segment(&g.char_tables[0], text).expect("segments");
    OutputAction::InsertSegments {
        table: TableId(0),
        shape: SegmentedText { text: text.to_string(), shape },
    }
}

fn allomorph(id: u32, lhs: Vec<Pattern>, rhs: Vec<OutputAction>) -> AffixAllomorphDef {
    AffixAllomorphDef {
        id: AllomorphId(id),
        environments: vec![],
        co_occurrence: vec![],
        required_syn_fs: hc_featstruct::FsId(0),
        vars: VarTable::default(),
        required_mpr: MprSet::EMPTY,
        excluded_mpr: MprSet::EMPTY,
        out_mpr: MprSet::EMPTY,
        redup_hint: ReduplicationHint::Suffix,
        lhs,
        rhs,
        properties: vec![],
    }
}

/// Push a single-allomorph AffixProcess suffix rule (`CopyFromInput(0) + InsertSegments(seg)`),
/// registering its allomorph in `g.allomorph_owners` the way `hc_grammar::load` would
/// (`AllomorphOwner::Affix(mrule_id, 0)` at the next sequential `AllomorphId`). This is required for
/// the cached production path: `RuleCache::build` eagerly compiles every registered allomorph and
/// `synth_affix_cached`/`analyze_cached` look matchers up by `AllomorphId` through that registry —
/// an allomorph minted with an arbitrary id (as the earlier, uncached-only test files in this crate
/// do; see `morph_gate.rs`'s `allomorph` helper) is never resolvable through `RuleCache`.
fn push_suffix_rule(g: &mut Grammar, morpheme: u32, seg: &str, partial: bool) -> MRuleId {
    let mrule_id = MRuleId(g.mrules.len() as u32);
    let allo_id = AllomorphId(g.allomorph_owners.len() as u32);
    g.allomorph_owners.push(AllomorphOwner::Affix(mrule_id, 0));
    let rule = MorphRuleDef::AffixProcess(AffixProcessRuleDef {
        morpheme: MorphemeId(morpheme),
        name: None,
        blockable: false,
        partial,
        max_apps: 1,
        required_syn_fs: hc_featstruct::FsId(0),
        out_syn_fs: hc_featstruct::FsId(0),
        obligatory_features: vec![],
        required_stem_name: None,
        is_template_rule: false,
        allomorphs: vec![allomorph(
            allo_id.0,
            vec![one_or_more("nc_any", g)],
            vec![OutputAction::Copy(PartRef::Input(0)), insert_segments(g, seg)],
        )],
    });
    g.mrules.push(rule);
    mrule_id
}

/// Retroactively tag `mid` as a template-slot rule, mirroring what `hc_grammar::load`'s
/// `IsTemplateRule` post-pass would do for a rule actually referenced from an
/// `AffixTemplateSlot` — these hand-built fixtures construct rules and templates independently of
/// the real loader, so this stands in for that post-pass.
fn mark_template_rule(g: &mut Grammar, mid: MRuleId) {
    if let MorphRuleDef::AffixProcess(def) = &mut g.mrules[mid.0 as usize] {
        def.is_template_rule = true;
    }
}

fn push_stratum(
    g: &mut Grammar,
    order: MorphRuleOrder,
    mrules: Vec<MRuleId>,
    templates: Vec<TemplateId>,
) -> StratumId {
    let id = StratumId(g.strata.len() as u8);
    g.strata.push(StratumDef {
        name: None,
        table: TableId(0),
        mrule_order: order,
        prules: vec![],
        mrules,
        templates,
        entries: vec![],
    });
    id
}

/// A one-slot (mandatory) template referencing `slot_rule`.
fn push_template(g: &mut Grammar, is_final: bool, slot_rule: MRuleId) -> TemplateId {
    let id = TemplateId(g.templates.len() as u32);
    g.templates.push(AffixTemplateDef {
        name: None,
        is_final,
        required_syn_fs: hc_featstruct::FsId(0),
        slots: vec![SlotDef { name: None, optional: false, rules: vec![slot_rule] }],
    });
    id
}

/// A lexicon root entry registered through `allomorph_owners` the way the loader would.
/// `root_is_partial` only reads `entries[..].partial`, but `RuleCache::build` eagerly walks every
/// registered owner (including `Root` ones) and indexes into `entries[le].allomorphs[idx]`, so a
/// real (if trivial) `RootAllomorphDef` must back the registration or the cache build panics.
fn push_root_entry(g: &mut Grammar, partial: bool) -> AllomorphId {
    let allo_id = AllomorphId(g.allomorph_owners.len() as u32);
    let lex_id = LexEntryId(g.entries.len() as u32);
    g.allomorph_owners.push(AllomorphOwner::Root(lex_id, 0));
    let shape = hc_grammar::segment::segment(&g.char_tables[0], "a").expect("segments");
    g.entries.push(LexEntryDef {
        morpheme: MorphemeId(900),
        syn_fs: hc_featstruct::FsId(0),
        mpr: MprSet::EMPTY,
        partial,
        allomorphs: vec![RootAllomorphDef {
            id: allo_id,
            shape: SegmentedText { text: "a".to_string(), shape },
            is_bound: false,
            environments: vec![],
            co_occurrence: vec![],
            properties: vec![],
            stem_name: None,
            is_pattern: false,
        }],
        family: None,
    });
    allo_id
}

// =================================================================================================
// Gate 1 — partial-word passthrough (stratum.rs `synth_apply_templates`).
// =================================================================================================

#[test]
fn gate1_partial_word_with_applicable_template_passes_through() {
    // One template whose required FS is trivially satisfied (empty FS on both sides) — so it IS
    // applicable — but whose single mandatory slot rule can never actually apply: the word starts
    // with no confirmed unapplication trail (`mrule_app_index == -1`, `Word::new`'s default), and
    // `guided_synth` refuses to apply anything without one (`w.mrule_app_index < 0` short-circuits
    // before even inspecting the rule). So the template always yields zero output here, isolating
    // gate 1 (the passthrough condition) from gate 2 (root-partial) and gate 3 (post-template rule
    // gating): `applicable = true`, the internal `out` map stays empty, and the only question left
    // is whether the passthrough fires.
    //
    // Before the fix the passthrough condition was `!applicable` alone, so an applicable-but-
    // unproductive template on a partial word was (wrongly) dropped instead of passed through.
    let mut g = load_alpha_grammar();
    let r = push_suffix_rule(&mut g, 200, "p", false);
    let tid = push_template(&mut g, true, r);
    let s = push_stratum(&mut g, MorphRuleOrder::Linear, vec![], vec![tid]);

    let mut input = word(&g, "a", s);
    input.flags.is_partial = true; // the word itself is partial
    // root_allomorph stays None => root_is_partial(g, input) == false => the template IS applicable.

    let cache = RuleCache::build(&g);
    let out = synthesize_stratum(&g, s, input, 10_000, &cache);
    assert_eq!(
        out.len(),
        1,
        "a partial word with an applicable-but-unapplied template must pass through, not be dropped"
    );
    // `synthesize_stratum` clears `is_last_applied_rule_final` to `None` on every surviving
    // candidate (cs:82, "clear the final flag" — the flag is stratum-internal orchestration state,
    // not part of a stratum's public output); the passthrough itself sets it `Some(true)` internally
    // (asserted indirectly by this word surviving `synthesize_stratum`'s own
    // `is_last_applied_rule_final != Some(true)` filter at all).
    assert_eq!(char_defs(&out[0].shape), char_defs(&shape_with_lanes(&g, "a")));
}

// =================================================================================================
// Gate 2 — partial-root blocks template applicability (stratum.rs `root_is_partial` / the
// `root_partial` short-circuit in `synth_apply_templates`'s per-template loop).
// =================================================================================================

#[test]
fn gate2_partial_root_morpheme_blocks_template_application() {
    // Same template as gate 1 (required FS trivially satisfied, slot rule never actually reachable
    // without an unapplication trail), but now `input.flags.is_partial` stays FALSE and instead the
    // word's ROOT is marked partial via a registered lexicon entry. With gate 2 active the template
    // is skipped before ever being counted `applicable` (the `root_partial` short-circuit), so
    // `applicable` stays false and the passthrough's `!applicable` disjunct fires unconditionally,
    // returning the untemplated word unchanged.
    //
    // Reverting gate 2 (dropping `root_partial` from the per-template `continue` condition) flips
    // `applicable` to true (the required FS is still trivially satisfiable) — and because the word
    // is *not* itself partial, gate 1 (still active, unaffected by this revert) now refuses the
    // passthrough (non-partial input + applicable template => drop, matching
    // `ApplicableTemplatesNotApplied`), while the slot rule still can't actually apply (no
    // unapplication trail). Net effect of reverting gate 2: the result collapses from 1 candidate to
    // 0 — a clean revert-to-red signal.
    let mut g = load_alpha_grammar();
    let r = push_suffix_rule(&mut g, 200, "p", false);
    let tid = push_template(&mut g, true, r);
    let s = push_stratum(&mut g, MorphRuleOrder::Linear, vec![], vec![tid]);
    let root = push_root_entry(&mut g, true); // partial root entry

    let mut input = word(&g, "a", s);
    input.root_allomorph = Some(root);
    // input.flags.is_partial stays false — isolates gate 2 from gate 1.

    let cache = RuleCache::build(&g);
    let out = synthesize_stratum(&g, s, input, 10_000, &cache);
    assert_eq!(
        out.len(),
        1,
        "a word whose root morpheme is partial must pass through untemplated, not be dropped"
    );
    assert_eq!(char_defs(&out[0].shape), char_defs(&shape_with_lanes(&g, "a")));
}

// =================================================================================================
// Gate 3 — partial rule prohibited right after a non-final template, unless the input is itself
// partial (morph.rs `synth_affix` / `synth_affix_cached`).
// =================================================================================================

#[test]
fn gate3_partial_rule_prohibited_after_nonfinal_template_unless_input_partial() {
    // `IsLastAppliedRuleFinal == Some(false)` is exactly the state a word carries immediately after
    // a non-final template applied (`SynthesisAffixTemplatesRule.cs:44-49`); hand-setting it here
    // (plus a one-entry confirmed unapplication trail so `guided_synth` will actually attempt the
    // rule) isolates gate 3 from gates 1/2's template machinery while still driving the REAL cached
    // production pipeline: `synthesize_stratum` -> `synth_apply_mrules` -> `guided_synth` ->
    // `synthesize_cached` -> `synth_affix_cached`.
    let mut g = load_alpha_grammar();
    let r = push_suffix_rule(&mut g, 200, "p", true); // a PARTIAL affix rule
    let s = push_stratum(&mut g, MorphRuleOrder::Linear, vec![r], vec![]);
    let cache = RuleCache::build(&g);

    // (A) non-partial input: the partial rule must be prohibited entirely (no candidate survives).
    let mut input_a = word(&g, "a", s);
    input_a.flags.is_last_applied_rule_final = Some(false);
    input_a.mrule_apps = vec![Some(r)];
    input_a.mrule_app_index = 0;
    let out_a = synthesize_stratum(&g, s, input_a, 10_000, &cache);
    assert!(
        out_a.is_empty(),
        "a partial rule must be prohibited right after a non-final template on a non-partial word; got {:?}",
        out_a.iter().map(|w| char_defs(&w.shape)).collect::<Vec<_>>()
    );

    // (B) already-partial input: the same rule must be ALLOWED (the exception clause) and produce
    // the suffixed word.
    let mut input_b = word(&g, "a", s);
    input_b.flags.is_last_applied_rule_final = Some(false);
    input_b.flags.is_partial = true;
    input_b.mrule_apps = vec![Some(r)];
    input_b.mrule_app_index = 0;
    let out_b = synthesize_stratum(&g, s, input_b, 10_000, &cache);
    assert_eq!(
        out_b.len(),
        1,
        "the same partial rule must be allowed to apply when the input is already partial"
    );
    assert_eq!(char_defs(&out_b[0].shape), char_defs(&shape_with_lanes(&g, "ap")));
}

// =================================================================================================
// Gate 4 (plan §6 item 6 / W1.6) — `IsTemplateRule`: a rule that IS itself a template-slot member
// must be EXEMPT from gate 3's post-non-final-template partial check, unlike an ordinary rule.
// C# `SynthesisAffixProcessRule.cs:64,86`'s `!_rule.IsTemplateRule &&` guard applies to both
// checks identically; this test isolates the second (gate 3's exact shape) since it's the one
// with an existing, directly-invertible sibling test (gate 3 above) to contrast against.
// =================================================================================================

#[test]
fn gate4_template_rule_is_exempt_from_the_post_template_gates() {
    // Same non-final-template state and PARTIAL rule as gate 3's case (A) above -- which, for an
    // ORDINARY rule, is prohibited outright on a non-partial word. Here the rule is ALSO tagged
    // `is_template_rule` (as `hc_grammar::load`'s post-pass would tag any rule referenced from an
    // `AffixTemplateSlot`), so it must NOT be gated at all, regardless of the word's partial state.
    // Before this fix `synth_affix_cached` applied the check unconditionally to every affix rule.
    let mut g = load_alpha_grammar();
    let r = push_suffix_rule(&mut g, 200, "p", true); // a PARTIAL rule...
    mark_template_rule(&mut g, r); // ...that is ALSO a template-slot member.
    let s = push_stratum(&mut g, MorphRuleOrder::Linear, vec![r], vec![]);
    let cache = RuleCache::build(&g);

    let mut input = word(&g, "a", s);
    input.flags.is_last_applied_rule_final = Some(false);
    // input.flags.is_partial stays FALSE -- gate 3 alone would prohibit this rule outright (see
    // gate3's case A); IsTemplateRule must exempt it regardless.
    input.mrule_apps = vec![Some(r)];
    input.mrule_app_index = 0;
    let out = synthesize_stratum(&g, s, input, 10_000, &cache);
    assert_eq!(
        out.len(),
        1,
        "a template-slot rule must be exempt from the post-non-final-template partial gate, \
         even on a non-partial word; got {:?}",
        out.iter().map(|w| char_defs(&w.shape)).collect::<Vec<_>>()
    );
    assert_eq!(char_defs(&out[0].shape), char_defs(&shape_with_lanes(&g, "ap")));
}
