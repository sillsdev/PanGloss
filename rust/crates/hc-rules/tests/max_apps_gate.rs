//! Regression gate for the `MaxApplicationCount` cap (Tier-1 #4, rust-conversion.md §13.1).
//!
//! C# gates every rule-(un)application attempt *before* trying any allomorph:
//! `input.GetUnapplicationCount(_rule) >= _rule.MaxApplicationCount` → skip the rule entirely for
//! this candidate (`AnalysisAffixProcessRule.cs:46-52`, identically `AnalysisCompoundingRule.cs:48`).
//! Every rule in the DTD defaults to `MaxApplicationCount = 1`. Without a matching gate in
//! `StratumAnalyzer::apply_one_mrule`, a rule whose LHS still matches its own unapplied output can
//! re-unapply indefinitely (bounded only by shape shrinkage / the step-cap safety valve), which is
//! both a search-space blowup and — because guided synthesis re-confirms whatever the analysis
//! trail recorded — a source of outright wrong surface forms.
//!
//! This uses a suffix rule whose pattern is `X+` over "any segment" (`nc_any`), so after unapplying
//! one trailing "p" the *result* still matches the rule's own LHS — exactly the self-re-application
//! shape the gate must block. Two cases:
//! - `max_apps: 1` (the universal DTD default): the rule may unapply at most once on any path.
//! - `max_apps: 2` (boundary case): confirms the gate is `count >= max_apps`, not a hardcoded "once
//!   forever" — the rule must be allowed to unapply exactly twice, no more.

mod common;

use common::load_alpha_grammar;
use hc_grammar::chardef::CharDefId;
use hc_grammar::model::{
    AffixAllomorphDef, AffixProcessRuleDef, AllomorphId, Grammar, MRuleId, MorphRuleDef,
    MorphRuleOrder, MorphemeId, MprSet, OutputAction, PartRef, Pattern, PatternNode,
    ReduplicationHint, SegmentedText, SimpleContext, StratumDef, StratumId, TableId, VarTable,
};
use hc_rules::stratum::{analyze_stratum, AnalyzerConfig, StepBudget};
use hc_rules::Word;
use hc_shape::{NodeKind, Shape, ShapeBuilder};

// ---- shape / word / rule builders (mirrors stratum_gate.rs) -----------------------------------

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

fn cd(g: &Grammar, xml_id: &str) -> u32 {
    common::char_def(g, xml_id).0
}

fn ctx(nc: &str, g: &Grammar) -> SimpleContext {
    common::ctx(common::nat_class(g, nc))
}

/// `X+` (one-or-more) over a natural class — the stem-copy part.
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

/// A single-allomorph suffix rule whose LHS is `nc_any+` — it still matches its own unapplied
/// output, so without the `max_apps` gate it can re-unapply to itself indefinitely.
/// `CopyFromInput(0) + InsertSegments(seg)`.
fn self_matching_suffix_rule(g: &Grammar, morpheme: u32, seg: &str, max_apps: u16) -> MorphRuleDef {
    MorphRuleDef::AffixProcess(AffixProcessRuleDef {
        morpheme: MorphemeId(morpheme),
        name: None,
        blockable: false,
        partial: false,
        max_apps,
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

fn candidate_shapes(words: &[Word]) -> Vec<Vec<u32>> {
    let mut v: Vec<Vec<u32>> = words.iter().map(|w| char_defs(&w.shape)).collect();
    v.sort();
    v.dedup();
    v
}

fn word(g: &Grammar, text: &str, stratum: StratumId) -> Word {
    Word::new(shape_with_lanes(g, text), stratum)
}

// =================================================================================================
// max_apps: 1 (the universal DTD default) — the rule fires at most once on any analysis path.
// =================================================================================================

#[test]
fn max_apps_one_gates_a_self_matching_rule_to_a_single_unapplication() {
    // Root "a" + suffix "p" + suffix "p" (same rule, re-applied at synthesis time) → surface "app".
    // The rule's LHS (`nc_any+`) still matches "ap" and "a", so an ungated cascade would unapply it
    // twice: app -> ap -> a. With `max_apps: 1`, only the first unapplication is allowed.
    let mut g = load_alpha_grammar();
    let r = self_matching_suffix_rule(&g, 200, "p", 1);
    let rid = push_mrule(&mut g, r);
    // Uncapped step budget: the gate itself must be what stops the walk, not the safety valve.
    let cfg = AnalyzerConfig::default();
    let budget = StepBudget::new(usize::MAX);

    for order in [MorphRuleOrder::Linear, MorphRuleOrder::Unordered] {
        let s = push_stratum(&mut g, order, vec![rid]);
        let out = analyze_stratum(&g, s, word(&g, "app", s), &cfg, &budget);
        assert!(
            !out.capped,
            "{order:?}: gate must terminate without the step cap firing"
        );

        let got = candidate_shapes(&out.words);
        let seed = vec![cd(&g, "char_a"), cd(&g, "char_p"), cd(&g, "char_p")];
        let one_unapplied = vec![cd(&g, "char_a"), cd(&g, "char_p")];
        let two_unapplied = vec![cd(&g, "char_a")];

        assert!(
            got.contains(&seed),
            "{order:?}: seed [app] present; got {got:?}"
        );
        assert!(
            got.contains(&one_unapplied),
            "{order:?}: one unapplication [ap] reachable; got {got:?}"
        );
        assert!(
            !got.contains(&two_unapplied),
            "{order:?}: max_apps=1 must block the second unapplication to [a]; got {got:?}"
        );
        assert_eq!(got.len(), 2, "{order:?}: exactly {{app, ap}}; got {got:?}");
    }
}

// =================================================================================================
// max_apps: 2 (boundary case) — the same rule now unapplies exactly twice, confirming the gate is
// an inequality (`count >= max_apps`), not an accidental "exactly 1 forever" hardcode.
// =================================================================================================

#[test]
fn max_apps_two_allows_exactly_two_unapplications_not_three() {
    // Root "a" + "p" + "p" + "p" -> surface "appp". With max_apps=2 the rule may unapply twice
    // (appp -> app -> ap) but the third attempt (-> a) must be blocked.
    let mut g = load_alpha_grammar();
    let r = self_matching_suffix_rule(&g, 200, "p", 2);
    let rid = push_mrule(&mut g, r);
    let cfg = AnalyzerConfig::default();
    let budget = StepBudget::new(usize::MAX);
    let s = push_stratum(&mut g, MorphRuleOrder::Unordered, vec![rid]);

    let out = analyze_stratum(&g, s, word(&g, "appp", s), &cfg, &budget);
    assert!(
        !out.capped,
        "gate must terminate without the step cap firing"
    );

    let got = candidate_shapes(&out.words);
    let seed = vec![
        cd(&g, "char_a"),
        cd(&g, "char_p"),
        cd(&g, "char_p"),
        cd(&g, "char_p"),
    ];
    let one = vec![cd(&g, "char_a"), cd(&g, "char_p"), cd(&g, "char_p")];
    let two = vec![cd(&g, "char_a"), cd(&g, "char_p")];
    let three = vec![cd(&g, "char_a")];

    assert!(got.contains(&seed), "seed [appp] present; got {got:?}");
    assert!(
        got.contains(&one),
        "one unapplication [app] reachable; got {got:?}"
    );
    assert!(
        got.contains(&two),
        "two unapplications [ap] reachable; got {got:?}"
    );
    assert!(
        !got.contains(&three),
        "max_apps=2 must block the third unapplication to [a]; got {got:?}"
    );
    assert_eq!(got.len(), 3, "exactly {{appp, app, ap}}; got {got:?}");
}
