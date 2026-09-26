use super::*;
use pg_grammar::grammar_health::check_grammar_health;
use pg_grammar::model::Grammar;

/// Same clean, zero-diagnostics shape `fst_health.rs`'s own fixture uses.
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
    let json = render_json(&report).expect("empty diagnostics serialize");
    let report_value: serde_json::Value = serde_json::from_str(&json).expect("versioned report");
    assert_eq!(report_value["schema_version"], 3);
    assert!(report_value["diagnostics"].is_array());
    assert_eq!(
        render_level_counts(&report),
        "0 error(s), 0 warning(s), 0 info"
    );
}

#[test]
fn json_is_versioned_by_default() {
    let g = grammar(CLEAN_GRAMMAR_XML);
    let report = check_grammar_health(&g, None).expect("clean grammar checks");
    let json = render_json(&report).expect("structured diagnostics serialize");
    let value: serde_json::Value = serde_json::from_str(&json).expect("structured JSON");
    assert_eq!(value["schema_version"], 3);
    assert!(value["diagnostics"].is_array());
}

#[test]
fn command_emits_versioned_json_by_default() {
    let grammar_path = std::env::temp_dir().join(format!(
        "pangloss-grammar-health-{}-input.xml",
        std::process::id()
    ));
    let output_path = grammar_path.with_extension("json");
    fs::write(&grammar_path, PARTIAL_ENTRY_GRAMMAR_XML).expect("write grammar fixture");

    let error = run_grammar_health(&[
        grammar_path.to_string_lossy().into_owned(),
        output_path.to_string_lossy().into_owned(),
    ])
    .expect_err("a partial morpheme fails the command");
    assert!(error.contains("1 error(s)"), "{error}");
    let report: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&output_path).expect("read output"))
            .expect("the report is still written");
    assert_eq!(report["schema_version"], 3);
    assert!(report["diagnostics"].is_array());
    assert_eq!(report["diagnostics"][0]["level"], "error");
    assert_eq!(report["diagnostics"].as_array().unwrap().len(), 1);

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

    let error = run_grammar_health(&[
        grammar_path.to_string_lossy().into_owned(),
        output_path.to_string_lossy().into_owned(),
    ])
    .expect_err("the fixture's error-level import issues fail the command");
    assert!(error.contains("error(s)"), "{error}");
    let report: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&output_path).expect("read output"))
            .expect("the report is still written");

    assert_eq!(report["schema_version"], 3);
    let error_count = report["diagnostics"]
        .as_array()
        .expect("diagnostics array")
        .iter()
        .filter(|diagnostic| diagnostic["level"] == "error")
        .count();
    assert!(
        error.starts_with(&format!("{error_count} error(s)")),
        "{error}"
    );
    assert_eq!(report["fieldworks_project"]["name"], "fixture");
    assert_eq!(report["fieldworks_project"]["source"], "fwdata_path");
    let warning = report["diagnostics"]
        .as_array()
        .expect("diagnostics array")
        .iter()
        .find(|diagnostic| diagnostic["code"] == "fwdata.unknown-morph-type-guid")
        .expect("unknown morph type import diagnostic");
    assert_eq!(warning["origin"], "import");
    assert_eq!(warning["level"], "warning");
    let description = warning["description"]
        .as_str()
        .expect("warning description");
    assert!(description.contains("'xxx'"));
    assert!(description.contains("unknown morph type"));
    assert!(description.contains("skipped"));
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

    let named_output_path = scratch.join("named-report.json");
    run_grammar_health(&[
        grammar_path.to_string_lossy().into_owned(),
        named_output_path.to_string_lossy().into_owned(),
        "--fw-project".to_string(),
        "  Chosen Project  ".to_string(),
    ])
    .expect_err("the same error-level issues fail the command with an explicit project name");
    let named_report: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&named_output_path).expect("read named output"))
            .expect("structured JSON");
    assert_eq!(named_report["fieldworks_project"]["name"], "Chosen Project");
    assert_eq!(named_report["fieldworks_project"]["source"], "argument");

    let _ = fs::remove_dir_all(scratch);
}

#[test]
fn partial_entry_grammar_reports_one_error_naming_its_code() {
    let g = grammar(PARTIAL_ENTRY_GRAMMAR_XML);
    let report = check_grammar_health(&g, None).expect("partial grammar checks");
    assert_eq!(report.len(), 1);
    let json = render_json(&report).expect("diagnostics serialize");
    assert!(json.contains("hc-stem-no-grammatical-category"));
    assert_eq!(
        render_level_counts(report.diagnostics()),
        "1 error(s), 0 warning(s), 0 info"
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
