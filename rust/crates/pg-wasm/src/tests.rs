use super::*;

#[test]
fn tokenize_reconstructs_input_exactly() {
    let text = "Mwana ali na nyumba, m'phole-m'phole.\n16 Iwe.";
    let pieces = tokenize(text);
    let rejoined: String = pieces
        .iter()
        .map(|p| match p {
            Piece::Word(s) => *s,
            Piece::Other(s) => *s,
        })
        .collect();
    assert_eq!(rejoined, text);
}

#[test]
fn tokenize_splits_words_from_punctuation_and_digits() {
    // "hi" / ", 16" / "th" / "!": punctuation, whitespace, and digits merge into one "other" run; only word-vs-other transitions split pieces.
    let pieces = tokenize("hi, 16th!");
    let kinds: Vec<bool> = pieces.iter().map(|p| matches!(p, Piece::Word(_))).collect();
    assert_eq!(kinds, vec![true, false, true, false]);
}

// A small, hand-built, original HermitCrab XML fixture, just enough to exercise affix_glosses/build_realize_map; PanGlossGrammar's wasm-bindgen methods can't easily be driven from a plain cargo test, so these tests target the plain-Rust helpers directly.
const TEST_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>WasmWiringTest</Name>
    <PartsOfSpeech>
      <PartOfSpeech id="posN"><Name>n</Name></PartOfSpeech>
    </PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cH"><Representations><Representation>h</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cO"><Representations><Representation>o</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cU"><Representations><Representation>u</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cS"><Representations><Representation>s</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cE"><Representations><Representation>e</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
      <BoundaryDefinitions>
        <BoundaryDefinition id="cPlus"><Representations><Representation>+</Representation></Representations></BoundaryDefinition>
      </BoundaryDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <SegmentNaturalClass id="ncAll">
        <Name>All</Name>
        <Segment segment="cH" /><Segment segment="cO" /><Segment segment="cU" />
        <Segment segment="cS" /><Segment segment="cE" />
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
              <MorphologicalSubrule id="subPl">
                <MorphologicalInput>
                  <PhoneticSequence id="stem1">
                    <OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAll" /></OptionalSegmentSequence>
                  </PhoneticSequence>
                </MorphologicalInput>
                <MorphologicalOutput>
                  <CopyFromInput index="stem1" />
                  <InsertSegments><PhoneticShape>+es</PhoneticShape></InsertSegments>
                </MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
          </MorphologicalRule>
        </MorphologicalRuleDefinitions>
        <LexicalEntries>
          <LexicalEntry id="eHouse" partOfSpeech="posN">
            <Allomorphs><Allomorph id="aHouse"><PhoneticShape>house</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>house</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

#[test]
fn affix_glosses_includes_rule_glosses_but_not_root_glosses() {
    let g = pg_grammar::load(TEST_XML).expect("test fixture loads");
    let glosses = affix_glosses(&g);
    assert!(
        glosses.iter().any(|s| s == "pl"),
        "the plural rule's gloss must be included: {glosses:?}"
    );
    assert!(
        !glosses.iter().any(|s| s == "house"),
        "a lexical-entry (root) gloss must not be treated as an affix gloss: {glosses:?}"
    );
}

#[test]
fn build_realize_map_infers_from_affix_glosses_when_no_sidecar() {
    let g = pg_grammar::load(TEST_XML).expect("test fixture loads");
    let map = build_realize_map(&g, None).expect("builds base map");
    assert_eq!(
        map.lookup("pl"),
        Some(pg_realize::FeatureAssignment::Num(pg_realize::Num::Pl)),
        "the built-in English alias table must recognize the affix rule's 'pl' gloss"
    );
}

#[test]
fn build_realize_map_lets_sidecar_override_the_inferred_base() {
    let g = pg_grammar::load(TEST_XML).expect("test fixture loads");
    let sidecar = "[features]\n\"pl\" = \"Ignore\"\n";
    let map = build_realize_map(&g, Some(sidecar)).expect("builds overridden map");
    assert_eq!(
        map.lookup("pl"),
        Some(pg_realize::FeatureAssignment::Ignore),
        "an explicit sidecar mapping must win over the inferred base for the same gloss key"
    );
}

#[test]
fn build_realize_map_treats_blank_sidecar_the_same_as_none() {
    let g = pg_grammar::load(TEST_XML).expect("test fixture loads");
    let with_none = build_realize_map(&g, None).expect("builds with None");
    let with_blank = build_realize_map(&g, Some("   \n")).expect("builds with blank sidecar");
    assert_eq!(with_none, with_blank);
}
