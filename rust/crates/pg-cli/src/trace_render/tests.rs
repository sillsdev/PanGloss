//! A golden test comparing the fixed text-tree output for a small hand-built grammar/word against a checked-in expected string.

use super::*;
use pg_parse::{Morpher, ParseOptions};

/// The smallest grammar exercising a real synthesis rule-applied event: one `posV` root ("sag") plus a suffix rule appending "+d".
fn golden_grammar() -> Grammar {
    const XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>Golden</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <HeadFeatures />
    <MorphologicalPhonologicalRuleFeatures>
      <MorphologicalPhonologicalRuleFeature id="mprA">Alpha</MorphologicalPhonologicalRuleFeature>
      <MorphologicalPhonologicalRuleFeatureGroup features="mprA"><Name>G</Name></MorphologicalPhonologicalRuleFeatureGroup>
    </MorphologicalPhonologicalRuleFeatures>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cS"><Representations><Representation>s</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cG"><Representations><Representation>g</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cD"><Representations><Representation>d</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
      <BoundaryDefinitions>
        <BoundaryDefinition id="cPlus"><Representations><Representation>+</Representation></Representations></BoundaryDefinition>
      </BoundaryDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <SegmentNaturalClass id="ncAny"><Name>Any</Name><Segment segment="cS" /><Segment segment="cA" /><Segment segment="cG" /><Segment segment="cD" /></SegmentNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" morphologicalRules="mrEd">
        <Name>S</Name>
        <MorphologicalRuleDefinitions>
          <MorphologicalRule id="mrEd" requiredPartsOfSpeech="posV"><Name>ed_suffix</Name><MorphemeId>PAST</MorphemeId>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subEd">
                <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>+d</PhoneticShape></InsertSegments></MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
          </MorphologicalRule>
        </MorphologicalRuleDefinitions>
        <LexicalEntries>
          <LexicalEntry id="e32" partOfSpeech="posV"><MorphemeId>32</MorphemeId>
            <Allomorphs><Allomorph id="a32"><PhoneticShape>sag</PhoneticShape></Allomorph></Allomorphs>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;
    pg_grammar::load(XML).unwrap_or_else(|e| panic!("golden_grammar failed to load: {e}"))
}

#[test]
fn text_render_matches_golden_string() {
    let g = golden_grammar();
    let m = Morpher::new(&g, usize::MAX);
    let sink = TreeTraceSink::new();
    let outcome = m.parse_word_traced("sagd", &ParseOptions::default(), &sink);
    assert!(
        !outcome.analyses.is_empty(),
        "sanity: \"sagd\" must still parse"
    );

    let root = sink.root().expect("analyze_word must mint a root");
    let rendered = render_text(&g, &sink, root);

    // Regenerated from this test's own computed `rendered` value, never hand-typed.
    let expected = "WordAnalysis  input=sagd\n\
                         \x20 StratumAnalysisInput \"S\"  input=sagd\n\
                         \x20 StratumAnalysisOutput \"S\"  shape=sagd\n\
                         \x20 MorphologicalRuleAnalysis \"ed_suffix\" subrule=0  shape=sag\n\
                         \x20   StratumAnalysisOutput \"S\"  shape=sag\n\
                         \x20   LexicalLookup \"S\"  input=sag\n\
                         \x20   StratumSynthesisInput \"S\"  input=sag\n\
                         \x20   MorphologicalRuleSynthesis \"ed_suffix\" subrule=0  shape=sagd\n\
                         \x20     StratumSynthesisOutput \"S\"  shape=sagd\n\
                         \x20     Successful  shape=sagd\n\
                         \x20   MorphologicalRuleSynthesis \"ed_suffix\"  [NonPartialRuleProhibitedAfterFinalTemplate]  input=sag\n\
                         \x20   Failed  [PartialParse]  shape=sag\n\
                         \x20 LexicalLookup \"S\"  input=sagd\n";
    assert_eq!(
        rendered, expected,
        "golden text-tree render changed:\n{rendered}"
    );
}

#[test]
fn json_render_is_well_formed_and_carries_the_same_shape() {
    let g = golden_grammar();
    let m = Morpher::new(&g, usize::MAX);
    let sink = TreeTraceSink::new();
    let _outcome = m.parse_word_traced("sagd", &ParseOptions::default(), &sink);
    let root = sink.root().expect("analyze_word must mint a root");
    let rendered = render_json(&g, &sink, root);

    assert!(rendered.contains("\"type\":\"WordAnalysis\""));
    assert!(rendered.contains("\"source\":\"ed_suffix\""));
    assert!(rendered.contains("\"type\":\"Successful\""));
    // Balanced braces/brackets -- a cheap well-formedness check without a JSON parser dependency.
    assert_eq!(rendered.matches('{').count(), rendered.matches('}').count());
    assert_eq!(rendered.matches('[').count(), rendered.matches(']').count());
}
