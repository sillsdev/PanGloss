//! `openspec/changes/cover-realizational-morphology-constraints`: proposer-to-confirm containment
//! for `MorphRuleDef::Realizational` (real_fs head-wrapped presence-blocking) plus three of the
//! constraint families ADR 0001 (`docs/adr/0001-honest-capability-boundary.md`) says stay
//! confirm-only-by-default: `<StemName>` region gating, `<Family>`/`Word::CheckBlocking`, and
//! `MorphemeCoOccurrenceRule` adjacency exclusion.
//!
//! ## Why these four, in one grammar
//! `pg_grammar::model`'s own doc calls `StemName`/`Family`/`RealizationalRule` "the realizational
//! cluster" (W5) — all three are ported together in `pg_rules::validity`/`pg_rules::morph`, and
//! `MorphemeCoOccurrenceRule` (W6) sits in the same `allomorphs_valid_impl` gate `StemName` does.
//! Exercising all four in one synthetic, delanguaged grammar (`openspec/changes/STAGING.md`'s
//! "Hard rule: synthetic data only" — invented CVC roots, no natural-language lexemes, named by
//! construct not language) is cheaper than four separate fixtures and, per this change's own
//! design.md, is exactly the "one grammar, several rows" shape `machine/conformance/languages/
//! fusional-realizational-morphology` (merged, `openspec/changes/cover-template-truncation-
//! reduplication`'s sibling lane) already established for the real conformance suite; this file's
//! grammar is smaller and single-owner (`pg-foma`'s own test tree, not the `machine/conformance`
//! submodule), built to isolate JUST these four constructs from that fixture's `Compounding`/
//! `MorphRuleOrder::Unordered` material (both still `Disposition::FailClosed` per `capability.rs`
//! — a NET-NEW, no-prior-owner Stage-2 lane per `openspec/changes/STAGING.md`, items 9-10 — so a
//! containment test that also depended on THEM would conflate two different constructs' proofs).
//! `morphologicalRuleOrder="linear"` throughout, deliberately, for the same reason: `Disposition::
//! Proven` today (`CharacteristicKind::OrderedMorphRuleApplication`), never `Unordered`'s `FailClosed`.
//!
//! ## The proposer-overapproximates / confirm-prunes property this file proves
//! Every construct below is `Disposition::ConfirmOnly` (`capability.rs`'s `RealizationalMorphology`/
//! `CoOccurrenceConstraint` — stem name and family/blocking have no `CharacteristicKind` of their
//! own; see this file's own "capability.rs disposition" note below for why). None of the four is a
//! *local* (single-morph-boundary) constraint the FST could safely admission-filter on without
//! risking a false negative: `RealizationalRule.IsBlocked` and `StemName`'s required/excluded-match
//! both depend on the word's ACCUMULATED feature structure built by every rule applied so far;
//! `Family`/`CheckBlocking` depends on a lexicon-wide search plus a forward resynthesis self-check;
//! `MorphemeCoOccurrenceRule`'s `adjacency="anywhere"` depends on which OTHER morphemes end up in
//! the SAME final derivation, an unbounded-window fact no per-transition FST filter can see. Per
//! ADR 0001 ("Confirm-only by default... the unsafe direction... is closed by default") the
//! faithful behavior is: the FST proposer emits the plain affix/realizational-rule candidate
//! regardless of these constraints (over-proposing), and `pg_foma::confirm`/`pg_rules::validity`
//! prune exactly the ones a real HermitCrab run would reject. Each `*_over_propose_confirm_prune`
//! test below proves BOTH halves for its construct: `candidates_generated > 0` (the FST really did
//! propose the shape) and `confirmed == 0` (confirm really did prune it) for the negative row, with
//! the positive row alongside it proving the SAME rule/allomorph machinery still recalls normally
//! when the constraint does not fire — using `pg_foma::composite::FomaAnalyzer` (propose UNION peel
//! → confirm, the real production pipeline) checked against `pg_parse::Morpher` (this codebase's
//! own full-HC oracle) for EXACT structured-set equality, never mere non-emptiness.
//!
//! ## `capability.rs` disposition note (deliverable 3)
//! `CharacteristicKind::RealizationalMorphology` and `::CoOccurrenceConstraint` are characterized
//! unconditionally (`ObservationDetail::None`, no per-configuration split) at `Disposition::
//! ConfirmOnly` — unlike `Reduplication`/`RightToLeftRewrite`/`Metathesis`/`MultiTable`/
//! `QuantifierPattern`, which are `Disposition::ConfigPredicate` because a REAL compiled/faithful
//! FST construction exists for SOME of their shapes (so a predicate is needed to discriminate
//! Admit-eligible from genuinely-`Refuse`-worthy configurations). No such compiled construction
//! exists, or is even conceivable, for realizational-feature/stem-name/family/co-occurrence
//! semantics (this file's own module doc above: all four need history/lexicon-wide state no
//! per-transition FST filter can see) — there is no shape of ANY of these four constructs for which
//! a proposer-side admission filter could ever be proven no-false-negative, so there is no
//! Admit-vs-Refuse split for a `CapabilityPredicate` to discriminate; every observed shape is
//! `ConfirmOnly`, unconditionally, by construction, not merely by omission. `default_disposition`
//! already reflects this (`Disposition::ConfirmOnly` needs no registered `CapabilityPredicate` —
//! only `FailClosed`/`ConfigPredicate` kinds do, `capability.rs`'s own `undischarged_kinds` doc) —
//! so the "upgrade" this change makes is not a new predicate but this file: oracle-backed positive
//! AND negative witnesses proving the `ConfirmOnly` claim is actually true (over-propose, never
//! under-propose) rather than an unproven assertion, for all four constructs at once. `StemName`/
//! `Family`/`Blocking` have no `CharacteristicKind` of their own because they are not `model.rs`
//! ENUM variants (design.md D1's own per-enum-family granularity) — `StemName`/`FamilyDef` are
//! plain structs, and `blockable`/`required_stem_name` are boolean/`Option` fields on the SAME
//! `MorphRuleDef` rule shapes `Affixation`/`RealizationalMorphology` already characterize; folding
//! them into a separate `CharacteristicKind` would double-count the same `ModelLocation::MorphRule`
//! occurrence design.md D1's table does not ask for. This file's tests are the missing proof that
//! these fields' `ConfirmOnly` handling is faithful wherever they DO occur, closing the "placeholder
//! disposition, never actually proven" gap the task names.

use std::collections::HashSet;

use pg_foma::composite::FomaAnalyzer;
use pg_grammar::model::Grammar;
use pg_parse::{Morpher, ParseOptions, WordAnalysis};

/// The synthetic, delanguaged fixture (invented CVC roots: `kib`, `zod`, `vem`, `fom`, `tay`/`toy`
/// — no natural-language lexemes, named by construct throughout). One `posV` part of speech, one
/// `linear`-order stratum, no `AffixTemplate` (none of these four constructs needs one — matching
/// `fusional-realizational-morphology`'s own "RealizationalRule: no AffixTemplate/Slot wrapper
/// needed at all" precedent), no `PhonologicalFeatureSystem`/`PhonologicalRuleDefinitions` (pure
/// affixation + one `RealizationalRule`, no phonological rewrite anywhere, so none is needed).
///
/// Rule/root pairing per construct (each isolated onto its OWN lexical entry so the four proofs
/// below never interact):
///  - **RealizationalRule presence-blocking** (`kib`): `mrTense` (ordinary, "+es", sets tense=pres)
///    then `rrPast` (`RealizationalRule`, "+id", `RealizationalFeatures` tense=past) — `rrPast`
///    alone succeeds (`kibid`); applied AFTER `mrTense` it is blocked (`kibesid`, tense already
///    present) — the exact `ferid`/`feresid` shape `fusional-realizational-morphology`'s own
///    words.yaml documents, reproduced here as an isolated, single-owner witness.
///  - **Family/blocking** (`zod`+`vem`, family `famZ`): `zod` is the regular member, `vem` is the
///    suppletive member with `AssignedHeadFeatures` tense=past fixed. `zod` alone and `zod`+`mrPl`
///    (num only, never touches tense) are unblocked; `zod`+`mrPast2` (tense=past) collides with
///    `vem`'s own fixed FS and is blocked (the `ducit`/`tul` shape).
///  - **StemName region gating** (`tay`/`toy`, one entry, two allomorphs): the default allomorph
///    `tay` carries no `stemName`; `toy` is restricted to `snPast` (region: tense=past). Bare `tay`
///    is valid (no region to satisfy OR fail); `tay`+`mrPast2` lands in `snPast`'s exclusive region
///    and is rejected (`ExcludedStemName`); `toy`+`mrPast2` matches `snPast` and succeeds; bare
///    `toy` fails (`RequiredStemName`, no tense assigned at all) — the `mun`/`man`/`min` shape.
///  - **MorphemeCoOccurrenceRule** (`fom`): `mrPl` and `mrPast2` alone are both fine; declaring
///    `mrPl` EXCLUDES `mrPast2` `adjacency="anywhere"` means using BOTH together (`fomuton`, `linear`
///    order forces `mrPast2` before `mrPl`) is rejected regardless of adjacency/order.
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

/// `(morpheme_ids, root_morpheme_index)` multiset key — `tests/f4_composite_gate.rs`'s own
/// `structured_multiset` shape, but as a `HashSet` (this file never needs multiplicity, only set
/// equality) so a diff prints legibly on failure.
fn analysis_set(v: &[WordAnalysis]) -> HashSet<(Vec<u32>, i32)> {
    v.iter()
        .map(|a| (a.morpheme_ids.clone(), a.root_morpheme_index))
        .collect()
}

/// Runs `word` through both the real propose→confirm composite and the full-HC oracle, and asserts
/// EXACT structured-set equality between them (never mere containment) — the "propose broadly,
/// confirm prunes to the exact HermitCrab set" invariant ADR 0001 states for every `ConfirmOnly`
/// construct, checked positively (both non-empty, same set) or negatively (both empty) depending on
/// `expect_nonempty`.
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

// =================================================================================================
// RealizationalRule presence-blocking (`kib`).
// =================================================================================================

#[test]
fn realizational_rule_presence_blocking_over_propose_confirm_prune() {
    let g = load();
    let mut analyzer = FomaAnalyzer::new(&g).expect("fixture must compile: no Compounding, no \
        Unordered stratum, plain affixation + one RealizationalRule");
    let morpher = Morpher::new(&g, usize::MAX);

    // Positive: rrPast alone, no prior tense value -> IsBlocked's presence check finds nothing to
    // collide with.
    let positive = assert_confirm_matches_oracle(&mut analyzer, &morpher, "kibid", true);
    assert!(
        positive.confirmed > 0,
        "precondition: kibid must actually confirm at least one analysis"
    );

    // Negative: mrTense (tense=pres) applied FIRST, then rrPast attempted -- tense is already
    // present (presence-based, not value-equality), so RealizationalRuleDef::real_fs's IsBlocked
    // check fires and the rule never applies. The FST proposer does NOT know about real_fs/word
    // history, so it still proposes this morpheme sequence (over-proposal) -- confirm is the one
    // that must prune it to zero.
    let negative = assert_confirm_matches_oracle(&mut analyzer, &morpher, "kibesid", false);
    assert_eq!(negative.confirmed, 0, "kibesid must confirm zero analyses (IsBlocked)");
    assert!(
        negative.candidates_generated > 0,
        "the FST proposer must still PROPOSE the kib+TENSE+RPAST candidate (over-propose) for \
         confirm's real_fs/IsBlocked check to have anything to prune -- candidates_generated=0 \
         would mean the proposer itself silently dropped this shape, not that confirm pruned it"
    );
}

// =================================================================================================
// Family / Word::CheckBlocking (`zod` + `vem`, family `famZ`).
// =================================================================================================

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

    // Negative: zod+PAST2 (tense=past) synthesizes a candidate whose accumulated FS collides with
    // vem's own lexically-fixed tense=past -- Word::CheckBlocking substitutes vem's own shape in,
    // which does not match the surface "zodut", so the candidate is discarded. The FST proposer
    // has no notion of Family/CheckBlocking, so it still proposes zod+PAST2 (over-propose);
    // confirm's validity/self-check pass is what prunes it.
    let negative = assert_confirm_matches_oracle(&mut analyzer, &morpher, "zodut", false);
    assert_eq!(negative.confirmed, 0, "zodut must confirm zero analyses (family blocking)");
    assert!(
        negative.candidates_generated > 0,
        "the FST proposer must still PROPOSE zod+PAST2 (over-propose) for confirm's family/\
         CheckBlocking pass to have anything to prune"
    );
}

// =================================================================================================
// StemName region gating (`tay` default / `toy` restricted to `snPast`).
// =================================================================================================

#[test]
fn stem_name_gating_over_propose_confirm_prune() {
    let g = load();
    let mut analyzer = FomaAnalyzer::new(&g).expect("fixture must compile");
    let morpher = Morpher::new(&g, usize::MAX);

    // Positive: bare "tay" (default, unrestricted allomorph) -- no tense assigned, so snPast's
    // region has nothing to exclude it over.
    assert_confirm_matches_oracle(&mut analyzer, &morpher, "tay", true);
    // Positive: "toy"+PAST2 -- toy is restricted to snPast (tense=past), and PAST2 assigns exactly
    // that, so the required-match holds.
    assert_confirm_matches_oracle(&mut analyzer, &morpher, "toyut", true);

    // Negative: bare "toy" -- stemName=snPast requires tense=past to already be assigned, but a
    // bare root has no rule-assigned features at all. RequiredStemName fails. The FST proposer
    // does not know about StemName gating at all, so it still proposes the bare "toy" root as a
    // candidate; confirm's stem_name_gates_ok check is what prunes it.
    let negative_bare = assert_confirm_matches_oracle(&mut analyzer, &morpher, "toy", false);
    assert_eq!(negative_bare.confirmed, 0, "bare toy must confirm zero analyses (RequiredStemName)");
    assert!(
        negative_bare.candidates_generated > 0,
        "the FST proposer must still PROPOSE bare toy (over-propose) for confirm's StemName gate \
         to have anything to prune"
    );

    // Negative: "tay"(default)+PAST2 -- the default allomorph is EXCLUDED once the word's fs lands
    // inside snPast's own region (a stemName-restricted sibling allomorph of the SAME entry exists
    // for that exact region), so this is rejected (ExcludedStemName) even though "tay" alone (no
    // tense) was fine.
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

// =================================================================================================
// MorphemeCoOccurrenceRule adjacency exclusion (`fom`).
// =================================================================================================

#[test]
fn morpheme_co_occurrence_exclude_anywhere_over_propose_confirm_prune() {
    let g = load();
    let mut analyzer = FomaAnalyzer::new(&g).expect("fixture must compile");
    let morpher = Morpher::new(&g, usize::MAX);

    // Positive: mrPast2 alone, and mrPl alone -- the exclude rule only fires when BOTH co-occur.
    assert_confirm_matches_oracle(&mut analyzer, &morpher, "fomut", true);
    assert_confirm_matches_oracle(&mut analyzer, &morpher, "fomon", true);

    // Negative: mrPast2 then mrPl together (linear order forces this relative order: mrPast2 is
    // listed before mrPl in the stratum's morphologicalRules) -- MorphemeCoOccurrenceRule excludes
    // mrPl co-occurring ANYWHERE with mrPast2 in the same derivation, regardless of the two rules'
    // relative application order or surface adjacency. The FST proposer has no notion of
    // cross-morpheme co-occurrence constraints, so it still proposes fom+PAST2+PL (over-propose);
    // confirm's `pg_rules::validity` co-occurrence check is what prunes it.
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
