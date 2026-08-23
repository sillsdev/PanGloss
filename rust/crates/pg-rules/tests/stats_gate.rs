//! `StatsCollector` morphological-rule instrumentation, reusing the `max_apps_gate` fixture.

mod common;

use common::load_alpha_grammar;
use pg_grammar::model::{
    AffixAllomorphDef, AffixProcessRuleDef, AllomorphId, AllomorphOwner, Dir, Grammar, MRuleId,
    MorphRuleDef, MorphRuleOrder, MorphemeId, MprSet, OutputAction, PRuleId, PartRef, Pattern,
    PatternNode, ReduplicationHint, RewriteMode, RewriteRuleDef, RewriteSubruleDef, SegmentedText,
    SimpleContext, StratumDef, StratumId, TableId, VarTable,
};
use pg_rules::cache::RuleCache;
use pg_rules::stats::{
    Direction, ObjectKind, PRuleStatsCtx, StatsCollector, ALLOMORPH_NONE, WIRED_COUNTERS,
};
use pg_rules::stratum::{
    analyze_stratum_scoped_filtered_ruled_traced, synthesize_stratum_traced, AnalyzerConfig,
    StepBudget,
};
use pg_rules::trace::{NoopSink, TraceHandle};
use pg_rules::Word;
use pg_shape::{NodeKind, Shape, ShapeBuilder};

// ---- shape / word / rule builders (mirrors max_apps_gate.rs) ----------------------------------

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

/// A single-allomorph suffix rule whose LHS is `nc_any+`, so it still matches its own unapplied output.
fn self_matching_suffix_rule(g: &Grammar, morpheme: u32, seg: &str, max_apps: u16) -> MorphRuleDef {
    MorphRuleDef::AffixProcess(AffixProcessRuleDef {
        morpheme: MorphemeId(morpheme),
        name: None,
        blockable: false,
        partial: false,
        max_apps,
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

/// `self_matching_suffix_rule`, but with an explicit allomorph id, for a caller that also registers a matching `Grammar::allomorph_owners` entry so `RuleCache::build` can index it.
fn self_matching_suffix_rule_with_allo_id(
    g: &Grammar,
    morpheme: u32,
    allo_id: u32,
    seg: &str,
    max_apps: u16,
) -> MorphRuleDef {
    MorphRuleDef::AffixProcess(AffixProcessRuleDef {
        morpheme: MorphemeId(morpheme),
        name: None,
        blockable: false,
        partial: false,
        max_apps,
        required_syn_fs: pg_featstruct::FsId(0),
        out_syn_fs: pg_featstruct::FsId(0),
        obligatory_features: vec![],
        required_stem_name: None,
        is_template_rule: false,
        allomorphs: vec![allomorph(
            allo_id,
            vec![one_or_more("nc_any", g)],
            vec![
                OutputAction::Copy(PartRef::Input(0)),
                insert_segments(g, seg),
            ],
        )],
    })
}

/// Like `self_matching_suffix_rule`, but with two identical allomorphs (analysis reaches both every tick).
fn two_allomorph_suffix_rule(g: &Grammar, morpheme: u32, seg: &str, max_apps: u16) -> MorphRuleDef {
    let rhs = || {
        vec![
            OutputAction::Copy(PartRef::Input(0)),
            insert_segments(g, seg),
        ]
    };
    MorphRuleDef::AffixProcess(AffixProcessRuleDef {
        morpheme: MorphemeId(morpheme),
        name: None,
        blockable: false,
        partial: false,
        max_apps,
        required_syn_fs: pg_featstruct::FsId(0),
        out_syn_fs: pg_featstruct::FsId(0),
        obligatory_features: vec![],
        required_stem_name: None,
        is_template_rule: false,
        allomorphs: vec![
            allomorph(morpheme, vec![one_or_more("nc_any", g)], rhs()),
            allomorph(morpheme + 1, vec![one_or_more("nc_any", g)], rhs()),
        ],
    })
}

fn push_mrule(g: &mut Grammar, rule: MorphRuleDef) -> MRuleId {
    let id = MRuleId(g.mrules.len() as u32);
    g.mrules.push(rule);
    id
}

fn push_stratum(g: &mut Grammar, order: MorphRuleOrder, mrules: Vec<MRuleId>) -> StratumId {
    let id = StratumId(g.strata.len() as u8);
    g.strata.push(StratumDef {
        name: None,
        table: TableId(0),
        mrule_order: order,
        prules: vec![],
        mrules,
        templates: vec![],
        entries: vec![],
    });
    id
}

fn word(g: &Grammar, text: &str, stratum: StratumId) -> Word {
    Word::new(shape_with_lanes(g, text), stratum)
}

fn morph_rule_row<'a>(
    rows: &'a [pg_rules::stats::StatsRow],
    rid: MRuleId,
) -> Option<&'a pg_rules::stats::StatsRow> {
    rows.iter()
        .find(|r| r.kind == ObjectKind::MorphRule && r.object_index == rid.0)
}

/// A rule invoked and rejected by `max_apps` on the same candidate trail must not add to `attempts`.
#[test]
fn max_apps_rejection_contributes_no_attempt() {
    // max_apps=2 on "appp" leaves one further, gated attempt on "ap" that must not count.
    let mut g = load_alpha_grammar();
    let r = self_matching_suffix_rule(&g, 200, "p", 2);
    let rid = push_mrule(&mut g, r);
    let cfg = AnalyzerConfig::default();
    let budget = StepBudget::new(usize::MAX);
    let s = push_stratum(&mut g, MorphRuleOrder::Unordered, vec![rid]);
    let stats = StatsCollector::new(&g);

    let out = analyze_stratum_scoped_filtered_ruled_traced(
        &g,
        s,
        word(&g, "appp", s),
        &cfg,
        None,
        None,
        None,
        None,
        &budget,
        Some(&stats),
        &NoopSink,
        TraceHandle::DUMMY,
    );
    assert!(!out.capped, "gate must terminate without the step cap");

    let rows = stats.rows();
    let row = morph_rule_row(&rows, rid).expect("the rule must have a recorded row");
    assert_eq!(
        row.counters.attempts, 2,
        "max_apps=2 must cap recorded attempts at exactly the two successful unapplications; a \
         count of 3 would mean the gated third call was wrongly counted as an attempt"
    );
}

/// An attempted rule whose body matches nothing must record `not_applied` with zero outputs.
#[test]
fn not_applied_fires_when_an_attempted_rule_matches_nothing() {
    let mut g = load_alpha_grammar();
    let r = self_matching_suffix_rule(&g, 200, "p", 5);
    let rid = push_mrule(&mut g, r);
    let cfg = AnalyzerConfig::default();
    let budget = StepBudget::new(usize::MAX);
    let s = push_stratum(&mut g, MorphRuleOrder::Unordered, vec![rid]);
    let stats = StatsCollector::new(&g);

    // "aaa" carries no appended "p" for the rule to strip, so it is attempted and fails.
    let out = analyze_stratum_scoped_filtered_ruled_traced(
        &g,
        s,
        word(&g, "aaa", s),
        &cfg,
        None,
        None,
        None,
        None,
        &budget,
        Some(&stats),
        &NoopSink,
        TraceHandle::DUMMY,
    );
    assert!(!out.capped);
    assert_eq!(out.words.len(), 1, "only the unmodified seed survives");

    let rows = stats.rows();
    let rule_rows: Vec<_> = rows
        .iter()
        .filter(|r| r.kind == ObjectKind::MorphRule && r.object_index == rid.0)
        .collect();
    assert!(
        !rule_rows.is_empty(),
        "the rule must have at least one recorded row"
    );
    let attempts: u64 = rule_rows.iter().map(|r| r.counters.attempts).sum();
    let outputs: u64 = rule_rows.iter().map(|r| r.counters.outputs).sum();
    let not_applied: u64 = rule_rows.iter().map(|r| r.counters.not_applied).sum();
    assert!(attempts >= 1, "the rule must have been attempted");
    assert_eq!(outputs, 0);
    assert!(
        not_applied >= 1,
        "an attempted rule that matched nothing must record not_applied"
    );
}

/// THE critical invariant: allomorph rows (incl. `ALLOMORPH_NONE`) sum their `attempts` to the rule's tick count.
#[test]
fn allomorph_rows_sum_to_the_rules_tick_count() {
    let mut g = load_alpha_grammar();
    let r = two_allomorph_suffix_rule(&g, 220, "p", 1);
    let rid = push_mrule(&mut g, r);
    let cfg = AnalyzerConfig::default();
    let budget = StepBudget::new(usize::MAX);
    let s = push_stratum(&mut g, MorphRuleOrder::Unordered, vec![rid]);
    let stats = StatsCollector::new(&g);

    let out = analyze_stratum_scoped_filtered_ruled_traced(
        &g,
        s,
        word(&g, "appp", s),
        &cfg,
        None,
        None,
        None,
        None,
        &budget,
        Some(&stats),
        &NoopSink,
        TraceHandle::DUMMY,
    );
    assert!(!out.capped, "gate must terminate without the step cap");

    let ticks = budget.steps() as u64;
    assert_eq!(
        ticks, 1,
        "max_apps=1 must yield exactly one tick() for this rule"
    );

    let rows = stats.rows();
    let rule_rows: Vec<_> = rows
        .iter()
        .filter(|r| r.kind == ObjectKind::MorphRule && r.object_index == rid.0)
        .collect();
    let sum_attempts: u64 = rule_rows.iter().map(|r| r.counters.attempts).sum();
    assert_eq!(
        sum_attempts, ticks,
        "allomorph rows (including ALLOMORPH_NONE) must sum their attempts to the rule's own tick \
         count -- a missed allomorph loop or a double-counted residual would break this"
    );

    let allo1 = rule_rows.iter().find(|r| r.allomorph == 1).expect(
        "allomorph index 0 (stored as allomorph=1, since 0 is ALLOMORPH_NONE) must have a row",
    );
    let allo2 = rule_rows
        .iter()
        .find(|r| r.allomorph == 2)
        .expect("allomorph index 1 (stored as allomorph=2) must have a row");
    let none_row = rule_rows
        .iter()
        .find(|r| r.allomorph == ALLOMORPH_NONE)
        .expect("the rule's own ALLOMORPH_NONE row must exist -- it carries every tick");
    assert_eq!(
        none_row.counters.attempts, 1,
        "attempts is rule-level: the tick belongs to ALLOMORPH_NONE, not to whichever allomorph \
         was reached first"
    );
    assert_eq!(
        allo1.counters.attempts, 0,
        "an allomorph is not the unit attempts counts -- it must carry none of its own"
    );
    assert_eq!(
        allo2.counters.attempts, 0,
        "same as allo1: attempts is rule-level, never per-allomorph"
    );
    assert!(
        allo1.counters.work > 0 && allo2.counters.work > 0,
        "both allomorphs are tried every tick, so both must record their own non-zero work"
    );
    assert_eq!(
        allo1.counters.work, allo2.counters.work,
        "both allomorphs see the identical segment count every tick"
    );
    assert_eq!(allo1.counters.outputs, 1);
    assert_eq!(allo2.counters.outputs, 1);
}

/// Falsifiable pin: no allomorph-dimension row may ever carry a nonzero `attempts`.
#[test]
fn allomorph_rows_never_carry_nonzero_attempts() {
    let mut g = load_alpha_grammar();
    let r = two_allomorph_suffix_rule(&g, 260, "p", 5);
    let rid = push_mrule(&mut g, r);
    let cfg = AnalyzerConfig::default();
    let budget = StepBudget::new(usize::MAX);
    let s = push_stratum(&mut g, MorphRuleOrder::Unordered, vec![rid]);
    let stats = StatsCollector::new(&g);

    let out = analyze_stratum_scoped_filtered_ruled_traced(
        &g,
        s,
        word(&g, "appp", s),
        &cfg,
        None,
        None,
        None,
        None,
        &budget,
        Some(&stats),
        &NoopSink,
        TraceHandle::DUMMY,
    );
    assert!(!out.capped, "gate must terminate without the step cap");

    let rows = stats.rows();
    let mut saw_allomorph_row = false;
    for row in rows
        .iter()
        .filter(|r| r.kind == ObjectKind::MorphRule && r.object_index == rid.0)
    {
        if row.allomorph != ALLOMORPH_NONE {
            saw_allomorph_row = true;
            assert_eq!(
                row.counters.attempts, 0,
                "allomorph {} must carry zero attempts -- attempts is rule-level only",
                row.allomorph
            );
        }
    }
    assert!(
        saw_allomorph_row,
        "the fixture must actually produce allomorph-dimension rows, or this test proves nothing"
    );
}

/// The synthesis-side confirm pass must record its own `Direction::Synthesis` rows, distinct from the analysis-side rows the peeling pass already recorded for the same rule.
#[test]
fn synthesis_confirm_pass_records_its_own_synthesis_direction_rows() {
    let mut g = load_alpha_grammar();
    // `RuleCache::build` indexes `AllomorphId` by position in `g.allomorph_owners` -- register it.
    let allo_id = g.allomorph_owners.len() as u32;
    let r = self_matching_suffix_rule_with_allo_id(&g, 320, allo_id, "p", 5);
    let rid = push_mrule(&mut g, r);
    g.allomorph_owners.push(AllomorphOwner::Affix(rid, 0));
    let cfg = AnalyzerConfig::default();
    let budget = StepBudget::new(usize::MAX);
    let s = push_stratum(&mut g, MorphRuleOrder::Unordered, vec![rid]);
    let cache = RuleCache::build(&g);
    let stats = StatsCollector::new(&g);

    let out = analyze_stratum_scoped_filtered_ruled_traced(
        &g,
        s,
        word(&g, "appp", s),
        &cfg,
        None,
        None,
        None,
        None,
        &budget,
        Some(&stats),
        &NoopSink,
        TraceHandle::DUMMY,
    );
    assert!(!out.capped, "gate must terminate without the step cap");

    let candidate = out
        .words
        .into_iter()
        .find(|w| w.mrule_app_index >= 0)
        .expect("the rule must unapply at least once, leaving a resynthesizable candidate");

    let synth_out = synthesize_stratum_traced(
        &g,
        s,
        candidate,
        usize::MAX,
        &cache,
        &budget,
        Some(&stats),
        &NoopSink,
        TraceHandle::DUMMY,
    );
    assert!(
        !synth_out.is_empty(),
        "the peeled candidate must resynthesize back to at least one word"
    );

    let rows = stats.rows();
    let synth_rows: Vec<_> = rows
        .iter()
        .filter(|r| {
            r.kind == ObjectKind::MorphRule
                && r.object_index == rid.0
                && r.direction == Direction::Synthesis
        })
        .collect();
    assert!(
        !synth_rows.is_empty(),
        "the confirm-pass reapplication must leave its own synthesis-direction rows, not silently \
         merge into (or be missing from) the analysis-direction rows"
    );
    let synth_attempts: u64 = synth_rows.iter().map(|r| r.counters.attempts).sum();
    assert!(
        synth_attempts >= 1,
        "the rule's synthesis-side reapplication must record a nonzero attempt"
    );

    let analysis_rows: Vec<_> = rows
        .iter()
        .filter(|r| {
            r.kind == ObjectKind::MorphRule
                && r.object_index == rid.0
                && r.direction == Direction::Analysis
        })
        .collect();
    assert!(
        !analysis_rows.is_empty(),
        "the analysis-side peeling pass must still have its own rows, untouched by synthesis"
    );
}

/// A phonological rule fixture: `p -> [voiced]` between two vowels.
fn voicing_prule(g: &Grammar) -> RewriteRuleDef {
    RewriteRuleDef {
        xml_id: "wired-probe".into(),
        name: None,
        mode: RewriteMode::Iterative,
        dir: Dir::LeftToRight,
        vars: Default::default(),
        lhs: Pattern {
            nodes: vec![PatternNode::CharDef(common::char_def(g, "char_p"))],
        },
        subrules: vec![RewriteSubruleDef {
            required_pos: None,
            required_mpr: MprSet::EMPTY,
            excluded_mpr: MprSet::EMPTY,
            rhs: Pattern {
                nodes: vec![PatternNode::Context(ctx("nc_voiced", g))],
            },
            left_env: Some(Pattern {
                nodes: vec![PatternNode::Context(ctx("nc_any", g))],
            }),
            right_env: Some(Pattern {
                nodes: vec![PatternNode::Context(ctx("nc_any", g))],
            }),
            self_opaquing: false,
        }],
    }
}

fn counter_value(c: &pg_rules::stats::Counters, name: &str) -> u64 {
    match name {
        "attempts" => c.attempts,
        "work" => c.work,
        "outputs" => c.outputs,
        "not_applied" => c.not_applied,
        "no_root" => c.no_root,
        "surface_mismatch" => c.surface_mismatch,
        "uses" => c.uses,
        other => panic!("unknown counter {other}"),
    }
}

const ALL_COUNTER_NAMES: [&str; 7] = [
    "attempts",
    "work",
    "outputs",
    "not_applied",
    "no_root",
    "surface_mismatch",
    "uses",
];

/// `WIRED_COUNTERS` pairs this crate can drive without a `Morpher`; the rest are pinned by pg-parse's `stats_collector_gate` instead.
const PG_RULES_REAL_COUNTERS: &[(ObjectKind, &str)] = &[
    (ObjectKind::MorphRule, "attempts"),
    (ObjectKind::MorphRule, "work"),
    (ObjectKind::MorphRule, "outputs"),
    (ObjectKind::MorphRule, "not_applied"),
    (ObjectKind::PhonRule, "attempts"),
    (ObjectKind::PhonRule, "work"),
    (ObjectKind::PhonRule, "outputs"),
    (ObjectKind::PhonRule, "not_applied"),
];

/// Pins `WIRED_COUNTERS` against reality for the pairs this crate can drive without a `Morpher`;
/// every row comes from a real analyzer/rewrite-rule run, never a direct recorder call.
#[test]
fn wired_counters_matches_reality() {
    let mut all_rows: Vec<pg_rules::stats::StatsRow> = Vec::new();

    // MorphRule: a real analyzer run over a two-allomorph rule.
    let mut mg = load_alpha_grammar();
    let mr = two_allomorph_suffix_rule(&mg, 270, "p", 5);
    let mrid = push_mrule(&mut mg, mr);
    let cfg = AnalyzerConfig::default();
    let budget = StepBudget::new(usize::MAX);
    let s = push_stratum(&mut mg, MorphRuleOrder::Unordered, vec![mrid]);
    let mstats = StatsCollector::new(&mg);
    let _ = analyze_stratum_scoped_filtered_ruled_traced(
        &mg,
        s,
        word(&mg, "appp", s),
        &cfg,
        None,
        None,
        None,
        None,
        &budget,
        Some(&mstats),
        &NoopSink,
        TraceHandle::DUMMY,
    );
    all_rows.extend(mstats.rows());

    // MorphRule not_applied: a rule attempted against a word carrying none of its suffix to strip.
    let mut na_g = load_alpha_grammar();
    let na_rule = self_matching_suffix_rule(&na_g, 280, "p", 5);
    let na_rid = push_mrule(&mut na_g, na_rule);
    let na_s = push_stratum(&mut na_g, MorphRuleOrder::Unordered, vec![na_rid]);
    let na_stats = StatsCollector::new(&na_g);
    let _ = analyze_stratum_scoped_filtered_ruled_traced(
        &na_g,
        na_s,
        word(&na_g, "aaa", na_s),
        &cfg,
        None,
        None,
        None,
        None,
        &budget,
        Some(&na_stats),
        &NoopSink,
        TraceHandle::DUMMY,
    );
    all_rows.extend(na_stats.rows());

    // PhonRule: real `analyze` calls, one that unapplies and one that matches nothing.
    let phon_g = load_alpha_grammar();
    let pr = voicing_prule(&phon_g);
    let pstats = StatsCollector::new(&phon_g);
    let pctx_ok = PRuleStatsCtx {
        stats: &pstats,
        stratum: StratumId(0),
        id: PRuleId(0),
        direction: Direction::Analysis,
    };
    let applied = pg_rules::rewrite::analyze(
        &phon_g,
        &pr,
        &shape_with_lanes(&phon_g, "aba"),
        Some(pctx_ok),
    );
    assert_eq!(applied.len(), 1, "sanity: the voiced 'b' must unapply");
    let pctx_fail = PRuleStatsCtx {
        stats: &pstats,
        stratum: StratumId(0),
        id: PRuleId(0),
        direction: Direction::Analysis,
    };
    let not_applied = pg_rules::rewrite::analyze(
        &phon_g,
        &pr,
        &shape_with_lanes(&phon_g, "aaa"),
        Some(pctx_fail),
    );
    assert!(
        not_applied.is_empty(),
        "sanity: an all-vowel shape has no voiced-consonant target to unapply"
    );
    all_rows.extend(pstats.rows());

    for &(kind, counter) in PG_RULES_REAL_COUNTERS {
        let observed_nonzero = all_rows
            .iter()
            .any(|r| r.kind == kind && counter_value(&r.counters, counter) > 0);
        assert!(
            observed_nonzero,
            "WIRED_COUNTERS claims ({kind:?}, {counter}) is measured, but no fixture ever wrote a \
             nonzero value for it"
        );
    }
    for row in &all_rows {
        for name in ALL_COUNTER_NAMES {
            let claimed = WIRED_COUNTERS.contains(&(row.kind, name));
            if !claimed {
                assert_eq!(
                    counter_value(&row.counters, name),
                    0,
                    "({:?}, {name}) is not in WIRED_COUNTERS but a row carries a nonzero value",
                    row.kind
                );
            }
        }
    }
}
