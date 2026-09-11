//! Order-invariant analysis-cascade memoization gate, on a hand-built tiny grammar where the whole candidate set is reasoned by hand: memo-on must equal memo-off (replay reconstructs exactly what the plain walk produces), and the memo must actually fire (both mrule and template memos hold entries).

mod common;

use common::load_alpha_grammar;
use pg_grammar::chardef::CharDefId;
use pg_grammar::model::{
    AffixAllomorphDef, AffixProcessRuleDef, AffixTemplateDef, AllomorphId, Grammar, MRuleId,
    MorphRuleDef, MorphRuleOrder, MorphemeId, MprSet, OutputAction, PartRef, Pattern, PatternNode,
    ReduplicationHint, SegmentedText, SimpleContext, SlotDef, StratumDef, StratumId, TableId,
    TemplateId, TemplateSlotZone, VarTable,
};
use pg_memo::AnalysisScope;
use pg_rules::stratum::{
    analyze_stratum, analyze_stratum_scoped,
    analyze_stratum_scoped_filtered_ruled_traced_with_policy, AnalyzerConfig,
    FinalTemplateAnalysisPolicy, MemoScope, StepBudget,
};
use pg_rules::trace::{NoopSink, TraceHandle};
use pg_rules::Word;
use pg_shape::{NodeKind, Shape, ShapeBuilder};
use std::cell::RefCell;

// ---- builders (mirror stratum_gate.rs) ------------------------------------------------------

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

fn suffix_rule(g: &Grammar, morpheme: u32, seg: &str) -> MorphRuleDef {
    MorphRuleDef::AffixProcess(AffixProcessRuleDef {
        morpheme: MorphemeId(morpheme),
        name: None,
        blockable: false,
        partial: false,
        max_apps: 1,
        required_syn_fs: pg_featstruct::FsId(0),
        out_syn_fs: pg_featstruct::FsId(0),
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

fn prefix_rule(g: &Grammar, morpheme: u32, seg: &str) -> MorphRuleDef {
    let mut rule = suffix_rule(g, morpheme, seg);
    if let MorphRuleDef::AffixProcess(def) = &mut rule {
        def.allomorphs[0].redup_hint = ReduplicationHint::Prefix;
        def.allomorphs[0].rhs.swap(0, 1);
    }
    rule
}

fn push_mrule(g: &mut Grammar, rule: MorphRuleDef) -> MRuleId {
    let id = MRuleId(g.mrules.len() as u32);
    g.mrules.push(rule);
    id
}

fn push_template(g: &mut Grammar, slots: Vec<SlotDef>) -> TemplateId {
    push_template_with_final(g, slots, true)
}

fn push_template_with_final(g: &mut Grammar, slots: Vec<SlotDef>, is_final: bool) -> TemplateId {
    let id = TemplateId(g.templates.len() as u32);
    g.templates.push(AffixTemplateDef {
        name: None,
        required_syn_fs: pg_featstruct::FsId(0),
        is_final,
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

// memo-on == memo-off on the non-commuting Unordered stratum (the k!-walk case).
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

    // The memo actually fired: at least one entry was stored, including a nogood (a state from which no rule unapplies).
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

// Saturating the key at max_apps must let word_b safely hit word_a's memo entry despite differing raw counts (1 vs. 5), reproducing memo-off exactly.
#[test]
fn state_key_saturates_unapplication_counts_past_max_apps() {
    let mut g = load_alpha_grammar();
    let (ra, rb) = (suffix_rule(&g, 200, "p"), suffix_rule(&g, 300, "k"));
    let a_id = push_mrule(&mut g, ra);
    let _b_id = push_mrule(&mut g, rb);
    let s = push_stratum(&mut g, MorphRuleOrder::Unordered, vec![a_id, _b_id], vec![]);
    let cfg = AnalyzerConfig::default();

    let mut word_a = word(&g, "akp", s);
    word_a.unapplied_rule_counts.insert(a_id, 1); // exactly at max_apps
    let mut word_b = word(&g, "akp", s);
    word_b.unapplied_rule_counts.insert(a_id, 5); // past max_apps -- e.g. Word::expand_alternatives' reconstruction

    // Ground truth: unmemoized, each word searched independently.
    let off_a = analyze_stratum(&g, s, word_a.clone(), &cfg, &StepBudget::new(usize::MAX));
    let off_b = analyze_stratum(&g, s, word_b.clone(), &cfg, &StepBudget::new(usize::MAX));
    assert!(!off_a.capped && !off_b.capped);
    assert_eq!(
        candidate_shapes(&off_a.words),
        candidate_shapes(&off_b.words),
        "rule `a` is past max_apps on both words, so their futures are identical regardless of the exact count"
    );

    // One shared scope: word_a populates it, then word_b must hit the same saturated key.
    let scope: MemoScope = RefCell::new(AnalysisScope::new());
    let _on_a = analyze_stratum_scoped(
        &g,
        s,
        word_a.clone(),
        &cfg,
        Some(&scope),
        &StepBudget::new(usize::MAX),
    );
    let before = pg_memo::profile::snapshot();
    let on_b = analyze_stratum_scoped(
        &g,
        s,
        word_b.clone(),
        &cfg,
        Some(&scope),
        &StepBudget::new(usize::MAX),
    );
    let after = pg_memo::profile::snapshot();
    assert!(
        after.memo_hits_positive + after.memo_hits_nogood
            > before.memo_hits_positive + before.memo_hits_nogood,
        "word_b's top-level state must hit the memo entry word_a stored under the same saturated key"
    );
    assert_eq!(
        candidate_shapes(&on_b.words),
        candidate_shapes(&off_b.words),
        "memo-on (replaying word_a's stored subtree) must equal memo-off for word_b, byte-identically"
    );
}

// memo-on == memo-off with an affix template in the mix (exercises the TemplateMemo table).
#[test]
fn memo_on_equals_memo_off_with_template() {
    let mut g = load_alpha_grammar();
    // One optional-slot template plus an unordered mrule, so both memos get exercised on the same parse.
    let tp = suffix_rule(&g, 200, "p");
    let tp_id = push_mrule(&mut g, tp);
    let slot = SlotDef {
        name: None,
        optional: true,
        zone: TemplateSlotZone::LegacyUnspecified,
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

// Parsing the same word twice on one scope forces a top-level positive hit deterministically, so the counters move without depending on cascade-internal permutation timing.
#[test]
fn memoprof_counters_move_on_tiny_grammar() {
    let (g, s) = build_unordered();
    let cfg = AnalyzerConfig::default();
    let scope: MemoScope = RefCell::new(AnalysisScope::new());

    let before = pg_memo::profile::snapshot();
    let first = analyze_stratum_scoped(
        &g,
        s,
        word(&g, "akp", s),
        &cfg,
        Some(&scope),
        &StepBudget::new(usize::MAX),
    );
    let second = analyze_stratum_scoped(
        &g,
        s,
        word(&g, "akp", s),
        &cfg,
        Some(&scope),
        &StepBudget::new(usize::MAX),
    );
    let after = pg_memo::profile::snapshot();

    assert_eq!(
        candidate_shapes(&first.words),
        candidate_shapes(&second.words),
        "a memo hit must reproduce the same candidate set"
    );
    assert!(
        after.memo_lookups > before.memo_lookups,
        "lookups must move"
    );
    assert!(
        after.memo_hits_positive + after.memo_hits_nogood
            > before.memo_hits_positive + before.memo_hits_nogood,
        "hits must move"
    );
    assert!(
        after.memo_inserts > before.memo_inserts,
        "inserts must move"
    );
    assert!(
        after.replay_clones > before.replay_clones,
        "replaying the second call's top-level hit must clone at least once"
    );
}

#[test]
fn memo_preserves_nonfinal_template_state_transition_before_final_template() {
    let mut g = load_alpha_grammar();
    let ordinary_def = suffix_rule(&g, 200, "p");
    let ordinary = push_mrule(&mut g, ordinary_def);
    let mut nonfinal_rule = prefix_rule(&g, 300, "p");
    if let MorphRuleDef::AffixProcess(def) = &mut nonfinal_rule {
        def.is_template_rule = true;
    }
    let nonfinal_rule = push_mrule(&mut g, nonfinal_rule);
    let mut final_rule = suffix_rule(&g, 400, "k");
    if let MorphRuleDef::AffixProcess(def) = &mut final_rule {
        def.is_template_rule = true;
    }
    let final_rule = push_mrule(&mut g, final_rule);
    let nonfinal_template = push_template_with_final(
        &mut g,
        vec![SlotDef {
            name: None,
            optional: false,
            zone: TemplateSlotZone::LegacyUnspecified,
            rules: vec![nonfinal_rule],
        }],
        false,
    );
    let final_template = push_template_with_final(
        &mut g,
        vec![SlotDef {
            name: None,
            optional: false,
            zone: TemplateSlotZone::LegacyUnspecified,
            rules: vec![final_rule],
        }],
        true,
    );
    let s = push_stratum(
        &mut g,
        MorphRuleOrder::Unordered,
        vec![ordinary],
        vec![nonfinal_template, final_template],
    );
    let cfg = AnalyzerConfig {
        merge_equivalent: false,
        ..AnalyzerConfig::default()
    };
    let policy = FinalTemplateAnalysisPolicy {
        enforce: true,
        all_templates_final: false,
    };
    let stats = pg_rules::stats::StatsCollector::new(&g);
    let off = analyze_stratum_scoped_filtered_ruled_traced_with_policy(
        &g,
        s,
        word(&g, "pakp", s),
        &cfg,
        None,
        None,
        None,
        None,
        &StepBudget::new(usize::MAX),
        policy,
        Some(&stats),
        &NoopSink,
        TraceHandle::DUMMY,
    );
    let scope: MemoScope = RefCell::new(AnalysisScope::new());
    let on = analyze_stratum_scoped_filtered_ruled_traced_with_policy(
        &g,
        s,
        word(&g, "pakp", s),
        &cfg,
        Some(&scope),
        None,
        None,
        None,
        &StepBudget::new(usize::MAX),
        policy,
        None,
        &NoopSink,
        TraceHandle::DUMMY,
    );
    let histories = |words: &[Word]| {
        words
            .iter()
            .map(|w| w.mrule_apps.iter().flatten().copied().collect::<Vec<_>>())
            .collect::<Vec<_>>()
    };
    let off_histories = histories(&off.words);
    let on_histories = histories(&on.words);
    assert_eq!(
        off_histories, on_histories,
        "memo must preserve state transitions"
    );
    assert!(
        on_histories.contains(&vec![ordinary, nonfinal_rule]),
        "ordinary -> nonfinal template must remain legal after the template clears state; got {on_histories:?}"
    );
    let prune_rows = stats.prune_rows();
    assert!(
        prune_rows
            .iter()
            .any(|row| row.counters.final_templates_skipped > 0),
        "a mixed-finality battery must skip individual final templates"
    );
    assert!(
        prune_rows
            .iter()
            .any(|row| row.counters.template_entries > 0),
        "the nonfinal templates in a mixed battery must still be entered"
    );
}
