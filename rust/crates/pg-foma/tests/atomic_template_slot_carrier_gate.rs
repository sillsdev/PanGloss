use pg_foma::templated_compile::{
    compile_templated_morphotactics, TemplatedCompileError,
};

const MIXED_SLOT_XML: &str = r#"
<HermitCrabInput><Language><Name>atomic-template-slot</Name>
  <PartsOfSpeech><PartOfSpeech id="pos"><Name>Root</Name></PartOfSpeech></PartsOfSpeech>
  <CharacterDefinitionTable id="table"><Name>Main</Name><SegmentDefinitions>
    <SegmentDefinition id="cp"><Representations><Representation>p</Representation><Representation>P</Representation></Representations></SegmentDefinition>
    <SegmentDefinition id="cq"><Representations><Representation>q</Representation></Representations></SegmentDefinition>
    <SegmentDefinition id="cs"><Representations><Representation>s</Representation><Representation>S</Representation></Representations></SegmentDefinition>
    <SegmentDefinition id="ct"><Representations><Representation>t</Representation></Representations></SegmentDefinition>
    <SegmentDefinition id="cu"><Representations><Representation>u</Representation></Representations></SegmentDefinition>
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
          <MorphologicalSubrule id="prefix">
            <MorphologicalInput><PhoneticSequence id="base"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="any" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
            <MorphologicalOutput><InsertSegments><PhoneticShape>u</PhoneticShape></InsertSegments><CopyFromInput index="base" /></MorphologicalOutput>
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

    for surface in ["pqs", "pqS", "Pqs", "PqS", "qt", "uq", "q"] {
        assert!(
            !proposer.propose(surface).is_empty(),
            "authored template path {surface:?} must remain reachable"
        );
    }

    for surface in [
        "pqt", "Pqt", "uqt", "uqs", "uqS", "pq", "Pq", "qs", "qS",
    ] {
        assert!(
            proposer.propose(surface).is_empty(),
            "unmatched or crossed slot path {surface:?} must not be manufactured"
        );
    }
}

#[test]
fn carrier_preserves_other_template_slots() {
    let xml = MIXED_SLOT_XML
        .replace(
            "</SegmentDefinitions>",
            r#"<SegmentDefinition id="ch"><Representations><Representation>h</Representation></Representations></SegmentDefinition></SegmentDefinitions>"#,
        )
        .replace(
            "</MorphologicalRuleDefinitions>",
            r#"<MorphologicalRule id="tail" requiredPartsOfSpeech="pos" outputPartOfSpeech="pos">
              <Name>tail</Name><MorphologicalSubrules><MorphologicalSubrule id="tail-subrule">
                <MorphologicalInput><PhoneticSequence id="tail-stem"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="any" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                <MorphologicalOutput><CopyFromInput index="tail-stem" /><InsertSegments><PhoneticShape>h</PhoneticShape></InsertSegments></MorphologicalOutput>
              </MorphologicalSubrule></MorphologicalSubrules><MorphemeId>TAIL</MorphemeId>
            </MorphologicalRule></MorphologicalRuleDefinitions>"#,
        )
        .replace(
            "</AffixTemplate></AffixTemplates>",
            r#"<Slot optional="false" morphologicalRules="tail"><Name>tail</Name></Slot></AffixTemplate></AffixTemplates>"#,
        );
    let grammar = pg_grammar::load(&xml).expect("multi-slot carrier fixture must load");
    let compiled = compile_templated_morphotactics(&grammar)
        .expect("one mixed slot among ordinary template slots must compile");
    let mut proposer = compiled.proposer;

    for surface in ["pqsh", "pqSh", "Pqsh", "PqSh", "qth", "uqh", "qh"] {
        assert!(
            !proposer.propose(surface).is_empty(),
            "authored multi-slot path {surface:?} must remain reachable"
        );
    }

    for surface in [
        "pqth", "Pqth", "uqth", "uqsh", "uqSh", "pqh", "Pqh", "qsh", "qSh",
    ] {
        assert!(
            proposer.propose(surface).is_empty(),
            "crossed or unmatched multi-slot path {surface:?} must not be manufactured"
        );
    }
}

#[test]
fn carrier_preserves_derivation_chains_around_the_root() {
    let xml = MIXED_SLOT_XML
        .replace(
            "</SegmentDefinitions>",
            r#"<SegmentDefinition id="cv"><Representations><Representation>v</Representation></Representations></SegmentDefinition><SegmentDefinition id="cw"><Representations><Representation>w</Representation></Representations></SegmentDefinition></SegmentDefinitions>"#,
        )
        .replace(
            "morphologicalRuleOrder=\"linear\" morphologicalRules=\"\"",
            "morphologicalRuleOrder=\"linear\" morphologicalRules=\"pre post\"",
        )
        .replace(
            "</MorphologicalRuleDefinitions>",
            r#"<MorphologicalRule id="pre" requiredPartsOfSpeech="pos" outputPartOfSpeech="pos">
              <Name>derivational prefix</Name><MorphologicalSubrules><MorphologicalSubrule id="pre-subrule">
                <MorphologicalInput><PhoneticSequence id="pre-stem"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="any" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                <MorphologicalOutput><InsertSegments><PhoneticShape>v</PhoneticShape></InsertSegments><CopyFromInput index="pre-stem" /></MorphologicalOutput>
              </MorphologicalSubrule></MorphologicalSubrules><MorphemeId>PRE</MorphemeId>
            </MorphologicalRule>
            <MorphologicalRule id="post" requiredPartsOfSpeech="pos" outputPartOfSpeech="pos">
              <Name>derivational suffix</Name><MorphologicalSubrules><MorphologicalSubrule id="post-subrule">
                <MorphologicalInput><PhoneticSequence id="post-stem"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="any" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                <MorphologicalOutput><CopyFromInput index="post-stem" /><InsertSegments><PhoneticShape>w</PhoneticShape></InsertSegments></MorphologicalOutput>
              </MorphologicalSubrule></MorphologicalSubrules><MorphemeId>POST</MorphemeId>
            </MorphologicalRule></MorphologicalRuleDefinitions>"#,
        );
    let grammar = pg_grammar::load(&xml).expect("carrier plus derivation fixture must load");
    let compiled = compile_templated_morphotactics(&grammar)
        .expect("carrier must preserve the normal derivation topology");
    let mut proposer = compiled.proposer;

    for surface in ["pvqws", "uvqw", "vqwt", "vqw"] {
        assert!(
            !proposer.propose(surface).is_empty(),
            "authored carrier plus derivation path {surface:?} must remain reachable"
        );
    }
    for surface in ["pvqwt", "uvqwt", "uvqws"] {
        assert!(
            proposer.propose(surface).is_empty(),
            "carrier must not cross alternatives while preserving derivation: {surface:?}"
        );
    }
}

fn assert_typed_unsupported(xml: &str, context: &str) {
    let grammar = pg_grammar::load(xml).unwrap_or_else(|error| panic!("{context}: {error}"));
    match compile_templated_morphotactics(&grammar) {
        Err(TemplatedCompileError::Unsupported(_)) => {}
        Err(other) => panic!("{context}: expected typed Unsupported, got {other}"),
        Ok(_) => panic!("{context}: unsupported carrier topology compiled"),
    }
}

#[test]
fn two_cross_root_slots_fail_closed() {
    let xml = MIXED_SLOT_XML.replace(
        "</AffixTemplate></AffixTemplates>",
        r#"<Slot optional="true" morphologicalRules="mixed"><Name>second mixed</Name></Slot></AffixTemplate></AffixTemplates>"#,
    );
    assert_typed_unsupported(&xml, "two cross-root slots");
}

#[test]
fn cross_root_slot_with_compounding_fails_closed() {
    let xml = MIXED_SLOT_XML
        .replace(
            "morphologicalRuleOrder=\"linear\" morphologicalRules=\"\"",
            "morphologicalRuleOrder=\"linear\" morphologicalRules=\"compound\"",
        )
        .replace(
            "</MorphologicalRuleDefinitions>",
            r#"<CompoundingRule id="compound" nonHeadPartsOfSpeech="pos">
              <Name>compound</Name><CompoundingSubrules><CompoundingSubrule>
                <HeadMorphologicalInput><PhoneticSequence id="head"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="any" /></OptionalSegmentSequence></PhoneticSequence></HeadMorphologicalInput>
                <NonHeadMorphologicalInput><PhoneticSequence id="nonhead"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="any" /></OptionalSegmentSequence></PhoneticSequence></NonHeadMorphologicalInput>
                <MorphologicalOutput><CopyFromInput index="head" /><CopyFromInput index="nonhead" /></MorphologicalOutput>
              </CompoundingSubrule></CompoundingSubrules>
            </CompoundingRule></MorphologicalRuleDefinitions>"#,
        );
    assert_typed_unsupported(&xml, "cross-root slot with compounding");
}
