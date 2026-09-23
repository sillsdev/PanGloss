use super::*;

/// A one-feature, one-table, no-rules grammar with a glossed root ("house") and an unglossed one ("mystery").
const MINI_GRAMMAR_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>N0GlossFixture</Name>
    <PartsOfSpeech><PartOfSpeech id="n"><Name>Noun</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="table1">
      <Name>Orthography</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="segH"><Representations><Representation>h</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="segO"><Representations><Representation>o</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="segU"><Representations><Representation>u</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="segS"><Representations><Representation>s</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="segE"><Representations><Representation>e</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="segM"><Representations><Representation>m</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="segY"><Representations><Representation>y</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="segT"><Representations><Representation>t</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="segR"><Representations><Representation>r</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="segI"><Representations><Representation>i</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
      <BoundaryDefinitions>
        <BoundaryDefinition id="bdry1"><Representations><Representation>+</Representation></Representations></BoundaryDefinition>
      </BoundaryDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses></NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="table1">
        <Name>main</Name>
        <LexicalEntries>
          <LexicalEntry id="leHouse" partOfSpeech="n">
            <Allomorphs><Allomorph id="leHouse-1"><PhoneticShape>house</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>house</Gloss>
          </LexicalEntry>
          <LexicalEntry id="leMystery" partOfSpeech="n">
            <Allomorphs><Allomorph id="leMystery-1"><PhoneticShape>mystery</PhoneticShape></Allomorph></Allomorphs>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

fn grammar() -> Grammar {
    pg_grammar::load(MINI_GRAMMAR_XML)
        .unwrap_or_else(|e| panic!("fixture grammar failed to load: {e}"))
}

/// A hand-built `WordAnalysis` exercises the same paths as a real parse, since `gloss_bundle` only reads `Grammar::morphemes` by ordinal.
fn wa(morpheme_ids: Vec<u32>, root_morpheme_index: i32, guessed: bool) -> WordAnalysis {
    let morpheme_count = morpheme_ids.len();
    WordAnalysis {
        morpheme_ids,
        morph_occurrences: Vec::new(),
        root_morpheme_index,
        pos_id: None,
        syn_fs: Default::default(),
        mpr: pg_grammar::model::MprSet::EMPTY,
        guessed,
        guessed_string: None,
        provenance: pg_parse::AnalysisProvenance::Grammar,
        supplied_root: None,
        morpheme_roots: vec![None; morpheme_count],
    }
}

fn morpheme_ordinal(g: &Grammar, xml_id: &str) -> u32 {
    g.morphemes
        .iter()
        .position(|m| m.xml_key == xml_id)
        .unwrap_or_else(|| panic!("no morpheme with xml_key {xml_id}")) as u32
}

#[test]
fn glossed_root_resolves_gloss_and_is_root() {
    let g = grammar();
    let house = morpheme_ordinal(&g, "leHouse");
    let bundle = gloss_bundle(&g, &wa(vec![house], 0, false));
    assert_eq!(bundle.tokens.len(), 1);
    assert_eq!(bundle.tokens[0].gloss.as_deref(), Some("house"));
    assert!(bundle.tokens[0].is_root);
    assert_eq!(bundle.root_index, Some(0));
    assert!(!bundle.guessed);
    assert_eq!(leipzig(&bundle, "house"), "house");
}

#[test]
fn unglossed_real_root_renders_bracket_question_not_surface() {
    let g = grammar();
    let mystery = morpheme_ordinal(&g, "leMystery");
    let bundle = gloss_bundle(&g, &wa(vec![mystery], 0, false));
    assert_eq!(bundle.tokens[0].gloss, None);
    assert!(bundle.tokens[0].is_root);
    assert!(!bundle.guessed, "not a guess branch analysis");
    assert_eq!(
        leipzig(&bundle, "mystery"),
        "[?]",
        "unglossed but NOT guessed -> [?], not *surface*"
    );
}

#[test]
fn guessed_sentinel_becomes_gloss_less_root_token_regardless_of_grammar_row() {
    let g = grammar();
    let bundle = gloss_bundle(&g, &wa(vec![u32::MAX], 0, true));
    assert_eq!(bundle.tokens.len(), 1);
    assert_eq!(bundle.tokens[0].gloss, None);
    assert!(bundle.tokens[0].properties.is_empty());
    assert!(bundle.tokens[0].is_root);
    assert_eq!(bundle.root_index, Some(0));
    assert!(bundle.guessed);
    assert_eq!(leipzig(&bundle, "gag"), "*gag*");
}

#[test]
fn guessed_root_plus_real_affix_renders_mixed_leipzig() {
    let g = grammar();
    let house = morpheme_ordinal(&g, "leHouse");
    // A guessed root (index 0) followed by a real, glossed morpheme (index 1) exercises both branches at once.
    let bundle = gloss_bundle(&g, &wa(vec![u32::MAX, house], 0, true));
    assert_eq!(bundle.tokens.len(), 2);
    assert!(bundle.tokens[0].is_root);
    assert!(!bundle.tokens[1].is_root);
    assert_eq!(leipzig(&bundle, "gaghouse"), "*gaghouse*-house");
}

#[test]
fn out_of_range_ordinal_never_panics_and_renders_bracket_question() {
    let g = grammar();
    // 999 is not a valid ordinal into `g.morphemes` for this 2-entry fixture; gloss_bundle must not panic on it.
    let bundle = gloss_bundle(&g, &wa(vec![999], -1, false));
    assert_eq!(bundle.tokens.len(), 1);
    assert_eq!(bundle.tokens[0].gloss, None);
    assert!(bundle.tokens[0].properties.is_empty());
    assert!(!bundle.tokens[0].is_root);
    assert_eq!(bundle.root_index, None, "root_morpheme_index=-1 -> no root");
    assert_eq!(leipzig(&bundle, "whatever"), "[?]");
}

#[test]
fn negative_root_index_yields_none_root_index() {
    let g = grammar();
    let house = morpheme_ordinal(&g, "leHouse");
    let bundle = gloss_bundle(&g, &wa(vec![house], -1, false));
    assert_eq!(bundle.root_index, None);
    assert!(!bundle.tokens[0].is_root);
}

#[test]
fn root_index_past_end_of_morpheme_ids_yields_none_root_index() {
    let g = grammar();
    let house = morpheme_ordinal(&g, "leHouse");
    let bundle = gloss_bundle(&g, &wa(vec![house], 5, false));
    assert_eq!(bundle.root_index, None);
    assert!(!bundle.tokens[0].is_root);
}

#[test]
fn empty_morpheme_ids_renders_empty_string() {
    let g = grammar();
    let bundle = gloss_bundle(&g, &wa(vec![], -1, false));
    assert!(bundle.tokens.is_empty());
    assert_eq!(bundle.root_index, None);
    assert_eq!(leipzig(&bundle, "whatever"), "");
}

#[test]
fn properties_are_carried_through_when_present() {
    // A <Properties> entry confirms MorphemeInfo::properties survives resolution unchanged.
    const XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>N0PropsFixture</Name>
    <PartsOfSpeech><PartOfSpeech id="n"><Name>Noun</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="table1">
      <Name>Orthography</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="segH"><Representations><Representation>h</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="segI"><Representations><Representation>i</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
      <BoundaryDefinitions>
        <BoundaryDefinition id="bdry1"><Representations><Representation>+</Representation></Representations></BoundaryDefinition>
      </BoundaryDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses></NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="table1">
        <Name>main</Name>
        <LexicalEntries>
          <LexicalEntry id="leHi" partOfSpeech="n">
            <Allomorphs><Allomorph id="leHi-1"><PhoneticShape>hi</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>hi</Gloss>
            <Properties><Property name="realize">Case:Loc</Property></Properties>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;
    let g = pg_grammar::load(XML)
        .unwrap_or_else(|e| panic!("props fixture grammar failed to load: {e}"));
    let hi = morpheme_ordinal(&g, "leHi");
    let bundle = gloss_bundle(&g, &wa(vec![hi], 0, false));
    assert_eq!(
        bundle.tokens[0].properties,
        vec![("realize".to_string(), "Case:Loc".to_string())]
    );
}
