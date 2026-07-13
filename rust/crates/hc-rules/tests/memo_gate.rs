//! M6 acceptance gate — the #451 order-invariant analysis-cascade memoization.
//!
//! Two properties, on a hand-built tiny grammar where the whole candidate set is reasoned by hand:
//! - **memo-on == memo-off**: the memo must not change the result set (the core correctness
//!   invariant — replay reconstructs exactly what the plain combination walk produces).
//! - **the memo actually fires**: after a run, both the mrule memo and the template memo hold entries
//!   (positive-replay and nogood), proving the paths were exercised rather than silently skipped.
//!
//! The order-invariant-key and replay-graft unit tests live in `hc-memo` / `hc-rules::word`.

mod common;

use common::load_alpha_grammar;
use hc_grammar::chardef::CharDefId;
use hc_grammar::model::{
    AffixAllomorphDef, AffixProcessRuleDef, AffixTemplateDef, AllomorphId, Grammar, MRuleId,
    MorphRuleDef, MorphRuleOrder, MorphemeId, MprSet, OutputAction, PartRef, Pattern, PatternNode,
    ReduplicationHint, SegmentedText, SimpleContext, SlotDef, StratumDef, StratumId, TableId,
    TemplateId, VarTable,
};
use hc_memo::AnalysisScope;
use hc_rules::stratum::{
    analyze_stratum, analyze_stratum_scoped, AnalyzerConfig, MemoScope, StepBudget,
};
use hc_rules::Word;
use hc_shape::{NodeKind, Shape, ShapeBuilder};
use std::cell::RefCell;

// ---- builders (mirror stratum_gate.rs) ------------------------------------------------------

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

fn ctx(nc: &str, g: &Grammar) -> SimpleContext {
    common::ctx(common::nat_class(g, nc))
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
    let shape = hc_grammar::segment::segment(&g.char_tables[0], text).expect("segments");
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

fn suffix_rule(g: &Grammar, morpheme: u32, seg: &str) -> MorphRuleDef {
    MorphRuleDef::AffixProcess(AffixProcessRuleDef {
        morpheme: MorphemeId(morpheme),
        name: None,
        blockable: false,
        partial: false,
        max_apps: 1,
        required_syn_fs: hc_featstruct::FsId(0),
        out_syn_fs: hc_featstruct::FsId(0),
        obligatory_features: vec![],
        required_stem_name: None,
        is_template_rule: false,
        allomorphs: vec![allomorph(
            morpheme,
            vec![one_or_more("nc_any", g)],
            vec![
                OutputAction::Copy(PartRef::Input(0)),
                insert_segments(g, seg),
            ],
        )],
    })
}

fn push_mrule(g: &mut Grammar, rule: MorphRuleDef) -> MRuleId {
    let id = MRuleId(g.mrules.len() as u32);
    g.mrules.push(rule);
    id
}

fn push_template(g: &mut Grammar, slots: Vec<SlotDef>) -> TemplateId {
    let id = TemplateId(g.templates.len() as u32);
    g.templates.push(AffixTemplateDef {
        name: None,
        required_syn_fs: hc_featstruct::FsId(0),
        is_final: true,
        slots,
    });
    id
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

fn word(g: &Grammar, text: &str, stratum: StratumId) -> Word {
    Word::new(shape_with_lanes(g, text), stratum)
}

fn candidate_shapes(words: &[Word]) -> Vec<Vec<u32>> {
    let mut v: Vec<Vec<u32>> = words.iter().map(|w| char_defs(&w.shape)).collect();
    v.sort();
    v.dedup();
    v
}

// =================================================================================================
// memo-on == memo-off on the non-commuting Unordered stratum (the k!-walk case).
// =================================================================================================

fn build_unordered() -> (Grammar, StratumId) {
    let mut g = load_alpha_grammar();
    let (ra, rb) = (suffix_rule(&g, 200, "p"), suffix_rule(&g, 300, "k"));
    let a = push_mrule(&mut g, ra);
    let b = push_mrule(&mut g, rb);
    let s = push_stratum(&mut g, MorphRuleOrder::Unordered, vec![a, b], vec![]);
    (g, s)
}

#[test]
fn memo_on_equals_memo_off_unordered() {
    let (g, s) = build_unordered();
    let cfg = AnalyzerConfig::default();

    let off = analyze_stratum(
        &g,
        s,
        word(&g, "akp", s),
        &cfg,
        &StepBudget::new(usize::MAX),
    );
    let scope: MemoScope = RefCell::new(AnalysisScope::new());
    let on = analyze_stratum_scoped(
        &g,
        s,
        word(&g, "akp", s),
        &cfg,
        Some(&scope),
        &StepBudget::new(usize::MAX),
    );

    assert!(!off.capped && !on.capped);
    assert_eq!(
        candidate_shapes(&off.words),
        candidate_shapes(&on.words),
        "memo must not change the candidate set"
    );
    // The full set is still { akp (seed), ak, a } — memo did not drop the deep root.
    assert!(candidate_shapes(&on.words).contains(&vec![common::char_def(&g, "char_a").0]));

    // The memo actually fired: at least one mrule-memo entry was stored during expansion, and it
    // includes a nogood (a state from which no rule unapplies — e.g. the bare root "a").
    let sc = scope.borrow();
    assert!(!sc.memo.is_empty(), "mrule memo must hold entries");
    assert!(
        sc.memo.values().any(|e| !e.is_positive()),
        "expected at least one nogood entry (a leaf state)"
    );
    assert!(
        sc.memo.values().any(|e| e.is_positive()),
        "expected at least one positive entry (a state with a subtree)"
    );
}

// =================================================================================================
// memo-on == memo-off with an affix template in the mix (exercises the TemplateMemo table).
// =================================================================================================

#[test]
fn memo_on_equals_memo_off_with_template() {
    let mut g = load_alpha_grammar();
    // One optional-slot template over a strip-"p" rule, plus an unordered strip-"k" mrule, so both
    // the mrule memo and the template memo get exercised on the same parse.
    let tp = suffix_rule(&g, 200, "p");
    let tp_id = push_mrule(&mut g, tp);
    let slot = SlotDef {
        name: None,
        optional: true,
        rules: vec![tp_id],
    };
    let tmpl = push_template(&mut g, vec![slot]);
    let rk = suffix_rule(&g, 300, "k");
    let rk_id = push_mrule(&mut g, rk);
    let s = push_stratum(&mut g, MorphRuleOrder::Unordered, vec![rk_id], vec![tmpl]);

    let cfg = AnalyzerConfig::default();
    let off = analyze_stratum(
        &g,
        s,
        word(&g, "akp", s),
        &cfg,
        &StepBudget::new(usize::MAX),
    );
    let scope: MemoScope = RefCell::new(AnalysisScope::new());
    let on = analyze_stratum_scoped(
        &g,
        s,
        word(&g, "akp", s),
        &cfg,
        Some(&scope),
        &StepBudget::new(usize::MAX),
    );

    assert!(!off.capped && !on.capped);
    assert_eq!(
        candidate_shapes(&off.words),
        candidate_shapes(&on.words),
        "memo (with a template) must not change the candidate set"
    );
    assert!(
        !scope.borrow().template_memo.is_empty(),
        "template memo must hold entries"
    );
}
