//! Shared hand-built fixture with two exact noun signatures and distinct plural rules.

pub(crate) const TOY_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>LexiconToy</Name>
    <PartsOfSpeech>
      <PartOfSpeech id="posN"><Name>n</Name></PartOfSpeech>
    </PartsOfSpeech>
    <MorphologicalPhonologicalRuleFeatures>
      <MorphologicalPhonologicalRuleFeature id="mprC1">C1</MorphologicalPhonologicalRuleFeature>
      <MorphologicalPhonologicalRuleFeature id="mprC2">C2</MorphologicalPhonologicalRuleFeature>
    </MorphologicalPhonologicalRuleFeatures>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cI"><Representations><Representation>i</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cK"><Representations><Representation>k</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cL"><Representations><Representation>l</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cM"><Representations><Representation>m</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cN"><Representations><Representation>n</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cO"><Representations><Representation>o</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cP"><Representations><Representation>p</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cS"><Representations><Representation>s</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cT"><Representations><Representation>t</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cU"><Representations><Representation>u</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
      <BoundaryDefinitions>
        <BoundaryDefinition id="cPlus"><Representations><Representation>+</Representation></Representations></BoundaryDefinition>
      </BoundaryDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <SegmentNaturalClass id="ncAll">
        <Name>All</Name>
        <Segment segment="cA" /><Segment segment="cI" /><Segment segment="cK" /><Segment segment="cL" />
        <Segment segment="cM" /><Segment segment="cN" /><Segment segment="cO" /><Segment segment="cP" />
        <Segment segment="cS" /><Segment segment="cT" /><Segment segment="cU" />
      </SegmentNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" morphologicalRules="mrPl">
        <Name>S</Name>
        <MorphologicalRuleDefinitions>
          <MorphologicalRule id="mrPl" requiredPartsOfSpeech="posN" outputPartOfSpeech="posN">
            <Name>plural</Name>
            <MorphemeId>PL</MorphemeId>
            <Gloss>pl</Gloss>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subPlC1">
                <MorphologicalInput requiredMPRFeatures="mprC1">
                  <PhoneticSequence id="stem1">
                    <OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAll" /></OptionalSegmentSequence>
                  </PhoneticSequence>
                </MorphologicalInput>
                <MorphologicalOutput>
                  <CopyFromInput index="stem1" />
                  <InsertSegments><PhoneticShape>+si</PhoneticShape></InsertSegments>
                </MorphologicalOutput>
              </MorphologicalSubrule>
              <MorphologicalSubrule id="subPlC2">
                <MorphologicalInput requiredMPRFeatures="mprC2">
                  <PhoneticSequence id="stem2">
                    <OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAll" /></OptionalSegmentSequence>
                  </PhoneticSequence>
                </MorphologicalInput>
                <MorphologicalOutput>
                  <CopyFromInput index="stem2" />
                  <InsertSegments><PhoneticShape>+ta</PhoneticShape></InsertSegments>
                </MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
          </MorphologicalRule>
        </MorphologicalRuleDefinitions>
        <LexicalEntries>
          <LexicalEntry id="eHouse" partOfSpeech="posN" ruleFeatures="mprC1">
            <Allomorphs><Allomorph id="aHouse"><PhoneticShape>milu</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>house</Gloss>
            <Properties><Property name="ID">101</Property></Properties>
          </LexicalEntry>
          <LexicalEntry id="eBook" partOfSpeech="posN" ruleFeatures="mprC1">
            <Allomorphs><Allomorph id="aBook"><PhoneticShape>kolo</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>book</Gloss>
          </LexicalEntry>
          <LexicalEntry id="eStone" partOfSpeech="posN" ruleFeatures="mprC2">
            <Allomorphs><Allomorph id="aStone"><PhoneticShape>tanu</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>stone</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;
