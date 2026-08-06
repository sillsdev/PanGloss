//! The three template/partial synthesis gates added to `stratum.rs`/`morph.rs`.
//! See `docs/research/pg-rules-template-partial-gate-design-notes.md`.

mod common;

use common::load_alpha_grammar;
use pg_grammar::chardef::CharDefId;
use pg_grammar::model::{
    AffixAllomorphDef, AffixProcessRuleDef, AffixTemplateDef, AllomorphId, AllomorphOwner, Grammar,
    LexEntryDef, LexEntryId, MRuleId, MorphRuleDef, MorphRuleOrder, MorphemeId, MprSet,
    OutputAction, PartRef, Pattern, PatternNode, ReduplicationHint, RootAllomorphDef,
    SegmentedText, SimpleContext, SlotDef, StratumDef, StratumId, TableId, TemplateId, VarTable,
};
use pg_rules::cache::RuleCache;
use pg_rules::stratum::synthesize_stratum;
use pg_rules::Word;
use pg_shape::{NodeKind, Shape, ShapeBuilder};

// ---- shared harness (mirrors morph_gate.rs / stratum_gate.rs) -----------------------------------

fn shape_with_lanes(g: &Grammar, text: &str) -> Shape {
    let t = &g.char_tables[0];
    let seg = pg_grammar::segment::segment(t, text).expect("segments");
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
    let shape = pg_grammar::segment::segment(&g.char_tables[0], text).expect("segments");
    OutputAction::InsertSegments {
        table: TableId(0),
        shape: SegmentedText {
            text: text.to_string(),
            shape,
        },
    }
}

fn allomorph(id: u32, lhs: Vec<Pattern>, rhs: Vec<OutputAction>) -> AffixAllomorphDef {
    AffixAllomorphDef {
        id: AllomorphId(id),
        environments: vec![],
        co_occurrence: vec![],
        required_syn_fs: pg_featstruct::FsId(0),
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

/// Push a single-allomorph AffixProcess suffix rule, registering its allomorph in `g.allomorph_owners` the way `pg_grammar::load` would.
/// See `docs/research/pg-rules-template-partial-gate-design-notes.md`.
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
        required_syn_fs: pg_featstruct::FsId(0),
        out_syn_fs: pg_featstruct::FsId(0),
        obligatory_features: vec![],
        required_stem_name: None,
        is_template_rule: false,
        allomorphs: vec![allomorph(
            allo_id.0,
            vec![one_or_more("nc_any", g)],
            vec![
                OutputAction::Copy(PartRef::Input(0)),
                insert_segments(g, seg),
            ],
        )],
    });
    g.mrules.push(rule);
    mrule_id
}

/// Retroactively tag `mid` as a template-slot rule, standing in for `pg_grammar::load`'s own post-pass.
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
        required_syn_fs: pg_featstruct::FsId(0),
        slots: vec![SlotDef {
            name: None,
            optional: false,
            rules: vec![slot_rule],
        }],
    });
    id
}

/// A lexicon root entry registered through `allomorph_owners` the way the loader would.
/// See `docs/research/pg-rules-template-partial-gate-design-notes.md`.
fn push_root_entry(g: &mut Grammar, partial: bool) -> AllomorphId {
    let allo_id = AllomorphId(g.allomorph_owners.len() as u32);
    let lex_id = LexEntryId(g.entries.len() as u32);
    g.allomorph_owners.push(AllomorphOwner::Root(lex_id, 0));
    let shape = pg_grammar::segment::segment(&g.char_tables[0], "a").expect("segments");
    g.entries.push(LexEntryDef {
        authored_id: format!("test-entry-{}", lex_id.0),
        morpheme: MorphemeId(900),
        syn_fs: pg_featstruct::FsId(0),
        mpr: MprSet::EMPTY,
        partial,
        allomorphs: vec![RootAllomorphDef {
            id: allo_id,
            shape: SegmentedText {
                text: "a".to_string(),
                shape,
            },
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

// Gate 1: partial-word passthrough (`stratum.rs::synth_apply_templates`).

#[test]
fn gate1_partial_word_with_applicable_template_passes_through() {
    // Isolates gate 1 (passthrough) from gates 2/3: the template is applicable but unproductive.
    // See `docs/research/pg-rules-template-partial-gate-design-notes.md`.
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
    // `synthesize_stratum` clears `is_last_applied_rule_final` on every surviving candidate (cs:82).
    assert_eq!(
        char_defs(&out[0].shape),
        char_defs(&shape_with_lanes(&g, "a"))
    );
}

// Gate 2: partial-root blocks template applicability (`stratum.rs::root_is_partial`).

#[test]
fn gate2_partial_root_morpheme_blocks_template_application() {
    // Isolates gate 2 from gate 1: the root, not the word, is marked partial.
    // See `docs/research/pg-rules-template-partial-gate-design-notes.md`.
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
    assert_eq!(
        char_defs(&out[0].shape),
        char_defs(&shape_with_lanes(&g, "a"))
    );
}

// Gate 3: partial rule prohibited after a non-final template, unless the input is itself partial.

#[test]
fn gate3_partial_rule_prohibited_after_nonfinal_template_unless_input_partial() {
    // Hand-sets the post-non-final-template state to isolate gate 3 from gates 1/2's template machinery.
    // See `docs/research/pg-rules-template-partial-gate-design-notes.md`.
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

    // (B) already-partial input: the same rule must be allowed and produce the suffixed word.
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
    assert_eq!(
        char_defs(&out_b[0].shape),
        char_defs(&shape_with_lanes(&g, "ap"))
    );
}

// Gate 4: `IsTemplateRule` exempts a template-slot member from gate 3's post-template partial check.

#[test]
fn gate4_template_rule_is_exempt_from_the_post_template_gates() {
    // Same state as gate 3's case (A), but the rule is also tagged `is_template_rule`.
    // See `docs/research/pg-rules-template-partial-gate-design-notes.md`.
    let mut g = load_alpha_grammar();
    let r = push_suffix_rule(&mut g, 200, "p", true); // a PARTIAL rule...
    mark_template_rule(&mut g, r); // ...that is ALSO a template-slot member.
    let s = push_stratum(&mut g, MorphRuleOrder::Linear, vec![r], vec![]);
    let cache = RuleCache::build(&g);

    let mut input = word(&g, "a", s);
    input.flags.is_last_applied_rule_final = Some(false);
    // is_partial stays false -- IsTemplateRule must exempt this rule regardless.
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
    assert_eq!(
        char_defs(&out[0].shape),
        char_defs(&shape_with_lanes(&g, "ap"))
    );
}
