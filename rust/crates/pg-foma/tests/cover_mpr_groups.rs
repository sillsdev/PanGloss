//! Proposer-to-confirm containment for `MprGroupOutput::Append`'s `mpr-group.append-output` configuration predicate (target: `ConfirmOnly` via a non-tracking baseline), plus the `mpr-group.overwrite-output` witness and the Append/Overwrite order-(in)dependence distinction.
//! See docs/research/pg-foma-cover-mpr-groups-notes.md for the non-tracking-baseline argument and the synthetic fixture's design.

mod common;

use std::collections::HashSet;

use pg_foma::capability::{compose_envelope, default_registry, CompileDecision};
use pg_foma::composite::FomaAnalyzer;
use pg_foma::enumerate::enumerate_default;
use pg_foma::junctions::PhonologyProbe;
use pg_foma::replace::SegAlphabet;
use pg_grammar::model::{Grammar, PhonRuleDef};
use pg_parse::{Morpher, ParseOptions, WordAnalysis};

/// `mrP`/`mrQ` isolate the `out_mpr`-accumulation-then-gate ordering witness; `eL`/`eM` isolate the `all`-type group-aware partial-match witness.
fn fixture_xml() -> &'static str {
    r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>CoverMprGroupsFixture</Name>
    <PartsOfSpeech>
      <PartOfSpeech id="posV"><Name>v</Name></PartOfSpeech>
    </PartsOfSpeech>
    <MorphologicalPhonologicalRuleFeatures>
      <MorphologicalPhonologicalRuleFeature id="mprX">X</MorphologicalPhonologicalRuleFeature>
      <MorphologicalPhonologicalRuleFeature id="mprY">Y</MorphologicalPhonologicalRuleFeature>
      <MorphologicalPhonologicalRuleFeatureGroup matchType="all" outputType="append" features="mprX mprY"><Name>GAppend</Name></MorphologicalPhonologicalRuleFeatureGroup>
    </MorphologicalPhonologicalRuleFeatures>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="ck"><Representations><Representation>k</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cl"><Representations><Representation>l</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cm"><Representations><Representation>m</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cp"><Representations><Representation>p</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cq"><Representations><Representation>q</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses><FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass></NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" morphologicalRules="mrP mrQ">
        <Name>Main</Name>
        <MorphologicalRuleDefinitions>
          <MorphologicalRule id="mrP" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
            <Name>p</Name>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subP">
                <MorphologicalInput><PhoneticSequence id="stemP"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                <MorphologicalOutput MPRFeatures="mprX mprY"><CopyFromInput index="stemP" /><InsertSegments><PhoneticShape>p</PhoneticShape></InsertSegments></MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
            <MorphemeId>P</MorphemeId>
          </MorphologicalRule>
          <MorphologicalRule id="mrQ" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
            <Name>q</Name>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subQ">
                <MorphologicalInput requiredMPRFeatures="mprX mprY"><PhoneticSequence id="stemQ"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                <MorphologicalOutput><CopyFromInput index="stemQ" /><InsertSegments><PhoneticShape>q</PhoneticShape></InsertSegments></MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
            <MorphemeId>Q</MorphemeId>
          </MorphologicalRule>
        </MorphologicalRuleDefinitions>
        <LexicalEntries>
          <!-- eK: the out_mpr-accumulation-then-gate ordering witness -- starts with an EMPTY MPR
               set, so mrQ (requires mprX+mprY) can only apply once mrP has already fired. -->
          <LexicalEntry id="eK" partOfSpeech="posV">
            <Allomorphs><Allomorph id="aK"><PhoneticShape>k</PhoneticShape></Allomorph></Allomorphs>
            <MorphemeId>K</MorphemeId>
          </LexicalEntry>
          <!-- eL: the all-type group-aware PARTIAL-match witness -- only mprX, missing mprY. -->
          <LexicalEntry id="eL" partOfSpeech="posV" ruleFeatures="mprX">
            <Allomorphs><Allomorph id="aL"><PhoneticShape>l</PhoneticShape></Allomorph></Allomorphs>
            <MorphemeId>L</MorphemeId>
          </LexicalEntry>
          <!-- eM: the all-type group-aware FULL-match positive control -- both mprX and mprY. -->
          <LexicalEntry id="eM" partOfSpeech="posV" ruleFeatures="mprX mprY">
            <Allomorphs><Allomorph id="aM"><PhoneticShape>m</PhoneticShape></Allomorph></Allomorphs>
            <MorphemeId>M</MorphemeId>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>"#
}

/// A separate, minimal `MprGroupOutput::Overwrite` grammar; no rule touches the group at all, since `characterize`'s per-group walk observes `MprGroupOverwrite` from the group's own declaration alone.
fn overwrite_group_fixture_xml() -> &'static str {
    r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>CoverMprGroupsOverwriteFixture</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>v</Name></PartOfSpeech></PartsOfSpeech>
    <MorphologicalPhonologicalRuleFeatures>
      <MorphologicalPhonologicalRuleFeature id="mprZ">Z</MorphologicalPhonologicalRuleFeature>
      <MorphologicalPhonologicalRuleFeatureGroup matchType="all" outputType="overwrite" features="mprZ"><Name>GOverwrite</Name></MorphologicalPhonologicalRuleFeatureGroup>
    </MorphologicalPhonologicalRuleFeatures>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cz"><Representations><Representation>z</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <Strata>
      <Stratum characterDefinitionTable="t1">
        <Name>Main</Name>
        <LexicalEntries>
          <LexicalEntry id="eZ" partOfSpeech="posV">
            <Allomorphs><Allomorph id="aZ"><PhoneticShape>z</PhoneticShape></Allomorph></Allomorphs>
            <MorphemeId>Z</MorphemeId>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>"#
}

fn load(xml: &str) -> Grammar {
    pg_grammar::load(xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}\n{xml}"))
}

/// `(morpheme_ids, root_morpheme_index)` multiset key -- same shape `tests/cover_compounding.rs::
/// analysis_set`/`tests/cover_unordered_morph_rules.rs::analysis_set` use.
fn analysis_set(v: &[WordAnalysis]) -> HashSet<(Vec<u32>, i32)> {
    v.iter()
        .map(|a| (a.morpheme_ids.clone(), a.root_morpheme_index))
        .collect()
}

/// Runs `word` through both the real propose-confirm composite and the full-HC oracle, and asserts exact structured-set equality (never mere containment).
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

/// This fixture's `Append`-output `MprGroup` must characterize as `mpr-group.append-output` and compose to `ConfirmOnly`, proving the containment tests below exercise its resting disposition, not an accident.
#[test]
fn fixture_is_append_only_and_confirm_only() {
    let g = load(fixture_xml());
    assert!(!g.mpr_groups.is_empty(), "fixture must declare an MprGroup");
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
        "an Append-only MprGroup fixture must compose to ConfirmOnly, never Refuse"
    );
}

/// The load-bearing containment witness: "kpq" (`mrP` accumulates both MPRs, then `mrQ`'s gate is satisfied) is oracle-confirmed; "kqp" (reverse order) is not, though propose still offers both unconditionally.
#[test]
fn out_mpr_accumulation_then_gate_over_propose_confirm_prune() {
    let g = load(fixture_xml());
    let mut analyzer = FomaAnalyzer::new(&g).expect(
        "fixture must compile: an Append-output MprGroup, an Unordered stratum, no phonology, no \
         templates",
    );
    let morpher = Morpher::new(&g, usize::MAX);

    let document_order = assert_confirm_matches_oracle(&mut analyzer, &morpher, "kpq", true);
    assert!(
        document_order.candidates_generated > 0,
        "kpq (mrP then mrQ) must be proposed"
    );
    assert_eq!(
        document_order.confirmed, 1,
        "kpq must confirm to exactly one analysis"
    );

    let reverse_order = assert_confirm_matches_oracle(&mut analyzer, &morpher, "kqp", false);
    assert!(
        reverse_order.candidates_generated > 0,
        "the FST proposer must still PROPOSE kqp (crate::emit::build_deriv_chain offers every rule \
         at every derivation-chain level, unconditional on any required_mpr/excluded_mpr/out_mpr \
         gate -- the non-tracking baseline this change's own predicate verifies) for confirm's \
         mpr_group_ok fold to have anything to prune"
    );
    assert_eq!(
        reverse_order.confirmed, 0,
        "kqp must confirm ZERO analyses: mrQ's requiredMPRFeatures=\"mprX mprY\" gate cannot be \
         satisfied when mrQ is the FIRST rule applied to eK's own empty MPR set"
    );
}

/// The group-aware `all`-type witness: `eL` carries only `mprX` of `{mprX,mprY}`, which a flat overlap test would wrongly admit but `Grammar::mpr_group_ok` correctly excludes; `eM` (both members) is the positive control.
#[test]
fn all_type_group_excludes_partial_match_like_confirm() {
    let g = load(fixture_xml());
    let mut analyzer = FomaAnalyzer::new(&g).expect("fixture must compile");
    let morpher = Morpher::new(&g, usize::MAX);

    let partial = assert_confirm_matches_oracle(&mut analyzer, &morpher, "lq", false);
    assert_eq!(
        partial.confirmed, 0,
        "lq must confirm zero analyses (eL is missing mprY)"
    );
    assert!(
        partial.candidates_generated > 0,
        "the FST proposer must still PROPOSE lq (no required_mpr check exists at propose time) for \
         confirm's group-aware gate to have anything to prune"
    );

    let full = assert_confirm_matches_oracle(&mut analyzer, &morpher, "mq", true);
    assert_eq!(
        full.confirmed, 1,
        "mq must confirm exactly one analysis (eM carries both members)"
    );
}

/// A pure model-level check (`mpr_add_output` directly, no grammar or FST): the same two-output multiset reaches an identical final state under `Append` (union is commutative) but a different one under `Overwrite`.
#[test]
fn append_output_is_order_invariant_overwrite_output_is_not() {
    use pg_grammar::model::{mpr_add_output, MprGroup, MprGroupMatchType, MprGroupOutput, MprSet};

    let x = MprSet(0b01);
    let y = MprSet(0b10);
    let base = MprSet::EMPTY;

    let append_groups = [MprGroup {
        name: None,
        match_type: MprGroupMatchType::All,
        output: MprGroupOutput::Append,
        members: x.union(y),
    }];
    let append_xy = mpr_add_output(&append_groups, mpr_add_output(&append_groups, base, x), y);
    let append_yx = mpr_add_output(&append_groups, mpr_add_output(&append_groups, base, y), x);
    assert_eq!(
        append_xy, append_yx,
        "Append accumulation must be order-invariant: X-then-Y and Y-then-X must reach the same \
         final MPR state"
    );
    assert_eq!(
        append_xy,
        x.union(y),
        "both members must be present after either order"
    );

    let overwrite_groups = [MprGroup {
        name: None,
        match_type: MprGroupMatchType::All,
        output: MprGroupOutput::Overwrite,
        members: x.union(y),
    }];
    let overwrite_xy = mpr_add_output(
        &overwrite_groups,
        mpr_add_output(&overwrite_groups, base, x),
        y,
    );
    let overwrite_yx = mpr_add_output(
        &overwrite_groups,
        mpr_add_output(&overwrite_groups, base, y),
        x,
    );
    assert_ne!(
        overwrite_xy, overwrite_yx,
        "Overwrite accumulation must NOT be order-invariant: the SAME rule multiset under two \
         admissible orderings must differ in final MPR state"
    );
    assert_eq!(
        overwrite_xy, y,
        "X-then-Y must retract X (the group's other member), leaving only Y"
    );
    assert_eq!(
        overwrite_yx, x,
        "Y-then-X must retract Y (the group's other member), leaving only X"
    );
}

/// `compose_envelope` (the check-only capability ledger, not yet wired into any production compile path) must report `ConfirmOnly` for a grammar declaring an `Overwrite`-output `MprGroup`.
#[test]
fn overwrite_group_composes_to_confirm_only() {
    let g = load(overwrite_group_fixture_xml());
    assert!(!g.mpr_groups.is_empty(), "fixture must declare an MprGroup");
    use pg_grammar::model::MprGroupOutput;
    assert_eq!(g.mpr_groups[0].output, MprGroupOutput::Overwrite);

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
        "Overwrite must use the non-tracking proposal superset and exact confirmation"
    );
}
