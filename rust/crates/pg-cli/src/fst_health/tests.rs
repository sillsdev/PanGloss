use super::*;

/// Same clean, `Admit`-verdict shape `pack.rs`'s own `CLEAN_GRAMMAR_XML` uses.
const CLEAN_GRAMMAR_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>FstHealthCleanFixture</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="table1">
      <Name>Orthography</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="segA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="segK"><Representations><Representation>k</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="segT"><Representations><Representation>t</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses></NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="table1">
        <Name>main</Name>
        <LexicalEntries>
          <LexicalEntry id="e1">
            <Allomorphs><Allomorph id="e1-1"><PhoneticShape>kat</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>kat</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

fn grammar(xml: &str) -> Grammar {
    pg_grammar::load(xml).unwrap_or_else(|e| panic!("fixture grammar failed to load: {e}"))
}

/// The printed fragment must name all four axes, not just the collapsed severity band.
#[test]
fn admission_summary_names_all_four_axes() {
    let g = grammar(CLEAN_GRAMMAR_XML);
    let report = build_health_report(&g);
    let summary = render_admission_summary(&report);
    assert!(summary.contains("representability="), "{summary}");
    assert!(summary.contains("readiness="), "{summary}");
    assert!(summary.contains("containment="), "{summary}");
    assert!(summary.contains("process="), "{summary}");
}

#[test]
fn explicit_strategy_is_reachable_from_fst_health_and_records_typed_refusal() {
    let scratch = tempfile::tempdir().unwrap();
    let input = scratch.path().join("grammar.xml");
    let output = scratch.path().join("report.json");
    fs::write(&input, CLEAN_GRAMMAR_XML).unwrap();
    run_fst_health(&[
        input.to_string_lossy().into_owned(),
        output.to_string_lossy().into_owned(),
        "--fst-strategy".into(),
        "factored".into(),
    ])
    .unwrap();
    let value: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(output).unwrap()).unwrap();
    let report = &value["construction"]["profile"]["construction"];
    assert!(report["selected"].is_null());
    assert_eq!(report["attempts"][0]["strategy"], "factored");
    assert_eq!(report["attempts"][0]["refusal"]["kind"], "not_implemented");
}
