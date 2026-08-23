use pg_foma::templated_compile::compile_templated_morphotactics;

const MIXED_SLOT_XML: &str = r#"
<HermitCrabInput><Language><Name>atomic-template-slot</Name>
  <PartsOfSpeech><PartOfSpeech id="pos"><Name>Root</Name></PartOfSpeech></PartsOfSpeech>
  <CharacterDefinitionTable id="table"><Name>Main</Name><SegmentDefinitions>
    <SegmentDefinition id="cp"><Representations><Representation>p</Representation><Representation>P</Representation></Representations></SegmentDefinition>
    <SegmentDefinition id="cq"><Representations><Representation>q</Representation></Representations></SegmentDefinition>
    <SegmentDefinition id="cs"><Representations><Representation>s</Representation><Representation>S</Representation></Representations></SegmentDefinition>
    <SegmentDefinition id="ct"><Representations><Representation>t</Representation></Representations></SegmentDefinition>
  </SegmentDefinitions></CharacterDefinitionTable>
  <NaturalClasses><FeatureNaturalClass id="any"><Name>Any</Name></FeatureNaturalClass></NaturalClasses>
  <Strata><Stratum characterDefinitionTable="table" morphologicalRuleOrder="linear" morphologicalRules="">
    <Name>Only</Name>
    <MorphologicalRuleDefinitions>
      <MorphologicalRule id="mixed" requiredPartsOfSpeech="pos" outputPartOfSpeech="pos">
        <Name>mixed slot alternatives</Name><MorphologicalSubrules>
          <MorphologicalSubrule id="wrapper">
            <MorphologicalInput><PhoneticSequence id="whole"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="any" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
            <MorphologicalOutput><InsertSegments><PhoneticShape>p</PhoneticShape></InsertSegments><CopyFromInput index="whole" /><InsertSegments><PhoneticShape>s</PhoneticShape></InsertSegments></MorphologicalOutput>
          </MorphologicalSubrule>
          <MorphologicalSubrule id="suffix">
            <MorphologicalInput><PhoneticSequence id="stem"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="any" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
            <MorphologicalOutput><CopyFromInput index="stem" /><InsertSegments><PhoneticShape>t</PhoneticShape></InsertSegments></MorphologicalOutput>
          </MorphologicalSubrule>
        </MorphologicalSubrules><MorphemeId>MIXED</MorphemeId>
      </MorphologicalRule>
    </MorphologicalRuleDefinitions>
    <AffixTemplates><AffixTemplate id="template" final="true" requiredPartsOfSpeech="pos">
      <Name>template</Name><Slot optional="true" morphologicalRules="mixed"><Name>mixed</Name></Slot>
    </AffixTemplate></AffixTemplates>
    <LexicalEntries><LexicalEntry id="root" partOfSpeech="pos"><Allomorphs>
      <Allomorph id="root-allomorph"><PhoneticShape>q</PhoneticShape></Allomorph>
    </Allomorphs><MorphemeId>ROOT</MorphemeId></LexicalEntry></LexicalEntries>
  </Stratum></Strata>
</Language></HermitCrabInput>
"#;

#[test]
fn one_slot_choice_stays_atomic_across_the_root() {
    let grammar = pg_grammar::load(MIXED_SLOT_XML).expect("mixed-slot fixture must load");
    let compiled = compile_templated_morphotactics(&grammar)
        .expect("the atomic carrier must compile before any lexc artifact is accepted");
    let mut proposer = compiled.proposer;

    for surface in ["pqs", "pqS", "Pqs", "PqS", "qt", "q"] {
        assert!(
            !proposer.propose(surface).is_empty(),
            "authored template path {surface:?} must remain reachable"
        );
    }

    for surface in ["pqt", "Pqt", "pq", "Pq", "qs", "qS"] {
        assert!(
            proposer.propose(surface).is_empty(),
            "unmatched or crossed slot path {surface:?} must not be manufactured"
        );
    }
}
