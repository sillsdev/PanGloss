//! Proposer-to-confirm containment for `MprGroupOutput::Append`'s `mpr-group.append-output`
//! configuration predicate (target: `ConfirmOnly` via a NON-TRACKING baseline), plus the
//! `mpr-group.overwrite-output` FailClosed witness and the Append/Overwrite
//! order-(in)dependence distinction.
//!
//! ## The non-tracking baseline this file proves, not merely asserts
//! Neither `crate::gate`'s static root-entry partition (keyed ONLY on `LexEntryDef::mpr`, never an
//! accumulated derivation-chain value) nor the ordinary morphological affix-allomorph emitter
//! (`crate::emit::build_deriv_chain`/`emit_rule_allomorphs`) ever reads `AffixAllomorphDef::
//! required_mpr`/`excluded_mpr`/`out_mpr` at all -- every allomorph is offered UNCONDITIONALLY at
//! every derivation-chain level, gated only by RHS emittability and `Role` classification. This
//! means propose was ALREADY at the safe `ConfirmOnly` baseline before this change touched a single
//! line of `pg-foma` production code: `MprGroupAppendNonNarrowingPredicate` (`crate::capability`)
//! documents and verifies this fact; it does not fix a narrowing bug, because there is none to fix.
//!
//! ## Synthetic, delanguaged fixture (invented CVC root/affixes, no natural-language lexemes,
//! named by construct)
//! One stratum, `morphologicalRuleOrder="unordered"` (needed so the cascade itself, not just the
//! MPR gate, admits BOTH orderings as legal candidates for confirm to weigh -- under `Linear`,
//! `Cascade::permutation`'s own non-decreasing-index restriction would already rule out the reverse
//! order for a reason that has NOTHING to do with MPR groups, confounding the witness; see
//! `tests/cover_unordered_morph_rules.rs`'s own module doc for the identical concern). One
//! `all`-type, `append`-output `MprGroup` over `{mprX, mprY}`. Two loose suffix rules:
//! - `mrP`'s own subrule declares NO MPR gate, and its `MorphologicalOutput` carries
//!   `MPRFeatures="mprX mprY"` (`out_mpr` -- sets BOTH group members at once, an `Append`
//!   accumulation).
//! - `mrQ`'s own subrule REQUIRES `mprX mprY` (the WHOLE `all`-type group) via
//!   `requiredMPRFeatures`.
//!   Root `eK` (`"k"`, `posV`) carries no `ruleFeatures` at all (starts with an EMPTY MPR set), so
//!   `mrQ` can only apply once `mrP` has already fired and added the group's members via `mpr_add_
//!   output` -- an order-DEPENDENT gate riding on top of an order-INVARIANT accumulation (see
//!   `append_output_is_order_invariant_overwrite_output_is_not` below for the accumulation half in
//!   isolation).
//!
//! Two more roots (`eL`/`eM`) isolate the group-AWARE `all`-type semantics directly, independent of
//! `out_mpr` timing: `eL` carries `ruleFeatures="mprX"` (PARTIAL group membership -- missing
//! `mprY`), `eM` carries `ruleFeatures="mprX mprY"` (FULL group membership). Applying `mrQ` directly
//! to each (no `mrP` involved at all) proves `Grammar::mpr_group_ok`'s `all`-type fold correctly
//! REJECTS the partial match (a flat, group-UNAWARE overlap test would have wrongly ADMITTED `eL`,
//! since `{mprX,mprY}` overlaps `{mprX}`) -- the group-(un)awareness contract, from the
//! ordinary-affix-rule side rather than the compounding side's own `compound_match`
//! (`tests/cover_compounding.rs::head_a_word_over_propose_confirm_prune` is the existing,
//! NOT-re-derived-here, group-UNAWARE-side witness for that other half; `MprSet::compound_match`
//! is categorically out of scope for `mpr-group.append-output`/`mpr-group.overwrite-output`).

mod common;

use std::collections::HashSet;

use pg_foma::capability::{compose_envelope, default_registry, CompileDecision};
use pg_foma::composite::FomaAnalyzer;
use pg_foma::enumerate::enumerate_default;
use pg_foma::junctions::PhonologyProbe;
use pg_foma::replace::SegAlphabet;
use pg_grammar::model::{Grammar, PhonRuleDef};
use pg_parse::{Morpher, ParseOptions, WordAnalysis};

/// The synthetic fixture (module doc). `mrP`/`mrQ` isolate the `out_mpr`-accumulation-then-gate
/// ordering witness; `eL`/`eM` isolate the `all`-type group-aware partial-match witness.
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

/// A separate, minimal `MprGroupOutput::Overwrite` grammar (deliverable 3's "Overwrite-group
/// grammar"). No `MorphologicalRule` touches the group at all -- `characterize`'s own per-group walk
/// (`crate::capability`) observes `MprGroupOverwrite` from the GROUP'S OWN DECLARATION alone, the
/// same granularity `crate::capability::tests::compose_envelope_confirm_only_for_append_group_alone`
/// already established for `Append` -- so this fixture needs no consuming rule to exercise the
/// predicate.
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

/// Runs `word` through both the real propose->confirm composite and the full-HC oracle, and asserts
/// EXACT structured-set equality between them (never mere containment) -- same helper shape as
/// `cover_compounding.rs`/`cover_unordered_morph_rules.rs`'s own `assert_confirm_matches_oracle`.
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

/// Deliverable 3 / capability.rs judgment call check: this fixture's OWN `Append`-output `MprGroup`
/// must characterize `mpr-group.append-output` and compose to `ConfirmOnly` -- proving the
/// containment tests below exercise the promoted disposition this change ships (well, restates: the
/// disposition was already `ConfirmOnly` before this change, see module doc -- this proves the
/// registered predicate reaches the SAME verdict), not an accident of some other predicate meeting
/// it down.
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
        "an Append-only MprGroup fixture must compose to ConfirmOnly, never Refuse/FailClosed"
    );
}

/// **The load-bearing containment witness.** `"kpq"` (`mrP` fires
/// first, its own `out_mpr` adding BOTH `mprX`/`mprY` via `Append` accumulation, THEN `mrQ`'s own
/// `requiredMPRFeatures="mprX mprY"` gate is satisfied) is a genuine, oracle-confirmed analysis.
/// `"kqp"` (the REVERSE order -- `mrQ` would have to fire FIRST, against `eK`'s still-EMPTY MPR set)
/// is NOT: the `all`-type group requires BOTH members present, and neither is yet. Confirm's own
/// `mpr_group_ok`/`mpr_add_output` fold (`pg_rules::morph.rs`) is what draws this distinction --
/// PROPOSE (the non-tracking baseline this change's predicate verifies, module doc) still offers
/// BOTH orderings unconditionally, over-proposing past what any accumulated-state-aware filter would
/// admit.
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

/// **The group-aware `all`-type witness (the group-(un)awareness contract, from the
/// ordinary-affix-rule side -- module doc).** `eL` carries only `mprX` of the `{mprX,mprY}`
/// `all`-type group `mrQ`'s own `requiredMPRFeatures` names -- a flat, group-UNAWARE overlap test
/// (`MprSet::compound_match`, categorically OUT OF SCOPE for this predicate) would have
/// wrongly ADMITTED it; the group-AWARE `Grammar::mpr_group_ok` correctly EXCLUDES it, matching
/// confirm exactly. `eM` (both members present) is the positive control proving the gate genuinely
/// discriminates, not a vacuous always-reject.
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

/// **The Append/Overwrite order-(in)dependence distinction.** A PURE model-level check
/// (`pg_grammar::model::mpr_add_output` directly, no XML grammar, no FST compile) rather than an
/// end-to-end fixture: an `Overwrite`-output `MprGroup` can never appear in a COMPILING grammar
/// under this crate's own capability regime (`overwrite_group_composes_to_refuse`, below), so the
/// only honest way to exercise it "inside the same fixture" (read here as "the same test file",
/// since no compiling grammar could ever host it) is to check the underlying algebra directly:
/// the SAME two-output multiset (`mprX` then `mprY`, or `mprY` then `mprX`) reaches the IDENTICAL
/// final accumulated state under `Append` (set union is commutative), but a DIFFERENT final state
/// under `Overwrite` (each new output retracts every OTHER member of its own group first) --
/// literally the property `mpr-group.append-output`'s `ConfirmOnly` promotion depends on and
/// `mpr-group.overwrite-output`'s permanent `FailClosed` refuses to assume.
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

/// **Deliverable 3's "Overwrite-group grammar stays FailClosed / overridable" -- the ledger half.**
/// `compose_envelope` (`crate::capability`, the CHECK-ONLY capability ledger -- that module's own
/// top doc: "does NOT wire a gate into any production compile path") must refuse this grammar,
/// naming `mpr-group.overwrite-output`. This mirrors `pg_foma::capability_entry`'s own
/// `evaluate_capability_refuses_recursive_compounding_grammar` precedent exactly: the FailClosed
/// witness for a Stage-2 construct checks the LEDGER verdict, not `FomaAnalyzer::new` (which this
/// crate does not yet wire to consult it for ANY construct -- `crate::capability`'s own top doc; the
/// production flip and the ADR 0005 override are later, not-yet-landed work this change does not
/// attempt).
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
