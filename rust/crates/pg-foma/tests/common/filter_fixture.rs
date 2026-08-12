//! One synthetic grammar the structural candidate-filter tests decide against.

use pg_grammar::model::{Grammar, MRuleId, MorphemeId};

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

/// The morpheme carried by the fixture element with this `id` attribute.
pub fn morpheme_of(g: &Grammar, xml_key: &str) -> MorphemeId {
    let index = g
        .morphemes
        .iter()
        .position(|m| m.xml_key == xml_key)
        .unwrap_or_else(|| panic!("no morpheme with xml id {xml_key:?}"));
    MorphemeId(index as u32)
}

/// A morpheme id past the end of the grammar's own table, which nothing can own.
pub fn unowned_morpheme(g: &Grammar) -> MorphemeId {
    MorphemeId(g.morphemes.len() as u32 + 1)
}

/// The rule that owns `morpheme`, read straight off `g.mrules`.
pub fn rule_of(g: &Grammar, morpheme: MorphemeId) -> MRuleId {
    for (index, rule) in g.mrules.iter().enumerate() {
        let owned = match rule {
            pg_grammar::model::MorphRuleDef::AffixProcess(def) => Some(def.morpheme),
            pg_grammar::model::MorphRuleDef::Realizational(def) => Some(def.morpheme),
            pg_grammar::model::MorphRuleDef::Compounding(_) => None,
        };
        if owned == Some(morpheme) {
            return MRuleId(index as u32);
        }
    }
    panic!("no rule owns morpheme {morpheme:?}");
}

/// The `(template, slot)` site listing `rule`, read straight off `g.templates`.
pub fn site_of(g: &Grammar, rule: MRuleId) -> (u16, u8) {
    for (template, def) in g.templates.iter().enumerate() {
        for (slot, def) in def.slots.iter().enumerate() {
            if def.rules.contains(&rule) {
                return (template as u16, slot as u8);
            }
        }
    }
    panic!("no template slot lists rule {rule:?}");
}

/// The stratum that declares `template`, read straight off `g.strata`.
pub fn stratum_of_template(g: &Grammar, template: u16) -> u8 {
    for (stratum, def) in g.strata.iter().enumerate() {
        if def.templates.iter().any(|t| t.0 == u32::from(template)) {
            return stratum as u8;
        }
    }
    panic!("no stratum declares template {template}");
}
