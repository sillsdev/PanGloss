//! Proposer-to-confirm containment for `MorphRuleDef::Realizational` (real_fs presence-blocking) plus three confirm-only-by-default constraint families: `<StemName>` region gating, `<Family>`/`Word::CheckBlocking`, and `MorphemeCoOccurrenceRule` adjacency exclusion -- none is a local constraint an FST admission filter could safely apply without risking a false negative, so each test proves the FST over-proposes and confirm prunes to the exact oracle set.

use std::collections::HashSet;

use pg_foma::composite::FomaAnalyzer;
use pg_grammar::model::Grammar;
use pg_parse::{Morpher, ParseOptions, WordAnalysis};

/// The synthetic, delanguaged fixture: one lexical entry per construct (`kib` realizational presence-blocking, `zod`+`vem` family blocking, `tay`/`toy` StemName gating, `fom` co-occurrence exclusion) so the four proofs below never interact.
fn fixture_xml() -> &'static str {
    r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>CoverRealizationalMorphologyConstraintsFixture</Name>
    <PartsOfSpeech>
      <PartOfSpeech id="posV"><Name>v</Name></PartOfSpeech>
    </PartsOfSpeech>
    <HeadFeatures>
      <SymbolicFeature id="featTense"><Name>tense</Name><Symbols><Symbol id="symPres">pres</Symbol><Symbol id="symPast">past</Symbol></Symbols></SymbolicFeature>
      <SymbolicFeature id="featNum"><Name>num</Name><Symbols><Symbol id="symSg">sg</Symbol><Symbol id="symPl">pl</Symbol></Symbols></SymbolicFeature>
    </HeadFeatures>
    <StemNames>
      <StemName id="snPast" partsOfSpeech="posV">
        <Name>PastStem</Name>
        <Regions>
          <Region><AssignedHeadFeatures><FeatureValue feature="featTense" symbolValues="symPast" /></AssignedHeadFeatures></Region>
        </Regions>
      </StemName>
    </StemNames>
    <CharacterDefinitionTable id="table1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cb"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cd"><Representations><Representation>d</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="ce"><Representations><Representation>e</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cf"><Representations><Representation>f</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="ci"><Representations><Representation>i</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="ck"><Representations><Representation>k</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cm"><Representations><Representation>m</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cn"><Representations><Representation>n</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="co"><Representations><Representation>o</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cs"><Representations><Representation>s</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="ct"><Representations><Representation>t</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cu"><Representations><Representation>u</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cv"><Representations><Representation>v</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cy"><Representations><Representation>y</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cz"><Representations><Representation>z</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass>
    </NaturalClasses>
    <Families>
      <Family id="famZ">FamZ</Family>
    </Families>
    <Strata>
      <Stratum characterDefinitionTable="table1" morphologicalRuleOrder="linear" morphologicalRules="mrTense rrPast mrPast2 mrPl">
        <Name>Main</Name>
        <MorphologicalRuleDefinitions>
          <MorphologicalRule id="mrTense" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
            <Name>tense</Name>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subTense">
                <MorphologicalInput><PhoneticSequence id="stemTense"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                <MorphologicalOutput><CopyFromInput index="stemTense" /><InsertSegments><PhoneticShape>es</PhoneticShape></InsertSegments></MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
            <OutputHeadFeatures><FeatureValue feature="featTense" symbolValues="symPres" /></OutputHeadFeatures>
            <MorphemeId>TENSE</MorphemeId>
          </MorphologicalRule>
          <RealizationalRule id="rrPast">
            <Name>realizPast</Name>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subRPast">
                <MorphologicalInput><PhoneticSequence id="stemRPast"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                <MorphologicalOutput><CopyFromInput index="stemRPast" /><InsertSegments><PhoneticShape>id</PhoneticShape></InsertSegments></MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
            <RealizationalFeatures><FeatureValue feature="featTense" symbolValues="symPast" /></RealizationalFeatures>
            <MorphemeId>RPAST</MorphemeId>
          </RealizationalRule>
          <MorphologicalRule id="mrPast2" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
            <Name>past2</Name>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subPast2">
                <MorphologicalInput><PhoneticSequence id="stemPast2"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                <MorphologicalOutput><CopyFromInput index="stemPast2" /><InsertSegments><PhoneticShape>ut</PhoneticShape></InsertSegments></MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
            <OutputHeadFeatures><FeatureValue feature="featTense" symbolValues="symPast" /></OutputHeadFeatures>
            <MorphemeId>PAST2</MorphemeId>
          </MorphologicalRule>
          <MorphologicalRule id="mrPl" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
            <Name>plural</Name>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subPl">
                <MorphologicalInput><PhoneticSequence id="stemPl"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                <MorphologicalOutput><CopyFromInput index="stemPl" /><InsertSegments><PhoneticShape>on</PhoneticShape></InsertSegments></MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
            <OutputHeadFeatures><FeatureValue feature="featNum" symbolValues="symPl" /></OutputHeadFeatures>
            <MorphemeId>PL</MorphemeId>
          </MorphologicalRule>
        </MorphologicalRuleDefinitions>
        <LexicalEntries>
          <LexicalEntry id="eKib" partOfSpeech="posV">
            <Allomorphs><Allomorph id="aKib"><PhoneticShape>kib</PhoneticShape></Allomorph></Allomorphs>
            <MorphemeId>KIB</MorphemeId>
          </LexicalEntry>
          <LexicalEntry id="eZod" partOfSpeech="posV" family="famZ">
            <Allomorphs><Allomorph id="aZod"><PhoneticShape>zod</PhoneticShape></Allomorph></Allomorphs>
            <MorphemeId>ZOD</MorphemeId>
          </LexicalEntry>
          <LexicalEntry id="eVem" partOfSpeech="posV" family="famZ">
            <Allomorphs><Allomorph id="aVem"><PhoneticShape>vem</PhoneticShape></Allomorph></Allomorphs>
            <AssignedHeadFeatures><FeatureValue feature="featTense" symbolValues="symPast" /></AssignedHeadFeatures>
            <MorphemeId>VEM</MorphemeId>
          </LexicalEntry>
          <LexicalEntry id="eFom" partOfSpeech="posV">
            <Allomorphs><Allomorph id="aFom"><PhoneticShape>fom</PhoneticShape></Allomorph></Allomorphs>
            <MorphemeId>FOM</MorphemeId>
          </LexicalEntry>
          <LexicalEntry id="eTay" partOfSpeech="posV">
            <Allomorphs>
              <Allomorph id="aTayDefault"><PhoneticShape>tay</PhoneticShape></Allomorph>
              <Allomorph id="aTayRestricted" stemName="snPast"><PhoneticShape>toy</PhoneticShape></Allomorph>
            </Allomorphs>
            <MorphemeId>TAY</MorphemeId>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
    <MorphemeCoOccurrenceRules>
      <MorphemeCoOccurrenceRule type="exclude" primaryMorpheme="mrPl" otherMorphemes="mrPast2" adjacency="anywhere" />
    </MorphemeCoOccurrenceRules>
  </Language>
</HermitCrabInput>"#
}

fn load() -> Grammar {
    let xml = fixture_xml();
    pg_grammar::load(xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}\n{xml}"))
}

/// `(morpheme_ids, root_morpheme_index)` set key, as a `HashSet` since this file never needs multiplicity, only set equality.
fn analysis_set(v: &[WordAnalysis]) -> HashSet<(Vec<u32>, i32)> {
    v.iter()
        .map(|a| (a.morpheme_ids.clone(), a.root_morpheme_index))
        .collect()
}

/// Runs `word` through both the real propose-confirm composite and the full-HC oracle, and asserts exact structured-set equality between them, never mere containment.
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

#[test]
fn realizational_rule_presence_blocking_over_propose_confirm_prune() {
    let g = load();
    let mut analyzer = FomaAnalyzer::new(&g).expect(
        "fixture must compile: no Compounding, no \
        Unordered stratum, plain affixation + one RealizationalRule",
    );
    let morpher = Morpher::new(&g, usize::MAX);

    // Positive: rrPast alone, no prior tense value, so IsBlocked's presence check has nothing to collide with.
    let positive = assert_confirm_matches_oracle(&mut analyzer, &morpher, "kibid", true);
    assert!(
        positive.confirmed > 0,
        "precondition: kibid must actually confirm at least one analysis"
    );

    // Negative: mrTense applied first makes tense already present, so IsBlocked fires; the FST still over-proposes this sequence, and confirm must prune it to zero.
    let negative = assert_confirm_matches_oracle(&mut analyzer, &morpher, "kibesid", false);
    assert_eq!(
        negative.confirmed, 0,
        "kibesid must confirm zero analyses (IsBlocked)"
    );
    assert!(
        negative.candidates_generated > 0,
        "the FST proposer must still PROPOSE the kib+TENSE+RPAST candidate (over-propose) for \
         confirm's real_fs/IsBlocked check to have anything to prune -- candidates_generated=0 \
         would mean the proposer itself silently dropped this shape, not that confirm pruned it"
    );

    let repeated = assert_confirm_matches_oracle(&mut analyzer, &morpher, "kibididididid", false);
    assert_eq!(repeated.confirmed, 0);
    assert!(
        repeated.candidates_generated > 0,
        "the regular FST loop must propose the five-repeat candidate so confirmation can reject it"
    );
}

#[test]
fn family_blocking_over_propose_confirm_prune() {
    let g = load();
    let mut analyzer = FomaAnalyzer::new(&g).expect("fixture must compile");
    let morpher = Morpher::new(&g, usize::MAX);

    // Positive: bare roots, direct lexical lookup, never reaches CheckBlocking at all.
    assert_confirm_matches_oracle(&mut analyzer, &morpher, "zod", true);
    assert_confirm_matches_oracle(&mut analyzer, &morpher, "vem", true);
    // Positive: zod+PL (num only) never collides with vem's tense-only fixed FS.
    assert_confirm_matches_oracle(&mut analyzer, &morpher, "zodon", true);

    // Negative: zod+PAST2 collides with vem's fixed tense=past via Word::CheckBlocking; the FST still over-proposes it, and confirm's validity/self-check pass prunes it.
    let negative = assert_confirm_matches_oracle(&mut analyzer, &morpher, "zodut", false);
    assert_eq!(
        negative.confirmed, 0,
        "zodut must confirm zero analyses (family blocking)"
    );
    assert!(
        negative.candidates_generated > 0,
        "the FST proposer must still PROPOSE zod+PAST2 (over-propose) for confirm's family/\
         CheckBlocking pass to have anything to prune"
    );
}

#[test]
fn stem_name_gating_over_propose_confirm_prune() {
    let g = load();
    let mut analyzer = FomaAnalyzer::new(&g).expect("fixture must compile");
    let morpher = Morpher::new(&g, usize::MAX);

    // Positive: bare "tay" has no tense assigned, so snPast's region has nothing to exclude it over.
    assert_confirm_matches_oracle(&mut analyzer, &morpher, "tay", true);
    // Positive: "toy"+PAST2 -- toy is restricted to snPast (tense=past), which PAST2 assigns, so the required-match holds.
    assert_confirm_matches_oracle(&mut analyzer, &morpher, "toyut", true);

    // Negative: bare "toy" needs tense=past already assigned (RequiredStemName), which a bare root never has; the FST still over-proposes it, and confirm's stem_name_gates_ok check prunes it.
    let negative_bare = assert_confirm_matches_oracle(&mut analyzer, &morpher, "toy", false);
    assert_eq!(
        negative_bare.confirmed, 0,
        "bare toy must confirm zero analyses (RequiredStemName)"
    );
    assert!(
        negative_bare.candidates_generated > 0,
        "the FST proposer must still PROPOSE bare toy (over-propose) for confirm's StemName gate \
         to have anything to prune"
    );

    // Negative: "tay"(default)+PAST2 lands inside snPast's own region, excluding the default allomorph (ExcludedStemName) even though bare "tay" was fine.
    let negative_excluded = assert_confirm_matches_oracle(&mut analyzer, &morpher, "tayut", false);
    assert_eq!(
        negative_excluded.confirmed, 0,
        "tay(default)+PAST2 must confirm zero analyses (ExcludedStemName)"
    );
    assert!(
        negative_excluded.candidates_generated > 0,
        "the FST proposer must still PROPOSE tay(default)+PAST2 (over-propose) for confirm's \
         StemName excluded-match check to have anything to prune"
    );
}

#[test]
fn morpheme_co_occurrence_exclude_anywhere_over_propose_confirm_prune() {
    let g = load();
    let mut analyzer = FomaAnalyzer::new(&g).expect("fixture must compile");
    let morpher = Morpher::new(&g, usize::MAX);

    // Positive: mrPast2 alone, and mrPl alone -- the exclude rule only fires when both co-occur.
    assert_confirm_matches_oracle(&mut analyzer, &morpher, "fomut", true);
    assert_confirm_matches_oracle(&mut analyzer, &morpher, "fomon", true);

    // Negative: mrPast2+mrPl co-occur anywhere in the derivation, so MorphemeCoOccurrenceRule excludes it regardless of order/adjacency; the FST still over-proposes it, and confirm's co-occurrence check prunes it.
    let negative = assert_confirm_matches_oracle(&mut analyzer, &morpher, "fomuton", false);
    assert_eq!(
        negative.confirmed, 0,
        "fom+PAST2+PL must confirm zero analyses (MorphemeCoOccurrenceRule exclude)"
    );
    assert!(
        negative.candidates_generated > 0,
        "the FST proposer must still PROPOSE fom+PAST2+PL (over-propose) for confirm's \
         co-occurrence check to have anything to prune"
    );
}
