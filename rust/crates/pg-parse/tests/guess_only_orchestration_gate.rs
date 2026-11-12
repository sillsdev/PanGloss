//! A parse without guessing followed, on a lexical miss, by one `guess_only` parse returns exactly what one `guess_root` parse returns.

use pg_grammar::load;
use pg_parse::{Morpher, ParseOptions, ParseOutcome};

const XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>AnalyzeWordCanGuess</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cG"><Representations><Representation>g</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cD"><Representations><Representation>d</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
      <BoundaryDefinitions>
        <BoundaryDefinition id="cPlus"><Representations><Representation>+</Representation></Representations></BoundaryDefinition>
      </BoundaryDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRules="mrEd">
        <Name>Morphophonemic</Name>
        <MorphologicalRuleDefinitions>
          <MorphologicalRule id="mrEd" requiredPartsOfSpeech="posV">
            <Name>ed_suffix</Name>
            <MorphemeId>PAST</MorphemeId>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subEd">
                <MorphologicalInput>
                  <PhoneticSequence id="stem">
                    <OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence>
                  </PhoneticSequence>
                </MorphologicalInput>
                <MorphologicalOutput>
                  <CopyFromInput index="stem" />
                  <InsertSegments><PhoneticShape>+d</PhoneticShape></InsertSegments>
                </MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
          </MorphologicalRule>
        </MorphologicalRuleDefinitions>
        <LexicalEntries>
          <LexicalEntry id="ePattern">
            <Allomorphs><Allomorph id="aPattern"><PhoneticShape>g[Any]*</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>pattern</Gloss>
          </LexicalEntry>
          <LexicalEntry id="eDad" partOfSpeech="posV">
            <Allomorphs><Allomorph id="aDad"><PhoneticShape>dad</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>dad</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

fn same(a: &ParseOutcome, b: &ParseOutcome, word: &str) {
    assert_eq!(a.analyses, b.analyses, "{word}: analyses and order");
    assert_eq!(a.guessed, b.guessed, "{word}: guessed flag");
    assert_eq!(a.invalid_shape, b.invalid_shape, "{word}: invalid shape");
    assert_eq!(a.capped, b.capped, "{word}: capped");
    assert_eq!(
        format!("{:?}", a.structured),
        format!("{:?}", b.structured),
        "{word}: structured analyses, guessed strings included"
    );
}

fn two_step(m: &Morpher, word: &str) -> ParseOutcome {
    let first = m.parse_word_opts(word, &ParseOptions::default());
    if first.analyses.is_empty() && !first.capped && !first.invalid_shape {
        return m.parse_word_opts(word, &ParseOptions::default().with_guess_only(true));
    }
    first
}

#[test]
fn miss_then_guess_only_equals_one_guess_root_call() {
    let g = load(XML).unwrap();
    let m = Morpher::new(&g, usize::MAX);
    assert_eq!(m.lexical_patterns().len(), 1);
    let one_call = ParseOptions::default().with_guess_root(true);
    let (mut normal, mut guessed, mut neither) = (0, 0, 0);
    for word in [
        "dad", "dadd", "gag", "gagd", "gad", "gadd", "aaa", "ad", "dag", "dadda", "x", "gx",
    ] {
        let expected = m.parse_word_opts(word, &one_call);
        let got = two_step(&m, word);
        same(&got, &expected, word);
        match (expected.guessed, expected.analyses.is_empty()) {
            (true, _) => guessed += 1,
            (false, false) => normal += 1,
            (false, true) => neither += 1,
        }
    }
    assert!(
        normal > 0 && guessed > 0 && neither > 0,
        "{normal} {guessed} {neither}"
    );
}
