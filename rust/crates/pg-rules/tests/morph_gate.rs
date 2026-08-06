//! Morphological-rule tests: hand-built affix-process/compounding rules against expected shapes and morph records, cross-referenced to C#'s HermitCrab unit tests, plus structural coverage over the real Sena grammar.

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

/// Build a feature-bearing shape from `text`, filling per-node lanes so feature matching is real.
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

// Suffix: CopyFromInput + InsertSegments, mirroring C#'s `AffixProcessRuleTests.MorphosyntacticRules`. Root "apa" (100) + suffix "n" (200).

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

// Prefix: InsertSegments + CopyFromInput, mirroring C#'s `AffixProcessRuleTests.PrefixRules`. Prefix "n" (200) + root "apa" (100) → "napa".

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

// Feature-modifying simulfix: CopyFromInput + ModifyFromInput, mirroring C#'s `AffixProcessRuleTests.SimulfixRules`. Stem "ap" synthesizes to a voiced final "p"; analysis underspecifies it back.

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
    // A `ModifyFromInput` output node's `char_def` clears to `NO_CHAR_DEF`, matching C#'s always-lane-based `CharacterDefinitionTable.GetMatchingStrReps` rather than keeping the pre-modification character.
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
    // C#'s inversion (AntiFeatureStruct+Union = S ∪ ¬S) is the full mask, so the target's voi lane is fully underspecified on unapply.
    assert!(
        recovered.iter().any(|w| {
            let s = &w.shape;
            char_defs(s).len() == 2 && s.node_lanes(2)[voi] == full_voi
        }),
        "analysis underspecifies the modified feature (voi → full mask)"
    );
}

// Compounding: CopyFromInput(head)+CopyFromInput(nonHead), mirroring C#'s `CompoundingRuleTests.SimpleRules`. Head "apa" (100) + non-head "ka" (300) → "apaka".

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
    // `non_head_unapplied` pushes AND advances `non_head_app_index` in lock-step, matching C#'s `Word.NonHeadUnapplied` -- required since `Word::current_non_head()` reads by that index, not `.last()`.
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
    // `non_heads` is never cleared, matching C#'s `SynthesisCompoundingRule` (only the app index moves): the non-head stays as permanent history, which lets `Word::dedup_key()` distinguish surface-homophone compounds with distinct non-heads.
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

// Dedup scope is per-allomorph / per-subrule, never shared across the whole rule, mirroring C#'s `RemoveDuplicates()` reset fresh for each rule index.

#[test]
fn ana_affix_dedup_is_scoped_per_allomorph_not_shared_across_the_rule() {
    let g = load_alpha_grammar();
    let stem = root_word(&g, "apa", 100);
    // Two allomorphs with the identical "copy everything" pattern must each independently produce their own analysis candidate, not have one suppressed as a spurious duplicate of the other's.
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
    // The Sena shape has zero phonological features, so every segment has identical lanes and identity lives only in the char-def/StrRep dimension, which C#'s `Duplicates` does compare; a lanes-only dedup comparator would wrongly collapse distinct analyses here.
    let g = common::load_zero_feat_grammar();
    // Infixing rule un-applied to "axbxc" un-inserts either "x", yielding two same-length, different-content candidates from the same allomorph: "abxc" and "axbc".
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
    // Two subrules with the identical head+/non-head+ pattern must each independently re-derive the "apa"|"ka" split, not have one suppressed as a spurious duplicate of the other's.
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

// Sena structural coverage.

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
                // Synthesis needs a non-head; analysis does not. `non_head_unapplied` keeps `non_head_app_index` in lock-step.
                let mut with_nh = probe.clone();
                with_nh.non_head_unapplied(probe.clone());
                let _ = synthesize(&g, &with_nh, rule);
                let _ = analyze(&g, &probe, rule);
            }
            // Sena has zero RealizationalRule occurrences; nothing to census here.
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
