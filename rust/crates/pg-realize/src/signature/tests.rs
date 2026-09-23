use super::*;

/// Synthetic fixture: `eRoot` (literal gloss), `eBare` (no gloss, no `<MorphemeId>`), `eAff` (no gloss, `<MorphemeId>` `M7`), and `eWeird` (a literal gloss containing every signature separator plus a quote and backslash, to pin JSON-quoting disambiguation).
const FIXTURE_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>R4GlossSignatureFixture</Name>
    <PartsOfSpeech><PartOfSpeech id="n"><Name>N</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="table1">
      <Name>Orthography</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="segA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="segK"><Representations><Representation>k</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="segL"><Representations><Representation>l</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="segP"><Representations><Representation>p</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="segI"><Representations><Representation>i</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="segM"><Representations><Representation>m</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="segT"><Representations><Representation>t</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="segU"><Representations><Representation>u</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="segZ"><Representations><Representation>z</Representation></Representations></SegmentDefinition>
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
          <LexicalEntry id="eRoot" partOfSpeech="n">
            <Allomorphs><Allomorph id="eRoot-1"><PhoneticShape>kal</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>GX</Gloss>
          </LexicalEntry>
          <LexicalEntry id="eBare" partOfSpeech="n">
            <Allomorphs><Allomorph id="eBare-1"><PhoneticShape>tuz</PhoneticShape></Allomorph></Allomorphs>
          </LexicalEntry>
          <LexicalEntry id="eAff" partOfSpeech="n">
            <Allomorphs><Allomorph id="eAff-1"><PhoneticShape>pim</PhoneticShape></Allomorph></Allomorphs>
            <MorphemeId>M7</MorphemeId>
          </LexicalEntry>
          <LexicalEntry id="eWeird" partOfSpeech="n">
            <Allomorphs><Allomorph id="eWeird-1"><PhoneticShape>tuk</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>a+b|c;d"e\f</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

fn grammar() -> Grammar {
    pg_grammar::load(FIXTURE_XML).unwrap_or_else(|e| panic!("fixture grammar failed to load: {e}"))
}

fn morpheme_ordinal(g: &Grammar, xml_id: &str) -> u32 {
    g.morphemes
        .iter()
        .position(|m| m.xml_key == xml_id)
        .unwrap_or_else(|| panic!("no morpheme with xml_key {xml_id}")) as u32
}

/// Synthetic `WordAnalysis`es don't need a real parse, since `gloss_bundle` only reads `Grammar::morphemes` by ordinal.
fn wa(morpheme_ids: Vec<u32>, root_morpheme_index: i32) -> WordAnalysis {
    let morpheme_count = morpheme_ids.len();
    WordAnalysis {
        morpheme_ids,
        morph_occurrences: Vec::new(),
        root_morpheme_index,
        pos_id: None,
        syn_fs: Default::default(),
        mpr: pg_grammar::model::MprSet::EMPTY,
        guessed: false,
        guessed_string: None,
        provenance: pg_parse::AnalysisProvenance::Grammar,
        supplied_root: None,
        morpheme_roots: vec![None; morpheme_count],
    }
}

#[test]
fn literal_gloss_renders_g_tag() {
    let g = grammar();
    let root = morpheme_ordinal(&g, "eRoot");
    let entry = gloss_signature_entry(&g, &wa(vec![root], 0), "kal");
    assert_eq!(entry, r#"g:"GX"|s:"kal""#);
}

#[test]
fn missing_gloss_renders_m_tag_with_owning_morpheme_id() {
    let g = grammar();
    let aff = morpheme_ordinal(&g, "eAff");
    let entry = gloss_signature_entry(&g, &wa(vec![aff], 0), "pim");
    assert_eq!(entry, r#"m:"M7"|s:"pim""#);
}

#[test]
fn missing_gloss_with_no_morpheme_id_renders_empty_string_id() {
    let g = grammar();
    let bare = morpheme_ordinal(&g, "eBare");
    let entry = gloss_signature_entry(&g, &wa(vec![bare], 0), "tuz");
    assert_eq!(entry, r#"m:""|s:"tuz""#);
}

#[test]
fn surface_shape_renders_s_tag_and_keeps_boundary_markers_verbatim() {
    // The shape half is passed through byte-for-byte; this module never re-renders shape, only encodes what the caller already computed.
    let g = grammar();
    let root = morpheme_ordinal(&g, "eRoot");
    let entry = gloss_signature_entry(&g, &wa(vec![root], 0), "kal+pim");
    assert_eq!(entry, r#"g:"GX"|s:"kal+pim""#);
}

#[test]
fn multi_component_analysis_joins_components_with_plus() {
    let g = grammar();
    let root = morpheme_ordinal(&g, "eRoot");
    let aff = morpheme_ordinal(&g, "eAff");
    let entry = gloss_signature_entry(&g, &wa(vec![root, aff], 0), "kal+pim");
    assert_eq!(entry, r#"g:"GX"+m:"M7"|s:"kal+pim""#);
}

#[test]
fn duplicate_analyses_are_preserved_not_deduped() {
    let g = grammar();
    let root = morpheme_ordinal(&g, "eRoot");
    let entry = gloss_signature_entry(&g, &wa(vec![root], 0), "kal");
    let sig = gloss_analysis_set_signature(&[entry.clone(), entry]);
    assert_eq!(sig, r#"g:"GX"|s:"kal";g:"GX"|s:"kal""#);
}

#[test]
fn sort_order_is_ordinal_byte_order_not_case_insensitive_or_length_based() {
    // Deliberately scrambled insertion order: byte/ordinal order places uppercase ASCII before lowercase, unlike a case-insensitive or locale-aware comparer, and the longer 2-component entry sorting before the shorter one pins that this is a byte comparison, not length-based.
    let entries = vec![
        r#"g:"b"|s:"x""#.to_string(),
        r#"g:"B"|s:"x""#.to_string(),
        r#"g:"A"+g:"A"|s:"x""#.to_string(),
    ];
    let sig = gloss_analysis_set_signature(&entries);
    assert_eq!(sig, r#"g:"A"+g:"A"|s:"x";g:"B"|s:"x";g:"b"|s:"x""#);
}

#[test]
fn literal_gloss_containing_every_separator_stays_disambiguated_by_json_quoting() {
    // `a+b|c;d"e\f` contains all three signature separators plus a quote and a backslash; canonical JSON escapes only the quote and backslash, so the separators pass through literally inside the quotes and can't be mistaken for real ones.
    let g = grammar();
    let weird = morpheme_ordinal(&g, "eWeird");
    let entry = gloss_signature_entry(&g, &wa(vec![weird], 0), "tuk");
    assert_eq!(entry, r#"g:"a+b|c;d\"e\\f"|s:"tuk""#);
}

#[test]
fn zero_analyses_render_dash() {
    assert_eq!(gloss_analysis_set_signature(&[]), "-");
}

#[test]
fn skipped_rows_reuse_the_same_dash_literal_as_zero_analyses() {
    // SKIPPED rows never call into this module; callers hardcode the literal `-` directly, so this pins that this module's empty-set literal is byte-identical to that hardcoded convention.
    let skipped_signature = "-".to_string();
    assert_eq!(gloss_analysis_set_signature(&[]), skipped_signature);
}

#[test]
fn word_gloss_signature_combines_entry_building_and_assembly() {
    let g = grammar();
    let root = morpheme_ordinal(&g, "eRoot");
    let aff = morpheme_ordinal(&g, "eAff");
    let analyses = vec![
        (wa(vec![root], 0), "kal".to_string()),
        (wa(vec![root, aff], 0), "kal+pim".to_string()),
    ];
    let sig = word_gloss_signature(&g, &analyses);
    assert_eq!(sig, r#"g:"GX"+m:"M7"|s:"kal+pim";g:"GX"|s:"kal""#);
}
