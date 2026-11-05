//! One suffix rule at three applications: the chain proposes it, and an over-budget chain refuses.

use pg_foma::emit::{self, FomaTier};
use pg_grammar::model::Grammar;
use pg_parse::{Morpher, ParseOptions};

/// One suffix rule `p` at `multipleApplication = max_apps`, one root `k`.
fn fixture_xml(max_apps: u32) -> String {
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE HermitCrabInput SYSTEM "HermitCrabInput.dtd">
<HermitCrabInput>
  <Language>
    <Name>CoverUnorderedMorphRulesFixture</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>v</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="ck"><Representations><Representation>k</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cp"><Representations><Representation>p</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cq"><Representations><Representation>q</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses><FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass></NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="linear" morphologicalRules="mrP">
        <Name>Main</Name>
        <MorphologicalRuleDefinitions>
          <MorphologicalRule id="mrP" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV" multipleApplication="{max_apps}">
            <Name>p</Name>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subP">
                <MorphologicalInput><PhoneticSequence id="stemP"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                <MorphologicalOutput><CopyFromInput index="stemP" /><InsertSegments><PhoneticShape>p</PhoneticShape></InsertSegments></MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
            <MorphemeId>P</MorphemeId>
          </MorphologicalRule>
        </MorphologicalRuleDefinitions>
        <LexicalEntries>
          <LexicalEntry id="eK" partOfSpeech="posV">
            <Allomorphs><Allomorph id="aK"><PhoneticShape>k</PhoneticShape></Allomorph></Allomorphs>
            <MorphemeId>K</MorphemeId>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>"#
    )
}

fn load(xml: &str) -> Grammar {
    pg_grammar::load(xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}\n{xml}"))
}

/// HermitCrab parses `kppp` (three applications); the FST proposer must propose it and confirm must agree.
#[test]
fn three_applications_are_proposed_and_confirmed() {
    let g = load(&fixture_xml(3));
    let morpher = Morpher::new(&g, usize::MAX);
    let oracle = morpher.parse_word_opts("kppp", &ParseOptions::default());
    assert_eq!(oracle.structured.len(), 1, "HermitCrab must parse kppp");
    let mut analyzer = pg_foma::composite::compile_analyzer(&g).expect("fixture must compile");
    let outcome = analyzer.analyze_word("kppp");
    assert!(
        outcome.candidates_generated > 0,
        "the FST proposer must propose kppp (chain depth = sum of max_apps = 3)"
    );
    assert_eq!(outcome.confirmed, 1);
    // A fourth application is past multipleApplication: HermitCrab refuses it, so confirm must too.
    let four = analyzer.analyze_word("kpppp");
    assert_eq!(four.confirmed, 0);
}

/// A chain deeper than the budget is refused with a typed reason, with no lexc source.
#[test]
fn chain_over_the_budget_is_refused_not_truncated() {
    let g = load(&fixture_xml(500));
    let result = emit::emit(&g);
    let FomaTier::Unsupported { reason } = &result.report.tier else {
        panic!(
            "over-budget chain must refuse, got {:?}",
            result.report.tier
        );
    };
    assert!(reason.contains("derivation chain"), "{reason}");
    assert!(result.lexc_source.is_empty());
    let budget = result
        .report
        .enum_budget_exceeded
        .as_ref()
        .expect("typed reason");
    assert_eq!(budget.measure, "derivation chain depth");
    assert_eq!(budget.value, 500);
    assert_eq!(budget.limit, 200);
}
