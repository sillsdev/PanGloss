use super::*;

#[test]
fn coverage_summary_json_round_trips_and_counts_match_the_ledger() {
    let summary = build_summary(None);
    let json = serde_json::to_string_pretty(&summary).expect("serialize");
    let value: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
    assert_eq!(value["schema_version"], COVERAGE_CLI_SCHEMA_VERSION);

    // No independent recount: every count must equal a direct tally over the embedded ledger's own rows.
    let ledger = &summary.ledger;
    let recount = compute_disposition_counts(ledger);
    assert_eq!(recount.proven, summary.disposition_counts.proven);
    assert_eq!(
        recount.confirm_only,
        summary.disposition_counts.confirm_only
    );
    assert_eq!(
        recount.config_predicate,
        summary.disposition_counts.config_predicate
    );
    assert_eq!(recount.total, summary.disposition_counts.total);
    assert_eq!(recount.total, CharacteristicKind::ALL.len());

    let recount_evidence = compute_evidence_counts(ledger);
    assert_eq!(
        recount_evidence.rows_with_discharging_predicate,
        summary.evidence_counts.rows_with_discharging_predicate
    );
    assert_eq!(
        recount_evidence.rows_with_containment_evidence,
        summary.evidence_counts.rows_with_containment_evidence
    );
    assert_eq!(
        recount_evidence.rows_mapped_to_conformance_construct,
        summary.evidence_counts.rows_mapped_to_conformance_construct
    );
    assert_eq!(
        recount_evidence.rows_conformance_covered,
        summary.evidence_counts.rows_conformance_covered
    );
    assert_eq!(
        recount_evidence.rows_unmappable,
        summary.evidence_counts.rows_unmappable
    );

    // supported_conformance_cross_check must be exactly the Proven subset of the SAME ledger.
    let proven_in_ledger = ledger
        .rows
        .iter()
        .filter(|r| r.disposition == Disposition::Proven)
        .count();
    assert_eq!(
        proven_in_ledger,
        summary.supported_conformance_cross_check.len()
    );

    // Plan-interaction section must be honestly absent when no grammar was supplied.
    assert!(value.get("plan_interaction").is_none() || value["plan_interaction"].is_null());

    // Full round trip: re-serializing the original struct must be stable (same content, not necessarily object identity).
    let json2 = serde_json::to_string_pretty(&summary).expect("serialize again");
    assert_eq!(json, json2, "serialization must be deterministic");
}

#[test]
fn render_human_mentions_headline_and_every_kind() {
    let summary = build_summary(None);
    let text = render_human(&summary);
    assert!(text.contains(&summary.headline));
    for &kind in CharacteristicKind::ALL {
        assert!(
            text.contains(&format!("{kind:?}")),
            "human summary must mention {kind:?}"
        );
    }
}

#[test]
fn plan_interaction_is_included_with_a_grammar_and_omitted_without() {
    const XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>CoverageCliFixture</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="table1">
      <Name>Orthography</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="segA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="segT"><Representations><Representation>t</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <Strata>
      <Stratum characterDefinitionTable="table1">
        <Name>main</Name>
        <LexicalEntries>
          <LexicalEntry id="e1">
            <Allomorphs><Allomorph id="e1-1"><PhoneticShape>at</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>at</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;
    let g = pg_grammar::load(XML).expect("fixture grammar must load");

    let without = build_summary(None);
    assert!(without.plan_interaction.is_none());

    let with = build_summary(Some(("coverage-cli-fixture", &g)));
    assert!(with.plan_interaction.is_some());
    let pi = with.plan_interaction.unwrap();
    assert_eq!(pi.grammar_path, "coverage-cli-fixture");
    assert_eq!(
        pi.required_total, 7,
        "must report all 7 documented legal adjacency tuples"
    );
}
