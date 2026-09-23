//! Synthetic, delanguaged fixtures only (this repo's own conformance-grammar convention),
//! hand-authored XML duplicated per test module rather than shared across files — the same
//! convention `enumerate.rs`/`capability.rs`/`oracle.rs`'s own test modules already hold
//! themselves to.

use super::*;

fn load(xml: &str) -> Grammar {
    pg_grammar::load(xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}\n{xml}"))
}

/// An MPR-gated 2-group grammar with a real phonological rule, so the plan root is `Union[Gate, Leaf/CompositeEmissionMarker]`, exercising 6 of the 7 legal tuples.
fn gated_two_group_with_rule_fixture_xml() -> &'static str {
    r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
<Name>PlanInteractionGatedTwoGroupFixture</Name>
<PartsOfSpeech>
  <PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech>
</PartsOfSpeech>
<MorphologicalPhonologicalRuleFeatures>
  <MorphologicalPhonologicalRuleFeature id="mpr1">f1</MorphologicalPhonologicalRuleFeature>
</MorphologicalPhonologicalRuleFeatures>
<CharacterDefinitionTable id="t1">
  <Name>Main</Name>
  <SegmentDefinitions>
    <SegmentDefinition id="c1"><Representations><Representation>p</Representation></Representations></SegmentDefinition>
    <SegmentDefinition id="c2"><Representations><Representation>q</Representation></Representations></SegmentDefinition>
  </SegmentDefinitions>
</CharacterDefinitionTable>
<PhonologicalRuleDefinitions>
  <PhonologicalRule id="prule1">
    <Name>gate1</Name>
    <PhoneticInput><PhoneticSequence><Segment segment="c1" /></PhoneticSequence></PhoneticInput>
    <PhonologicalSubrules>
      <PhonologicalSubrule requiredMPRFeatures="mpr1">
        <PhoneticOutput><PhoneticSequence><Segment segment="c2" /></PhoneticSequence></PhoneticOutput>
      </PhonologicalSubrule>
    </PhonologicalSubrules>
  </PhonologicalRule>
</PhonologicalRuleDefinitions>
<Strata>
  <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" phonologicalRules="prule1">
    <Name>S</Name>
    <LexicalEntries>
      <LexicalEntry id="e0" partOfSpeech="posV">
        <Allomorphs><Allomorph id="allo0"><PhoneticShape>p</PhoneticShape></Allomorph></Allomorphs>
        <Gloss>e0</Gloss>
      </LexicalEntry>
      <LexicalEntry id="e1" partOfSpeech="posV" ruleFeatures="mpr1">
        <Allomorphs><Allomorph id="allo1"><PhoneticShape>p</PhoneticShape></Allomorph></Allomorphs>
        <Gloss>e1</Gloss>
      </LexicalEntry>
    </LexicalEntries>
  </Stratum>
</Strata>
  </Language>
</HermitCrabInput>
"#
}

/// A minimal `MprGroupOutput::Overwrite` grammar, ungated (no phon rules at all), so its plan root collapses directly to a single-group `Gate` node.
fn overwrite_group_fixture_xml() -> &'static str {
    r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
<Name>PlanInteractionOverwriteFixture</Name>
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

// legal_adjacency_tuples

#[test]
fn legal_adjacency_tuples_has_exactly_seven_documented_shapes() {
    let legal = legal_adjacency_tuples();
    assert_eq!(legal.len(), 7, "legal set: {legal:?}");
    assert!(legal.contains(&AdjacencyTuple {
        parent_kind: "Gate",
        child_kind: "Compose",
        child_detail: None,
        compose_strategy: Some("Static"),
    }));
    assert!(legal.contains(&AdjacencyTuple {
        parent_kind: "Replace",
        child_kind: "Leaf",
        child_detail: Some("RewriteRule"),
        compose_strategy: None,
    }));
}

// observed_adjacency_tuples

#[test]
fn observed_adjacency_tuples_on_gated_two_group_fixture_matches_six_of_seven_legal_shapes() {
    let g = load(gated_two_group_with_rule_fixture_xml());
    let (plan, _profile) = plan_and_profile(&g);
    let observed = observed_adjacency_tuples(&plan);

    for tuple in legal_adjacency_tuples() {
        let is_structural_marker = tuple.child_detail == Some("StructuralCompositeMarker");
        assert_eq!(
            observed.contains(&tuple),
            !is_structural_marker,
            "tuple {tuple:?}: expected present={} on this fixture (no circumfix/dropped-material \
             construct declared), observed={}",
            !is_structural_marker,
            observed.contains(&tuple)
        );
    }
}

#[test]
fn observed_adjacency_tuples_on_overwrite_fixture_has_no_replace_rewrite_rule_leaf() {
    let g = load(overwrite_group_fixture_xml());
    let (plan, _profile) = plan_and_profile(&g);
    let observed = observed_adjacency_tuples(&plan);
    assert!(
        !observed.contains(&AdjacencyTuple {
            parent_kind: "Replace",
            child_kind: "Leaf",
            child_detail: Some("RewriteRule"),
            compose_strategy: None,
        }),
        "fixture declares zero phonological rules -- no RewriteRule leaf can exist: {observed:?}"
    );
    assert!(
        observed.contains(&AdjacencyTuple {
            parent_kind: "Gate",
            child_kind: "Compose",
            child_detail: None,
            compose_strategy: Some("Static"),
        }),
        "the ungated single group must still realize a Gate -> Compose edge: {observed:?}"
    );
}

// compute_interaction_coverage: required/covered/uncovered

#[test]
fn compute_interaction_coverage_reports_seven_required_tuples_and_no_unexpected_ones() {
    let g = load(gated_two_group_with_rule_fixture_xml());
    let (plan, profile) = plan_and_profile(&g);
    let report = compute_interaction_coverage(&[("fixture-a", &plan, &profile)]);

    assert_eq!(report.required.len(), 7);
    assert!(
        report.unexpected_tuples.is_empty(),
        "no tuple outside the documented legal set should ever be observed: {:?}",
        report.unexpected_tuples
    );
    assert_eq!(report.retired.len(), 2);
}

#[test]
fn compute_interaction_coverage_marks_the_structural_marker_tuple_uncovered_when_absent() {
    let g = load(gated_two_group_with_rule_fixture_xml());
    let (plan, profile) = plan_and_profile(&g);
    let report = compute_interaction_coverage(&[("fixture-a", &plan, &profile)]);

    let structural_row = report
        .required
        .iter()
        .find(|r| r.tuple.child_detail == Some("StructuralCompositeMarker"))
        .expect("the StructuralCompositeMarker tuple must be in the required set");
    assert_eq!(structural_row.status, TupleStatus::Uncovered);
    assert!(structural_row.covering_fixtures.is_empty());

    let uncovered = report.uncovered();
    assert!(uncovered.iter().any(|r| r.tuple == structural_row.tuple));
}

#[test]
fn compute_interaction_coverage_covers_a_tuple_once_any_supplied_fixture_exhibits_it() {
    let g = load(gated_two_group_with_rule_fixture_xml());
    let (plan, profile) = plan_and_profile(&g);
    let report = compute_interaction_coverage(&[("only-fixture", &plan, &profile)]);

    let gate_compose_row = report
        .required
        .iter()
        .find(|r| r.tuple.parent_kind == "Gate" && r.tuple.child_kind == "Compose")
        .expect("Gate -> Compose must be in the required set");
    assert_eq!(gate_compose_row.status, TupleStatus::Covered);
    assert_eq!(
        gate_compose_row.covering_fixtures,
        vec!["only-fixture".to_string()]
    );
}

#[test]
fn compute_interaction_coverage_covers_confirm_only_overwrite_tagged_gate_edge() {
    let g = load(overwrite_group_fixture_xml());
    let (plan, profile) = plan_and_profile(&g);
    assert!(
        profile
            .observations()
            .iter()
            .any(|o| o.kind == CharacteristicKind::MprGroupOverwrite),
        "fixture sanity: must observe MprGroupOverwrite at all"
    );

    let report = compute_interaction_coverage(&[("overwrite-fixture", &plan, &profile)]);
    let gate_compose_row = report
        .required
        .iter()
        .find(|r| r.tuple.parent_kind == "Gate" && r.tuple.child_kind == "Compose")
        .expect("Gate -> Compose must be in the required set");
    assert_eq!(
        gate_compose_row.status,
        TupleStatus::Covered,
        "an MprGroupOverwrite tag on the Gate node is informative context, never a reason to \
         withhold coverage: {gate_compose_row:?}"
    );
    assert!(gate_compose_row
        .tags
        .contains(&CharacteristicKind::MprGroupOverwrite));
    assert!(!report
        .uncovered()
        .iter()
        .any(|r| r.tuple == gate_compose_row.tuple));
}

/// Every fixture exhibiting a tuple is credited: `covering_fixtures` names both, in supplied order.
#[test]
fn compute_interaction_coverage_credits_every_fixture_exhibiting_a_tuple() {
    let ordinary_g = load(gated_two_group_with_rule_fixture_xml());
    let (ordinary_plan, ordinary_profile) = plan_and_profile(&ordinary_g);
    let overwrite_g = load(overwrite_group_fixture_xml());
    let (overwrite_plan, overwrite_profile) = plan_and_profile(&overwrite_g);

    let report = compute_interaction_coverage(&[
        ("ordinary", &ordinary_plan, &ordinary_profile),
        ("overwrite", &overwrite_plan, &overwrite_profile),
    ]);

    let gate_compose_row = report
        .required
        .iter()
        .find(|r| r.tuple.parent_kind == "Gate" && r.tuple.child_kind == "Compose")
        .expect("Gate -> Compose must be in the required set");
    assert_eq!(
        gate_compose_row.status,
        TupleStatus::Covered,
        "both fixtures' Gate -> Compose occurrences count: {gate_compose_row:?}"
    );
    assert_eq!(
        gate_compose_row.covering_fixtures,
        vec!["ordinary".to_string(), "overwrite".to_string()]
    );
    assert!(gate_compose_row
        .tags
        .contains(&CharacteristicKind::MprGroupOverwrite));
}

#[test]
fn compute_interaction_coverage_over_empty_corpus_marks_every_required_tuple_uncovered() {
    let report = compute_interaction_coverage(&[]);
    assert_eq!(report.required.len(), 7);
    for row in &report.required {
        assert_eq!(row.status, TupleStatus::Uncovered, "{row:?}");
        assert!(row.covering_fixtures.is_empty());
    }
}

// retired_interactions

#[test]
fn retired_interactions_names_the_two_cited_proofs() {
    let retired = retired_interactions();
    assert_eq!(retired.len(), 2);
    assert!(retired
        .iter()
        .any(|r| r.label.contains("unordered-application")));
    assert!(retired
        .iter()
        .any(|r| r.label.contains("sibling reordering")));
    for r in &retired {
        assert!(!r.evidence.is_empty());
    }
}

// gate_group_count + fuzz_gate_group_reordering_for_grammar

#[test]
fn gate_group_count_matches_the_fixtures_own_two_groups() {
    let g = load(gated_two_group_with_rule_fixture_xml());
    let (plan, _profile) = plan_and_profile(&g);
    assert_eq!(gate_group_count(&plan), 2);
}

#[test]
fn fuzz_gate_group_reordering_agrees_on_the_two_group_fixture() {
    let g = load(gated_two_group_with_rule_fixture_xml());
    let (groups, result) = fuzz_gate_group_reordering_for_grammar(&g, &["p", "q"])
        .expect("both the default plan and its permuted twin must build on this fixture");
    assert_eq!(groups, 2);
    match result {
        OracleResult::Agree => {}
        OracleResult::Disagree {
            word,
            only_in_a,
            only_in_b,
            ..
        } => panic!(
            "gate-group reordering must Agree on this fixture (retirement #2's own claim) -- \
             got a real divergence at {word:?}: only_in_a={only_in_a:?}, only_in_b={only_in_b:?}"
        ),
    }
}
