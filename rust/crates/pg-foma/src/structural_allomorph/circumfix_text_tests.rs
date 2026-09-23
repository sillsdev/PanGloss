use super::*;
use pg_featstruct::SymbolBits;
use pg_grammar::featsys::FlatIndex;
use pg_grammar::model::{NatClassId, SegmentedText};

const XML: &str = r#"<HermitCrabInput><Language><Name>CrossTableCircumfix</Name>
      <PartsOfSpeech><PartOfSpeech id="p"><Name>P</Name></PartOfSpeech></PartsOfSpeech>
      <CharacterDefinitionTable id="inner"><Name>Inner</Name><SegmentDefinitions>
        <SegmentDefinition id="ix"><Representations><Representation>x</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="iz"><Representations><Representation>z</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="iq"><Representations><Representation>q</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions></CharacterDefinitionTable>
      <CharacterDefinitionTable id="outer"><Name>Outer</Name><SegmentDefinitions>
        <SegmentDefinition id="ow"><Representations><Representation>w</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="oq"><Representations><Representation>q</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="oz"><Representations><Representation>z</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="ox"><Representations><Representation>x</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions></CharacterDefinitionTable>
      <NaturalClasses><FeatureNaturalClass id="any"><Name>Any</Name></FeatureNaturalClass></NaturalClasses>
      <Strata><Stratum characterDefinitionTable="inner" morphologicalRuleOrder="unordered" morphologicalRules="m">
        <Name>Inner</Name><MorphologicalRuleDefinitions><MorphologicalRule id="m" requiredPartsOfSpeech="p" outputPartOfSpeech="p">
          <Name>M</Name><MorphologicalSubrules><MorphologicalSubrule id="a">
            <MorphologicalInput><PhoneticSequence id="s"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="any" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
            <MorphologicalOutput><InsertSegments><PhoneticShape>x</PhoneticShape></InsertSegments><CopyFromInput index="s" /><InsertSegments><PhoneticShape>z</PhoneticShape></InsertSegments></MorphologicalOutput>
          </MorphologicalSubrule></MorphologicalSubrules><MorphemeId>M</MorphemeId>
        </MorphologicalRule></MorphologicalRuleDefinitions></Stratum>
        <Stratum characterDefinitionTable="outer" morphologicalRuleOrder="unordered"><Name>Outer</Name></Stratum>
      </Strata></Language></HermitCrabInput>"#;

#[test]
fn circumfix_text_uses_surface_table_tokens_for_foreign_insertions() {
    let g = pg_grammar::load(XML).expect("fixture must load");
    let allomorph = match &g.mrules[0] {
        MorphRuleDef::AffixProcess(def) => &def.allomorphs[0],
        other => panic!("m must be affix-process, got {other:?}"),
    };
    let surface = &g.char_tables[1];
    let alphabet = SegAlphabet::new(surface);
    let texts = circumfix_texts(&g, &alphabet, allomorph).expect("circumfix shape");
    assert_eq!(
        texts,
        vec![(
            alphabet.token(surface.lookup_nfd("x").unwrap()).to_string(),
            alphabet.token(surface.lookup_nfd("z").unwrap()).to_string(),
        )]
    );
}

#[test]
fn feature_context_rejects_values_outside_feature_mask() {
    let mut g = pg_grammar::load(XML).expect("fixture must load");
    g.natural_classes[0].kind =
        NaturalClassKind::Feature(vec![(FlatIndex(0), SymbolBits(1u64 << 63))]);
    let context = SimpleContext {
        nat_class: NatClassId(0),
        vars: Vec::new(),
    };
    assert_eq!(
        validate_context_features(&g, &context),
        Err("invalid-source-feature-mask")
    );
}

#[test]
fn shape_member_extraction_rejects_non_segment_nodes() {
    let g = pg_grammar::load(XML).expect("fixture must load");
    let mut builder = pg_shape::ShapeBuilder::new();
    builder.push_boundary(0);
    let node = PatternNode::Segments {
        table: TableId(0),
        shape: SegmentedText {
            text: "x".into(),
            shape: builder.finish(),
        },
    };
    let mut members = Vec::new();
    assert_eq!(
        collect_node_members(&g, TableId(0), &node, &mut members),
        Err("invalid-source-segment-node")
    );
}
