//! One suffix rule at three applications: the chain proposes it, and an over-budget chain refuses.

use pg_foma::emit::{self, FomaTier};
use pg_grammar::model::Grammar;
use pg_parse::{Morpher, ParseOptions};

/// Suffix rules `p`, `q`, `r` (as many as `apps` has entries) at those `multipleApplication` values, one root `k`.
fn fixture_xml(apps: &[u32]) -> String {
    let letters = ['p', 'q', 'r'];
    let segs: String = letters
        .iter()
        .map(|l| format!(r#"<SegmentDefinition id="c{l}"><Representations><Representation>{l}</Representation></Representations></SegmentDefinition>"#))
        .collect();
    let ids: Vec<String> = (0..apps.len())
        .map(|i| format!("mr{}", letters[i]))
        .collect();
    let rules: String = apps
        .iter()
        .zip(letters)
        .map(|(m, l)| {
            format!(
                r#"<MorphologicalRule id="mr{l}" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV" multipleApplication="{m}">
            <Name>{l}</Name>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="sub{l}">
                <MorphologicalInput><PhoneticSequence id="stem{l}"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                <MorphologicalOutput><CopyFromInput index="stem{l}" /><InsertSegments><PhoneticShape>{l}</PhoneticShape></InsertSegments></MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
            <MorphemeId>M{l}</MorphemeId>
          </MorphologicalRule>"#
            )
        })
        .collect();
    let ids = ids.join(" ");
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE HermitCrabInput SYSTEM "HermitCrabInput.dtd">
<HermitCrabInput>
  <Language>
    <Name>MultipleApplicationChainFixture</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>v</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="ck"><Representations><Representation>k</Representation></Representations></SegmentDefinition>
        {segs}
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses><FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass></NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="linear" morphologicalRules="{ids}">
        <Name>Main</Name>
        <MorphologicalRuleDefinitions>
          {rules}
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
    let g = load(&fixture_xml(&[3]));
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
}

/// The depth sums `max_apps` over several rules: p (2) and q (2) need depth 4 for `kpqpq`.
#[test]
fn depth_is_the_sum_over_several_rules() {
    let g = load(&fixture_xml(&[2, 2]));
    let morpher = Morpher::new(&g, usize::MAX);
    let oracle = morpher.parse_word_opts("kppqq", &ParseOptions::default());
    assert_eq!(oracle.structured.len(), 1, "HermitCrab must parse kppqq");
    let mut analyzer = pg_foma::composite::compile_analyzer(&g).expect("fixture must compile");
    let outcome = analyzer.analyze_word("kppqq");
    assert!(outcome.candidates_generated > 0);
    assert_eq!(outcome.confirmed, 1);
}

fn refusal_of(apps: &[u32]) -> Option<(String, usize, usize)> {
    let result = emit::emit(&load(&fixture_xml(apps)));
    match &result.report.tier {
        FomaTier::Unsupported { reason } => {
            assert!(result.lexc_source.is_empty());
            let b = result
                .report
                .enum_budget_exceeded
                .as_ref()
                .expect("typed reason");
            assert_eq!(b.measure, "derivation chain depth");
            assert!(result.report.closure_evidence.is_none());
            Some((reason.clone(), b.value, b.limit))
        }
        _ => None,
    }
}

/// Exactly the budget compiles; one more level refuses, and the message names the measure and the setting.
#[test]
fn budget_boundary_is_200_ok_and_201_refused() {
    assert!(
        refusal_of(&[100, 100]).is_none(),
        "200 levels is within budget"
    );
    let (reason, value, limit) = refusal_of(&[100, 101]).expect("201 levels must refuse");
    assert_eq!((value, limit), (201, 200));
    assert!(
        reason.contains("derivation chain depth") && reason.contains("HC_DERIV_CHAIN_DEPTH_BUDGET"),
        "{reason}"
    );
    assert!(refusal_of(&[500]).is_some());
}

/// The env value overrides the default, and a malformed one is an error, not a silent default.
#[test]
fn env_budget_parses_and_rejects_malformed() {
    assert_eq!(emit::parse_deriv_chain_budget(None), Ok(200));
    assert_eq!(emit::parse_deriv_chain_budget(Some(" 7 ")), Ok(7));
    assert!(emit::parse_deriv_chain_budget(Some("lots")).is_err());
    assert!(emit::parse_deriv_chain_budget(Some("-1")).is_err());
}
