//! `openspec/changes/cover-compounding`: proposer-to-confirm containment for
//! `MorphRuleDef::Compounding`'s non-recursive case — the license-gated head/non-head cross
//! product `crate::emit::compound_license` proposes (design.md D3's `Gate`/`Compose`/`Union` shape,
//! authored directly against this crate's lexc "bounded compound loop" ahead of
//! `reify-compilation-plans` wiring the emitters to a real `Plan`), checked against `pg_parse::
//! Morpher` (this codebase's own full-HC oracle) via `pg_foma::composite::FomaAnalyzer` (propose
//! UNION peel → confirm, the real production pipeline) — following `tests/
//! cover_realizational_morphology_constraints.rs`'s established methodology exactly.
//!
//! Synthetic, delanguaged fixture (`openspec/changes/STAGING.md`'s "Hard rule: synthetic data
//! only" — invented CVCV/CVC roots, no natural-language lexemes, named by construct). One
//! `CompoundingRuleDef` ("cr1"), one subrule, `morphologicalRuleOrder="linear"`,
//! `multipleApplication` at its DTD default (1), and no other `Compounding` rule anywhere in the
//! grammar — the exact shape `compounding_recursive` (`crate::capability`) characterizes
//! non-recursive, so this fixture's own `compose_envelope`/`evaluate_capability` verdict is
//! `ConfirmOnly`, never `Refuse` (proven directly below, `fixture_is_non_recursive_and_confirm_only`).
//!
//! ## The (un)group-awareness contract this fixture pins (design.md D4, tasks.md 3.3)
//! `cr1.headProdRestrictionsMprFeatures="mpr1 mpr2"` (RULE-level, tested with `MprSet::compound_match`
//! — group-UNAWARE) and `mpr1`/`mpr2` belong to an `all`-type `MprGroup`. `headA`'s own
//! `ruleFeatures="mpr1 mpr3 mpr4"` carries ONLY `mpr1` from that group — `compound_match` admits it
//! (flat overlap), but a group-aware `mpr_required_ok` reading of the SAME field would demand BOTH
//! `mpr1` AND `mpr2` present (the `all`-type semantics) and WOULD wrongly exclude it — the exact
//! "silently refusing stems `compound_match` would admit" bug design.md D4 names. `headA_word_over_propose_confirm_prune`
//! below is the load-bearing witness (tasks.md 3.3): headA must still be PROPOSED
//! (`candidates_generated > 0`) and CONFIRMED (`confirmed == oracle exact`), proving `crate::emit::
//! compound_license` uses `compound_match`, not the group-aware helper, for this field.
//!
//! The SUBRULE's own `requiredMPRFeatures="mpr3 mpr4"` (tested with the group-AWARE
//! `Grammar::mpr_group_ok`, per the SAME D4 contract, the opposite direction) belongs to a SECOND
//! `all`-type `MprGroup`. `headB` carries `mpr3` but NOT `mpr4` — `mpr_group_ok` correctly excludes
//! it (the `all`-type group demands both), matching confirm's own `synth_compound`/
//! `synth_compound_subrule` gate exactly (`subrule_group_gate_excludes_partial_match_like_confirm`,
//! below) — a complementary precision check (not itself the 3.3 witness, which is the RULE-level
//! `headA` case above) proving the subrule field is NOT loosened to the flat `compound_match` test.
//!
//! ## Left to confirm, deliberately (design.md D3)
//! `cr1.nonHeadPartsOfSpeech="posHead"` (a syntactic-FS gate) is never checked by `crate::emit::
//! compound_license` at all — `headA_plus_bad_pos_non_head_over_propose_confirm_prune` proves a
//! non-head candidate the coarse MPR gate licenses (`non_head_prod_restrictions_mpr` is empty/
//! vacuous) but whose OWN part of speech disagrees is still PROPOSED (over-approximation) and
//! PRUNED entirely by confirm's `is_unifiable` check — never silently dropped by propose, never
//! silently kept past confirm.
//!
//! ## A pre-existing (not this-change-introduced) compound-loop surface-order finding
//! `crate::emit`'s "bounded compound loop" (module doc, "Bounded compound loop" — predates this
//! change entirely) concatenates HEAD-root-text THEN non-head-root-text unconditionally (its own
//! physical lexc continuation order, `TLPost -> TLCmp -> TLCmpRoots`), regardless of a
//! `CompoundingSubruleDef`'s own `MorphologicalOutput` action order. This fixture's
//! `<MorphologicalOutput>` therefore copies `h0` THEN `n0` (head-first, matching `pg_grammar_gen::
//! build::compounding`'s own established convention) — an earlier draft used the non-head-first
//! order design.md's own worked examples show (`<CopyFromInput index="n0"/><CopyFromInput
//! index="h0"/>`) and found the FST proposer never proposes the corresponding "non-head+head"
//! spelling at all when the two differ (a genuine, pre-existing scope limitation of the compound
//! loop's over-approximation, newly SURFACED by this file's real oracle-containment run rather than
//! introduced by it — recorded here, not silently routed around).

mod common;

use std::collections::HashSet;

use pg_foma::capability::{
    compose_envelope, default_registry, CompileDecision,
};
use pg_foma::composite::FomaAnalyzer;
use pg_foma::enumerate::enumerate_default;
use pg_foma::junctions::PhonologyProbe;
use pg_foma::replace::SegAlphabet;
use pg_grammar::model::{Grammar, PhonRuleDef};
use pg_parse::{Morpher, ParseOptions, WordAnalysis};

/// The synthetic fixture (module doc). `headA`/`headB`/`headC` isolate the three head-side MPR
/// scenarios; `nonHeadOk`/`nonHeadBadPos` isolate the syntactic-FS-left-to-confirm scenario. No
/// phonological rules, no templates (compounding needs neither — matches every other compounding
/// fixture in this crate, e.g. `tests/phase_c_compounding.rs`'s own generator-built grammar).
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
          <!-- headA: RULE-level trap witness -- only mpr1 (of the {mpr1,mpr2} all-group), but
               BOTH mpr3+mpr4 (of the {mpr3,mpr4} all-group) -- passes head_prod_restrictions_mpr
               via compound_match's flat overlap AND the subrule's required_mpr via mpr_group_ok. -->
          <LexicalEntry id="eHeadA" partOfSpeech="posHead" ruleFeatures="mpr1 mpr3 mpr4">
            <Allomorphs><Allomorph id="aHeadA"><PhoneticShape>fasu</PhoneticShape></Allomorph></Allomorphs>
            <MorphemeId>HEADA</MorphemeId>
          </LexicalEntry>
          <!-- headB: SUBRULE-level precision witness -- mpr1 (rule-level, admitted) + mpr3 only
               (subrule-level, missing mpr4) -- mpr_group_ok's all-type semantics must exclude it. -->
          <LexicalEntry id="eHeadB" partOfSpeech="posHead" ruleFeatures="mpr1 mpr3">
            <Allomorphs><Allomorph id="aHeadB"><PhoneticShape>tiku</PhoneticShape></Allomorph></Allomorphs>
            <MorphemeId>HEADB</MorphemeId>
          </LexicalEntry>
          <!-- headC: rule-level negative control -- no mpr features at all, so
               head_prod_restrictions_mpr's compound_match (self non-empty, stem empty) rejects it. -->
          <LexicalEntry id="eHeadC" partOfSpeech="posHead">
            <Allomorphs><Allomorph id="aHeadC"><PhoneticShape>numo</PhoneticShape></Allomorph></Allomorphs>
            <MorphemeId>HEADC</MorphemeId>
          </LexicalEntry>
          <!-- nonHeadOk: posHead -- unifies with cr1's own nonHeadPartsOfSpeech="posHead". -->
          <LexicalEntry id="eNonHeadOk" partOfSpeech="posHead">
            <Allomorphs><Allomorph id="aNonHeadOk"><PhoneticShape>bel</PhoneticShape></Allomorph></Allomorphs>
            <MorphemeId>NONHEADOK</MorphemeId>
          </LexicalEntry>
          <!-- nonHeadBadPos: posOther -- MPR-licensed (non_head_prod_restrictions_mpr is empty/
               vacuous, so crate::emit::compound_license admits it), but disagrees with
               nonHeadPartsOfSpeech="posHead" at confirm -- left to confirm, design.md D3. -->
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

/// Runs `word` through both the real propose→confirm composite and the full-HC oracle, and asserts
/// EXACT structured-set equality between them (never mere containment) — same helper shape as
/// `cover_realizational_morphology_constraints.rs::assert_confirm_matches_oracle`.
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

/// Deliverable 3 / capability.rs judgment call check: this fixture's OWN `CompoundingRuleDef` must
/// characterize `compounding.non-recursive` and compose to `ConfirmOnly` — proving the containment
/// tests below exercise the promoted, non-`FailClosed` disposition this change ships, not an
/// accident of some other predicate meeting it down.
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
    let alphabet = SegAlphabet::new(&g.char_tables[0]);
    let plan = enumerate_default(&g, &alphabet, &ro, phon.as_ref());
    let registry = default_registry();

    assert_eq!(
        compose_envelope(&g, &plan, &registry),
        CompileDecision::ConfirmOnly,
        "a non-recursive Compounding fixture must compose to ConfirmOnly, never Refuse/FailClosed"
    );
}

/// **The load-bearing group-(un)awareness trap witness (tasks.md 3.3, design.md D4).** `headA`
/// carries only ONE of the two `{mpr1,mpr2}` `all`-group members `cr1.headProdRestrictionsMprFeatures`
/// names — admitted by the group-UNAWARE `compound_match` (correct), but would be WRONGLY EXCLUDED
/// by a group-aware `mpr_required_ok` reading of the same field (the exact recall-loss bug design.md
/// D4 names). Both halves of over-propose/confirm-prune are proven here too: the non-head-side
/// syntactic-FS gate (`nonHeadPartsOfSpeech`) is left entirely to confirm.
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
    assert_eq!(positive.confirmed, 1, "exactly one compound analysis expected for fasubel");
}

/// Negative witness (deliverable 3.2, design.md D3 "left to confirm"): `zon` (posOther) is
/// MPR-licensed as a non-head (`non_head_prod_restrictions_mpr` is empty/vacuous — `crate::emit::
/// compound_license` never checks syntactic FS at all) but disagrees with `cr1`'s own
/// `nonHeadPartsOfSpeech="posHead"` — confirm's `is_unifiable` check prunes it to zero, proving the
/// FST proposer over-generates past what a syntactic-FS gate would allow and confirm is what
/// narrows back to the oracle-exact (empty) set.
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

/// Complementary precision check (NOT the 3.3 witness — see `head_a_word_over_propose_confirm_prune`
/// for that): `headB` carries only ONE of the two `{mpr3,mpr4}` all-group members the SUBRULE's own
/// `requiredMPRFeatures` names. The group-AWARE `Grammar::mpr_group_ok` (correctly used for subrule
/// fields, design.md D4) excludes it — matching confirm's own `synth_compound`/
/// `synth_compound_subrule` gate exactly, so BOTH propose and confirm agree on zero for this word;
/// proves the subrule field is not loosened to the flat `compound_match` test (which WOULD have
/// admitted `headB`, since `{mpr3,mpr4}` overlaps `headB`'s own `{mpr1,mpr3}` on `mpr3`).
#[test]
fn subrule_group_gate_excludes_partial_match_like_confirm() {
    let g = load();
    let mut analyzer = FomaAnalyzer::new(&g).expect("fixture must compile");
    let morpher = Morpher::new(&g, usize::MAX);

    let outcome = assert_confirm_matches_oracle(&mut analyzer, &morpher, "tikubel", false);
    assert_eq!(outcome.confirmed, 0, "tikubel must confirm zero analyses (subrule mpr_group_ok)");
}

/// Sanity negative control: `headC` carries no MPR features at all, so `cr1`'s own
/// `headProdRestrictionsMprFeatures="mpr1 mpr2"` (non-empty) fails `compound_match` outright
/// (`self.overlaps(EMPTY) == false`) — proving the rule-level gate genuinely restricts something,
/// not a vacuous always-admit.
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
