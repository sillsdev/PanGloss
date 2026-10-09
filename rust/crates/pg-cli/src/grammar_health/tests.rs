use super::*;
use pg_grammar::grammar_health::check_grammar_health;
use pg_grammar::model::Grammar;
use std::sync::atomic::{AtomicUsize, Ordering};

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

const STORED_ANALYSIS_CODE: &str = "grammar.stored-analysis.no-longer-parses";
static STORED_ANALYSIS_TEST_ID: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone, Copy)]
struct StoredAnalysisFixture {
    wordform: &'static str,
    allomorph: &'static str,
    msa: &'static str,
    expected_surface: &'static str,
}

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
    assert_eq!(report_value["schema_version"], 4);
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
    assert_eq!(value["schema_version"], 4);
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
    assert_eq!(report["schema_version"], 4);
    assert!(report["diagnostics"].is_array());
    assert_eq!(report["diagnostics"][0]["level"], "error");
    assert_eq!(report["diagnostics"].as_array().unwrap().len(), 1);
    assert_eq!(report["locale"], "en");
    let finding = &report["diagnostics"][0];
    assert!(finding["explanation"]
        .as_str()
        .is_some_and(|text| !text.trim().is_empty()));
    assert_eq!(
        finding["help_path"],
        "docs/diagnostics/hc-stem-no-grammatical-category.md"
    );
    assert!(finding["help_body"]
        .as_str()
        .is_some_and(|text| text.contains("stem")));
    assert_eq!(finding["fieldworks_places"][0]["tool"], "lexiconEdit");
    assert_eq!(finding["subjects"][0]["status"], "object");

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
    .expect("provisional letters keep the fixture runnable");
    let report: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&output_path).expect("read output"))
            .expect("the report is still written");

    assert_eq!(report["schema_version"], 4);
    let error_count = report["diagnostics"]
        .as_array()
        .expect("diagnostics array")
        .iter()
        .filter(|diagnostic| diagnostic["level"] == "error")
        .count();
    assert_eq!(error_count, 0);
    let diagnostics = report["diagnostics"].as_array().unwrap();
    assert_eq!(diagnostics.len(), 5);
    let letters: Vec<_> = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic["code"] == "provisional.letter")
        .collect();
    assert_eq!(letters.len(), 4);
    for letter in ["k", "r", "n", "s"] {
        let finding = letters
            .iter()
            .find(|finding| {
                finding["description"]
                    .as_str()
                    .unwrap()
                    .contains(&format!("letter '{letter}'"))
            })
            .expect("each missing letter has a finding");
        assert_eq!(finding["level"], "info");
    }
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
    .expect("provisional letters also work with an explicit project name");
    let named_report: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&named_output_path).expect("read named output"))
            .expect("structured JSON");
    assert_eq!(named_report["fieldworks_project"]["name"], "Chosen Project");
    assert_eq!(named_report["fieldworks_project"]["source"], "argument");

    let (mut snapshot, _) = pg_fwdata::import_file(&grammar_path).unwrap();
    let allomorph = snapshot
        .lexicon
        .entries
        .iter_mut()
        .flat_map(|entry| &mut entry.allomorphs)
        .find(|allomorph| allomorph.forms.iter().any(|form| form.form == "kat"))
        .expect("the fixture contains the selected kat stem");
    let allomorph_guid = allomorph.guid.clone();
    allomorph.forms[0].form = "ka\u{0001}t".into();
    let control_path = scratch.join("control.json");
    let control_output = scratch.join("control-report.json");
    fs::write(&control_path, snapshot.to_json()).unwrap();
    let error = run_grammar_health(&[
        control_path.to_string_lossy().into_owned(),
        control_output.to_string_lossy().into_owned(),
    ])
    .expect_err("an unclassifiable control still refuses the grammar");
    assert!(error.contains("error(s)"), "{error}");
    let control_report: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(control_output).unwrap()).unwrap();
    let refusal = control_report["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .find(|finding| finding["code"] == "substrate.classification-ambiguous")
        .expect("the named refusal remains in the report");
    assert_eq!(refusal["level"], "error");
    assert!(refusal["subjects"]
        .as_array()
        .unwrap()
        .iter()
        .any(|subject| {
            subject["fieldworks"]["guid"].as_str() == Some(allomorph_guid.as_str())
        }));

    let _ = fs::remove_dir_all(scratch);
}

fn append_stored_analyses(
    project: &str,
    writing_system: &str,
    analyses: &[StoredAnalysisFixture],
) -> String {
    let mut records = String::new();
    for (index, analysis) in analyses.iter().enumerate() {
        let wordform_guid = format!("e0000000-0000-0000-0000-{index:012x}");
        let analysis_guid = format!("f0000000-0000-0000-0000-{index:012x}");
        let bundle_guid = format!("c0000000-0000-0000-0000-{index:012x}");
        records.push_str(&format!(
            "<rt class=\"WfiWordform\" guid=\"{wordform_guid}\"><Form><AUni ws=\"{writing_system}\">{wordform}</AUni></Form><Analyses><objsur guid=\"{analysis_guid}\" t=\"r\" /></Analyses></rt>\
             <rt class=\"WfiAnalysis\" guid=\"{analysis_guid}\"><MorphBundles><objsur guid=\"{bundle_guid}\" t=\"o\" /></MorphBundles></rt>\
             <rt class=\"WfiMorphBundle\" guid=\"{bundle_guid}\"><Morph><objsur guid=\"{allomorph}\" t=\"r\" /></Morph><Msa><objsur guid=\"{msa}\" t=\"r\" /></Msa></rt>",
            wordform = analysis.wordform,
            allomorph = analysis.allomorph,
            msa = analysis.msa,
        ));
    }
    let insertion = project
        .rfind("</languageproject>")
        .expect("FieldWorks project has a root closing tag");
    let mut result = project.to_string();
    result.insert_str(insertion, &records);
    result
}

fn run_stored_analysis_health(
    label: &str,
    project: &str,
    writing_system: &str,
    analyses: &[StoredAnalysisFixture],
) -> serde_json::Value {
    let scratch = std::env::temp_dir().join(format!(
        "pangloss-stored-analysis-{label}-{}-{}",
        std::process::id(),
        STORED_ANALYSIS_TEST_ID.fetch_add(1, Ordering::Relaxed),
    ));
    fs::create_dir_all(&scratch).expect("create stored-analysis fixture directory");
    let project_path = scratch.join("project.fwdata");
    let output_path = scratch.join("report.json");
    fs::write(
        &project_path,
        append_stored_analyses(project, writing_system, analyses),
    )
    .expect("write stored-analysis project");
    run_grammar_health(&[
        project_path.to_string_lossy().into_owned(),
        output_path.to_string_lossy().into_owned(),
    ])
    .unwrap_or_else(|error| panic!("grammar health failed for {label}: {error}"));
    let report: serde_json::Value =
        serde_json::from_slice(&fs::read(&output_path).expect("read stored-analysis report"))
            .expect("stored-analysis report is JSON");
    fs::remove_dir_all(&scratch).expect("remove stored-analysis fixture directory");
    report
}

fn run_stored_analysis_health_with_synthesis_cap(
    project: &str,
    writing_system: &str,
    analyses: &[StoredAnalysisFixture],
    work_cap: usize,
) -> Vec<pg_grammar::grammar_health::GrammarHealthDiagnostic> {
    let scratch = std::env::temp_dir().join(format!(
        "pangloss-stored-analysis-cap-{}-{}",
        std::process::id(),
        STORED_ANALYSIS_TEST_ID.fetch_add(1, Ordering::Relaxed),
    ));
    fs::create_dir_all(&scratch).expect("create capped stored-analysis fixture directory");
    let project_path = scratch.join("project.fwdata");
    fs::write(
        &project_path,
        append_stored_analyses(project, writing_system, analyses),
    )
    .expect("write capped stored-analysis project");
    let path = project_path.to_string_lossy().into_owned();
    let loaded = crate::load_grammar_impl(&path, false, false)
        .unwrap_or_else(|error| panic!("load capped stored-analysis fixture: {error}"));
    let diagnostics = crate::stored_analysis_health::check_with_synthesis_work_cap(
        &loaded.grammar,
        &loaded.stored_analyses,
        work_cap,
    )
    .expect("capped synthesis is reported as a finding");
    fs::remove_dir_all(&scratch).expect("remove capped stored-analysis fixture directory");
    diagnostics
}

fn staged_underdefined_project(folder: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../conformance-staging/underdefined")
        .join(folder)
        .join("fieldworks/project.fwdata");
    fs::read_to_string(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

fn assert_rule_attributed_findings(
    report: &serde_json::Value,
    expected: &[StoredAnalysisFixture],
    rule_guid: &str,
) {
    let diagnostics = report["diagnostics"].as_array().expect("diagnostic array");
    let findings: Vec<_> = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic["code"] == STORED_ANALYSIS_CODE)
        .collect();
    assert_eq!(findings.len(), expected.len(), "{findings:#?}");
    for analysis in expected {
        let finding = findings
            .iter()
            .find(|finding| {
                finding["description"]
                    .as_str()
                    .is_some_and(|text| text.contains(&format!("wordform {:?}", analysis.wordform)))
            })
            .unwrap_or_else(|| {
                panic!("missing finding for {:?}: {findings:#?}", analysis.wordform)
            });
        assert_eq!(finding["level"], "info");
        let description = finding["description"].as_str().expect("description");
        assert!(description.contains(analysis.allomorph), "{description}");
        assert!(description.contains(analysis.msa), "{description}");
        assert!(description.contains("probe-rewrite"), "{description}");
        assert!(description.contains(rule_guid), "{description}");
        assert!(
            description.contains(analysis.expected_surface),
            "{description}"
        );
        assert!(
            description.contains(&format!(
                "forward-synthesized surfaces: {:?}",
                analysis.expected_surface
            )),
            "{description}"
        );
        assert!(!description.contains("unattributed:"), "{description}");
    }
}

#[test]
fn grammar_health_attributes_rule_context_stored_analysis_losses() {
    let analyses = [
        StoredAnalysisFixture {
            wordform: "muma",
            allomorph: "630dc2aa-eb27-4c92-8e55-15eb12c0db6b",
            msa: "cbb849b1-13ac-4b86-9a5a-a4a5e950d164",
            expected_surface: "pupa",
        },
        StoredAnalysisFixture {
            wordform: "xuma",
            allomorph: "b7002eed-ba64-4ec2-baa2-f877369b535b",
            msa: "1e40e644-88dd-4429-8a13-3fa5c49abd2f",
            expected_surface: "xupa",
        },
        StoredAnalysisFixture {
            wordform: "xmuma",
            allomorph: "391454a0-50fb-4cec-a965-26ef4db345f2",
            msa: "dfb85f97-d422-47ae-94b8-1b3dee2b8111",
            expected_surface: "xpupa",
        },
    ];
    let report = run_stored_analysis_health(
        "08-rule-context",
        &staged_underdefined_project("08-rule-context"),
        "en",
        &analyses,
    );
    assert_rule_attributed_findings(&report, &analyses, "f6e5d881-e704-40d0-ad59-d1466817443b");
}

#[test]
fn grammar_health_attributes_featureless_rule_class_stored_analysis_losses() {
    let analyses = [
        StoredAnalysisFixture {
            wordform: "muma",
            allomorph: "099bb5b8-80ad-4673-8144-e6e0df7b1c67",
            msa: "22944e36-1b47-4b6a-83ef-dd23b444f738",
            expected_surface: "mupa",
        },
        StoredAnalysisFixture {
            wordform: "xuma",
            allomorph: "a7c08446-d98b-431c-b38a-d28ceadb4bee",
            msa: "d53918cb-330f-4068-b9bd-3dbde34e7349",
            expected_surface: "xupa",
        },
        StoredAnalysisFixture {
            wordform: "xmuma",
            allomorph: "6b13eb01-44e4-44ad-bc99-168adc22d63c",
            msa: "95814806-412e-4e8a-995a-0a57892891d4",
            expected_surface: "xmupa",
        },
    ];
    let report = run_stored_analysis_health(
        "12-featureless-rule-class",
        &staged_underdefined_project("12-featureless-rule-class"),
        "en",
        &analyses,
    );
    assert_rule_attributed_findings(&report, &analyses, "d2dbfb44-5405-45f6-8fde-60df386f8eaf");
    let findings = report["diagnostics"].as_array().expect("diagnostic array");
    for (wordform, ordinary_surface, actual_surface) in [
        ("muma", "xxxx", "mupa"),
        ("xuma", "xxxx", "xupa"),
        ("xmuma", "xxxxx", "xmupa"),
    ] {
        let description = findings
            .iter()
            .find(|finding| {
                finding["code"] == STORED_ANALYSIS_CODE
                    && finding["description"]
                        .as_str()
                        .is_some_and(|text| text.contains(&format!("wordform {wordform:?}")))
            })
            .expect("featureless wordform finding")["description"]
            .as_str()
            .expect("description");
        assert!(description.contains(actual_surface), "{description}");
        assert!(
            description.contains(&format!(
                "ordinary surface renderer displays {ordinary_surface:?}"
            )),
            "{description}"
        );
    }
}

#[test]
fn grammar_health_reports_unattributed_stored_analysis_loss_and_keeps_control() {
    const FIXTURE: &str = include_str!("../../../pg-fwdata/tests/data/fixture.fwdata");
    let analyses = [
        StoredAnalysisFixture {
            wordform: "kat",
            allomorph: "00000000-0000-0000-0000-000000000031",
            msa: "00000000-0000-0000-0000-000000000032",
            expected_surface: "kat",
        },
        StoredAnalysisFixture {
            wordform: "missing",
            allomorph: "00000000-0000-0000-0000-000000000031",
            msa: "00000000-0000-0000-0000-000000000032",
            expected_surface: "kat",
        },
    ];
    let fixed_fixture = FIXTURE.replace(
        "guid=\"00000000-0000-0000-0000-0000000000ff\"",
        "guid=\"00000000-0000-0000-0000-000000000017\"",
    );
    let no_rules_fixture = fixed_fixture.replace(
        "<objsur guid=\"00000000-0000-0000-0000-000000000018\" t=\"o\" />",
        "",
    );
    let report = run_stored_analysis_health("unattributed", &no_rules_fixture, "fx", &analyses);
    let findings: Vec<_> = report["diagnostics"]
        .as_array()
        .expect("diagnostic array")
        .iter()
        .filter(|diagnostic| diagnostic["code"] == STORED_ANALYSIS_CODE)
        .collect();
    assert_eq!(findings.len(), 1, "{findings:#?}");
    let description = findings[0]["description"].as_str().expect("description");
    assert!(
        description.contains("wordform \"missing\""),
        "{description}"
    );
    assert!(description.contains("unattributed:"), "{description}");
    assert!(
        description.contains("no authored phonological rule application"),
        "{description}"
    );
    assert!(
        description.contains("forward-synthesized surfaces: \"kat\""),
        "{description}"
    );
    assert!(!description.contains("wordform \"kat\""), "{description}");
}

#[test]
fn grammar_health_reports_synthesis_budget_as_unattributed() {
    let analysis = [StoredAnalysisFixture {
        wordform: "muma",
        allomorph: "630dc2aa-eb27-4c92-8e55-15eb12c0db6b",
        msa: "cbb849b1-13ac-4b86-9a5a-a4a5e950d164",
        expected_surface: "pupa",
    }];
    let diagnostics = run_stored_analysis_health_with_synthesis_cap(
        &staged_underdefined_project("08-rule-context"),
        "en",
        &analysis,
        0,
    );
    let finding = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code.wire() == STORED_ANALYSIS_CODE)
        .expect("stored-analysis finding");
    assert!(
        finding
            .message
            .contains("unattributed: synthesis step budget reached"),
        "{}",
        finding.message
    );
    assert!(!finding.message.contains("authored phonological rule"));
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

#[test]
fn fatal_conversion_issues_are_written_as_health_findings() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("unknown-provenance.json");
    let output = scratch.path().join("health.json");
    let mut snapshot = pg_snapshot::Snapshot::new(
        Default::default(),
        Default::default(),
        pg_snapshot::phonology::Phonology {
            boundary_markers: vec![pg_snapshot::phonology::BoundaryMarker {
                guid: "00000000-0000-0000-0000-000000000001".into(),
                name: "Morpheme boundary".into(),
                representations: vec![pg_snapshot::WsForm {
                    ws: "und".into(),
                    form: "+".into(),
                }],
            }],
            ..Default::default()
        },
        Default::default(),
        Default::default(),
    );
    snapshot.conversion_provenance = Default::default();
    fs::write(&path, snapshot.to_json()).unwrap();
    let error = run_grammar_health(&[
        path.to_string_lossy().into_owned(),
        output.to_string_lossy().into_owned(),
    ])
    .expect_err("fatal conversion must remain a failure");
    assert!(error.contains("error(s)"), "{error}");
    let wire: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(output).unwrap()).unwrap();
    let findings = wire["diagnostics"].as_array().unwrap();
    let finding = findings
        .iter()
        .find(|finding| finding["code"] == "conversion.source-provenance-unknown")
        .expect("the compiler's fatal issue reaches grammar-health");
    assert_eq!(finding["level"], "error");
    assert!(!finding["guidance"].as_str().unwrap().is_empty());
}

#[test]
fn invalid_xml_is_reported_before_grammar_health_returns_failure() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("invalid.xml");
    let output = scratch.path().join("health.json");
    fs::write(&path, "<HermitCrabInput>").unwrap();
    run_grammar_health(&[
        path.to_string_lossy().into_owned(),
        output.to_string_lossy().into_owned(),
    ])
    .expect_err("the invalid grammar remains unusable");
    let wire: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(output).unwrap()).unwrap();
    assert_eq!(wire["diagnostics"][0]["code"], "grammar.compile.failed");
    assert_eq!(wire["diagnostics"][0]["level"], "error");
}
