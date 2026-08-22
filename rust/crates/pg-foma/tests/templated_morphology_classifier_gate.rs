//! Pins the closed templated-morphology classifier grammar with invented construct witnesses.

use std::collections::HashSet;
use std::panic::{catch_unwind, AssertUnwindSafe};

use pg_featstruct::SymbolBits;
use pg_foma::structural_allomorph::{
    marker_binding_for, MarkerBindingError, MarkerKey, MarkerZone, MorphologyRewrite,
    MorphologyRewriteClassifier, RewriteProvenance, ZoneRequirement,
};
use pg_grammar::chardef::CharDefId;
use pg_grammar::featsys::FlatIndex;
use pg_grammar::model::{
    AllomorphId, AnchorSide, Grammar, MorphRuleDef, NatClassId, NaturalClass, NaturalClassKind,
    OutputAction, PartRef, PatternNode, SegmentedText, SimpleContext, TableId,
};
use pg_shape::ShapeBuilder;

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
  </SegmentDefinitions><BoundaryDefinitions>
    <BoundaryDefinition id="bPlus"><Representations><Representation>+</Representation></Representations></BoundaryDefinition>
  </BoundaryDefinitions></CharacterDefinitionTable>
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
        <!-- 19/20: boundary definitions cannot satisfy segment-only input or output atoms. -->
        <MorphologicalSubrule id="drop-terminal-boundary"><MorphologicalInput><PhoneticSequence id="bd0"><SimpleContext naturalClass="ncAny"/></PhoneticSequence><PhoneticSequence id="bd1"><BoundaryMarker boundary="bPlus"/></PhoneticSequence></MorphologicalInput><MorphologicalOutput><CopyFromInput index="bd0"/><InsertSegments><PhoneticShape>x</PhoneticShape></InsertSegments></MorphologicalOutput></MorphologicalSubrule>
        <MorphologicalSubrule id="infix-boundary-output"><MorphologicalInput><PhoneticSequence id="bo0"><SimpleContext naturalClass="ncAny"/></PhoneticSequence><PhoneticSequence id="bo1"><SimpleContext naturalClass="ncAny"/></PhoneticSequence></MorphologicalInput><MorphologicalOutput><CopyFromInput index="bo0"/><InsertSegments><PhoneticShape>+</PhoneticShape></InsertSegments><CopyFromInput index="bo1"/></MorphologicalOutput></MorphologicalSubrule>
      </MorphologicalSubrules><MorphemeId>SYN</MorphemeId>
    </MorphologicalRule></MorphologicalRuleDefinitions>
  </Stratum></Strata>
</Language></HermitCrabInput>
"#;

const REALIZATIONAL_OWNER_XML: &str = r#"
<HermitCrabInput><Language><Name>realizational-classifier-owner</Name>
  <PartsOfSpeech><PartOfSpeech id="p"><Name>P</Name></PartOfSpeech></PartsOfSpeech>
  <CharacterDefinitionTable id="t"><Name>Main</Name><SegmentDefinitions>
    <SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
  </SegmentDefinitions></CharacterDefinitionTable>
  <NaturalClasses><SegmentNaturalClass id="ncAny"><Name>Any</Name><Segment segment="ca"/></SegmentNaturalClass></NaturalClasses>
  <Strata><Stratum characterDefinitionTable="t" morphologicalRules="rr">
    <Name>S</Name><MorphologicalRuleDefinitions><RealizationalRule id="rr">
      <Name>R</Name><MorphologicalSubrules><MorphologicalSubrule id="ra">
        <MorphologicalInput><PhoneticSequence id="r0"><SimpleContext naturalClass="ncAny"/></PhoneticSequence></MorphologicalInput>
        <MorphologicalOutput><CopyFromInput index="r0"/></MorphologicalOutput>
      </MorphologicalSubrule></MorphologicalSubrules>
    </RealizationalRule></MorphologicalRuleDefinitions>
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
    match classify(g, index) {
        MorphologyRewrite::OrdinaryLiteral { provenance, .. } => {
            assert_eq!(provenance, expected_provenance(g, index, TableId(0)));
        }
        other => panic!("allomorph {index} must be ordinary literal, got {other:?}"),
    }
}

fn active_representations(g: &Grammar, char_def: CharDefId) -> Vec<String> {
    g.char_tables[0]
        .get(char_def)
        .representations_nfd()
        .to_vec()
}

fn expected_provenance(g: &Grammar, index: usize, active_table: TableId) -> RewriteProvenance {
    RewriteProvenance {
        allomorph: allomorph(g, index).id,
        source_table: g.strata[0].table,
        active_table,
    }
}

fn assert_marked_recipe(
    g: &Grammar,
    index: usize,
    expected_shape: &str,
    expected_refs: Vec<u16>,
    expected_literal_runs: Vec<Vec<String>>,
    expected_output_segments: Option<Vec<String>>,
    expected_zone_requirement: ZoneRequirement,
) {
    match classify(g, index) {
        MorphologyRewrite::MarkedStructural {
            shape_id,
            recipe,
            zone_requirement,
            provenance,
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
            assert_eq!(
                provenance,
                expected_provenance(g, index, TableId(0)),
                "owner-derived provenance for {expected_shape}"
            );
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
            allomorph: result_allomorph,
            source_table,
            active_table,
        } => {
            assert_eq!(
                shape_id, expected_shape,
                "stable shape id for allomorph {index}"
            );
            assert_eq!(
                reason_id, expected_reason,
                "stable reason id for allomorph {index}"
            );
            assert_eq!(result_allomorph, allomorph(g, index).id);
            assert_eq!(source_table, g.strata[0].table);
            assert_eq!(active_table, TableId(0));
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
            provenance,
        } => {
            // Expected variants come from active-table character definitions rather than this assertion.
            assert_eq!(prefix_variants, active_representations(&g, CharDefId(7)));
            assert_eq!(suffix_variants, active_representations(&g, CharDefId(8)));
            assert_eq!(prefix_variants.len(), 2);
            assert_eq!(suffix_variants.len(), 2);
            assert_eq!(provenance, expected_provenance(&g, 2, TableId(0)));
        }
        other => panic!("wrapper must be direct and marker-free, got {other:?}"),
    }

    let finite_output_class = [0u32, 1, 2, 3, 4, 5, 6, 7, 8]
        .into_iter()
        .flat_map(|id| active_representations(&g, CharDefId(id)))
        .collect::<Vec<_>>();
    assert_marked_recipe(
        &g,
        3,
        "AmharicInteriorInsertion",
        vec![0, 1, 2],
        vec![vec!["x".into()], vec![]],
        None,
        ZoneRequirement::Caller,
    );
    assert_marked_recipe(
        &g,
        4,
        "AmharicInteriorInsertion",
        vec![0, 1, 2],
        vec![vec!["x".into()], vec!["y".into()]],
        None,
        ZoneRequirement::Caller,
    );
    assert_marked_recipe(
        &g,
        5,
        "AmharicTerminalModify",
        vec![0, 1],
        vec![],
        Some(finite_output_class),
        ZoneRequirement::Caller,
    );
    assert_marked_recipe(
        &g,
        6,
        "AmharicInitialVowelReplacement",
        vec![1],
        vec![active_representations(&g, CharDefId(7))],
        None,
        ZoneRequirement::Intrinsic(MarkerZone::Prefix),
    );
    assert_marked_recipe(
        &g,
        7,
        "AdjacentTerminalDrop",
        vec![0],
        vec![vec!["x".into()]],
        None,
        ZoneRequirement::Intrinsic(MarkerZone::Suffix),
    );
    assert_marked_recipe(
        &g,
        8,
        "AdjacentInitialDrop",
        vec![1],
        vec![],
        None,
        ZoneRequirement::Intrinsic(MarkerZone::Prefix),
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
        MorphologyRewrite::OrdinaryLiteral { variants, .. } => assert_eq!(variants, vec!["x"]),
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
            OutputAction::InsertSegments {
                table: TableId(1),
                ..
            }
        ));
    }
}

#[test]
fn segment_only_classifier_rejects_boundary_input_and_output_atoms() {
    let g = load();
    assert_unsupported(&g, 19, "AdjacentTerminalDrop", "non-segment-input-atom");
    assert_unsupported(
        &g,
        20,
        "AmharicInteriorInsertion",
        "non-segment-output-atom",
    );
}

#[test]
fn marker_binding_identity_is_allomorph_and_zone_and_intrinsic_mismatch_refuses() {
    let allomorph = AllomorphId(7);
    let prefix_key = MarkerKey {
        allomorph,
        zone: MarkerZone::Prefix,
    };
    let suffix_key = MarkerKey {
        allomorph,
        zone: MarkerZone::Suffix,
    };

    let prefix = marker_binding_for(prefix_key, ZoneRequirement::Caller)
        .expect("caller-zoned prefix binding");
    let suffix = marker_binding_for(suffix_key, ZoneRequirement::Caller)
        .expect("caller-zoned suffix binding");
    assert_eq!(prefix.key, prefix_key);
    assert_eq!(suffix.key, suffix_key);
    assert_ne!(prefix.symbol, suffix.symbol);

    assert_eq!(
        marker_binding_for(suffix_key, ZoneRequirement::Intrinsic(MarkerZone::Prefix)),
        Err(MarkerBindingError::IntrinsicZoneMismatch {
            required: MarkerZone::Prefix,
            actual: MarkerZone::Suffix,
        })
    );
    assert_eq!(
        marker_binding_for(prefix_key, ZoneRequirement::Intrinsic(MarkerZone::Suffix)),
        Err(MarkerBindingError::IntrinsicZoneMismatch {
            required: MarkerZone::Suffix,
            actual: MarkerZone::Prefix,
        })
    );

    let mut symbols = HashSet::new();
    for id in [0, 1, 7, 65_535] {
        for zone in [MarkerZone::Prefix, MarkerZone::Suffix] {
            let binding = marker_binding_for(
                MarkerKey {
                    allomorph: AllomorphId(id),
                    zone,
                },
                ZoneRequirement::Caller,
            )
            .expect("every in-range allomorph/zone pair has a marker");
            assert!(
                symbols.insert(binding.symbol),
                "marker collision for {id:?}/{zone:?}"
            );
        }
    }
    assert_eq!(
        marker_binding_for(
            MarkerKey {
                allomorph: AllomorphId(65_535),
                zone: MarkerZone::Suffix,
            },
            ZoneRequirement::Caller,
        )
        .expect("maximum marker")
        .symbol,
        char::from_u32(0x10_FFFF).unwrap()
    );
    assert_eq!(
        marker_binding_for(
            MarkerKey {
                allomorph: AllomorphId(65_536),
                zone: MarkerZone::Prefix,
            },
            ZoneRequirement::Caller,
        ),
        Err(MarkerBindingError::InvalidScalar)
    );
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

#[test]
fn replacement_and_modify_atoms_exist_in_the_owning_source_table() {
    let mut g = load();
    if let MorphRuleDef::AffixProcess(rule) = &mut g.mrules[0] {
        rule.allomorphs[5].lhs[1].nodes[0] = PatternNode::CharDef(CharDefId(8));
        rule.allomorphs[6].lhs[0].nodes[0] = PatternNode::CharDef(CharDefId(8));
    }

    for index in [5, 6] {
        let result = MorphologyRewriteClassifier::classify_with_tables(
            &g,
            allomorph(&g, index),
            TableId(1),
            TableId(0),
        );
        assert!(
            matches!(result, MorphologyRewrite::Unsupported { .. }),
            "allomorph {index} cannot use source-table char-def 8 because only the active table contains it"
        );
    }
}

#[test]
fn wrapper_classifier_accepts_empty_runs_on_either_side() {
    let original = match &load().mrules[0] {
        MorphRuleDef::AffixProcess(rule) => rule.allomorphs[2].rhs.clone(),
        _ => unreachable!(),
    };
    let cases = [
        (
            vec![original[0].clone(), original[1].clone()],
            active_representations(&load(), CharDefId(7)),
            vec![String::new()],
        ),
        (
            vec![original[1].clone(), original[2].clone()],
            vec![String::new()],
            active_representations(&load(), CharDefId(8)),
        ),
        (
            vec![original[1].clone()],
            vec![String::new()],
            vec![String::new()],
        ),
    ];

    for (rhs, expected_prefixes, expected_suffixes) in cases {
        let mut g = load();
        if let MorphRuleDef::AffixProcess(rule) = &mut g.mrules[0] {
            rule.allomorphs[2].rhs = rhs;
        }
        match classify(&g, 2) {
            MorphologyRewrite::DirectWholeRootWrapper {
                prefix_variants,
                suffix_variants,
                provenance,
            } => {
                assert_eq!(prefix_variants, expected_prefixes);
                assert_eq!(suffix_variants, expected_suffixes);
                assert_eq!(provenance, expected_provenance(&g, 2, TableId(0)));
            }
            other => panic!("one-sided or pure-copy wrapper must classify, got {other:?}"),
        }
    }
}

#[test]
fn every_referenced_lhs_part_must_consume_at_least_one_segment() {
    let mut empty_builder = ShapeBuilder::new();
    let cases = [
        PatternNode::Anchor(AnchorSide::Left),
        PatternNode::Segments {
            table: TableId(0),
            shape: SegmentedText {
                text: String::new(),
                shape: empty_builder.finish(),
            },
        },
        PatternNode::Quantifier {
            min: 0,
            max: Some(1),
            children: vec![PatternNode::CharDef(CharDefId(0))],
        },
    ];

    for node in cases {
        let mut g = load();
        if let MorphRuleDef::AffixProcess(rule) = &mut g.mrules[0] {
            rule.allomorphs[3].lhs[0].nodes = vec![node];
        }
        assert_unsupported(
            &g,
            3,
            "AmharicInteriorInsertion",
            "non-consuming-input-part",
        );
    }
}

#[test]
fn one_segment_group_is_a_proven_terminal_modify_atom() {
    let mut builder = ShapeBuilder::new();
    builder.push_segment(0);
    let mut g = load();
    if let MorphRuleDef::AffixProcess(rule) = &mut g.mrules[0] {
        rule.allomorphs[5].lhs[1].nodes = vec![PatternNode::Segments {
            table: TableId(0),
            shape: SegmentedText {
                text: "a".into(),
                shape: builder.finish(),
            },
        }];
    }

    match classify(&g, 5) {
        MorphologyRewrite::MarkedStructural {
            shape_id,
            recipe,
            provenance,
            ..
        } => {
            assert_eq!(shape_id, "AmharicTerminalModify");
            assert_eq!(recipe.input_refs(), vec![0, 1]);
            assert_eq!(recipe.translated_input_members()[1], vec!["a"]);
            assert_eq!(provenance, expected_provenance(&g, 5, TableId(0)));
        }
        other => panic!("one-segment Segments node must classify, got {other:?}"),
    }
}

#[test]
fn admitted_drop_recipe_retains_the_dropped_input_members() {
    let g = load();
    match classify(&g, 7) {
        MorphologyRewrite::MarkedStructural {
            recipe, provenance, ..
        } => {
            let members = recipe.translated_input_members();
            assert_eq!(
                members.len(),
                2,
                "one finite member set per authored LHS part"
            );
            assert!(members[1].contains(&"a".to_string()));
            assert!(members[1].contains(&"b".to_string()));
            assert_eq!(provenance, expected_provenance(&g, 7, TableId(0)));
        }
        other => panic!("terminal drop must retain its dropped atom, got {other:?}"),
    }
}

#[test]
fn terminal_modify_uses_owner_table_and_skips_only_unmapped_representations() {
    let mut g = load();
    g.strata[0].table = TableId(1);
    let mixed = NatClassId(g.natural_classes.len() as u32);
    g.natural_classes.push(NaturalClass {
        xml_id: "ncForeignMixed".into(),
        name: None,
        kind: NaturalClassKind::Segments(vec![CharDefId(5), CharDefId(7)]),
    });
    if let MorphRuleDef::AffixProcess(rule) = &mut g.mrules[0] {
        let candidate = &mut rule.allomorphs[5];
        candidate.lhs[0].nodes = vec![PatternNode::CharDef(CharDefId(5))];
        candidate.lhs[1].nodes = vec![PatternNode::CharDef(CharDefId(5))];
        if let OutputAction::Modify(_, context) = &mut candidate.rhs[1] {
            context.nat_class = mixed;
        }
    }

    match classify(&g, 5) {
        MorphologyRewrite::MarkedStructural {
            recipe, provenance, ..
        } => {
            assert_eq!(recipe.output_segments(), vec!["x"]);
            assert_eq!(
                recipe.translated_input_members(),
                vec![vec!["x"], vec!["x"]]
            );
            assert_eq!(provenance, expected_provenance(&g, 5, TableId(0)));
        }
        other => panic!("later shared representation must translate, got {other:?}"),
    }

    if let MorphRuleDef::AffixProcess(rule) = &mut g.mrules[0] {
        if let OutputAction::Modify(_, context) = &mut rule.allomorphs[5].rhs[1] {
            context.nat_class = NatClassId(mixed.0 + 1);
        }
    }
    g.natural_classes.push(NaturalClass {
        xml_id: "ncForeignUnmapped".into(),
        name: None,
        kind: NaturalClassKind::Segments(vec![CharDefId(7)]),
    });
    assert_unsupported(
        &g,
        5,
        "AmharicTerminalModify",
        "untranslatable-output-table",
    );
}

#[test]
fn malformed_feature_reference_is_a_stable_refusal_not_a_panic() {
    let mut g = load();
    let malformed = NatClassId(g.natural_classes.len() as u32);
    g.natural_classes.push(NaturalClass {
        xml_id: "ncMalformedFeature".into(),
        name: None,
        kind: NaturalClassKind::Feature(vec![(FlatIndex(u32::MAX), SymbolBits(1))]),
    });
    if let MorphRuleDef::AffixProcess(rule) = &mut g.mrules[0] {
        rule.allomorphs[7].lhs[1].nodes = vec![PatternNode::Context(SimpleContext {
            nat_class: malformed,
            vars: vec![],
        })];
    }
    assert_unsupported(
        &g,
        7,
        "InvalidReferences",
        "invalid-source-feature-reference",
    );

    if let MorphRuleDef::AffixProcess(rule) = &mut g.mrules[0] {
        if let OutputAction::Modify(_, context) = &mut rule.allomorphs[5].rhs[1] {
            context.nat_class = malformed;
        }
    }
    assert_unsupported(
        &g,
        5,
        "InvalidReferences",
        "invalid-source-feature-reference",
    );
}

#[test]
fn owner_resolution_accepts_realizational_affix_allomorphs() {
    let g = pg_grammar::load(REALIZATIONAL_OWNER_XML)
        .unwrap_or_else(|e| panic!("realizational owner fixture failed: {e}"));
    let candidate = match &g.mrules[0] {
        MorphRuleDef::Realizational(rule) => &rule.allomorphs[0],
        other => panic!("fixture must contain a realizational rule, got {other:?}"),
    };
    match MorphologyRewriteClassifier::classify(&g, candidate, TableId(0)) {
        MorphologyRewrite::DirectWholeRootWrapper {
            prefix_variants,
            suffix_variants,
            provenance,
        } => {
            assert_eq!(prefix_variants, vec![""]);
            assert_eq!(suffix_variants, vec![""]);
            assert_eq!(provenance.allomorph, candidate.id);
            assert_eq!(provenance.source_table, TableId(0));
            assert_eq!(provenance.active_table, TableId(0));
        }
        other => panic!("realizational affix allomorph must have a real owner, got {other:?}"),
    }
}

#[test]
fn owner_resolution_accepts_template_slot_only_rules() {
    let g = pg_grammar::load(include_str!(
        "../../../../conformance-staging/edge-cases/circumfix-in-template-slot/grammar.xml"
    ))
    .unwrap_or_else(|e| panic!("template-only owner fixture failed: {e}"));
    let candidate = match &g.mrules[0] {
        MorphRuleDef::AffixProcess(rule) => &rule.allomorphs[0],
        other => panic!("mrCircum must be an affix-process rule, got {other:?}"),
    };
    assert!(matches!(
        MorphologyRewriteClassifier::classify(&g, candidate, TableId(0)),
        MorphologyRewrite::DirectWholeRootWrapper { .. }
    ));
}

#[test]
fn caller_cannot_borrow_a_registry_id_for_an_unowned_allomorph_object() {
    let owner = load();
    let mut foreign = load();
    if let MorphRuleDef::AffixProcess(rule) = &mut foreign.mrules[0] {
        rule.allomorphs[2].rhs.clear();
    }
    match MorphologyRewriteClassifier::classify(&owner, allomorph(&foreign, 2), TableId(0)) {
        MorphologyRewrite::Unsupported {
            shape_id,
            reason_id,
            ..
        } => {
            assert_eq!(shape_id, "InvalidReferences");
            assert_eq!(reason_id, "invalid-allomorph-owner");
        }
        other => panic!("a foreign object with a borrowed registry id must refuse, got {other:?}"),
    }
}

#[test]
fn malformed_initial_drop_reports_the_initial_shape() {
    let mut g = load();
    if let MorphRuleDef::AffixProcess(rule) = &mut g.mrules[0] {
        rule.allomorphs[8].lhs[0].nodes = vec![PatternNode::CharDef(CharDefId(9))];
    }
    assert_unsupported(
        &g,
        8,
        "AdjacentInitialDrop",
        "non-segment-input-atom",
    );
}
