pub(crate) use pg_foma::test_support::{assert_canonical_lf_text_eq, assert_rendered_text_eq};

/// A minimal HC grammar whose only purpose is to give the identity projection real morpheme rows and a real part-of-speech symbol table to resolve against; three entries, deliberately unrelated to any real language, since the parity relation is a property of the comparison, not of any grammar.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) const PARITY_FIXTURE_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>ParityFixture</Name>
    <PartsOfSpeech>
      <PartOfSpeech id="posN"><Name>N</Name></PartOfSpeech>
      <PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech>
    </PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cB"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cC"><Representations><Representation>c</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered">
        <Name>S</Name>
        <LexicalEntries>
          <LexicalEntry id="entryA" partOfSpeech="posN">
            <Allomorphs><Allomorph id="alloA"><PhoneticShape>ab</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>a-root</Gloss>
          </LexicalEntry>
          <LexicalEntry id="entryB" partOfSpeech="posV">
            <Allomorphs><Allomorph id="alloB"><PhoneticShape>ba</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>b-root</Gloss>
          </LexicalEntry>
          <LexicalEntry id="entryC" partOfSpeech="posN">
            <Allomorphs><Allomorph id="alloC"><PhoneticShape>ca</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>c-root</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

/// The compiled `PARITY_FIXTURE_XML`.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn parity_fixture_grammar() -> pg_grammar::model::Grammar {
    let g = pg_grammar::load(PARITY_FIXTURE_XML).expect("the parity fixture grammar must load");
    assert!(
        g.morphemes.len() >= 3,
        "the parity fixture must expose at least three morpheme ordinals to project, got {}",
        g.morphemes.len()
    );
    g
}

/// One analysis of a single morpheme, with every identity-invisible field left at its neutral value; helpers below vary exactly one field at a time from this.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn parity_analysis(morpheme_ordinal: u32) -> pg_parse::WordAnalysis {
    pg_parse::WordAnalysis {
        morpheme_ids: vec![morpheme_ordinal],
        morph_occurrences: Vec::new(),
        root_morpheme_index: 0,
        pos_id: None,
        syn_fs: pg_featstruct::FeatureStruct::EMPTY,
        mpr: pg_grammar::model::MprSet::EMPTY,
        guessed: false,
        guessed_string: None,
        provenance: pg_parse::AnalysisProvenance::Grammar,
        supplied_root: None,
        morpheme_roots: vec![None],
    }
}
