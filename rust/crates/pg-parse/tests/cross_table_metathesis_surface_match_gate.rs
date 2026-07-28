//! Regression gate for the "cross-table surface-match gate" defect (2026-07-27 follow-up to
//! `conformance-staging/edge-cases/multi-table-metathesis-shared-representation`'s own STAGING.md):
//! `pg_parse::Morpher::is_match_traced` renders a synthesized word's concrete char-def identities
//! via `pg_parse::surface::matching_reps_for_node` against the grammar's OUTERMOST stratum's table.
//! For an ordinary word that is exactly correct (a fully-synthesized word's own current stratum IS
//! the outermost one), but a `MetathesisRule`-relocated segment used to carry its ORIGIN table's raw
//! `char_def` index all the way there (`pg_rules::metathesis::synthesis_reorder` moved segments
//! without ever resetting `char_def`, unlike every rewrite-rule identity-changing path,
//! `syn_feature`/`sim_feature`, which reset a changed node's `char_def` to `NO_CHAR_DEF`) — an
//! apples-to-oranges raw-index collision once a WORD's own root was entered on a DIFFERENT
//! (inner) stratum's table than the metathesis rule's own (outer) stratum.
//!
//! Grammar shape mirrors the conformance fixture above (inline here, not read from that file, so
//! this gate is self-contained and cannot go stale if that fixture's own XML changes): two
//! `CharacterDefinitionTable`s at DELIBERATELY MISALIGNED raw indices for the same two spellings
//! ("m"/"x") -- `t0` ("Inner", `m`=raw 0, `x`=raw 1) and `t1` ("Outer", `z`=raw 0 [decoy], `m`=raw 1,
//! `x`=raw 2, `w`=raw 3 [decoy]). `ROOT1` (Inner stratum, table `t0`) is spelled "mx"; the obligatory
//! `MetathesisRule` lives on the Outer stratum (table `t1`) and swaps ROOT1's material to surface
//! "xm". `ROOT2` (Outer stratum, table `t1`, spelled "wx") is a same-table positive control using
//! `ncSwitchA`'s OTHER, table-`t1`-only member "w" -- proving ordinary same-table metathesis recall
//! is untouched by this fix. `ncSwitchA`/`ncSwitchB` are `FeatureNaturalClass`es (table-agnostic by
//! construction, `pg_rules::bridge::nat_class_lanes`'s `Feature` branch never reads `self.table`) so
//! this fixture isolates the ONE remaining table-dependent mechanism: the surface-match gate's own
//! raw `char_def` comparison, not natural-class resolution (already covered by
//! `pg-rules/src/cache.rs`'s `owning_table_tests`).

use pg_parse::Morpher;

const XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>CrossTableMetathesisSurfaceMatchProbe</Name>
    <PhonologicalFeatureSystem>
      <SymbolicFeature id="featId">
        <Name>id</Name>
        <Symbols>
          <Symbol id="symM">m</Symbol>
          <Symbol id="symX">x</Symbol>
          <Symbol id="symW">w</Symbol>
          <Symbol id="symZ">z</Symbol>
        </Symbols>
      </SymbolicFeature>
    </PhonologicalFeatureSystem>
    <CharacterDefinitionTable id="t0">
      <Name>Inner</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="c0m">
          <Representations><Representation>m</Representation></Representations>
          <FeatureValue feature="featId" symbolValues="symM" />
        </SegmentDefinition>
        <SegmentDefinition id="c0x">
          <Representations><Representation>x</Representation></Representations>
          <FeatureValue feature="featId" symbolValues="symX" />
        </SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <CharacterDefinitionTable id="t1">
      <Name>Outer</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="c1z">
          <Representations><Representation>z</Representation></Representations>
          <FeatureValue feature="featId" symbolValues="symZ" />
        </SegmentDefinition>
        <SegmentDefinition id="c1m">
          <Representations><Representation>m</Representation></Representations>
          <FeatureValue feature="featId" symbolValues="symM" />
        </SegmentDefinition>
        <SegmentDefinition id="c1x">
          <Representations><Representation>x</Representation></Representations>
          <FeatureValue feature="featId" symbolValues="symX" />
        </SegmentDefinition>
        <SegmentDefinition id="c1w">
          <Representations><Representation>w</Representation></Representations>
          <FeatureValue feature="featId" symbolValues="symW" />
        </SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <FeatureNaturalClass id="ncSwitchA"><Name>SwitchA</Name>
        <FeatureValue feature="featId" symbolValues="symM symW" />
      </FeatureNaturalClass>
      <FeatureNaturalClass id="ncSwitchB"><Name>SwitchB</Name>
        <FeatureValue feature="featId" symbolValues="symX" />
      </FeatureNaturalClass>
    </NaturalClasses>
    <PhonologicalRuleDefinitions>
      <MetathesisRule id="mrCrossTableSwap" leftSwitch="swB" rightSwitch="swA">
        <Name>crossTableSwap</Name>
        <StructuralDescription>
          <PhoneticTemplate>
            <PhoneticSequence>
              <SimpleContext id="swA" naturalClass="ncSwitchA" />
              <SimpleContext id="swB" naturalClass="ncSwitchB" />
            </PhoneticSequence>
          </PhoneticTemplate>
        </StructuralDescription>
      </MetathesisRule>
    </PhonologicalRuleDefinitions>
    <Strata>
      <Stratum characterDefinitionTable="t0" morphologicalRuleOrder="unordered">
        <Name>Inner</Name>
        <LexicalEntries>
          <LexicalEntry id="eRoot1">
            <Allomorphs><Allomorph id="aRoot1"><PhoneticShape>mx</PhoneticShape></Allomorph></Allomorphs>
            <MorphemeId>ROOT1</MorphemeId>
            <Gloss>root1</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" phonologicalRules="mrCrossTableSwap">
        <Name>Outer</Name>
        <LexicalEntries>
          <LexicalEntry id="eRoot2">
            <Allomorphs><Allomorph id="aRoot2"><PhoneticShape>wx</PhoneticShape></Allomorph></Allomorphs>
            <MorphemeId>ROOT2</MorphemeId>
            <Gloss>root2</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

fn load() -> pg_grammar::model::Grammar {
    pg_grammar::load(XML)
        .unwrap_or_else(|e| panic!("cross-table metathesis surface-match probe grammar loads: {e}"))
}

/// The direct inverse of the defect: a root entered on a DIFFERENT (inner) stratum's table than the
/// metathesis rule's own (outer) stratum must still analyze once correctly metathesized. Before this
/// fix, `synthesis_reorder` never reset the relocated segment's `char_def`, so it kept carrying
/// table `t0`'s raw index into `is_match_traced`'s comparison against table `t1` -- an
/// apples-to-oranges collision that rejected every genuinely correct candidate (empty signature "-",
/// not a graceful acceptance), reproduced by this test failing if the reset is ever removed again.
#[test]
fn cross_table_metathesized_root_matches_its_own_surface() {
    let g = load();
    assert_eq!(
        g.char_tables.len(),
        2,
        "fixture must declare exactly 2 tables"
    );
    assert_eq!(g.strata.len(), 2, "fixture must declare exactly 2 strata");

    let morpher = Morpher::new(&g, usize::MAX).with_memo(true);
    assert_eq!(
        morpher.parse_word("xm").signature(),
        "ROOT1|xm",
        "ROOT1 (Inner stratum, table t0), correctly metathesized to \"xm\" on the Outer stratum \
         (table t1), must analyze -- an empty result here means the metathesized segment's stale \
         origin-table char_def collided with table t1's own raw indices at the surface-match gate"
    );
}

/// ROOT1's own raw (un-metathesized) spelling must never be a valid surface form -- metathesis is
/// obligatory. Negative control: proves the fix does not make the gate vacuously permissive.
#[test]
fn cross_table_root_raw_spelling_still_rejected() {
    let g = load();
    let morpher = Morpher::new(&g, usize::MAX).with_memo(true);
    assert_eq!(
        morpher.parse_word("mx").signature(),
        "-",
        "ROOT1's raw, un-metathesized spelling must still find zero analyses"
    );
}

/// ROOT2 (same-table control, Outer stratum throughout, never crosses tables): correctly
/// metathesized "xw" must keep matching -- proves this fix does not regress ordinary same-table
/// metathesis recall.
#[test]
fn same_table_metathesis_recall_is_unaffected() {
    let g = load();
    let morpher = Morpher::new(&g, usize::MAX).with_memo(true);
    assert_eq!(morpher.parse_word("xw").signature(), "ROOT2|xw");
    assert_eq!(
        morpher.parse_word("wx").signature(),
        "-",
        "ROOT2's own raw spelling must still find zero analyses (metathesis is obligatory)"
    );
}
