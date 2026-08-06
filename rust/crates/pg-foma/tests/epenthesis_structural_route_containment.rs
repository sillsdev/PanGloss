//! `EpenthesisStructuralRoutePredicate`'s containment witness: propose over-generates for an obligatory-epenthesis grammar, and confirm prunes to exactly the full-HC oracle's analysis set.

mod common;

use std::collections::HashSet;

use pg_foma::capability::{compose_envelope, default_registry, CompileDecision};
use pg_foma::composite::FomaAnalyzer;
use pg_foma::enumerate::enumerate_default;
use pg_foma::junctions::PhonologyProbe;
use pg_foma::replace::SegAlphabet;
use pg_grammar::model::{Grammar, PhonRuleDef};
use pg_parse::{Morpher, ParseOptions, WordAnalysis};

fn load(xml: &str) -> Grammar {
    pg_grammar::load(xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}\n{xml}"))
}

/// Root "x" + a suffix rule appending "y" + an obligatory epenthesis rule inserting "e" between them, so the real surface is "xey", never the naive "xy" concatenation.
fn fixture_xml() -> &'static str {
    r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>EpenthesisStructuralRouteFixture</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>v</Name></PartOfSpeech></PartsOfSpeech>
    <PhonologicalFeatureSystem>
      <!-- One distinct symbol value per segment (mirrors `tests/phase_c_right_to_left.rs`'s own
           `RTL_EPENTHESIS_XML` comment): without this, every `SegmentNaturalClass`'s feature
           bundle is vacuously empty (indistinguishable from any other segment's), and the
           epenthesis rule's `PhoneticOutput` natural-class reference has nothing to pick the
           RIGHT concrete segment BY -- verified empirically while building this fixture: omitting
           this system, `Morpher::generate_words` inserted the WRONG segment ("x", the table's
           first entry) instead of the intended "e". -->
      <SymbolicFeature id="featId">
        <Name>id</Name>
        <Symbols><Symbol id="symX">x</Symbol><Symbol id="symE">e</Symbol><Symbol id="symY">y</Symbol></Symbols>
      </SymbolicFeature>
    </PhonologicalFeatureSystem>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cx"><Representations><Representation>x</Representation></Representations><FeatureValue feature="featId" symbolValues="symX" /></SegmentDefinition>
        <SegmentDefinition id="ce"><Representations><Representation>e</Representation></Representations><FeatureValue feature="featId" symbolValues="symE" /></SegmentDefinition>
        <SegmentDefinition id="cy"><Representations><Representation>y</Representation></Representations><FeatureValue feature="featId" symbolValues="symY" /></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <SegmentNaturalClass id="ncE"><Name>Epenthetic</Name><Segment segment="ce" /></SegmentNaturalClass>
      <SegmentNaturalClass id="ncX"><Name>X</Name><Segment segment="cx" /></SegmentNaturalClass>
      <SegmentNaturalClass id="ncY"><Name>Y</Name><Segment segment="cy" /></SegmentNaturalClass>
      <FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass>
    </NaturalClasses>
    <PhonologicalRuleDefinitions>
      <PhonologicalRule id="prEpenthesis">
        <Name>epenthesisDemo</Name>
        <PhoneticInput><PhoneticSequence /></PhoneticInput>
        <PhonologicalSubrules>
          <PhonologicalSubrule>
            <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncE" /></PhoneticSequence></PhoneticOutput>
            <Environment>
              <LeftEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncX" /></PhoneticSequence></PhoneticTemplate></LeftEnvironment>
              <RightEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncY" /></PhoneticSequence></PhoneticTemplate></RightEnvironment>
            </Environment>
          </PhonologicalSubrule>
        </PhonologicalSubrules>
      </PhonologicalRule>
    </PhonologicalRuleDefinitions>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" morphologicalRules="mrSuf" phonologicalRules="prEpenthesis">
        <Name>Main</Name>
        <MorphologicalRuleDefinitions>
          <MorphologicalRule id="mrSuf" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
            <Name>suf</Name>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subSuf">
                <MorphologicalInput><PhoneticSequence id="stemSuf"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                <MorphologicalOutput><CopyFromInput index="stemSuf" /><InsertSegments><PhoneticShape>y</PhoneticShape></InsertSegments></MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
            <MorphemeId>SUF</MorphemeId>
          </MorphologicalRule>
        </MorphologicalRuleDefinitions>
        <LexicalEntries>
          <LexicalEntry id="eX" partOfSpeech="posV">
            <Allomorphs><Allomorph id="aX"><PhoneticShape>x</PhoneticShape></Allomorph></Allomorphs>
            <MorphemeId>X</MorphemeId>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>"#
}

/// `(morpheme_ids, root_morpheme_index)` multiset key -- same shape `tests/cover_mpr_groups.rs::
/// analysis_set`/`tests/cover_compounding.rs::analysis_set` use.
fn analysis_set(v: &[WordAnalysis]) -> HashSet<(Vec<u32>, i32)> {
    v.iter()
        .map(|a| (a.morpheme_ids.clone(), a.root_morpheme_index))
        .collect()
}

/// This fixture's `Epenthesis` occurrence must compose to `ConfirmOnly`, proving the containment test below exercises the disposition `EpenthesisStructuralRoutePredicate` actually ships.
#[test]
fn fixture_has_epenthesis_and_composes_to_confirm_only() {
    let g = load(fixture_xml());
    assert!(
        g.prules
            .iter()
            .any(|pr| matches!(pr, PhonRuleDef::Rewrite(r) if r.lhs.nodes.is_empty())),
        "fixture must declare an empty-LHS (epenthesis) rewrite rule"
    );
    let ro: Vec<&PhonRuleDef> = g
        .strata
        .iter()
        .flat_map(|s| &s.prules)
        .map(|&id| &g.prules[id.0 as usize])
        .collect();
    let phon = PhonologyProbe::new(&g);
    let alphabet = SegAlphabet::new(&g.char_tables[0]);
    let plan = enumerate_default(&g, &alphabet, &ro, phon.as_ref());
    let registry = default_registry();

    assert_eq!(
        compose_envelope(&g, &plan, &registry),
        CompileDecision::ConfirmOnly,
        "an epenthesis + ordinary-suffix fixture must compose to ConfirmOnly, never Refuse"
    );
}

/// Runs `word` through both the real propose->confirm composite and the full-HC oracle, and asserts exact structured-set equality between them, never mere containment.
fn assert_confirm_matches_oracle(
    analyzer: &mut FomaAnalyzer,
    morpher: &Morpher,
    word: &str,
    expect_nonempty: bool,
) -> pg_foma::composite::FomaOutcome {
    let oracle = morpher.parse_word_opts(word, &ParseOptions::default());
    let outcome = analyzer.analyze_word(word);

    assert_eq!(
        !oracle.structured.is_empty(),
        expect_nonempty,
        "oracle precondition for {word:?}: expected non-empty={expect_nonempty}, got {:?}",
        oracle.structured
    );
    assert_eq!(
        outcome.confirmed,
        oracle.structured.len(),
        "confirmed count must equal the oracle's exact analysis count for {word:?}"
    );
    assert_eq!(
        analysis_set(&outcome.structured),
        analysis_set(&oracle.structured),
        "FST-confirmed set must equal the oracle's own set for {word:?}"
    );
    outcome
}

/// The load-bearing containment witness: "xey" is a genuine, oracle-confirmed analysis; "xy" (the raw, un-inserted-into concatenation) must still be proposed but confirm to zero analyses, since epenthesis here is obligatory, never optional.
#[test]
fn epenthesis_over_propose_confirm_prune_matches_oracle_exactly() {
    let g = load(fixture_xml());
    let mut analyzer = FomaAnalyzer::new(&g).expect(
        "fixture must compile: one Suffix rule, one obligatory epenthesis rule, no templates",
    );
    let morpher = Morpher::new(&g, usize::MAX);

    let correct = assert_confirm_matches_oracle(&mut analyzer, &morpher, "xey", true);
    assert!(
        correct.candidates_generated > 0,
        "xey (root x + suffix y, epenthesis fired) must be proposed"
    );
    assert_eq!(
        correct.confirmed, 1,
        "xey must confirm to exactly one analysis"
    );

    let raw = assert_confirm_matches_oracle(&mut analyzer, &morpher, "xy", false);
    assert!(
        raw.candidates_generated > 0,
        "the FST proposer must still PROPOSE the raw, un-inserted-into 'xy' concatenation (no \
         phonology-aware filter exists at propose time -- confirm's own ana_epenthesis fold is the \
         only thing that prunes it) for confirm to have anything to prune"
    );
    assert_eq!(
        raw.confirmed, 0,
        "xy must confirm ZERO analyses: epenthesis between an ncX segment and an ncY segment is \
         obligatory, so the un-inserted-into spelling is never a valid surface"
    );

    // "x" alone must also round-trip, as a positive control that the root is independently well-formed.
    let bare = assert_confirm_matches_oracle(&mut analyzer, &morpher, "x", true);
    assert_eq!(
        bare.confirmed, 1,
        "bare root 'x' must confirm to exactly one analysis"
    );
}
