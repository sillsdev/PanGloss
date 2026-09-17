//! Mbugwe-derived regressions for complex reduplication and five-rule structural fallback.

mod common;

use std::collections::HashSet;

use pg_foma::composite::FomaAnalyzer;
use pg_foma::emit;
use pg_grammar::model::Grammar;
use pg_parse::{GenMorpheme, Morpher, ParseOptions, WordAnalysis};

use common::gate_template::{entry_id_of, mrule_id_of};

fn load(xml: &str) -> Grammar {
    pg_grammar::load(xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}\n{xml}"))
}

fn analysis_set(v: &[WordAnalysis]) -> HashSet<(Vec<u32>, i32)> {
    v.iter()
        .map(|a| (a.morpheme_ids.clone(), a.root_morpheme_index))
        .collect()
}

/// The oracle requires Insert(ch) plus repeated and multi-segment copies to render `cheefu`.
const COMPLEX_REDUP_INSERT_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>ComplexRedupInsert</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="t1"><Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="ce"><Representations><Representation>e</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cf"><Representations><Representation>f</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cu"><Representations><Representation>u</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cc"><Representations><Representation>c</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="ch"><Representations><Representation>h</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cp"><Representations><Representation>p</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cx"><Representations><Representation>x</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cz"><Representations><Representation>z</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <SegmentNaturalClass id="ncE"><Name>E</Name><Segment segment="ce" /></SegmentNaturalClass>
      <SegmentNaturalClass id="ncF"><Name>F</Name><Segment segment="cf" /></SegmentNaturalClass>
      <SegmentNaturalClass id="ncU"><Name>U</Name><Segment segment="cu" /></SegmentNaturalClass>
      <SegmentNaturalClass id="ncP"><Name>P</Name><Segment segment="cp" /></SegmentNaturalClass>
      <SegmentNaturalClass id="ncZ"><Name>Z</Name><Segment segment="cz" /></SegmentNaturalClass>
      <FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="linear" morphologicalRules="mrMixed">
        <Name>Main</Name>
        <MorphologicalRuleDefinitions>
          <MorphologicalRule id="mrMixed">
            <Name>mixedRedup</Name>
            <MorphologicalSubrules>
              <!-- Allomorph zero keeps the rule in the broad Prefix candidate set. -->
              <MorphologicalSubrule id="subPrefix">
                <MorphologicalInput><PhoneticSequence id="qP"><SimpleContext naturalClass="ncP" /></PhoneticSequence></MorphologicalInput>
                <MorphologicalOutput><InsertSegments><PhoneticShape>x</PhoneticShape></InsertSegments><CopyFromInput index="qP" /></MorphologicalOutput>
              </MorphologicalSubrule>
              <!-- The target allomorph has Insert(ch), Copy(qA), Copy(qA), Copy(qB). -->
              <MorphologicalSubrule id="subComplexRedup">
                <MorphologicalInput>
                  <PhoneticSequence id="qA"><SimpleContext naturalClass="ncE" /></PhoneticSequence>
                  <PhoneticSequence id="qB"><SimpleContext naturalClass="ncF" /><SimpleContext naturalClass="ncU" /></PhoneticSequence>
                </MorphologicalInput>
                <MorphologicalOutput redupMorphType="suffix">
                  <InsertSegments><PhoneticShape>ch</PhoneticShape></InsertSegments>
                  <CopyFromInput index="qA" />
                  <CopyFromInput index="qA" />
                  <CopyFromInput index="qB" />
                </MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
            <MorphemeId>MIXED</MorphemeId>
          </MorphologicalRule>
        </MorphologicalRuleDefinitions>
        <LexicalEntries>
          <LexicalEntry id="eRoot"><Allomorphs><Allomorph id="aRoot"><PhoneticShape>efu</PhoneticShape></Allomorph></Allomorphs><MorphemeId>ROOT</MorphemeId></LexicalEntry>
          <LexicalEntry id="pRoot"><Allomorphs><Allomorph id="pAllo"><PhoneticShape>p</PhoneticShape></Allomorph></Allomorphs><MorphemeId>PROOT</MorphemeId></LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>"#;

#[test]
fn inserted_complex_reduplication_uses_the_structural_route() {
    let g = load(COMPLEX_REDUP_INSERT_XML);
    let mid = mrule_id_of(&g, "mrMixed");
    let diag = emit::composite_candidate_rules(&g);
    assert!(
        diag.structural_candidates.contains(&mid.0),
        "complex reduplication must be admitted to the structural candidate set: {:?}",
        diag.structural_candidates
    );

    let emitted = emit::emit(&g);
    assert!(
        emitted.report.uncovered.is_empty(),
        "structural ownership must remove the stale complex-redup uncovered item: {:?}",
        emitted.report.uncovered
    );

    let root = entry_id_of(&g, "eRoot");
    let morpher = Morpher::new(&g, 20_000);
    let words = morpher.generate_words(
        root,
        &[GenMorpheme::Rule(mid)],
        pg_featstruct::FeatureStruct::EMPTY,
    );
    assert_eq!(words, vec!["cheefu".to_string()]);

    let oracle = morpher.parse_word_opts("cheefu", &ParseOptions::default());
    let mut analyzer = FomaAnalyzer::new(&g).expect("complex redup fixture must compile");
    let outcome = analyzer.analyze_word("cheefu");
    assert_eq!(outcome.confirmed, oracle.structured.len());
    assert_eq!(
        analysis_set(&outcome.structured),
        analysis_set(&oracle.structured)
    );
}
/// Empty-LHS broadening requires an exact five-rule prefix/circumfix analysis and tag order.
const FIVE_RULE_CHAIN_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>FiveRuleStructuralChain</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <PhonologicalFeatureSystem>
      <SymbolicFeature id="featId"><Name>id</Name><Symbols>
        <Symbol id="symA">a</Symbol><Symbol id="symB">b</Symbol><Symbol id="symC">c</Symbol>
        <Symbol id="symD">d</Symbol><Symbol id="symE">e</Symbol><Symbol id="symF">f</Symbol>
        <Symbol id="symG">g</Symbol><Symbol id="symZ">z</Symbol>
      </Symbols></SymbolicFeature>
    </PhonologicalFeatureSystem>
    <CharacterDefinitionTable id="t1"><Name>Main</Name>
      <SegmentDefinitions>        <SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations><FeatureValue feature="featId" symbolValues="symA" /></SegmentDefinition>
        <SegmentDefinition id="cb"><Representations><Representation>b</Representation></Representations><FeatureValue feature="featId" symbolValues="symB" /></SegmentDefinition>
        <SegmentDefinition id="cc"><Representations><Representation>c</Representation></Representations><FeatureValue feature="featId" symbolValues="symC" /></SegmentDefinition>
        <SegmentDefinition id="cd"><Representations><Representation>d</Representation></Representations><FeatureValue feature="featId" symbolValues="symD" /></SegmentDefinition>
        <SegmentDefinition id="ce"><Representations><Representation>e</Representation></Representations><FeatureValue feature="featId" symbolValues="symE" /></SegmentDefinition>
        <SegmentDefinition id="cf"><Representations><Representation>f</Representation></Representations><FeatureValue feature="featId" symbolValues="symF" /></SegmentDefinition>
        <SegmentDefinition id="cg"><Representations><Representation>g</Representation></Representations><FeatureValue feature="featId" symbolValues="symG" /></SegmentDefinition>
        <SegmentDefinition id="cz"><Representations><Representation>z</Representation></Representations><FeatureValue feature="featId" symbolValues="symZ" /></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <SegmentNaturalClass id="ncZ"><Name>Z</Name><Segment segment="cz" /></SegmentNaturalClass>
      <FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass>
    </NaturalClasses>
    <PhonologicalRuleDefinitions>
      <PhonologicalRule id="prImpossibleEpenthesis">
        <Name>impossibleEpenthesis</Name>
        <PhoneticInput><PhoneticSequence /></PhoneticInput>
        <PhonologicalSubrules><PhonologicalSubrule>
          <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncZ" /></PhoneticSequence></PhoneticOutput>
          <Environment>
            <LeftEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncZ" /></PhoneticSequence></PhoneticTemplate></LeftEnvironment>
            <RightEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncZ" /></PhoneticSequence></PhoneticTemplate></RightEnvironment>
          </Environment>
        </PhonologicalSubrule></PhonologicalSubrules>
      </PhonologicalRule>
    </PhonologicalRuleDefinitions>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" morphologicalRules="mr1 mr2 mr3 mr4 mr5" phonologicalRules="prImpossibleEpenthesis">
        <Name>Main</Name>
        <MorphologicalRuleDefinitions>
          <MorphologicalRule id="mr1" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV"><Name>one</Name><MorphologicalSubrules><MorphologicalSubrule id="s1"><MorphologicalInput><PhoneticSequence id="stem1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput><MorphologicalOutput><InsertSegments><PhoneticShape>b</PhoneticShape></InsertSegments><CopyFromInput index="stem1" /></MorphologicalOutput></MorphologicalSubrule></MorphologicalSubrules><MorphemeId>R1</MorphemeId></MorphologicalRule>
          <MorphologicalRule id="mr2" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV"><Name>two</Name><MorphologicalSubrules><MorphologicalSubrule id="s2"><MorphologicalInput><PhoneticSequence id="stem2"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput><MorphologicalOutput><InsertSegments><PhoneticShape>c</PhoneticShape></InsertSegments><CopyFromInput index="stem2" /></MorphologicalOutput></MorphologicalSubrule></MorphologicalSubrules><MorphemeId>R2</MorphemeId></MorphologicalRule>
          <MorphologicalRule id="mr3" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV"><Name>three</Name><MorphologicalSubrules><MorphologicalSubrule id="s3"><MorphologicalInput><PhoneticSequence id="stem3"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput><MorphologicalOutput><InsertSegments><PhoneticShape>d</PhoneticShape></InsertSegments><CopyFromInput index="stem3" /></MorphologicalOutput></MorphologicalSubrule></MorphologicalSubrules><MorphemeId>R3</MorphemeId></MorphologicalRule>
          <MorphologicalRule id="mr4" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV"><Name>four</Name><MorphologicalSubrules><MorphologicalSubrule id="s4"><MorphologicalInput><PhoneticSequence id="stem4"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput><MorphologicalOutput><InsertSegments><PhoneticShape>e</PhoneticShape></InsertSegments><CopyFromInput index="stem4" /></MorphologicalOutput></MorphologicalSubrule></MorphologicalSubrules><MorphemeId>R4</MorphemeId></MorphologicalRule>
          <MorphologicalRule id="mr5" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV"><Name>five</Name><MorphologicalSubrules><MorphologicalSubrule id="s5"><MorphologicalInput><PhoneticSequence id="stem5"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput><MorphologicalOutput><InsertSegments><PhoneticShape>f</PhoneticShape></InsertSegments><CopyFromInput index="stem5" /><InsertSegments><PhoneticShape>g</PhoneticShape></InsertSegments></MorphologicalOutput></MorphologicalSubrule></MorphologicalSubrules><MorphemeId>R5</MorphemeId></MorphologicalRule>
        </MorphologicalRuleDefinitions>
        <LexicalEntries><LexicalEntry id="eRoot" partOfSpeech="posV"><Allomorphs><Allomorph id="aRoot"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs><MorphemeId>ROOT</MorphemeId></LexicalEntry></LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>"#;

#[test]
fn empty_lhs_broadening_contains_exact_five_rule_structural_chain() {
    let g = load(FIVE_RULE_CHAIN_XML);
    let mids: Vec<_> = ["mr1", "mr2", "mr3", "mr4", "mr5"]
        .iter()
        .map(|id| mrule_id_of(&g, id))
        .collect();
    let diag = emit::composite_candidate_rules(&g);
    assert!(
        mids.iter()
            .all(|mid| diag.structural_candidates.contains(&mid.0)),
        "empty-LHS broadening must admit every rule in the five-rule chain: {:?}",
        diag.structural_candidates
    );

    let root = entry_id_of(&g, "eRoot");
    let morpher = Morpher::new(&g, usize::MAX);
    let rules = mids
        .iter()
        .rev()
        .copied()
        .map(GenMorpheme::Rule)
        .collect::<Vec<_>>();
    let words = morpher.generate_words(root, &rules, pg_featstruct::FeatureStruct::EMPTY);
    assert_eq!(words, vec!["fedcbag".to_string()]);

    let oracle = morpher.parse_word_opts("fedcbag", &ParseOptions::default());

    let mut analyzer = FomaAnalyzer::new(&g).expect("five-rule fixture must compile");
    let outcome = analyzer.analyze_word("fedcbag");
    assert!(outcome.candidates_generated > 0);
    assert_eq!(outcome.confirmed, oracle.structured.len());
    assert_eq!(
        analysis_set(&outcome.structured),
        analysis_set(&oracle.structured)
    );
}
