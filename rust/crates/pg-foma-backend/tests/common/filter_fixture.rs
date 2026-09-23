//! One synthetic grammar the structural candidate-filter tests decide against.

use pg_grammar::model::{Grammar, MorphemeId};

/// Two strata, a template each, one loose rule, two lexical entries.
pub const STRUCTURAL_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE HermitCrabInput SYSTEM "HermitCrabInput.dtd">
<HermitCrabInput>
  <Language>
    <Name>FilterStructural</Name>
    <PartsOfSpeech>
      <PartOfSpeech id="posV"><Name>v</Name></PartOfSpeech>
    </PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cK"><Representations><Representation>k</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cE"><Representations><Representation>e</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cB"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cL"><Representations><Representation>l</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cQ"><Representations><Representation>q</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" morphologicalRules="mrLoose">
        <Name>S0</Name>
        <MorphologicalRuleDefinitions>
          <MorphologicalRule id="mrP0" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
            <Name>p0</Name>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subP0">
                <MorphologicalInput><PhoneticSequence id="stemP0"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                <MorphologicalOutput><CopyFromInput index="stemP0" /><InsertSegments><PhoneticShape>a</PhoneticShape></InsertSegments></MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
            <MorphemeId>P0</MorphemeId>
          </MorphologicalRule>
          <MorphologicalRule id="mrP1" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
            <Name>p1</Name>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subP1">
                <MorphologicalInput><PhoneticSequence id="stemP1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                <MorphologicalOutput><CopyFromInput index="stemP1" /><InsertSegments><PhoneticShape>b</PhoneticShape></InsertSegments></MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
            <MorphemeId>P1</MorphemeId>
          </MorphologicalRule>
          <MorphologicalRule id="mrLoose" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
            <Name>loose</Name>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subLoose">
                <MorphologicalInput><PhoneticSequence id="stemLoose"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                <MorphologicalOutput><CopyFromInput index="stemLoose" /><InsertSegments><PhoneticShape>l</PhoneticShape></InsertSegments></MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
            <MorphemeId>Loose</MorphemeId>
          </MorphologicalRule>
        </MorphologicalRuleDefinitions>
        <AffixTemplates>
          <AffixTemplate requiredPartsOfSpeech="posV">
            <Name>T</Name>
            <Slot morphologicalRules="mrP0"><Name>s0</Name></Slot>
            <Slot morphologicalRules="mrP1"><Name>s1</Name></Slot>
          </AffixTemplate>
        </AffixTemplates>
        <LexicalEntries>
          <LexicalEntry id="eRoot" partOfSpeech="posV">
            <Allomorphs><Allomorph id="aRoot"><PhoneticShape>k</PhoneticShape></Allomorph></Allomorphs>
            <MorphemeId>Root</MorphemeId>
          </LexicalEntry>
          <LexicalEntry id="eExtra" partOfSpeech="posV">
            <Allomorphs><Allomorph id="aExtra"><PhoneticShape>e</PhoneticShape></Allomorph></Allomorphs>
            <MorphemeId>Extra</MorphemeId>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered">
        <Name>S1</Name>
        <MorphologicalRuleDefinitions>
          <MorphologicalRule id="mrQ" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
            <Name>q</Name>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subQ">
                <MorphologicalInput><PhoneticSequence id="stemQ"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                <MorphologicalOutput><CopyFromInput index="stemQ" /><InsertSegments><PhoneticShape>q</PhoneticShape></InsertSegments></MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
            <MorphemeId>Q</MorphemeId>
          </MorphologicalRule>
        </MorphologicalRuleDefinitions>
        <AffixTemplates>
          <AffixTemplate requiredPartsOfSpeech="posV">
            <Name>U</Name>
            <Slot morphologicalRules="mrQ"><Name>u0</Name></Slot>
          </AffixTemplate>
        </AffixTemplates>
        <LexicalEntries></LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>"#;

pub fn grammar() -> Grammar {
    pg_grammar::load(STRUCTURAL_XML).unwrap_or_else(|e| panic!("fixture failed to load: {e}"))
}

/// A morpheme id past the end of the grammar's own table, which nothing can own.
pub fn unowned_morpheme(g: &Grammar) -> MorphemeId {
    MorphemeId(g.morphemes.len() as u32 + 1)
}
