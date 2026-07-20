//! M4b (part 2/2) acceptance gate — per-stratum analysis orchestration + affix-template battery.
//!
//! Part 1: hand-built tiny strata over the alpha grammar, with candidate sets reasoned by hand from
//! HermitCrab's `AnalysisStratumRule` / `Analysis*AffixTemplateRule` semantics and cross-checked
//! against the C# cascade behavior:
//! - (a) a LINEAR stratum, two ordered suffix rules;
//! - (b) an UNORDERED stratum, two order-dependent suffix rules — the `CombinationRuleCascade`
//!   reaches a root the `PermutationRuleCascade` (over the reversed list) cannot;
//! - (c) an affix template with an optional slot — both the slot-filled and slot-skipped analyses
//!   appear, contrasted against the same template with the slot made mandatory.
//!
//! Part 2: structural coverage over the real Sena grammar (0 prules / 140 mrules / 24 templates,
//! Unordered) — run the analysis stratum on several short words under the step cap; assert
//! termination + a non-empty candidate set, no panic; report counts + whether the cap fired.

mod common;

use common::load_alpha_grammar;
use pg_grammar::chardef::CharDefId;
use pg_grammar::model::{
    AffixAllomorphDef, AffixProcessRuleDef, AffixTemplateDef, AllomorphId, AllomorphOwner, Grammar,
    MRuleId, MorphRuleDef, MorphRuleOrder, MorphemeId, MprSet, OutputAction, PartRef, Pattern,
    PatternNode, ReduplicationHint, SegmentedText, SimpleContext, SlotDef, StratumDef, StratumId,
    TableId, TemplateId, VarTable,
};
use pg_rules::cache::RuleCache;
use pg_rules::stratum::{
    analyze_stratum, synthesize_stratum_traced, synthesize_template, AnalyzerConfig, StepBudget,
};
use pg_rules::trace::{NoopSink, TraceHandle};
use pg_rules::{MorphRecord, Word};
use pg_shape::{NodeKind, Shape, ShapeBuilder};
use std::time::Duration;

// ---- shape / word builders (mirrors morph_gate.rs) -----------------------------------------

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

/// A single-allomorph suffix rule: `CopyFromInput(0) + InsertSegments(seg)`.
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

/// A single-allomorph suffix rule that ALSO registers its allomorph in `g.allomorph_owners`, the
/// way `pg_grammar::load` would (`AllomorphOwner::Affix(mrule_id, 0)` at the next sequential
/// `AllomorphId`) -- required for `RuleCache::build`/`synthesize_stratum_traced`'s cached
/// production path (`guided_synth` -> `synthesize_cached_traced` -> `synth_affix_cached`), which
/// indexes allomorphs through that registry. Plain `suffix_rule`/`push_mrule` above (used by every
/// other test in this file) mint an `AllomorphId` out of thin air (the `morpheme` number) and are
/// only ever exercised through the UNCACHED `analyze_stratum`/`synthesize_template` entry points,
/// which never consult `g.allomorph_owners` at all -- see `pg-rules/tests/template_partial_gate.rs`'s
/// `push_suffix_rule`, whose doc comment spells out this exact distinction; this is that same helper.
fn push_cache_suffix_rule(g: &mut Grammar, morpheme: u32, seg: &str) -> MRuleId {
    let mrule_id = MRuleId(g.mrules.len() as u32);
    let allo_id = AllomorphId(g.allomorph_owners.len() as u32);
    g.allomorph_owners.push(AllomorphOwner::Affix(mrule_id, 0));
    let rule = suffix_rule_with_allomorph(g, morpheme, seg, allo_id);
    g.mrules.push(rule);
    mrule_id
}

fn suffix_rule_with_allomorph(
    g: &Grammar,
    morpheme: u32,
    seg: &str,
    allo_id: AllomorphId,
) -> MorphRuleDef {
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
            allo_id.0,
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

/// Register a fresh stratum with the given morphological rules / templates / order.
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

/// The set of interior char-def sequences of an analysis candidate set (order-insensitive compare).
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
// (a) LINEAR stratum, two ordered suffix rules.
// =================================================================================================

#[test]
fn linear_stratum_unapplies_suffixes_in_reversed_order() {
    // Root "a" + suffix A "p" + suffix B "k"  →  surface "apk"  ("p" and "k" are mutually exclusive
    // by place of articulation, so each suffix rule strips only its own segment).
    // Stratum mrules = [A, B]; AnalysisStratumRule reverses them → the PermutationRuleCascade walks
    // [B, A]. From "apk": unapply B (drop "k") → "ap"; then A (drop "p") → "a". B cannot unapply
    // from "apk" any other way, and A cannot unapply from "apk" directly (it ends in "k", not "p").
    // The cascade seeds the (post-prule) input word.
    //   Expected candidate SHAPES = { apk (seed), ap, a }.
    let mut g = load_alpha_grammar();
    let (ra, rb) = (suffix_rule(&g, 200, "p"), suffix_rule(&g, 300, "k"));
    let a = push_mrule(&mut g, ra);
    let b = push_mrule(&mut g, rb);
    let s = push_stratum(&mut g, MorphRuleOrder::Linear, vec![a, b], vec![]);

    let input = word(&g, "apk", s);
    let out = analyze_stratum(
        &g,
        s,
        input,
        &AnalyzerConfig::default(),
        &StepBudget::new(usize::MAX),
    );
    assert!(!out.capped, "tiny linear stratum must not hit the cap");

    let got = candidate_shapes(&out.words);
    let want = {
        let mut v = vec![
            vec![cd(&g, "char_a"), cd(&g, "char_p"), cd(&g, "char_k")],
            vec![cd(&g, "char_a"), cd(&g, "char_p")],
            vec![cd(&g, "char_a")],
        ];
        v.sort();
        v
    };
    assert_eq!(got, want, "linear candidate shapes = {{apk, ap, a}}");
}

// =================================================================================================
// (b) UNORDERED stratum: combination reaches a root permutation-over-reversed cannot.
// =================================================================================================

#[test]
fn unordered_combination_reaches_root_linear_permutation_misses() {
    // Root "a" + suffix A "p" (inner) + suffix B "k" (outer)  →  surface "akp"  — the non-commuting
    // case. Because the *inner* suffix "p" ended up at the surface end, to strip back to "a" you
    // must unapply A (drop trailing "p") FIRST, exposing "k", then unapply B (drop trailing "k").
    //
    // Stratum mrules = [A(strip-p), B(strip-k)]; AnalysisStratumRule reverses → [B(idx0), A(idx1)].
    //  • PermutationRuleCascade (LINEAR) walks index-ordered subsets of [B, A]: it can do B-then-A
    //    but NOT A-then-B. B (strip-k) does not apply to "akp" (ends in "p"), so it only reaches
    //    "ak" (via A alone). It never reaches "a".
    //  • CombinationRuleCascade (UNORDERED, multi-app) recurses from index 0 at every level → it
    //    tries A(→"ak")-then-B(→"a"). It reaches "a".
    let build = |order: MorphRuleOrder| -> (Grammar, StratumId) {
        let mut g = load_alpha_grammar();
        let (ra, rb) = (suffix_rule(&g, 200, "p"), suffix_rule(&g, 300, "k"));
        let a = push_mrule(&mut g, ra);
        let b = push_mrule(&mut g, rb);
        let s = push_stratum(&mut g, order, vec![a, b], vec![]);
        (g, s)
    };

    // UNORDERED: reaches the root "a".
    let (gu, su) = build(MorphRuleOrder::Unordered);
    let out_u = analyze_stratum(
        &gu,
        su,
        word(&gu, "akp", su),
        &AnalyzerConfig::default(),
        &StepBudget::new(usize::MAX),
    );
    assert!(!out_u.capped);
    let unordered = candidate_shapes(&out_u.words);
    let root = vec![cd(&gu, "char_a")];
    assert!(
        unordered.contains(&root),
        "combination reaches root [a]; got {unordered:?}"
    );
    // Full unordered set: { akp (seed), ak, a }.
    assert_eq!(unordered, {
        let mut v = vec![
            vec![cd(&gu, "char_a"), cd(&gu, "char_k"), cd(&gu, "char_p")],
            vec![cd(&gu, "char_a"), cd(&gu, "char_k")],
            vec![cd(&gu, "char_a")],
        ];
        v.sort();
        v
    });

    // LINEAR: same rules, but permutation-over-reversed cannot reach the root.
    let (gl, sl) = build(MorphRuleOrder::Linear);
    let out_l = analyze_stratum(
        &gl,
        sl,
        word(&gl, "akp", sl),
        &AnalyzerConfig::default(),
        &StepBudget::new(usize::MAX),
    );
    assert!(!out_l.capped);
    let linear = candidate_shapes(&out_l.words);
    assert!(
        !linear.contains(&vec![cd(&gl, "char_a")]),
        "permutation over reversed [B,A] must NOT reach root [a]; got {linear:?}"
    );
    // Full linear set: { akp (seed), ak }.
    assert_eq!(linear, {
        let mut v = vec![
            vec![cd(&gl, "char_a"), cd(&gl, "char_k"), cd(&gl, "char_p")],
            vec![cd(&gl, "char_a"), cd(&gl, "char_k")],
        ];
        v.sort();
        v
    });
}

// =================================================================================================
// (c) Affix template with an optional slot → both slot-filled and slot-skipped analyses.
// =================================================================================================

/// Build a stratum with one two-slot template over the alpha grammar. Slot 0 = suffix "p" (its
/// optionality is the parameter); slot 1 = mandatory suffix "k". Surface "apk" = root "a" +
/// slot0"p" + slot1"k".
fn template_stratum(slot0_optional: bool) -> (Grammar, StratumId) {
    let mut g = load_alpha_grammar();
    let (ra, rb) = (suffix_rule(&g, 200, "p"), suffix_rule(&g, 300, "k"));
    let a = push_mrule(&mut g, ra); // slot 0
    let b = push_mrule(&mut g, rb); // slot 1
    let tid = TemplateId(g.templates.len() as u32);
    g.templates.push(AffixTemplateDef {
        name: None,
        is_final: true,
        required_syn_fs: pg_featstruct::FsId(0),
        slots: vec![
            SlotDef {
                name: None,
                optional: slot0_optional,
                rules: vec![a],
            },
            SlotDef {
                name: None,
                optional: false,
                rules: vec![b],
            },
        ],
    });
    let s = push_stratum(&mut g, MorphRuleOrder::Unordered, vec![], vec![tid]);
    (g, s)
}

#[test]
fn optional_template_slot_yields_both_filled_and_skipped() {
    // AnalysisAffixTemplateRule.ApplySlots descends from the last slot. On "apk":
    //   slot 1 (mandatory, suffix "k"): drop "k" → "ap", recurse into slot 0.
    //     slot 0 (OPTIONAL, suffix "p"): drop "p" → "a"  (slot 0 FILLED), then — because slot 0 is
    //       optional — fall through and add "ap"          (slot 0 SKIPPED).
    //   slot 1 is mandatory, so "apk" itself is NOT added (its material had to be consumed).
    // Template outputs = { a, ap }; with the seed the analysis set = { apk, ap, a }.
    let (g, s) = template_stratum(true);
    let out = analyze_stratum(
        &g,
        s,
        word(&g, "apk", s),
        &AnalyzerConfig::default(),
        &StepBudget::new(usize::MAX),
    );
    assert!(!out.capped);
    let got = candidate_shapes(&out.words);
    let filled = vec![cd(&g, "char_a")]; // both slots unapplied
    let skipped = vec![cd(&g, "char_a"), cd(&g, "char_p")]; // slot 0 skipped
    assert!(
        got.contains(&filled),
        "slot-filled analysis [a] present; got {got:?}"
    );
    assert!(
        got.contains(&skipped),
        "slot-skipped analysis [a,p] present; got {got:?}"
    );
    assert_eq!(got, {
        let mut v = vec![
            vec![cd(&g, "char_a"), cd(&g, "char_p"), cd(&g, "char_k")],
            skipped.clone(),
            filled.clone(),
        ];
        v.sort();
        v
    });
}

#[test]
fn mandatory_slot_suppresses_the_skipped_analysis() {
    // Same template, slot 0 made MANDATORY. Now the "skip slot 0" fall-through returns without
    // adding "ap", so only the fully-unapplied "a" survives from the template.
    //   Analysis set = { apk (seed), a }  — the "ap" candidate is gone.
    let (g, s) = template_stratum(false);
    let out = analyze_stratum(
        &g,
        s,
        word(&g, "apk", s),
        &AnalyzerConfig::default(),
        &StepBudget::new(usize::MAX),
    );
    assert!(!out.capped);
    let got = candidate_shapes(&out.words);
    assert!(
        !got.contains(&vec![cd(&g, "char_a"), cd(&g, "char_p")]),
        "mandatory slot 0 must suppress the slot-skipped [a,p] analysis; got {got:?}"
    );
    assert!(
        got.contains(&vec![cd(&g, "char_a")]),
        "the filled [a] analysis survives; got {got:?}"
    );
}

// =================================================================================================
// Synthesis template battery (forward direction) — SynthesisAffixTemplateRule.ApplySlots.
// =================================================================================================

#[test]
fn synthesis_template_optional_slot_yields_filled_and_skipped() {
    // Same two-slot template (slot 0 OPTIONAL "p", slot 1 mandatory "k"), applied forward to root
    // "a". SynthesisAffixTemplateRule.ApplySlots ascends from slot 0:
    //   • fill slot 0 ("a"→"ap") then slot 1 ("ap"→"apk")  → "apk";
    //   • skip slot 0 (optional), fill slot 1 ("a"→"ak")    → "ak".
    // Expected synthesized shapes = { apk, ak }.
    let mut g = load_alpha_grammar();
    let (ra, rb) = (suffix_rule(&g, 200, "p"), suffix_rule(&g, 300, "k"));
    let a = push_mrule(&mut g, ra);
    let b = push_mrule(&mut g, rb);
    let tid = TemplateId(g.templates.len() as u32);
    g.templates.push(AffixTemplateDef {
        name: None,
        is_final: true,
        required_syn_fs: pg_featstruct::FsId(0),
        slots: vec![
            SlotDef {
                name: None,
                optional: true,
                rules: vec![a],
            },
            SlotDef {
                name: None,
                optional: false,
                rules: vec![b],
            },
        ],
    });
    // Root word "a" carrying a root morph (so morph attribution has a source).
    let mut root = word(&g, "a", StratumId(0));
    root.morphs
        .push(MorphRecord::new(AllomorphId(100), MorphemeId(100), 0));

    let out = synthesize_template(&g, tid, &root, 10_000);
    let got = candidate_shapes(&out);
    assert_eq!(
        got,
        {
            let mut v = vec![
                vec![cd(&g, "char_a"), cd(&g, "char_p"), cd(&g, "char_k")],
                vec![cd(&g, "char_a"), cd(&g, "char_k")],
            ];
            v.sort();
            v
        },
        "synthesis template optional slot 0 yields both filled (apk) and skipped (ak)"
    );
}

// =================================================================================================
// Part 2 — Sena structural coverage.
// =================================================================================================

fn sena_path() -> String {
    format!(
        "{}/../../../samples/data/sena-hc.xml",
        env!("CARGO_MANIFEST_DIR")
    )
}

#[test]
#[ignore = "needs local gitignored corpus data (samples/data/sena-hc.xml); run with --include-ignored"]
fn sena_analysis_stratum_terminates_on_short_words() {
    let path = sena_path();
    let Ok(xml) = std::fs::read_to_string(&path) else {
        eprintln!("skipping Sena structural test: {path} not found");
        return;
    };
    let g = pg_grammar::load(&xml).expect("Sena grammar loads");

    // Pick the analysis stratum: the one carrying morphological rules / templates.
    let stratum = g
        .strata
        .iter()
        .position(|s| !s.mrules.is_empty() || !s.templates.is_empty())
        .map(|i| StratumId(i as u8))
        .expect("Sena has a stratum with morphological rules or templates");
    let sd = &g.strata[stratum.0 as usize];
    eprintln!(
        "Sena stratum {stratum:?}: order={:?}, {} mrules, {} templates",
        sd.mrule_order,
        sd.mrules.len(),
        sd.templates.len()
    );

    // Short words only (the heavy-13 blow up unmemoized). The cap is the safety valve; unmemoized
    // even short words can exhaust it, in which case the candidate set is partial (reported).
    let cfg = AnalyzerConfig {
        merge_equivalent: true,
        max_unapplications: 0,
        max_stem_count: 2,
    };
    for text in ["leka", "kuti", "wace", "anthu", "mbuto"] {
        if pg_grammar::segment::segment(&g.char_tables[0], text).is_err() {
            eprintln!("  {text}: not segmentable against table 0 — skipped");
            continue;
        }
        // Build with per-node lanes so feature matching is real.
        let input = word(&g, text, stratum);
        let out = analyze_stratum(&g, stratum, input, &cfg, &StepBudget::new(12_000));
        assert!(
            !out.words.is_empty(),
            "{text}: candidate set always contains at least the seed"
        );
        eprintln!(
            "  {text}: {} candidate(s){}",
            out.words.len(),
            if out.capped {
                " [CAP FIRED — partial]"
            } else {
                ""
            }
        );
    }
}

// =================================================================================================
// Tier-2 #14 — `MergeEquivalentAnalyses` + `Alternatives` + `expand_alternatives` across a stratum
// boundary (`AnalysisStratumRule.cs:150-177`, `Word.cs:491-533`, `Morpher.cs:478`).
// =================================================================================================

#[test]
fn merge_equivalent_analyses_folds_homophonous_suffixes_and_expand_recovers_both() {
    // Two DIFFERENT morphological rules (idA, idB) whose suffixes are phonologically identical
    // ("n") — a homophonous-suffix pair (distinct morphology, identical phonology), the textbook
    // case `MergeEquivalentAnalyses` exists for. Stratum 0 (surface) unapplies either one from
    // "agn", landing on the SAME shape "ag" via two different mrule histories —
    // `AnalysisStratumRule`'s shape-keyed merge folds the second into the first's `Alternatives`
    // (cs:166-172) rather than emitting two top-level candidates. Stratum 1 (deeper) then unapplies
    // a further suffix "g" ("ag" -> "a") on whichever single canonical word descended — exactly what
    // the merge is *for*: only one word flows into the deeper (potentially expensive) stratum
    // instead of two.
    //
    // The correctness question `expand_alternatives` must answer: after both strata, does the fully
    // expanded candidate set still contain BOTH the idA and idB histories (now each also carrying
    // the deeper idZ unapplication) — i.e. is the merge lossless once expanded, matching
    // `Word.ExpandAlternatives` (Word.cs:491-533) / `Morpher.cs:478`'s contract that synthesis sees
    // exactly the candidate set a non-merging engine would have produced?
    let mut g = load_alpha_grammar();
    let rule_a = suffix_rule(&g, 200, "n"); // morpheme 200
    let rule_b = suffix_rule(&g, 201, "n"); // morpheme 201, phonologically identical suffix
    let rule_z = suffix_rule(&g, 300, "g"); // a further (deeper) suffix
    let id_a = push_mrule(&mut g, rule_a);
    let id_b = push_mrule(&mut g, rule_b);
    let id_z = push_mrule(&mut g, rule_z);
    let s0 = push_stratum(&mut g, MorphRuleOrder::Unordered, vec![id_a, id_b], vec![]);
    let s1 = push_stratum(&mut g, MorphRuleOrder::Unordered, vec![id_z], vec![]);

    let cfg = AnalyzerConfig {
        merge_equivalent: true,
        max_unapplications: 0,
        max_stem_count: 2,
    };

    // Stratum 0: "agn" -> merge collapses the idA/idB candidates sharing shape "ag" into one
    // canonical + one alternative.
    let input0 = word(&g, "agn", s0);
    let out0 = analyze_stratum(&g, s0, input0, &cfg, &StepBudget::new(10_000));
    assert!(!out0.capped);
    let ag_shape = vec![cd(&g, "char_a"), cd(&g, "char_g")];
    let canonical = out0
        .words
        .iter()
        .find(|w| char_defs(&w.shape) == ag_shape)
        .expect("stratum 0 reaches shape [a,g]")
        .clone();
    assert_eq!(
        canonical.alternatives.len(),
        1,
        "the second homophonous-suffix candidate must be folded into Alternatives, not a sibling \
         top-level candidate; canonical.mrule_apps={:?}",
        canonical.mrule_apps
    );
    // The canonical and its lone alternative are together the {idA, idB} pair, in either order.
    let mut seen_ids = vec![
        canonical.mrule_apps[0].expect("analysis always records a known rule"),
        canonical.alternatives[0].mrule_apps[0].expect("analysis always records a known rule"),
    ];
    seen_ids.sort_by_key(|id| id.0);
    let mut want_ids = vec![id_a, id_b];
    want_ids.sort_by_key(|id| id.0);
    assert_eq!(
        seen_ids, want_ids,
        "canonical + alternative together cover both idA and idB"
    );

    // Stratum 1: feed the single canonical word in — the alternative rides along inside it, never
    // itself descending as a separate candidate (the perf win: one word, not two, enters the deeper
    // stratum's analysis).
    let out1 = analyze_stratum(&g, s1, canonical, &cfg, &StepBudget::new(10_000));
    assert!(!out1.capped);
    let a_shape = vec![cd(&g, "char_a")];
    let final_word = out1
        .words
        .iter()
        .find(|w| char_defs(&w.shape) == a_shape)
        .expect("stratum 1 reaches shape [a]")
        .clone();

    // The payoff: expand_alternatives reconstructs BOTH histories, each now also carrying idZ — the
    // candidate set an unmerged (keep-every-candidate) engine would have produced.
    let expanded = final_word.expand_alternatives();
    let mut got: Vec<Vec<Option<MRuleId>>> =
        expanded.iter().map(|w| w.mrule_apps.clone()).collect();
    got.sort();
    let mut want = vec![vec![Some(id_a), Some(id_z)], vec![Some(id_b), Some(id_z)]];
    want.sort();
    assert_eq!(
        got, want,
        "expand_alternatives must recover both the idA+idZ and idB+idZ histories, matching the \
         signatures a non-merging engine would have produced"
    );
    for w in &expanded {
        assert_eq!(
            char_defs(&w.shape),
            a_shape,
            "every expanded alternative shares the final shape"
        );
    }
}

// =================================================================================================
// Fix 2 regression gate — `--word-timeout-ms` must be enforced during SYNTHESIS, not just analysis.
// =================================================================================================
//
// WHY THIS IS DETERMINISTIC (and why a wall-clock-race version was tried and rejected): an earlier
// version of this regression guard (`pg-parse/tests/word_timeout_synthesis_gate.rs`, since deleted)
// tried to prove the bug via a real corpus word, racing a wall-clock `--word-timeout-ms` deadline
// against the boundary between a word's analysis phase and its synthesis phase — banking on analysis
// alone finishing (or step-capping) well inside the deadline, so the deadline was still "live" once
// synthesis began. That boundary's timing is machine-dependent: on the test machine analysis alone
// took LONGER than the configured deadline, so the deadline fired DURING analysis, the test's
// `capped` assertion failed, and even when it didn't fail, a deadline caught during analysis proves
// nothing about synthesis at all. There is no wall-clock deadline that can be placed reliably inside
// a `[analysis_time, analysis_time + synthesis_time]` window whose two ends both vary by machine.
//
// `synthesize_stratum_traced` (`pg_rules::stratum`) is pure synthesis — no analysis, no corpus, no
// I/O — and takes a `&StepBudget` directly. Instead of racing the clock, drive it with a budget whose
// deadline has ALREADY elapsed at construction (`with_timeout(Some(Duration::ZERO))`: the deadline
// instant equals the constructing `Instant::now()`, so by the time any code runs,
// `Instant::now() >= deadline` is unconditionally true). Every synthesis entry point this fix touches
// (`synth_apply_mrules`/`synth_apply_templates`/`guided_template_apply`/`synth_slots_generic`, plus
// `synthesize_stratum_traced`'s own trailing prule fold) calls `budget.deadline_expired()` before
// doing any work, so a pre-expired budget makes the very first check bail out AND latch
// `timed_out() == true` — deterministically, on every machine, with no timing window at all. Pre-fix,
// `synthesize_stratum_traced` had no `&StepBudget` parameter whatsoever, so this exact test could not
// even compile against the old signature: it is inherently a post-fix-only proof, unlike the deleted
// wall-clock version, which could "pass" (via `timed_out` set during analysis) without ever having
// exercised the synthesis code path the fix actually changed.
//
// Do NOT restore the wall-clock-race version if this one is ever found "less realistic" — the whole
// point is that this one cannot flake by construction, while that one already flaked once.
#[test]
fn synth_stratum_traced_pre_expired_deadline_times_out_and_cuts_the_walk_short() {
    // `synthesize_stratum_traced` is GUIDED synthesis: `guided_synth` (this crate's `stratum.rs`)
    // only reapplies a rule that the word's OWN `mrule_apps`/`mrule_app_index` trail says is next --
    // a hand-built `Word` with no trail (`mrule_app_index == -1`, `Word::new`'s default) synthesizes
    // zero candidates unconditionally, deadline or not, which would prove nothing. So this fixture
    // hand-sets a two-step confirmed unapplication trail -- root "a" + suffix A "p" + suffix B "k"
    // -> surface "apk", applied in DECLARATION order (`SynthesisStratumRule.cs:27-40`: synthesis
    // does NOT reverse mrule order the way analysis does) -- exactly the same "confirmed trail"
    // shape `template_partial_gate.rs`'s gate 3/4 tests use to drive the real cached production
    // path (`synth_apply_mrules` -> `guided_synth` -> `synthesize_cached_traced` ->
    // `synth_affix_cached`) without needing a prior analysis call.
    //
    // `RuleCache::build`/`synth_affix_cached` resolve allomorphs through `g.allomorph_owners`, so
    // this uses `push_cache_suffix_rule` (registers there), NOT this file's plain `suffix_rule`/
    // `push_mrule` (used by every other test above, all of which stay on the UNCACHED
    // `analyze_stratum`/`synthesize_template` entry points and never touch `allomorph_owners`).
    let mut g = load_alpha_grammar();
    let a = push_cache_suffix_rule(&mut g, 200, "p");
    let b = push_cache_suffix_rule(&mut g, 300, "k");
    let s = push_stratum(&mut g, MorphRuleOrder::Linear, vec![a, b], vec![]);

    let mut root = word(&g, "a", s);
    // The trail a real analysis of "apk" would have produced: outer suffix "k" (rule B) unapplied
    // first (pushed at index 0), then inner suffix "p" (rule A) unapplied second (pushed at index
    // 1, hence `mrule_app_index == 1` -- `Word::morphological_rule_unapplied`'s "index = len - 1"
    // invariant). Guided synthesis walks the trail back off the end: rule A (index 1) first,
    // decrementing to index 0, then rule B.
    root.mrule_apps = vec![Some(b), Some(a)];
    root.mrule_app_index = 1;

    let cache = RuleCache::build(&g);
    const CAP: usize = 10_000;

    // STEP A -- baseline: no deadline armed at all (`StepBudget::new(usize::MAX)`, matching every
    // other step-cap-only call site in this file). Establishes empirically that this trail
    // genuinely drives synthesis back toward the original surface, and records the uninterrupted
    // output count N.
    let full_budget = StepBudget::new(usize::MAX);
    let full = synthesize_stratum_traced(
        &g,
        s,
        root.clone(),
        CAP,
        &cache,
        &full_budget,
        &NoopSink,
        TraceHandle::DUMMY,
    );
    assert!(
        !full_budget.timed_out(),
        "no --word-timeout-ms deadline was armed -- must never time out"
    );
    let n = full.len();
    eprintln!(
        "baseline (no deadline) synthesis produced {n} word(s), shapes={:?}",
        candidate_shapes(&full)
    );
    assert!(
        n >= 1,
        "the analysis root must genuinely drive synthesis to at least one output; got {n}"
    );
    let surface_shape = vec![cd(&g, "char_a"), cd(&g, "char_p"), cd(&g, "char_k")];
    assert!(
        candidate_shapes(&full).contains(&surface_shape),
        "the uninterrupted synthesis walk must reconstruct the original surface [a,p,k]; got {:?}",
        candidate_shapes(&full)
    );

    // STEP B -- the guard: an otherwise-identical call, but with a budget whose deadline already
    // elapsed at construction. The load-bearing assertion is `timed_out() == true` coming out of a
    // pure-synthesis call -- proof synthesis itself now consults the wall-clock deadline.
    let expired_budget = StepBudget::new(usize::MAX).with_timeout(Some(Duration::ZERO));
    let capped = synthesize_stratum_traced(
        &g,
        s,
        root,
        CAP,
        &cache,
        &expired_budget,
        &NoopSink,
        TraceHandle::DUMMY,
    );
    assert!(
        expired_budget.timed_out(),
        "a pre-expired --word-timeout-ms deadline must be caught by synthesis's own \
         `budget.deadline_expired()` checks (synth_apply_mrules/synth_apply_templates/\
         guided_template_apply/synth_slots_generic) -- pre-fix, `synthesize_stratum_traced` took no \
         `&StepBudget` parameter at all and could not observe any deadline during synthesis"
    );
    assert!(
        !expired_budget.capped(),
        "the step cap (usize::MAX) must never fire -- this budget's `timed_out` must come from the \
         deadline, not the step count"
    );
    if n > 1 {
        assert!(
            capped.len() < n,
            "a pre-expired deadline must cut the synthesis walk short of its full uninterrupted \
             output ({n} word(s)); got {} word(s) -- the deadline doesn't appear to have shortened \
             the walk at all",
            capped.len()
        );
    }
}
