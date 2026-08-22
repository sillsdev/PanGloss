//! Pins the closed templated-morphology classifier grammar with invented construct witnesses.

use std::panic::{catch_unwind, AssertUnwindSafe};

use pg_foma::structural_allomorph::{
    MarkerZone, MorphologyRewrite, MorphologyRewriteClassifier, ZoneRequirement,
};
use pg_grammar::chardef::CharDefId;
use pg_grammar::model::{Grammar, MorphRuleDef, OutputAction, PartRef, PatternNode, TableId};

const CLASSIFIER_XML: &str = r#"
<HermitCrabInput><Language><Name>synthetic-templated-closed-grammar</Name>
  <PartsOfSpeech><PartOfSpeech id="p"><Name>P</Name></PartOfSpeech></PartsOfSpeech>
  <CharacterDefinitionTable id="active"><Name>Active</Name><SegmentDefinitions>
    <SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
    <SegmentDefinition id="cb"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
    <SegmentDefinition id="cc"><Representations><Representation>c</Representation></Representations></SegmentDefinition>
    <SegmentDefinition id="cd"><Representations><Representation>d</Representation></Representations></SegmentDefinition>
    <SegmentDefinition id="ce"><Representations><Representation>e</Representation></Representations></SegmentDefinition>
    <SegmentDefinition id="cx"><Representations><Representation>x</Representation></Representations></SegmentDefinition>
    <SegmentDefinition id="cy"><Representations><Representation>y</Representation></Representations></SegmentDefinition>
    <SegmentDefinition id="cp"><Representations><Representation>p</Representation><Representation>P</Representation></Representations></SegmentDefinition>
    <SegmentDefinition id="cs"><Representations><Representation>s</Representation><Representation>S</Representation></Representations></SegmentDefinition>
  </SegmentDefinitions></CharacterDefinitionTable>
  <CharacterDefinitionTable id="foreign"><Name>Foreign</Name><SegmentDefinitions>
    <SegmentDefinition id="cf0"><Representations><Representation>!</Representation></Representations></SegmentDefinition>
    <SegmentDefinition id="cf1"><Representations><Representation>?</Representation></Representations></SegmentDefinition>
    <SegmentDefinition id="cf2"><Representations><Representation>#</Representation></Representations></SegmentDefinition>
    <SegmentDefinition id="cf3"><Representations><Representation>$</Representation></Representations></SegmentDefinition>
    <SegmentDefinition id="cf4"><Representations><Representation>%</Representation></Representations></SegmentDefinition>
    <SegmentDefinition id="cf5"><Representations><Representation>^</Representation><Representation>x</Representation></Representations></SegmentDefinition>
    <SegmentDefinition id="cf6"><Representations><Representation>&amp;</Representation></Representations></SegmentDefinition>
    <SegmentDefinition id="cf7"><Representations><Representation>*</Representation></Representations></SegmentDefinition>
  </SegmentDefinitions></CharacterDefinitionTable>
  <NaturalClasses>
    <SegmentNaturalClass id="ncAny"><Name>Any</Name>
      <Segment segment="ca"/><Segment segment="cb"/><Segment segment="cc"/><Segment segment="cd"/>
      <Segment segment="ce"/><Segment segment="cx"/><Segment segment="cy"/><Segment segment="cp"/><Segment segment="cs"/>
    </SegmentNaturalClass>
    <SegmentNaturalClass id="ncVowel"><Name>Vowel</Name><Segment segment="ca"/></SegmentNaturalClass>
    <SegmentNaturalClass id="ncEmpty"><Name>Empty</Name></SegmentNaturalClass>
  </NaturalClasses>
  <Strata><Stratum characterDefinitionTable="active" morphologicalRules="mr">
    <Name>synthetic</Name><MorphologicalRuleDefinitions><MorphologicalRule id="mr" requiredPartsOfSpeech="p" outputPartOfSpeech="p">
      <Name>synthetic</Name><MorphologicalSubrules>
        <!-- 0: ordinary literal; 1: empty ordinary -->
        <MorphologicalSubrule id="ordinary"><MorphologicalInput><PhoneticSequence id="o"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny"/></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput><MorphologicalOutput><InsertSegments><PhoneticShape>x</PhoneticShape></InsertSegments></MorphologicalOutput></MorphologicalSubrule>
        <MorphologicalSubrule id="empty"><MorphologicalInput><PhoneticSequence id="e"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny"/></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput><MorphologicalOutput/></MorphologicalSubrule>
        <!-- 2: direct whole-root wrapper, with a 2x2 prefix/suffix variant product -->
        <MorphologicalSubrule id="wrapper"><MorphologicalInput><PhoneticSequence id="w"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny"/></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput><MorphologicalOutput><InsertSegments><PhoneticShape>p</PhoneticShape></InsertSegments><CopyFromInput index="w"/><InsertSegments><PhoneticShape>s</PhoneticShape></InsertSegments></MorphologicalOutput></MorphologicalSubrule>
        <!-- 3/4: one and two interior insertion runs -->
        <MorphologicalSubrule id="infix-one"><MorphologicalInput><PhoneticSequence id="i0"><SimpleContext naturalClass="ncAny"/></PhoneticSequence><PhoneticSequence id="i1"><SimpleContext naturalClass="ncAny"/></PhoneticSequence><PhoneticSequence id="i2"><SimpleContext naturalClass="ncAny"/></PhoneticSequence></MorphologicalInput><MorphologicalOutput><CopyFromInput index="i0"/><InsertSegments><PhoneticShape>x</PhoneticShape></InsertSegments><CopyFromInput index="i1"/><CopyFromInput index="i2"/></MorphologicalOutput></MorphologicalSubrule>
        <MorphologicalSubrule id="infix-two"><MorphologicalInput><PhoneticSequence id="j0"><SimpleContext naturalClass="ncAny"/></PhoneticSequence><PhoneticSequence id="j1"><SimpleContext naturalClass="ncAny"/></PhoneticSequence><PhoneticSequence id="j2"><SimpleContext naturalClass="ncAny"/></PhoneticSequence></MorphologicalInput><MorphologicalOutput><CopyFromInput index="j0"/><InsertSegments><PhoneticShape>x</PhoneticShape></InsertSegments><CopyFromInput index="j1"/><InsertSegments><PhoneticShape>y</PhoneticShape></InsertSegments><CopyFromInput index="j2"/></MorphologicalOutput></MorphologicalSubrule>
        <!-- 5: terminal Modify over a proven one-segment final part -->
        <MorphologicalSubrule id="terminal-modify"><MorphologicalInput><PhoneticSequence id="t0"><SimpleContext naturalClass="ncAny"/></PhoneticSequence><PhoneticSequence id="t1"><Segment segment="ca"/></PhoneticSequence></MorphologicalInput><MorphologicalOutput><CopyFromInput index="t0"/><ModifyFromInput index="t1"><SimpleContext naturalClass="ncAny"/></ModifyFromInput></MorphologicalOutput></MorphologicalSubrule>
        <!-- 6: initial replacement requires exactly one fixed CharDef vowel, not a broad class -->
        <MorphologicalSubrule id="initial-replace"><MorphologicalInput><PhoneticSequence id="v0"><Segment segment="ca"/></PhoneticSequence><PhoneticSequence id="v1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny"/></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput><MorphologicalOutput><InsertSegments><PhoneticShape>p</PhoneticShape></InsertSegments><CopyFromInput index="v1"/></MorphologicalOutput></MorphologicalSubrule>
        <!-- 7/8: existing bounded adjacent terminal and initial drops -->
        <MorphologicalSubrule id="drop-terminal"><MorphologicalInput><PhoneticSequence id="d0"><SimpleContext naturalClass="ncAny"/></PhoneticSequence><PhoneticSequence id="d1"><SimpleContext naturalClass="ncAny"/></PhoneticSequence></MorphologicalInput><MorphologicalOutput><CopyFromInput index="d0"/><InsertSegments><PhoneticShape>x</PhoneticShape></InsertSegments></MorphologicalOutput></MorphologicalSubrule>
        <MorphologicalSubrule id="drop-initial"><MorphologicalInput><PhoneticSequence id="q0"><Segment segment="ca"/></PhoneticSequence><PhoneticSequence id="q1"><SimpleContext naturalClass="ncAny"/></PhoneticSequence></MorphologicalInput><MorphologicalOutput><CopyFromInput index="q1"/></MorphologicalOutput></MorphologicalSubrule>
        <!-- 9: InsertContext is never part of the closed grammar -->
        <MorphologicalSubrule id="insert-context"><MorphologicalInput><PhoneticSequence id="c0"><SimpleContext naturalClass="ncAny"/></PhoneticSequence></MorphologicalInput><MorphologicalOutput><CopyFromInput index="c0"/><InsertSimpleContext><SimpleContext naturalClass="ncAny"/></InsertSimpleContext></MorphologicalOutput></MorphologicalSubrule>
        <!-- 10: nonterminal Modify -->
        <MorphologicalSubrule id="modify-nonterminal"><MorphologicalInput><PhoneticSequence id="n0"><SimpleContext naturalClass="ncAny"/></PhoneticSequence><PhoneticSequence id="n1"><SimpleContext naturalClass="ncAny"/></PhoneticSequence></MorphologicalInput><MorphologicalOutput><ModifyFromInput index="n0"><SimpleContext naturalClass="ncAny"/></ModifyFromInput><CopyFromInput index="n1"/></MorphologicalOutput></MorphologicalSubrule>
        <!-- 11: multi-segment terminal Modify -->
        <MorphologicalSubrule id="modify-multi"><MorphologicalInput><PhoneticSequence id="m0"><SimpleContext naturalClass="ncAny"/></PhoneticSequence><PhoneticSequence id="m1"><SimpleContext naturalClass="ncAny"/><SimpleContext naturalClass="ncAny"/></PhoneticSequence></MorphologicalInput><MorphologicalOutput><CopyFromInput index="m0"/><ModifyFromInput index="m1"><SimpleContext naturalClass="ncAny"/></ModifyFromInput></MorphologicalOutput></MorphologicalSubrule>
        <!-- 12: quantified terminal Modify -->
        <MorphologicalSubrule id="modify-quantified"><MorphologicalInput><PhoneticSequence id="u0"><SimpleContext naturalClass="ncAny"/></PhoneticSequence><PhoneticSequence id="u1"><OptionalSegmentSequence min="1" max="2"><SimpleContext naturalClass="ncAny"/></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput><MorphologicalOutput><CopyFromInput index="u0"/><ModifyFromInput index="u1"><SimpleContext naturalClass="ncAny"/></ModifyFromInput></MorphologicalOutput></MorphologicalSubrule>
        <!-- 13: empty modification output -->
        <MorphologicalSubrule id="modify-empty"><MorphologicalInput><PhoneticSequence id="z0"><SimpleContext naturalClass="ncAny"/></PhoneticSequence><PhoneticSequence id="z1"><Segment segment="ca"/></PhoneticSequence></MorphologicalInput><MorphologicalOutput><CopyFromInput index="z0"/><ModifyFromInput index="z1"><SimpleContext naturalClass="ncEmpty"/></ModifyFromInput></MorphologicalOutput></MorphologicalSubrule>
        <!-- 14: foreign-table output cannot translate to the active table -->
        <MorphologicalSubrule id="foreign-literal"><MorphologicalInput><PhoneticSequence id="f0"><SimpleContext naturalClass="ncAny"/></PhoneticSequence></MorphologicalInput><MorphologicalOutput><InsertSegments><PhoneticShape>x</PhoneticShape></InsertSegments></MorphologicalOutput></MorphologicalSubrule>
        <!-- 15: an unlisted topology (omits one of three parts) -->
        <MorphologicalSubrule id="unlisted"><MorphologicalInput><PhoneticSequence id="r0"><SimpleContext naturalClass="ncAny"/></PhoneticSequence><PhoneticSequence id="r1"><SimpleContext naturalClass="ncAny"/></PhoneticSequence><PhoneticSequence id="r2"><SimpleContext naturalClass="ncAny"/></PhoneticSequence></MorphologicalInput><MorphologicalOutput><CopyFromInput index="r0"/><CopyFromInput index="r2"/></MorphologicalOutput></MorphologicalSubrule>
        <!-- 16/17/18: explicit missing, repeated, and reordered copies -->
        <MorphologicalSubrule id="missing-copy"><MorphologicalInput><PhoneticSequence id="mc0"><SimpleContext naturalClass="ncAny"/></PhoneticSequence><PhoneticSequence id="mc1"><SimpleContext naturalClass="ncAny"/></PhoneticSequence><PhoneticSequence id="mc2"><SimpleContext naturalClass="ncAny"/></PhoneticSequence></MorphologicalInput><MorphologicalOutput><CopyFromInput index="mc0"/><CopyFromInput index="mc1"/></MorphologicalOutput></MorphologicalSubrule>
        <MorphologicalSubrule id="repeated-copy"><MorphologicalInput><PhoneticSequence id="rc0"><SimpleContext naturalClass="ncAny"/></PhoneticSequence><PhoneticSequence id="rc1"><SimpleContext naturalClass="ncAny"/></PhoneticSequence></MorphologicalInput><MorphologicalOutput><CopyFromInput index="rc0"/><CopyFromInput index="rc0"/></MorphologicalOutput></MorphologicalSubrule>
        <MorphologicalSubrule id="reordered-copy"><MorphologicalInput><PhoneticSequence id="oc0"><SimpleContext naturalClass="ncAny"/></PhoneticSequence><PhoneticSequence id="oc1"><SimpleContext naturalClass="ncAny"/></PhoneticSequence><PhoneticSequence id="oc2"><SimpleContext naturalClass="ncAny"/></PhoneticSequence></MorphologicalInput><MorphologicalOutput><CopyFromInput index="oc2"/><CopyFromInput index="oc1"/><CopyFromInput index="oc0"/></MorphologicalOutput></MorphologicalSubrule>
      </MorphologicalSubrules><MorphemeId>SYN</MorphemeId>
    </MorphologicalRule></MorphologicalRuleDefinitions>
  </Stratum></Strata>
</Language></HermitCrabInput>
"#;

fn load() -> Grammar {
    pg_grammar::load(CLASSIFIER_XML).unwrap_or_else(|e| panic!("synthetic fixture failed: {e}"))
}

fn allomorph(g: &Grammar, index: usize) -> &pg_grammar::model::AffixAllomorphDef {
    match &g.mrules[0] {
        MorphRuleDef::AffixProcess(rule) => &rule.allomorphs[index],
        other => panic!("synthetic rule must be AffixProcess, got {other:?}"),
    }
}

fn classify(g: &Grammar, index: usize) -> MorphologyRewrite {
    MorphologyRewriteClassifier::classify(g, allomorph(g, index), TableId(0))
}

fn assert_ordinary(g: &Grammar, index: usize) {
    assert!(
        matches!(
            classify(g, index),
            MorphologyRewrite::OrdinaryLiteral { .. }
        ),
        "allomorph {index} must be ordinary literal"
    );
}

fn active_representations(g: &Grammar, char_def: CharDefId) -> Vec<String> {
    g.char_tables[0]
        .get(char_def)
        .representations_nfd()
        .to_vec()
}

fn assert_marked_recipe(
    g: &Grammar,
    index: usize,
    expected_shape: &str,
    expected_refs: Vec<u16>,
    expected_literal_runs: Vec<Vec<String>>,
    expected_output_segments: Option<Vec<String>>,
    expected_zone_requirement: ZoneRequirement,
) -> char {
    match classify(g, index) {
        MorphologyRewrite::MarkedStructural {
            shape_id,
            recipe,
            marker,
            zone_requirement,
        } => {
            assert_eq!(
                shape_id, expected_shape,
                "stable shape id for allomorph {index}"
            );
            assert_eq!(
                recipe.input_refs(),
                expected_refs,
                "validated refs for {expected_shape}"
            );
            assert_eq!(
                recipe.literal_runs(),
                expected_literal_runs,
                "ordered insertion runs for {expected_shape}"
            );
            if let Some(expected) = expected_output_segments {
                assert_eq!(
                    recipe.output_segments(),
                    expected,
                    "finite translated output class for {expected_shape}"
                );
            }
            assert_eq!(
                zone_requirement, expected_zone_requirement,
                "zone requirement for {expected_shape}"
            );
            marker
        }
        other => panic!("allomorph {index} must be {expected_shape}, got {other:?}"),
    }
}

fn assert_unsupported(g: &Grammar, index: usize, expected_shape: &str, expected_reason: &str) {
    let result = catch_unwind(AssertUnwindSafe(|| classify(g, index)));
    let result = result.unwrap_or_else(|_| panic!("allomorph {index} classifier must not panic"));
    match result {
        MorphologyRewrite::Unsupported {
            shape_id,
            reason_id,
            ..
        } => {
            assert_eq!(
                shape_id, expected_shape,
                "stable shape id for allomorph {index}"
            );
            assert_eq!(
                reason_id, expected_reason,
                "stable reason id for allomorph {index}"
            );
        }
        other => panic!("allomorph {index} must fail closed, got {other:?}"),
    }
}

#[test]
fn closed_classifier_accepts_the_five_listed_families_and_ordinary_literals() {
    let g = load();
    assert_ordinary(&g, 0);
    assert_ordinary(&g, 1);

    match classify(&g, 2) {
        MorphologyRewrite::DirectWholeRootWrapper {
            prefix_variants,
            suffix_variants,
            ..
        } => {
            // Expected variants come from active-table character definitions rather than this assertion.
            assert_eq!(prefix_variants, active_representations(&g, CharDefId(7)));
            assert_eq!(suffix_variants, active_representations(&g, CharDefId(8)));
            assert_eq!(prefix_variants.len(), 2);
            assert_eq!(suffix_variants.len(), 2);
            let pairs: Vec<String> = prefix_variants
                .iter()
                .flat_map(|p| suffix_variants.iter().map(move |s| format!("{p}ROOT{s}")))
                .collect();
            assert_eq!(pairs, vec!["pROOTs", "pROOTS", "PROOTs", "PROOTS"]);
        }
        other => panic!("wrapper must be direct and marker-free, got {other:?}"),
    }

    let finite_output_class = [0u32, 1, 2, 3, 4, 5, 6, 7, 8]
        .into_iter()
        .flat_map(|id| active_representations(&g, CharDefId(id)))
        .collect::<Vec<_>>();
    let markers = [
        assert_marked_recipe(
            &g,
            3,
            "AmharicInteriorInsertion",
            vec![0, 1, 2],
            vec![vec!["x".into()], vec![]],
            None,
            ZoneRequirement::Caller,
        ),
        assert_marked_recipe(
            &g,
            4,
            "AmharicInteriorInsertion",
            vec![0, 1, 2],
            vec![vec!["x".into()], vec!["y".into()]],
            None,
            ZoneRequirement::Caller,
        ),
        assert_marked_recipe(
            &g,
            5,
            "AmharicTerminalModify",
            vec![0, 1],
            vec![],
            Some(finite_output_class),
            ZoneRequirement::Caller,
        ),
        assert_marked_recipe(
            &g,
            6,
            "AmharicInitialVowelReplacement",
            vec![1],
            vec![active_representations(&g, CharDefId(7))],
            None,
            ZoneRequirement::Intrinsic(MarkerZone::Prefix),
        ),
        assert_marked_recipe(
            &g,
            7,
            "AdjacentTerminalDrop",
            vec![0],
            vec![vec!["x".into()]],
            None,
            ZoneRequirement::Intrinsic(MarkerZone::Suffix),
        ),
        assert_marked_recipe(
            &g,
            8,
            "AdjacentInitialDrop",
            vec![1],
            vec![],
            None,
            ZoneRequirement::Intrinsic(MarkerZone::Prefix),
        ),
    ];
    let unique_markers = markers
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        unique_markers.len(),
        markers.len(),
        "each marked structural allomorph needs one unique marker"
    );
}

#[test]
fn closed_classifier_default_denies_every_unlisted_action_or_shape() {
    let mut g = load();
    assert_unsupported(&g, 9, "InsertContext", "insert-context");
    assert_unsupported(&g, 10, "ModifyFromInput", "modify-nonterminal");
    assert_unsupported(&g, 11, "ModifyFromInput", "terminal-modify-multi-segment");
    assert_unsupported(&g, 12, "ModifyFromInput", "terminal-modify-quantified");
    assert_unsupported(&g, 13, "ModifyFromInput", "terminal-modify-empty-output");
    // Foreign char-def 5 has an unmapped spelling before a later spelling shared with the active table.
    if let MorphRuleDef::AffixProcess(rule) = &mut g.mrules[0] {
        if let OutputAction::InsertSegments { table, .. } = &mut rule.allomorphs[14].rhs[0] {
            *table = TableId(1);
        }
    }
    match classify(&g, 14) {
        MorphologyRewrite::OrdinaryLiteral { variants } => assert_eq!(variants, vec!["x"]),
        other => panic!("later shared spelling must translate, got {other:?}"),
    }

    // Reinterpret active char-def 7 against foreign char-def 7, whose spelling has no active peer.
    let untranslatable = match &g.mrules[0] {
        MorphRuleDef::AffixProcess(rule) => rule.allomorphs[2].rhs[0].clone(),
        _ => unreachable!(),
    };
    if let MorphRuleDef::AffixProcess(rule) = &mut g.mrules[0] {
        rule.allomorphs[14].rhs = vec![untranslatable];
        if let OutputAction::InsertSegments { table, .. } = &mut rule.allomorphs[14].rhs[0] {
            *table = TableId(1);
        }
    }
    assert_unsupported(&g, 14, "OrdinaryLiteral", "untranslatable-output-table");
    assert_unsupported(&g, 15, "UnlistedTopology", "missing-input-copy");
    assert_unsupported(&g, 16, "UnlistedTopology", "missing-input-copy");
    assert_unsupported(&g, 17, "UnlistedTopology", "repeated-input-reference");
    assert_unsupported(&g, 18, "UnlistedTopology", "reordered-input-reference");

    if let MorphRuleDef::AffixProcess(rule) = &g.mrules[0] {
        assert!(matches!(
            rule.allomorphs[14].rhs[0],
            OutputAction::InsertSegments { table: TableId(1), .. }
        ));
    }
}

#[test]
fn malformed_copy_references_are_stable_fail_closed_results_without_panics() {
    let mut g = load();
    let original = match &g.mrules[0] {
        MorphRuleDef::AffixProcess(rule) => rule.allomorphs[3].rhs.clone(),
        _ => unreachable!(),
    };
    let modify_context = match &g.mrules[0] {
        MorphRuleDef::AffixProcess(rule) => match &rule.allomorphs[5].rhs[1] {
            OutputAction::Modify(_, context) => context.clone(),
            _ => unreachable!(),
        },
        _ => unreachable!(),
    };

    let cases = [
        (
            vec![OutputAction::Copy(PartRef::Input(99))],
            "InvalidReferences",
            "invalid-input-reference",
        ),
        (
            vec![
                OutputAction::Copy(PartRef::Input(0)),
                OutputAction::Copy(PartRef::Input(0)),
            ],
            "UnlistedTopology",
            "repeated-input-reference",
        ),
        (
            vec![
                OutputAction::Copy(PartRef::Input(2)),
                OutputAction::Copy(PartRef::Input(1)),
                OutputAction::Copy(PartRef::Input(0)),
            ],
            "UnlistedTopology",
            "reordered-input-reference",
        ),
        (
            vec![OutputAction::Copy(PartRef::Head(0))],
            "InvalidReferences",
            "invalid-part-reference-kind",
        ),
        (
            vec![OutputAction::Copy(PartRef::NonHead(0))],
            "InvalidReferences",
            "invalid-part-reference-kind",
        ),
        (
            vec![OutputAction::Modify(
                PartRef::Head(0),
                modify_context.clone(),
            )],
            "InvalidReferences",
            "invalid-part-reference-kind",
        ),
        (
            vec![OutputAction::Modify(PartRef::NonHead(0), modify_context)],
            "InvalidReferences",
            "invalid-part-reference-kind",
        ),
    ];

    for (rhs, shape_id, reason) in cases {
        if let MorphRuleDef::AffixProcess(rule) = &mut g.mrules[0] {
            rule.allomorphs[3].rhs = rhs;
        }
        assert_unsupported(&g, 3, shape_id, reason);
    }
    if let MorphRuleDef::AffixProcess(rule) = &mut g.mrules[0] {
        rule.allomorphs[3].rhs = original;
    }
}

#[test]
fn quantified_and_multi_node_terminal_parts_cannot_be_treated_as_one_segment() {
    let g = load();
    for index in [11, 12] {
        let result = catch_unwind(AssertUnwindSafe(|| classify(&g, index)));
        assert!(
            result.is_ok(),
            "terminal-modify negative {index} must not panic"
        );
        assert!(
            matches!(result.unwrap(), MorphologyRewrite::Unsupported { .. }),
            "terminal-modify negative {index} must fail closed"
        );
    }
}

#[test]
fn classifier_has_no_role_based_fallback_for_nonliteral_ordinary_actions() {
    let mut g = load();
    if let MorphRuleDef::AffixProcess(rule) = &mut g.mrules[0] {
        rule.allomorphs[0].rhs = vec![OutputAction::Copy(PartRef::Input(0))];
    }
    let result = catch_unwind(AssertUnwindSafe(|| classify(&g, 0)));
    assert!(
        result.is_ok(),
        "nonliteral ordinary candidate must not panic"
    );
    assert!(
        !matches!(result.unwrap(), MorphologyRewrite::OrdinaryLiteral { .. }),
        "a Copy action must never be accepted as ordinary literal text"
    );
}

#[test]
fn bounded_drop_atoms_are_resolved_in_the_owning_source_table() {
    let mut g = load();
    if let MorphRuleDef::AffixProcess(rule) = &mut g.mrules[0] {
        rule.allomorphs[8].lhs[0].nodes[0] = PatternNode::CharDef(CharDefId(8));
    }

    let result = MorphologyRewriteClassifier::classify_with_tables(
        &g,
        allomorph(&g, 8),
        TableId(1),
        TableId(0),
    );
    assert!(
        matches!(result, MorphologyRewrite::Unsupported { .. }),
        "source-table char-def 8 is absent even though active-table char-def 8 exists"
    );
}
