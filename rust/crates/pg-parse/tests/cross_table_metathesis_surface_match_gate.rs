//! Metathesis-relocated segments must reset `char_def`, or a stale raw index collides across tables at the surface-match gate.

use pg_parse::Morpher;

const XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>CrossTableMetathesisSurfaceMatchProbe</Name>
    <PartsOfSpeech><PartOfSpeech id="p"><Name>P</Name></PartOfSpeech></PartsOfSpeech>
    <PhonologicalFeatureSystem>
      <SymbolicFeature id="featId">
        <Name>id</Name>
        <Symbols>
          <Symbol id="symM">m</Symbol>
          <Symbol id="symX">x</Symbol>
          <Symbol id="symW">w</Symbol>
          <Symbol id="symZ">z</Symbol>
        </Symbols>
      </SymbolicFeature>
    </PhonologicalFeatureSystem>
    <CharacterDefinitionTable id="t0">
      <Name>Inner</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="c0m">
          <Representations><Representation>m</Representation></Representations>
          <FeatureValue feature="featId" symbolValues="symM" />
        </SegmentDefinition>
        <SegmentDefinition id="c0x">
          <Representations><Representation>x</Representation></Representations>
          <FeatureValue feature="featId" symbolValues="symX" />
        </SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <CharacterDefinitionTable id="t1">
      <Name>Outer</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="c1z">
          <Representations><Representation>z</Representation></Representations>
          <FeatureValue feature="featId" symbolValues="symZ" />
        </SegmentDefinition>
        <SegmentDefinition id="c1m">
          <Representations><Representation>m</Representation></Representations>
          <FeatureValue feature="featId" symbolValues="symM" />
        </SegmentDefinition>
        <SegmentDefinition id="c1x">
          <Representations><Representation>x</Representation></Representations>
          <FeatureValue feature="featId" symbolValues="symX" />
        </SegmentDefinition>
        <SegmentDefinition id="c1w">
          <Representations><Representation>w</Representation></Representations>
          <FeatureValue feature="featId" symbolValues="symW" />
        </SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <FeatureNaturalClass id="ncSwitchA"><Name>SwitchA</Name>
        <FeatureValue feature="featId" symbolValues="symM symW" />
      </FeatureNaturalClass>
      <FeatureNaturalClass id="ncSwitchB"><Name>SwitchB</Name>
        <FeatureValue feature="featId" symbolValues="symX" />
      </FeatureNaturalClass>
    </NaturalClasses>
    <PhonologicalRuleDefinitions>
      <MetathesisRule id="mrCrossTableSwap" leftSwitch="swB" rightSwitch="swA">
        <Name>crossTableSwap</Name>
        <StructuralDescription>
          <PhoneticTemplate>
            <PhoneticSequence>
              <SimpleContext id="swA" naturalClass="ncSwitchA" />
              <SimpleContext id="swB" naturalClass="ncSwitchB" />
            </PhoneticSequence>
          </PhoneticTemplate>
        </StructuralDescription>
      </MetathesisRule>
    </PhonologicalRuleDefinitions>
    <Strata>
      <Stratum characterDefinitionTable="t0" morphologicalRuleOrder="unordered">
        <Name>Inner</Name>
        <LexicalEntries>
          <LexicalEntry id="eRoot1">
            <Allomorphs><Allomorph id="aRoot1"><PhoneticShape>mx</PhoneticShape></Allomorph></Allomorphs>
            <MorphemeId>ROOT1</MorphemeId>
            <Gloss>root1</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" phonologicalRules="mrCrossTableSwap">
        <Name>Outer</Name>
        <LexicalEntries>
          <LexicalEntry id="eRoot2">
            <Allomorphs><Allomorph id="aRoot2"><PhoneticShape>wx</PhoneticShape></Allomorph></Allomorphs>
            <MorphemeId>ROOT2</MorphemeId>
            <Gloss>root2</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

fn load() -> pg_grammar::model::Grammar {
    pg_grammar::load(XML)
        .unwrap_or_else(|e| panic!("cross-table metathesis surface-match probe grammar loads: {e}"))
}

/// A root on a different (inner) stratum's table than the metathesis rule's (outer) stratum must still analyze once correctly metathesized.
#[test]
fn cross_table_metathesized_root_matches_its_own_surface() {
    let g = load();
    assert_eq!(
        g.char_tables.len(),
        2,
        "fixture must declare exactly 2 tables"
    );
    assert_eq!(g.strata.len(), 2, "fixture must declare exactly 2 strata");

    let morpher = Morpher::new(&g, usize::MAX).with_memo(true);
    assert_eq!(
        morpher.parse_word("xm").signature(),
        "ROOT1|xm",
        "ROOT1 (Inner stratum, table t0), correctly metathesized to \"xm\" on the Outer stratum \
         (table t1), must analyze -- an empty result here means the metathesized segment's stale \
         origin-table char_def collided with table t1's own raw indices at the surface-match gate"
    );
}

/// ROOT1's raw, un-metathesized spelling must never be a valid surface form; metathesis is obligatory.
#[test]
fn cross_table_root_raw_spelling_still_rejected() {
    let g = load();
    let morpher = Morpher::new(&g, usize::MAX).with_memo(true);
    assert_eq!(
        morpher.parse_word("mx").signature(),
        "-",
        "ROOT1's raw, un-metathesized spelling must still find zero analyses"
    );
}

/// ROOT2 (same-table control) correctly metathesized to "xw" must keep matching, so ordinary same-table metathesis recall is unaffected.
#[test]
fn same_table_metathesis_recall_is_unaffected() {
    let g = load();
    let morpher = Morpher::new(&g, usize::MAX).with_memo(true);
    assert_eq!(morpher.parse_word("xw").signature(), "ROOT2|xw");
    assert_eq!(
        morpher.parse_word("wx").signature(),
        "-",
        "ROOT2's own raw spelling must still find zero analyses (metathesis is obligatory)"
    );
}
