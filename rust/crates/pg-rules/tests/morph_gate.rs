//! M4a acceptance gate — Part 3 (morphological rules).
//!
//! Part 1: hand-built affix-process rules (suffix, prefix, feature-modifying simulfix) and a
//! compounding rule, exercised on hand-authored [`Word`]s with expected shapes + morph records
//! reasoned from HermitCrab semantics and cross-referenced to the C# unit tests
//! (`tests/SIL.Machine.Morphology.HermitCrab.Tests/MorphologicalRules/*Tests.cs`). Includes a
//! synthesize→analyze round trip (shape recovery — the load-bearing invariant, since C#
//! `AnalysisAffixProcessAllomorphRuleSpec.ApplyRhs` regenerates the shape and morphs re-attach at
//! synthesis-confirm; morph *ordering* is asserted on the synthesis side).
//!
//! Part 2: structural coverage over the real **Sena** grammar (132 affix-process + 8 compounding
//! rules, zero phonological rules): every affix rule's allomorph LHS parts compile via the bridge,
//! and `analyze`/`synthesize` run on a probe word without panic.

mod common;

use common::{load_alpha_grammar, nat_class};
use pg_grammar::chardef::CharDefId;
use pg_grammar::model::{
    AffixAllomorphDef, AffixProcessRuleDef, AllomorphId, CompoundingRuleDef, CompoundingSubruleDef,
    Grammar, MorphRuleDef, MorphemeId, MprSet, OutputAction, PartRef, Pattern, PatternNode,
    ReduplicationHint, SegmentedText, SimpleContext, StratumId, TableId, VarTable,
};
use pg_rules::morph::{analyze, synthesize};
use pg_rules::{MorphRecord, Word};
use pg_shape::{NodeKind, Shape, ShapeBuilder};

// ---- shape / word builders -----------------------------------------------------------------

/// Build a feature-bearing shape from `text` (segments against table 0, fills per-node lanes from
/// the char table so feature matching is real — unlike the loader's `feat_width == 0` `Segments`).
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

/// The interior segment/boundary char-def id sequence of a shape (for assertions).
fn char_defs(shape: &Shape) -> Vec<u32> {
    shape.interior().map(|(_, _, cd, _)| cd).collect()
}

fn cd(g: &Grammar, xml_id: &str) -> u32 {
    common::char_def(g, xml_id).0
}

/// A word carrying a single "root" morph at position 0.
fn root_word(g: &Grammar, text: &str, morpheme: u32) -> Word {
    let mut w = Word::new(shape_with_lanes(g, text), StratumId(0));
    w.morphs.push(MorphRecord::new(
        AllomorphId(morpheme),
        MorphemeId(morpheme),
        0,
    ));
    w
}

// ---- pattern / rule builders ---------------------------------------------------------------

fn ctx(nc: &str, g: &Grammar) -> SimpleContext {
    common::ctx(nat_class(g, nc))
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

/// A single-node part matching a natural class.
fn single(nc: &str, g: &Grammar) -> Pattern {
    Pattern {
        nodes: vec![PatternNode::Context(ctx(nc, g))],
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

fn affix_rule(morpheme: u32, allomorphs: Vec<AffixAllomorphDef>) -> MorphRuleDef {
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
        allomorphs,
    })
}

// =================================================================================================
// Suffix: CopyFromInput("1") + InsertSegments — C# AffixProcessRuleTests.MorphosyntacticRules
// (AffixProcessRuleTests.cs:28, `Rhs = { new CopyFromInput("1"), new InsertSegments(Table3,"s") }`,
// "sag" → "sags", morphs "32 NMLZ"). Here root "apa" (morpheme 100) + suffix "n" (morpheme 200).
// =================================================================================================

#[test]
fn suffix_synthesis_appends_and_orders_root_then_affix() {
    let g = load_alpha_grammar();
    let stem = root_word(&g, "apa", 100);
    let rule = affix_rule(
        200,
        vec![allomorph(
            200,
            vec![one_or_more("nc_any", &g)],
            vec![
                OutputAction::Copy(PartRef::Input(0)),
                insert_segments(&g, "n"),
            ],
        )],
    );

    let out = synthesize(&g, &stem, &rule);
    assert_eq!(out.len(), 1, "one synthesis output");
    // Shape: root "apa" copied, then affix "n" appended → a p a n.
    assert_eq!(
        char_defs(&out[0].shape),
        vec![
            cd(&g, "char_a"),
            cd(&g, "char_p"),
            cd(&g, "char_a"),
            cd(&g, "char_n")
        ]
    );
    // Morph order = surface order: root (100) then suffix (200).
    assert_eq!(
        out[0].morpheme_sequence(),
        vec![MorphemeId(100), MorphemeId(200)]
    );
}

#[test]
fn suffix_round_trip_recovers_stem_shape() {
    let g = load_alpha_grammar();
    let stem = root_word(&g, "apa", 100);
    let rule = affix_rule(
        200,
        vec![allomorph(
            200,
            vec![one_or_more("nc_any", &g)],
            vec![
                OutputAction::Copy(PartRef::Input(0)),
                insert_segments(&g, "n"),
            ],
        )],
    );

    let synth = synthesize(&g, &stem, &rule);
    let recovered = analyze(&g, &synth[0], &rule);
    // Some analysis (the a p a | n split) recovers exactly the stem shape.
    assert!(
        recovered.iter().any(|w| w.shape == stem.shape),
        "analyze(synthesize(stem)) recovers the stem shape; got {:?}",
        recovered
            .iter()
            .map(|w| char_defs(&w.shape))
            .collect::<Vec<_>>()
    );
}

// =================================================================================================
// Prefix: InsertSegments + CopyFromInput("1") — C# AffixProcessRuleTests.PrefixRules
// (AffixProcessRuleTests.cs:540, `Rhs = { new InsertSegments(Table3,"zi"), new CopyFromInput("1") }`,
// prefix then stem, morphs "3SG 32"). Here prefix "n" (200) + root "apa" (100) → "napa".
// =================================================================================================

#[test]
fn prefix_synthesis_prepends_and_orders_affix_then_root() {
    let g = load_alpha_grammar();
    let stem = root_word(&g, "apa", 100);
    let rule = affix_rule(
        200,
        vec![allomorph(
            200,
            vec![one_or_more("nc_any", &g)],
            vec![
                insert_segments(&g, "n"),
                OutputAction::Copy(PartRef::Input(0)),
            ],
        )],
    );

    let out = synthesize(&g, &stem, &rule);
    assert_eq!(out.len(), 1);
    assert_eq!(
        char_defs(&out[0].shape),
        vec![
            cd(&g, "char_n"),
            cd(&g, "char_a"),
            cd(&g, "char_p"),
            cd(&g, "char_a")
        ]
    );
    // Surface order: prefix (200) then root (100).
    assert_eq!(
        out[0].morpheme_sequence(),
        vec![MorphemeId(200), MorphemeId(100)]
    );
}

// =================================================================================================
// Feature-modifying simulfix: CopyFromInput("1") + ModifyFromInput("2", [+voice]) — C#
// AffixProcessRuleTests.SimulfixRules (AffixProcessRuleTests.cs:908,
// `Rhs = { new CopyFromInput("1"), new ModifyFromInput("2", voiced) }`, "pib" → morphs "41 SIMUL").
// Here root part "1" = any+, target part "2" = one consonant; modify it to [+voice].
// stem "ap" → synthesis voices the final "p" (voi lane → {+}); analysis underspecifies it back.
// =================================================================================================

#[test]
fn simulfix_synthesis_voices_target_segment() {
    let g = load_alpha_grammar();
    let voi = common::feat(&g, "feat_voi").0 as usize;
    let vp = g
        .phon_features
        .symbol_index(pg_grammar::featsys::FlatIndex(voi as u32), "sym_vp")
        .unwrap();
    let voiced_bits = 1u64 << vp; // {voi+}

    let stem = root_word(&g, "ap", 100);
    let rule = affix_rule(
        200,
        vec![allomorph(
            200,
            vec![one_or_more("nc_any", &g), single("nc_cons", &g)],
            vec![
                OutputAction::Copy(PartRef::Input(0)),
                OutputAction::Modify(PartRef::Input(1), ctx("nc_voiced", &g)),
            ],
        )],
    );

    let out = synthesize(&g, &stem, &rule);
    assert_eq!(out.len(), 1);
    // Two segments: "a" (unchanged) and the modified "p" (voi lane now {+}). Plan item 1 / wave-3
    // fix: a `ModifyFromInput` output node's `char_def` is now cleared to `NO_CHAR_DEF` (was
    // incorrectly asserted here as staying literally "p" -- that was exactly the bug
    // `csharp_port_affix_process.rs::simulfix_rules` found end-to-end: a modified segment kept
    // rendering/matching as its PRE-modification character forever). `pg_shape::Shape::node_cd_set`
    // now correctly falls back to lane-only unification for this node, matching C#'s own
    // always-lane-based `CharacterDefinitionTable.GetMatchingStrReps`.
    let s = &out[0].shape;
    assert_eq!(char_defs(s), vec![cd(&g, "char_a"), pg_shape::NO_CHAR_DEF]);
    assert_eq!(
        s.node_lanes(2)[voi],
        voiced_bits,
        "modified segment's voi lane is priority-unioned to {{voi+}}"
    );
    // Morph order: root (100) then the simulfix (200), which owns the modified segment.
    assert_eq!(
        out[0].morpheme_sequence(),
        vec![MorphemeId(100), MorphemeId(200)]
    );
}

#[test]
fn simulfix_analysis_underspecifies_modified_feature() {
    let g = load_alpha_grammar();
    let voi = common::feat(&g, "feat_voi").0 as usize;
    let full_voi = g
        .phon_features
        .mask(pg_grammar::featsys::FlatIndex(voi as u32));

    let stem = root_word(&g, "ap", 100);
    let rule = affix_rule(
        200,
        vec![allomorph(
            200,
            vec![one_or_more("nc_any", &g), single("nc_cons", &g)],
            vec![
                OutputAction::Copy(PartRef::Input(0)),
                OutputAction::Modify(PartRef::Input(1), ctx("nc_voiced", &g)),
            ],
        )],
    );

    let synth = synthesize(&g, &stem, &rule);
    let recovered = analyze(&g, &synth[0], &rule);
    // The C# inversion is AntiFeatureStruct+Union = S ∪ ¬S = full mask: the target's voi lane is
    // underspecified on unapply. Assert some analysis has 2 segments with voi fully underspecified.
    assert!(
        recovered.iter().any(|w| {
            let s = &w.shape;
            char_defs(s).len() == 2 && s.node_lanes(2)[voi] == full_voi
        }),
        "analysis underspecifies the modified feature (voi → full mask)"
    );
}

// =================================================================================================
// Compounding: CopyFromInput(head) + CopyFromInput(nonHead) — C# CompoundingRuleTests.SimpleRules
// (CompoundingRuleTests.cs:20, `Rhs = { CopyFromInput("head"), InsertSegments("+"), CopyFromInput
// ("nonHead") }`, morphs "5 8"/"5 9"). The alpha grammar has no boundary char, so no "+" linker;
// head "apa" (100) + non-head "ka" (300) → "apaka", morphs [100, 300].
// =================================================================================================

fn compound_rule() -> MorphRuleDef {
    MorphRuleDef::Compounding(CompoundingRuleDef {
        xml_id: "c".into(),
        name: None,
        blockable: false,
        max_apps: 1,
        head_required_syn_fs: pg_featstruct::FsId(0),
        non_head_required_syn_fs: pg_featstruct::FsId(0),
        out_syn_fs: pg_featstruct::FsId(0),
        head_prod_restrictions_mpr: MprSet::EMPTY,
        non_head_prod_restrictions_mpr: MprSet::EMPTY,
        output_prod_restrictions_mpr: MprSet::EMPTY,
        obligatory_features: vec![],
        subrules: vec![CompoundingSubruleDef {
            vars: VarTable::default(),
            required_mpr: MprSet::EMPTY,
            excluded_mpr: MprSet::EMPTY,
            out_mpr: MprSet::EMPTY,
            head_lhs: vec![], // filled per test (needs the grammar)
            non_head_lhs: vec![],
            rhs: vec![
                OutputAction::Copy(PartRef::Head(0)),
                OutputAction::Copy(PartRef::NonHead(0)),
            ],
        }],
    })
}

fn compound_rule_with(g: &Grammar) -> MorphRuleDef {
    let mut r = compound_rule();
    if let MorphRuleDef::Compounding(def) = &mut r {
        def.subrules[0].head_lhs = vec![one_or_more("nc_any", g)];
        def.subrules[0].non_head_lhs = vec![one_or_more("nc_any", g)];
    }
    r
}

#[test]
fn compound_synthesis_joins_head_and_non_head() {
    let g = load_alpha_grammar();
    let mut head = root_word(&g, "apa", 100);
    // `non_head_unapplied` (not a raw `.push`): pushes AND advances `non_head_app_index` in
    // lock-step, matching C#'s `Word.NonHeadUnapplied` (Word.cs:477-482) -- required since P4
    // (2026-07-09) made `Word::current_non_head()` read by that index rather than `.last()`.
    head.non_head_unapplied(root_word(&g, "ka", 300));
    let rule = compound_rule_with(&g);

    let out = synthesize(&g, &head, &rule);
    assert_eq!(out.len(), 1);
    assert_eq!(
        char_defs(&out[0].shape),
        vec![
            cd(&g, "char_a"),
            cd(&g, "char_p"),
            cd(&g, "char_a"),
            cd(&g, "char_k"),
            cd(&g, "char_a")
        ]
    );
    // Head root (100) then non-head root (300), by surface position.
    assert_eq!(
        out[0].morpheme_sequence(),
        vec![MorphemeId(100), MorphemeId(300)]
    );
    // The current non-head's material was consumed into the compound's `shape`/`morphs`, but
    // (P4, 2026-07-09) `non_heads` itself is NOT cleared: C#'s `SynthesisCompoundingRule` never
    // removes an entry from `_nonHeadApps` (only `_nonHeadAppIndex` moves, via the confirmation
    // step in `stratum.rs`'s `guided_synth`, not exercised by this raw `morph::synthesize` call) --
    // the non-head stays as permanent history, which is exactly what lets `Word::dedup_key()`
    // distinguish two compounds built from surface-homophone but lexically distinct non-heads
    // (see `pg-parse/tests/csharp_port_compounding.rs`'s
    // `simple_rules_1_homophone_disjunction_finding`).
    assert_eq!(out[0].non_heads.len(), 1);
    assert_eq!(
        char_defs(&out[0].non_heads[0].shape),
        char_defs(&shape_with_lanes(&g, "ka"))
    );
}

#[test]
fn compound_analysis_splits_into_head_and_non_head() {
    let g = load_alpha_grammar();
    let input = root_word(&g, "apaka", 100);
    let rule = compound_rule_with(&g);

    let out = analyze(&g, &input, &rule);
    // Among the splits, the head="apa" / non-head="ka" partition is present.
    let want_head = char_defs(&shape_with_lanes(&g, "apa"));
    let want_nh = char_defs(&shape_with_lanes(&g, "ka"));
    assert!(
        out.iter().any(|w| {
            char_defs(&w.shape) == want_head
                && w.current_non_head().map(|nh| char_defs(&nh.shape)) == Some(want_nh.clone())
        }),
        "compound analysis yields the apa|ka split; got {:?}",
        out.iter()
            .map(|w| (
                char_defs(&w.shape),
                w.current_non_head().map(|nh| char_defs(&nh.shape))
            ))
            .collect::<Vec<_>>()
    );
}

// =================================================================================================
// Tier-2 #10 dedup scope: per-allomorph / per-subrule, NOT shared across the whole rule
// (HermitCrabExtensions.cs:180-207 `Duplicates`/`RemoveDuplicates`; AnalysisAffixProcessRule.cs:58
// `_rules[i].Apply(input).RemoveDuplicates()`; AnalysisCompoundingRule.cs:56-58 `srOutput`, both
// reset fresh for each `i`).
// =================================================================================================

#[test]
fn ana_affix_dedup_is_scoped_per_allomorph_not_shared_across_the_rule() {
    let g = load_alpha_grammar();
    let stem = root_word(&g, "apa", 100);
    // Two allomorphs with the identical "copy everything" pattern: each, run independently
    // against the same word, produces exactly one (identical-shaped) analysis candidate. Before
    // the Tier-2 #10 fix, `ana_affix` shared a single dedup set across every allomorph of the
    // rule, so allomorph 201's candidate would spuriously be suppressed as a "duplicate" of
    // allomorph 200's — even though C# resets the dedup set per-allomorph
    // (`_rules[i].Apply(input).RemoveDuplicates()`, a fresh `RemoveDuplicates()` call each `i`).
    let rule = affix_rule(
        200,
        vec![
            allomorph(
                200,
                vec![one_or_more("nc_any", &g)],
                vec![OutputAction::Copy(PartRef::Input(0))],
            ),
            allomorph(
                201,
                vec![one_or_more("nc_any", &g)],
                vec![OutputAction::Copy(PartRef::Input(0))],
            ),
        ],
    );
    let out = analyze(&g, &stem, &rule);
    let matching = out.iter().filter(|w| w.shape == stem.shape).count();
    assert_eq!(
        matching, 2,
        "each allomorph's identical-shaped analysis must survive independently, got {matching}"
    );
}

#[test]
fn ana_affix_dedup_distinguishes_segment_identity_on_zero_feature_grammars() {
    // The Sena shape: zero phonological features, so EVERY segment has identical lanes (just the
    // always-appended Type lane) and identity lives only in the char-def/StrRep dimension — which
    // C#'s `Duplicates` DOES compare (`NodeComparer` projects the node's whole
    // `Annotation.FeatureStruct`, and on a zero-feature grammar that FS is Type + StrRep,
    // CharacterDefinitionTable.cs:68-76 / XmlLanguageLoader.cs:670-673). A lanes-only comparator
    // (the original Tier-2 #10 comparator, corrected by this test's commit) treated ANY two
    // same-length candidates as duplicates and longer-wins-collapsed genuinely distinct analyses.
    let g = common::load_zero_feat_grammar();
    // Infixing rule: LHS [all+, all+], RHS [Copy(0), InsertSegments("x"), Copy(1)]. Un-applying
    // it to "axbxc" un-inserts either "x", yielding two SAME-LENGTH, DIFFERENT-CONTENT candidates
    // from the SAME allomorph: "abxc" (first x consumed) and "axbc" (second x consumed).
    let stem = root_word(&g, "axbxc", 100);
    let rule = affix_rule(
        200,
        vec![allomorph(
            200,
            vec![one_or_more("nc_all", &g), one_or_more("nc_all", &g)],
            vec![
                OutputAction::Copy(PartRef::Input(0)),
                insert_segments(&g, "x"),
                OutputAction::Copy(PartRef::Input(1)),
            ],
        )],
    );
    let out = analyze(&g, &stem, &rule);
    let non_optional_cds = |w: &Word| -> Vec<u32> {
        (0..w.shape.len())
            .filter(|&i| w.shape.kind(i) == NodeKind::Segment && !w.shape.flags(i).is_optional())
            .map(|i| w.shape.char_def(i))
            .collect()
    };
    let abxc = vec![
        cd(&g, "char_a"),
        cd(&g, "char_b"),
        cd(&g, "char_x"),
        cd(&g, "char_c"),
    ];
    let axbc = vec![
        cd(&g, "char_a"),
        cd(&g, "char_x"),
        cd(&g, "char_b"),
        cd(&g, "char_c"),
    ];
    let got: Vec<Vec<u32>> = out.iter().map(non_optional_cds).collect();
    assert!(
        got.contains(&abxc) && got.contains(&axbc),
        "both same-length different-content candidates must survive dedup \
         (lanes are identical on a zero-feature grammar; identity is the char-def set); got {got:?}"
    );
}

fn compound_subrule_any_plus_any(g: &Grammar) -> CompoundingSubruleDef {
    CompoundingSubruleDef {
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
    }
}

#[test]
fn ana_compound_dedup_is_scoped_per_subrule_not_shared_across_the_rule() {
    let g = load_alpha_grammar();
    let input = root_word(&g, "apaka", 100);
    // Two subrules with the identical head+/non-head+ pattern, so each independently re-derives
    // the same set of head|non-head splits (including "apa"|"ka"). Before the Tier-2 #10 fix,
    // `ana_compound` shared a single dedup set across every subrule of the rule, so subrule 1's
    // "apa"|"ka" split would spuriously be suppressed as a duplicate of subrule 0's — even though
    // C# resets `srOutput` fresh for each subrule index `i` (AnalysisCompoundingRule.cs:56-58).
    let mut rule = compound_rule();
    if let MorphRuleDef::Compounding(def) = &mut rule {
        def.subrules = vec![
            compound_subrule_any_plus_any(&g),
            compound_subrule_any_plus_any(&g),
        ];
    }
    let out = analyze(&g, &input, &rule);
    let want_head = char_defs(&shape_with_lanes(&g, "apa"));
    let want_nh = char_defs(&shape_with_lanes(&g, "ka"));
    let matching = out
        .iter()
        .filter(|w| {
            char_defs(&w.shape) == want_head
                && w.current_non_head().map(|nh| char_defs(&nh.shape)) == Some(want_nh.clone())
        })
        .count();
    assert_eq!(
        matching, 2,
        "each subrule's identical apa|ka split must survive independently, got {matching}"
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
fn sena_affix_and_compounding_rules_compile_and_run_without_panic() {
    let path = sena_path();
    let Ok(xml) = std::fs::read_to_string(&path) else {
        eprintln!("skipping Sena structural test: {path} not found");
        return;
    };
    let g = pg_grammar::load(&xml).expect("Sena grammar loads");

    // A probe word from the first lexical entry's first allomorph (guaranteed segmentable).
    let probe_text = g
        .entries
        .iter()
        .find_map(|e| e.allomorphs.first().map(|a| a.shape.text.clone()))
        .expect("Sena has a lexical entry");
    let probe_shape =
        pg_grammar::segment::segment(&g.char_tables[0], &probe_text).expect("probe segments");
    let mut probe = Word::new(probe_shape, StratumId(0));
    probe
        .morphs
        .push(MorphRecord::new(AllomorphId(0), MorphemeId(0), 0));

    let mut affix_rules = 0usize;
    let mut compounding_rules = 0usize;
    let mut allomorphs = 0usize;
    let mut parts_compiled = 0usize;

    for rule in &g.mrules {
        match rule {
            MorphRuleDef::AffixProcess(def) => {
                affix_rules += 1;
                let bridge = pg_rules::bridge::PatternBridge::new(&g).with_table(TableId(0));
                for allo in &def.allomorphs {
                    allomorphs += 1;
                    for part in &allo.lhs {
                        bridge
                            .compile_pattern(part)
                            .unwrap_or_else(|e| panic!("affix LHS part failed to compile: {e}"));
                        parts_compiled += 1;
                    }
                }
                // Both directions run without panic on the probe word.
                let _ = synthesize(&g, &probe, rule);
                let _ = analyze(&g, &probe, rule);
            }
            MorphRuleDef::Compounding(def) => {
                compounding_rules += 1;
                let bridge = pg_rules::bridge::PatternBridge::new(&g).with_table(TableId(0));
                for sr in &def.subrules {
                    for part in sr.head_lhs.iter().chain(sr.non_head_lhs.iter()) {
                        bridge
                            .compile_pattern(part)
                            .unwrap_or_else(|e| panic!("compound LHS part failed to compile: {e}"));
                        parts_compiled += 1;
                    }
                }
                // Synthesis needs a non-head; analysis does not. `non_head_unapplied` (not a raw
                // `.push`) keeps `non_head_app_index` in lock-step -- see the sibling fix above.
                let mut with_nh = probe.clone();
                with_nh.non_head_unapplied(probe.clone());
                let _ = synthesize(&g, &with_nh, rule);
                let _ = analyze(&g, &probe, rule);
            }
            // Sena has zero RealizationalRule occurrences (W5's realizational cluster is exercised
            // by rust/conformance/realizational/* instead); nothing to census here.
            MorphRuleDef::Realizational(_) => {}
        }
    }

    eprintln!(
        "Sena structural coverage: {affix_rules} affix-process rules, {compounding_rules} \
         compounding rules, {allomorphs} affix allomorphs, {parts_compiled} LHS parts compiled; \
         analyze+synthesize ran on probe word {probe_text:?} without panic"
    );
    assert!(
        affix_rules >= 100,
        "expected ~132 affix-process rules, got {affix_rules}"
    );
    assert!(
        compounding_rules >= 8,
        "expected 8 compounding rules, got {compounding_rules}"
    );
    assert!(parts_compiled > 0);
}
