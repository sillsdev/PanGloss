use super::*;
use std::sync::atomic::{AtomicU32, Ordering};

const XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>PlanDiagramCliFixture</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="c1"><Representations><Representation>p</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <Strata>
      <Stratum characterDefinitionTable="t1">
        <Name>S</Name>
        <LexicalEntries>
          <LexicalEntry id="e1" partOfSpeech="posV">
            <Allomorphs><Allomorph id="a1"><PhoneticShape>p</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>e1</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

fn scratch_path(tag: &str, ext: &str) -> std::path::PathBuf {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("pangloss-cli-plan-diagram-test-{tag}-{n}.{ext}"))
}

fn write_fixture_grammar(tag: &str) -> std::path::PathBuf {
    let path = scratch_path(tag, "xml");
    fs::write(&path, XML).expect("write fixture grammar");
    path
}

#[test]
fn plan_diagram_cli_writes_mermaid_by_default() {
    let grammar_path = write_fixture_grammar("mermaid");
    let out_path = scratch_path("mermaid-out", "mmd");

    run_plan_diagram(&[
        grammar_path.to_str().unwrap().to_string(),
        out_path.to_str().unwrap().to_string(),
    ])
    .expect("plan-diagram must succeed");

    let text = fs::read_to_string(&out_path).expect("read output");
    assert!(text.contains("flowchart TD"));
    assert!(text.contains("nodes emitted"));

    let _ = fs::remove_file(&grammar_path);
    let _ = fs::remove_file(&out_path);
}

#[test]
fn plan_diagram_cli_writes_json_with_flag() {
    let grammar_path = write_fixture_grammar("json");
    let out_path = scratch_path("json-out", "json");

    run_plan_diagram(&[
        "--json".to_string(),
        grammar_path.to_str().unwrap().to_string(),
        out_path.to_str().unwrap().to_string(),
    ])
    .expect("plan-diagram --json must succeed");

    let text = fs::read_to_string(&out_path).expect("read output");
    let value: serde_json::Value = serde_json::from_str(&text).expect("valid JSON");
    assert!(value.get("schema_version").is_some());
    assert!(value.get("nodes").is_some());

    let _ = fs::remove_file(&grammar_path);
    let _ = fs::remove_file(&out_path);
}

#[test]
fn plan_diagram_cli_rejects_full_and_threshold_together() {
    let grammar_path = write_fixture_grammar("conflict");
    let err = run_plan_diagram(&[
        "--full".to_string(),
        "--threshold=5".to_string(),
        grammar_path.to_str().unwrap().to_string(),
    ])
    .expect_err("must reject --full combined with --threshold");
    assert!(err.contains("mutually exclusive"));

    let _ = fs::remove_file(&grammar_path);
}
