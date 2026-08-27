//! Proposer-to-confirm containment for `MorphRuleDef::Compounding`'s non-recursive case: the license-gated head/non-head cross product `crate::emit::compound_license` proposes, checked against `pg_parse::Morpher` (the full-HC oracle) via `pg_foma::composite::FomaAnalyzer`. Synthetic, delanguaged fixture (invented CVCV/CVC roots).
//! See `docs/research/pg-foma-cover-compounding-fixture-notes.md` for the group-(un)awareness contract, the left-to-confirm syntactic-FS gate, and a pre-existing compound-loop surface-order finding this fixture pins.

mod common;

use std::collections::HashSet;

use pg_foma::capability::{compose_envelope, default_registry, CompileDecision};
use pg_foma::composite::FomaAnalyzer;
use pg_foma::enumerate::enumerate_default;
use pg_foma::junctions::PhonologyProbe;
use pg_grammar::model::{Grammar, PhonRuleDef};
use pg_parse::{Morpher, ParseOptions, WordAnalysis};

/// The synthetic fixture (module doc): `headA`/`headB`/`headC` isolate the three head-side MPR scenarios; `nonHeadOk`/`nonHeadBadPos` isolate the syntactic-FS-left-to-confirm scenario.
fn fixture_xml() -> &'static str {
    r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>CoverCompoundingFixture</Name>
    <PartsOfSpeech>
      <PartOfSpeech id="posHead"><Name>head</Name></PartOfSpeech>
      <PartOfSpeech id="posOther"><Name>other</Name></PartOfSpeech>
    </PartsOfSpeech>
    <MorphologicalPhonologicalRuleFeatures>
      <MorphologicalPhonologicalRuleFeature id="mpr1">M1</MorphologicalPhonologicalRuleFeature>
      <MorphologicalPhonologicalRuleFeature id="mpr2">M2</MorphologicalPhonologicalRuleFeature>
      <MorphologicalPhonologicalRuleFeature id="mpr3">M3</MorphologicalPhonologicalRuleFeature>
      <MorphologicalPhonologicalRuleFeature id="mpr4">M4</MorphologicalPhonologicalRuleFeature>
      <MorphologicalPhonologicalRuleFeatureGroup matchType="all" outputType="append" features="mpr1 mpr2"><Name>GRuleLevel</Name></MorphologicalPhonologicalRuleFeatureGroup>
      <MorphologicalPhonologicalRuleFeatureGroup matchType="all" outputType="append" features="mpr3 mpr4"><Name>GSubruleLevel</Name></MorphologicalPhonologicalRuleFeatureGroup>
    </MorphologicalPhonologicalRuleFeatures>
    <CharacterDefinitionTable id="table1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cb"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="ce"><Representations><Representation>e</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cf"><Representations><Representation>f</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="ci"><Representations><Representation>i</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="ck"><Representations><Representation>k</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cl"><Representations><Representation>l</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cm"><Representations><Representation>m</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cn"><Representations><Representation>n</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="co"><Representations><Representation>o</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cs"><Representations><Representation>s</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="ct"><Representations><Representation>t</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cu"><Representations><Representation>u</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cz"><Representations><Representation>z</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="table1" morphologicalRuleOrder="linear" morphologicalRules="cr1">
        <Name>Main</Name>
        <MorphologicalRuleDefinitions>
          <CompoundingRule id="cr1" headProdRestrictionsMprFeatures="mpr1 mpr2" nonHeadPartsOfSpeech="posHead">
            <Name>Compound</Name>
            <CompoundingSubrules>
              <CompoundingSubrule>
                <HeadMorphologicalInput requiredMPRFeatures="mpr3 mpr4">
                  <PhoneticSequence id="h0"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence>
                </HeadMorphologicalInput>
                <NonHeadMorphologicalInput>
                  <PhoneticSequence id="n0"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence>
                </NonHeadMorphologicalInput>
                <MorphologicalOutput>
                  <CopyFromInput index="h0" />
                  <CopyFromInput index="n0" />
                </MorphologicalOutput>
              </CompoundingSubrule>
            </CompoundingSubrules>
          </CompoundingRule>
        </MorphologicalRuleDefinitions>
        <LexicalEntries>
          <!-- headA: RULE-level trap witness - only mpr1 (of the {mpr1,mpr2} all-group), but
               BOTH mpr3+mpr4 (of the {mpr3,mpr4} all-group) - passes head_prod_restrictions_mpr
               via compound_match's flat overlap AND the subrule's required_mpr via mpr_group_ok. -->
          <LexicalEntry id="eHeadA" partOfSpeech="posHead" ruleFeatures="mpr1 mpr3 mpr4">
            <Allomorphs><Allomorph id="aHeadA"><PhoneticShape>fasu</PhoneticShape></Allomorph></Allomorphs>
            <MorphemeId>HEADA</MorphemeId>
          </LexicalEntry>
          <!-- headB: SUBRULE-level precision witness - mpr1 (rule-level, admitted) + mpr3 only
               (subrule-level, missing mpr4) - mpr_group_ok's all-type semantics must exclude it. -->
          <LexicalEntry id="eHeadB" partOfSpeech="posHead" ruleFeatures="mpr1 mpr3">
            <Allomorphs><Allomorph id="aHeadB"><PhoneticShape>tiku</PhoneticShape></Allomorph></Allomorphs>
            <MorphemeId>HEADB</MorphemeId>
          </LexicalEntry>
          <!-- headC: rule-level negative control - no mpr features at all, so
               head_prod_restrictions_mpr's compound_match (self non-empty, stem empty) rejects it. -->
          <LexicalEntry id="eHeadC" partOfSpeech="posHead">
            <Allomorphs><Allomorph id="aHeadC"><PhoneticShape>numo</PhoneticShape></Allomorph></Allomorphs>
            <MorphemeId>HEADC</MorphemeId>
          </LexicalEntry>
          <!-- nonHeadOk: posHead - unifies with cr1's own nonHeadPartsOfSpeech="posHead". -->
          <LexicalEntry id="eNonHeadOk" partOfSpeech="posHead">
            <Allomorphs><Allomorph id="aNonHeadOk"><PhoneticShape>bel</PhoneticShape></Allomorph></Allomorphs>
            <MorphemeId>NONHEADOK</MorphemeId>
          </LexicalEntry>
          <!-- nonHeadBadPos: posOther - MPR-licensed (non_head_prod_restrictions_mpr is empty/
               vacuous, so crate::emit::compound_license admits it), but disagrees with
               nonHeadPartsOfSpeech="posHead" at confirm - left to confirm, design.md D3. -->
          <LexicalEntry id="eNonHeadBadPos" partOfSpeech="posOther">
            <Allomorphs><Allomorph id="aNonHeadBadPos"><PhoneticShape>zon</PhoneticShape></Allomorph></Allomorphs>
            <MorphemeId>NONHEADBADPOS</MorphemeId>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>"#
}

fn load() -> Grammar {
    let xml = fixture_xml();
    pg_grammar::load(xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}\n{xml}"))
}

/// `(morpheme_ids, root_morpheme_index)` multiset key — same shape `tests/
/// cover_realizational_morphology_constraints.rs::analysis_set` uses.
fn analysis_set(v: &[WordAnalysis]) -> HashSet<(Vec<u32>, i32)> {
    v.iter()
        .map(|a| (a.morpheme_ids.clone(), a.root_morpheme_index))
        .collect()
}

/// Runs `word` through both the real propose->confirm composite and the full-HC oracle, and asserts exact structured-set equality between them (never mere containment).
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

/// This fixture's own `CompoundingRuleDef` must characterize `compounding.non-recursive` and compose to `ConfirmOnly`, proving the containment tests below exercise this construct's own resting disposition.
#[test]
fn fixture_is_non_recursive_and_confirm_only() {
    let g = load();
    let ro: Vec<&PhonRuleDef> = g
        .strata
        .iter()
        .flat_map(|s| &s.prules)
        .map(|&id| &g.prules[id.0 as usize])
        .collect();
    let phon = PhonologyProbe::new(&g);
    let plan = enumerate_default(&g, &ro, phon.as_ref());
    let registry = default_registry();

    assert_eq!(
        compose_envelope(&g, &plan, &registry),
        CompileDecision::ConfirmOnly,
        "a non-recursive Compounding fixture must compose to ConfirmOnly, never Refuse"
    );
}

/// The load-bearing group-(un)awareness trap witness (see `docs/research/pg-foma-cover-compounding-fixture-notes.md`): headA is admitted by the group-unaware `compound_match` but would be wrongly excluded by a group-aware reading of the same field.
#[test]
fn head_a_word_over_propose_confirm_prune() {
    let g = load();
    let mut analyzer = FomaAnalyzer::new(&g).expect(
        "fixture must compile: a single non-recursive CompoundingRule, no templates, no phonology",
    );
    let morpher = Morpher::new(&g, usize::MAX);

    // Positive: bel(nonhead, posHead) + fasu(headA) -- licensed on BOTH sides, syntactic FS agrees.
    let positive = assert_confirm_matches_oracle(&mut analyzer, &morpher, "fasubel", true);
    assert!(
        positive.candidates_generated > 0,
        "the FST proposer must PROPOSE fasubel (headA licensed via compound_match's flat overlap \
         on the partial {{mpr1}} match against the {{mpr1,mpr2}} all-group)"
    );
    assert_eq!(
        positive.confirmed, 1,
        "exactly one compound analysis expected for fasubel"
    );
}

/// Negative witness (left to confirm, deliberately): `zon` (posOther) is MPR-licensed as a non-head but disagrees with `cr1`'s `nonHeadPartsOfSpeech="posHead"`, so confirm's `is_unifiable` check prunes it to zero.
#[test]
fn head_a_plus_bad_pos_non_head_over_propose_confirm_prune() {
    let g = load();
    let mut analyzer = FomaAnalyzer::new(&g).expect("fixture must compile");
    let morpher = Morpher::new(&g, usize::MAX);

    let negative = assert_confirm_matches_oracle(&mut analyzer, &morpher, "fasuzon", false);
    assert_eq!(
        negative.confirmed, 0,
        "fasuzon must confirm zero analyses (nonHeadPartsOfSpeech mismatch)"
    );
    assert!(
        negative.candidates_generated > 0,
        "the FST proposer must still PROPOSE zon+headA (over-propose: compound_license never \
         checks non_head_required_syn_fs) for confirm's is_unifiable check to have anything to prune"
    );
}

/// Complementary precision check: `headB` carries only one of the two subrule-level `{mpr3,mpr4}` group members, and the group-aware `Grammar::mpr_group_ok` excludes it, matching confirm exactly (see `docs/research/pg-foma-cover-compounding-fixture-notes.md`).
#[test]
fn subrule_group_gate_excludes_partial_match_like_confirm() {
    let g = load();
    let mut analyzer = FomaAnalyzer::new(&g).expect("fixture must compile");
    let morpher = Morpher::new(&g, usize::MAX);

    let outcome = assert_confirm_matches_oracle(&mut analyzer, &morpher, "tikubel", false);
    assert_eq!(
        outcome.confirmed, 0,
        "tikubel must confirm zero analyses (subrule mpr_group_ok)"
    );
}

/// Sanity negative control: `headC` carries no MPR features at all, so `cr1`'s non-empty `headProdRestrictionsMprFeatures` fails `compound_match` outright, proving the rule-level gate genuinely restricts something.
#[test]
fn head_c_excluded_by_rule_level_gate_like_confirm() {
    let g = load();
    let mut analyzer = FomaAnalyzer::new(&g).expect("fixture must compile");
    let morpher = Morpher::new(&g, usize::MAX);

    let outcome = assert_confirm_matches_oracle(&mut analyzer, &morpher, "numobel", false);
    assert_eq!(
        outcome.confirmed, 0,
        "numobel must confirm zero analyses (headC has no MPR features at all)"
    );
}
