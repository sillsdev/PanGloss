//! M5c acceptance gate — the compounding-analysis **non-head root filter**, the completion of
//! `AnalysisCompoundingRule.Apply`'s root-allomorph-search gate (AnalysisCompoundingRule.cs:61-125):
//! "for computational complexity reasons, we ensure that the non-head is a root, otherwise we assume
//! it is not a valid analysis and throw it away."
//!
//! `pg-rules` cannot depend on `pg-parse` (crate-boundary: `pg-parse` depends on `pg-rules`, not the
//! reverse — plan §5.1 crate map), so [`pg_rules::stratum::NonHeadRootFilter`] is injected. These
//! tests stand in for `pg-parse`'s `RootAllomorphIndex::search` with a hand-written closure over a
//! tiny hand-built lexicon (`Grammar::entries`), exercising [`analyze_stratum_scoped_filtered`]
//! directly — no need for a real trie.
//!
//! Reuses the compound-rule grammar shape from `morph_gate.rs`'s Part 1 (alpha grammar,
//! head="apa"(1+) / non-head="ka"(1+) split of "apaka").

mod common;

use common::load_alpha_grammar;
use pg_featstruct::{FeatId, FeatureStructBuilder, FeatureValue, FsId, SymbolBits};
use pg_grammar::chardef::CharDefId;
use pg_grammar::model::{
    AllomorphId, CompoundingRuleDef, CompoundingSubruleDef, Grammar, LexEntryDef, LexEntryId,
    MRuleId, MorphRuleDef, MorphRuleOrder, MorphemeId, MorphemeInfo, MprId, MprSet, OutputAction,
    PartRef, Pattern, PatternNode, RootAllomorphDef, SegmentedText, SimpleContext, StratumDef,
    StratumId, TableId, VarTable,
};
use pg_rules::stratum::{
    analyze_stratum, analyze_stratum_scoped_filtered, AnalyzerConfig, NonHeadRootFilter, StepBudget,
};
use pg_rules::Word;
use pg_shape::{NodeKind, Shape, ShapeBuilder};

// ---- shape / grammar plumbing (mirrors morph_gate.rs / stratum_gate.rs) -------------------------

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

fn push_mrule(g: &mut Grammar, rule: MorphRuleDef) -> MRuleId {
    let id = MRuleId(g.mrules.len() as u32);
    g.mrules.push(rule);
    id
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

/// The `head="apa"(1+) + non-head="ka"(1+)` compounding rule (morph_gate.rs's `compound_rule`/
/// `compound_rule_with`), with the non-head syntactic-FS requirement and MPR restriction left
/// open so each test can vary the gate independently.
fn compound_rule_with(
    g: &Grammar,
    non_head_required_syn_fs: FsId,
    non_head_prod_restrictions_mpr: MprSet,
) -> MorphRuleDef {
    MorphRuleDef::Compounding(CompoundingRuleDef {
        xml_id: "c".into(),
        name: None,
        blockable: false,
        max_apps: 1,
        head_required_syn_fs: FsId(0), // EMPTY (interned first by the loader)
        non_head_required_syn_fs,
        out_syn_fs: FsId(0),
        head_prod_restrictions_mpr: MprSet::EMPTY,
        non_head_prod_restrictions_mpr,
        output_prod_restrictions_mpr: MprSet::EMPTY,
        obligatory_features: vec![],
        subrules: vec![CompoundingSubruleDef {
            vars: VarTable::default(),
            required_mpr: MprSet::EMPTY,
            excluded_mpr: MprSet::EMPTY,
            out_mpr: MprSet::EMPTY,
            head_lhs: vec![one_or_more("nc_any", g)],
            non_head_lhs: vec![one_or_more("nc_any", g)],
            rhs: vec![
                OutputAction::Copy(PartRef::Head(0)),
                OutputAction::Copy(PartRef::NonHead(0)),
            ],
        }],
    })
}

/// Push a lexicon entry and return its id. Registers a real [`MorphemeInfo`] too (at
/// `StratumId(0)` — every test in this file calls `push_stratum` exactly once, immediately after,
/// so its stratum is always id 0): plan Tier-2 #7's non-head resolution reads `entry.morpheme`
/// back through `g.morphemes[..].stratum` (mirroring `pg-parse::Morpher::set_root_allomorph`), so
/// a `LexEntryDef` pointing at an unregistered `MorphemeId` would panic on the index, unlike the
/// pre-#7 code path this test suite originally exercised.
fn push_entry(g: &mut Grammar, syn_fs: FsId, mpr: MprSet) -> LexEntryId {
    let morpheme = MorphemeId(g.morphemes.len() as u32);
    g.morphemes.push(MorphemeInfo {
        xml_key: format!("m{}", morpheme.0),
        morph_id: None,
        gloss: None,
        stratum: StratumId(0),
        properties: vec![],
        co_occurrence: vec![],
    });
    let id = LexEntryId(g.entries.len() as u32);
    g.entries.push(LexEntryDef {
        morpheme,
        syn_fs,
        mpr,
        partial: false,
        allomorphs: vec![],
        family: None,
    });
    id
}

/// Attach a single root allomorph (surface `text`, id `AllomorphId(300)` — the sentinel every
/// filter closure in this file returns) to the most recently [`push_entry`]'d entry, so plan
/// Tier-2 #7's resolution (`pg-rules::morph::resolve_non_head_roots`) has a real `RootAllomorphDef`
/// to find and re-segment.
fn push_allomorph(g: &mut Grammar, entry: LexEntryId, text: &str) {
    let shape = shape_with_lanes(g, text);
    g.entries[entry.0 as usize]
        .allomorphs
        .push(RootAllomorphDef {
            id: AllomorphId(300),
            shape: SegmentedText {
                text: text.to_string(),
                shape,
            },
            is_bound: false,
            environments: vec![],
            co_occurrence: vec![],
            properties: vec![],
            stem_name: None,
            is_pattern: false,
        });
}

/// Whether the candidate set contains the head="apa" / non-head="ka" split.
fn has_apa_ka_split(g: &Grammar, out: &[Word]) -> bool {
    let want_head = char_defs(&shape_with_lanes(g, "apa"));
    let want_nh = char_defs(&shape_with_lanes(g, "ka"));
    out.iter().any(|w| {
        char_defs(&w.shape) == want_head
            && w.current_non_head().map(|nh| char_defs(&nh.shape)) == Some(want_nh.clone())
    })
}

/// A disjoint pair of single-feature syntactic FS values (FeatId(0), disjoint symbol bits) — enough
/// to drive a real `is_unifiable` failure without needing a full syntactic feature system.
fn syn_fs(bit: u32) -> pg_featstruct::FeatureStruct {
    let mut b = FeatureStructBuilder::new();
    b.add(FeatId(0), FeatureValue::Symbolic(SymbolBits::single(bit)));
    b.build()
}

// =================================================================================================
// (a) A candidate split whose non-head IS a lexicon root survives.
// =================================================================================================

#[test]
fn split_survives_when_non_head_is_a_lexicon_root() {
    let mut g = load_alpha_grammar();
    let entry = push_entry(&mut g, FsId(0), MprSet::EMPTY);
    push_allomorph(&mut g, entry, "ka");
    let rule = compound_rule_with(&g, FsId(0), MprSet::EMPTY);
    let r = push_mrule(&mut g, rule);
    let s = push_stratum(&mut g, vec![r]);

    let filter: NonHeadRootFilter =
        &|_st, _shape| vec![(pg_grammar::model::AllomorphId(300), entry)];

    let cache = pg_rules::cache::RuleCache::build(&g);
    let out = analyze_stratum_scoped_filtered(
        &g,
        s,
        word(&g, "apaka", s),
        &AnalyzerConfig::default(),
        None,
        Some(filter),
        &cache,
        &StepBudget::new(usize::MAX),
    );
    assert!(!out.capped);
    assert!(
        has_apa_ka_split(&g, &out.words),
        "root found, both sub-checks trivial: split must survive"
    );
}

// =================================================================================================
// (b) A candidate split whose non-head is NOT a root (empty lexicon search) is dropped.
// =================================================================================================

#[test]
fn split_dropped_when_non_head_is_not_a_root() {
    let mut g = load_alpha_grammar();
    let rule = compound_rule_with(&g, FsId(0), MprSet::EMPTY);
    let r = push_mrule(&mut g, rule);
    let s = push_stratum(&mut g, vec![r]);

    let filter: NonHeadRootFilter = &|_st, _shape| Vec::new();

    let cache = pg_rules::cache::RuleCache::build(&g);
    let out = analyze_stratum_scoped_filtered(
        &g,
        s,
        word(&g, "apaka", s),
        &AnalyzerConfig::default(),
        None,
        Some(filter),
        &cache,
        &StepBudget::new(usize::MAX),
    );
    assert!(!out.capped);
    assert!(
        !has_apa_ka_split(&g, &out.words),
        "no matching root in the lexicon: the split must be thrown away, got {:?}",
        out.words
            .iter()
            .map(|w| char_defs(&w.shape))
            .collect::<Vec<_>>()
    );
}

// =================================================================================================
// (b') A root is found, but its MPR features fail `NonHeadProdRestrictionsMprFeatures` — dropped.
// =================================================================================================

#[test]
fn split_dropped_when_root_found_but_mpr_restriction_unsatisfied() {
    let mut g = load_alpha_grammar();
    // The entry carries no MPR features at all.
    let entry = push_entry(&mut g, FsId(0), MprSet::EMPTY);
    // The rule requires one — `CompoundMprFeaturesMatch` needs a nonempty intersection.
    let mut restriction = MprSet::EMPTY;
    restriction.insert(MprId(0));
    let rule = compound_rule_with(&g, FsId(0), restriction);
    let r = push_mrule(&mut g, rule);
    let s = push_stratum(&mut g, vec![r]);

    let filter: NonHeadRootFilter =
        &|_st, _shape| vec![(pg_grammar::model::AllomorphId(300), entry)];

    let cache = pg_rules::cache::RuleCache::build(&g);
    let out = analyze_stratum_scoped_filtered(
        &g,
        s,
        word(&g, "apaka", s),
        &AnalyzerConfig::default(),
        None,
        Some(filter),
        &cache,
        &StepBudget::new(usize::MAX),
    );
    assert!(
        !has_apa_ka_split(&g, &out.words),
        "root exists but fails the MPR productivity-restriction match: split must be dropped"
    );
}

// =================================================================================================
// (b'') A root is found, but its syntactic FS does not unify with
// `NonHeadRequiredSyntacticFeatureStruct` — dropped.
// =================================================================================================

#[test]
fn split_dropped_when_root_found_but_syntactic_fs_conflicts() {
    let mut g = load_alpha_grammar();
    let entry_syn_fs = g.fs_interner.intern(syn_fs(1)); // e.g. "Verb"
    let entry = push_entry(&mut g, entry_syn_fs, MprSet::EMPTY);
    let required_syn_fs = g.fs_interner.intern(syn_fs(0)); // e.g. "Noun" — disjoint bit
    let rule = compound_rule_with(&g, required_syn_fs, MprSet::EMPTY);
    let r = push_mrule(&mut g, rule);
    let s = push_stratum(&mut g, vec![r]);

    let filter: NonHeadRootFilter =
        &|_st, _shape| vec![(pg_grammar::model::AllomorphId(300), entry)];

    let cache = pg_rules::cache::RuleCache::build(&g);
    let out = analyze_stratum_scoped_filtered(
        &g,
        s,
        word(&g, "apaka", s),
        &AnalyzerConfig::default(),
        None,
        Some(filter),
        &cache,
        &StepBudget::new(usize::MAX),
    );
    assert!(
        !has_apa_ka_split(&g, &out.words),
        "root exists but its syntactic FS conflicts with NonHeadRequiredSyntacticFeatureStruct: \
         split must be dropped"
    );
}

// =================================================================================================
// (c) With no filter configured, the split survives regardless of the rule's restrictions —
// backward compatibility with every pre-existing (lexicon-free) pg-rules test.
// =================================================================================================

#[test]
fn unfiltered_backward_compat_ignores_the_gate_entirely() {
    let mut g = load_alpha_grammar();
    // The same "impossible to satisfy" restriction as the MPR-mismatch test above — but with no
    // filter wired in, `apply_one_mrule` never calls `non_head_root_matches` at all.
    let mut restriction = MprSet::EMPTY;
    restriction.insert(MprId(0));
    let rule = compound_rule_with(&g, FsId(0), restriction);
    let r = push_mrule(&mut g, rule);
    let s = push_stratum(&mut g, vec![r]);

    let out = analyze_stratum(
        &g,
        s,
        word(&g, "apaka", s),
        &AnalyzerConfig::default(),
        &StepBudget::new(usize::MAX),
    );
    assert!(!out.capped);
    assert!(
        has_apa_ka_split(&g, &out.words),
        "no filter configured: the split must survive exactly as before M5c"
    );
}
