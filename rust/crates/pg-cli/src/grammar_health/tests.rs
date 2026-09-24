use super::*;
use pg_grammar::grammar_health::check_grammar_health;
use pg_grammar::model::Grammar;

/// Same clean, zero-findings shape `fst_health.rs`'s own fixture uses.
const CLEAN_GRAMMAR_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
<Name>GrammarHealthCleanFixture</Name>
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

const PARTIAL_ENTRY_GRAMMAR_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
<Name>GrammarHealthPartialFixture</Name>
<PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
<CharacterDefinitionTable id="table1">
  <Name>Orthography</Name>
  <SegmentDefinitions>
    <SegmentDefinition id="segA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
  </SegmentDefinitions>
</CharacterDefinitionTable>
<NaturalClasses></NaturalClasses>
<Strata>
  <Stratum characterDefinitionTable="table1">
    <Name>main</Name>
    <LexicalEntries>
      <LexicalEntry id="entry1" partial="true">
        <Allomorphs><Allomorph id="e1-1"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs>
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

#[test]
fn clean_grammar_serializes_to_an_empty_versioned_report() {
    let g = grammar(CLEAN_GRAMMAR_XML);
    let report = check_grammar_health(&g, None).expect("clean grammar checks");
    assert!(report.is_empty());
    let json = render_json(&report).expect("empty findings serialize");
    let report_value: serde_json::Value = serde_json::from_str(&json).expect("versioned report");
    assert_eq!(report_value["schema_version"], 2);
    assert!(report_value["findings"].is_array());
    assert_eq!(render_severity_counts(&report), "0 error(s), 0 warning(s)");
}

#[test]
fn json_is_versioned_by_default() {
    let g = grammar(CLEAN_GRAMMAR_XML);
    let report = check_grammar_health(&g, None).expect("clean grammar checks");
    let json = render_json(&report).expect("structured findings serialize");
    let value: serde_json::Value = serde_json::from_str(&json).expect("structured JSON");
    assert_eq!(value["schema_version"], 2);
    assert!(value["findings"].is_array());
}

#[test]
fn command_emits_versioned_json_by_default() {
    let grammar_path = std::env::temp_dir().join(format!(
        "pangloss-grammar-health-{}-input.xml",
        std::process::id()
    ));
    let output_path = grammar_path.with_extension("json");
    fs::write(&grammar_path, PARTIAL_ENTRY_GRAMMAR_XML).expect("write grammar fixture");

    run_grammar_health(&[
        grammar_path.to_string_lossy().into_owned(),
        output_path.to_string_lossy().into_owned(),
    ])
    .expect("grammar-health command");
    let report: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&output_path).expect("read output"))
            .expect("structured JSON");
    assert_eq!(report["schema_version"], 2);
    assert!(report["findings"].is_array());
    assert_eq!(report["findings"].as_array().unwrap().len(), 1);

    let _ = fs::remove_file(grammar_path);
    let _ = fs::remove_file(output_path);
}

#[test]
fn command_includes_import_warnings_and_infers_fwdata_project() {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../pg-fwdata/tests/data/fixture.fwdata");
    let xml = fs::read_to_string(&fixture).expect("read synthetic fixture");
    let xml = xml.replace(
        "guid=\"00000000-0000-0000-0000-0000000000ff\"",
        "guid=\"00000000-0000-0000-0000-000000000017\"",
    );
    let scratch = std::env::temp_dir().join(format!(
        "pangloss-grammar-health-import-{}",
        std::process::id()
    ));
    fs::create_dir_all(&scratch).expect("create scratch directory");
    let grammar_path = scratch.join("fixture.fwdata");
    let output_path = scratch.join("report.json");
    fs::write(&grammar_path, xml).expect("write synthetic fixture");

    run_grammar_health(&[
        grammar_path.to_string_lossy().into_owned(),
        output_path.to_string_lossy().into_owned(),
    ])
    .expect("grammar-health command");
    let report: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&output_path).expect("read output"))
            .expect("structured JSON");

    assert_eq!(report["schema_version"], 2);
    assert_eq!(report["fieldworks_project"]["name"], "fixture");
    assert_eq!(report["fieldworks_project"]["source"], "fwdata_path");
    let warning = report["findings"]
        .as_array()
        .expect("findings array")
        .iter()
        .find(|finding| finding["code"] == "fwdata.unknown-morph-type-guid")
        .expect("unknown morph type import finding");
    assert_eq!(warning["origin"], "import");
    assert_eq!(warning["audience"], "linguist");
    assert_eq!(
        warning["description"],
        "Allomorph 'xxx' has an unknown morph type and was skipped."
    );
    assert_eq!(
        warning["guidance"],
        pg_snapshot::import_warning_metadata(
            pg_snapshot::ImportWarningCode::FwdataUnknownMorphTypeGuid
        )
        .guidance
        .expect("warning metadata supplies guidance")
    );
    assert_eq!(warning["subjects"][0]["kind"], "MoForm");
    assert_eq!(warning["subjects"][0]["title"], "xxx");
    assert_eq!(warning["subjects"][0]["fieldworks"]["status"], "available");

    let _ = fs::remove_dir_all(scratch);
}

#[test]
fn partial_entry_grammar_reports_one_warning_naming_its_code() {
    let g = grammar(PARTIAL_ENTRY_GRAMMAR_XML);
    let report = check_grammar_health(&g, None).expect("partial grammar checks");
    assert_eq!(report.len(), 1);
    let json = render_json(&report).expect("findings serialize");
    assert!(json.contains("hc-stem-no-grammatical-category"));
    assert_eq!(
        render_severity_counts(report.findings()),
        "0 error(s), 1 warning(s)"
    );
}

#[test]
fn fieldworks_project_defaults_to_fwdata_stem_and_honors_override() {
    let inferred =
        fieldworks_project_for_path(r"C:\FieldWorks\Projects\Sena 3\Sena 3.fwdata", None);
    assert_eq!(
        inferred,
        FieldWorksProject {
            name: Some("Sena 3".to_string()),
            source: Some(FieldWorksProjectSource::FwdataPath),
        }
    );

    let explicit = fieldworks_project_for_path("project.fwdata", Some("Chosen Name"));
    assert_eq!(
        explicit,
        FieldWorksProject {
            name: Some("Chosen Name".to_string()),
            source: Some(FieldWorksProjectSource::Argument),
        }
    );

    assert_eq!(
        fieldworks_project_for_path("grammar.xml", None),
        FieldWorksProject::default()
    );
}
