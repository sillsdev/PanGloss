use std::fs;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_CASE: AtomicUsize = AtomicUsize::new(0);

const REQUEST: &str = r#"{
  "schemaVersion": 1,
  "root": {
    "entryGuid": "00000000-0000-0000-0000-000000000030",
    "msaGuid": "00000000-0000-0000-0000-000000000032",
    "msaClass": "MoStemMsa"
  },
  "morphemes": [
    {
      "entryGuid": "00000000-0000-0000-0000-000000000050",
      "msaGuid": "00000000-0000-0000-0000-000000000052",
      "msaClass": "MoInflAffMsa"
    }
  ]
}"#;

const ROOT_ONLY_REQUEST: &str = r#"{
  "schemaVersion": 1,
  "root": {
    "entryGuid": "00000000-0000-0000-0000-000000000030",
    "msaGuid": "00000000-0000-0000-0000-000000000032",
    "msaClass": "MoStemMsa"
  },
  "morphemes": []
}"#;

const FW_DATA_ROOT_AND_AFFIX_REQUEST: &str = r#"{
  "schemaVersion": 1,
  "root": {
    "entryGuid": "00000000-0000-0000-0000-000000000040",
    "msaGuid": "00000000-0000-0000-0000-000000000042",
    "msaClass": "MoStemMsa"
  },
  "morphemes": [
    {
      "entryGuid": "00000000-0000-0000-0000-000000000050",
      "msaGuid": "00000000-0000-0000-0000-000000000052",
      "msaClass": "MoInflAffMsa"
    }
  ]
}"#;

const MINI_HC_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>MiniGenerateTest</Name>
    <PartsOfSpeech><PartOfSpeech id="n"><Name>Noun</Name></PartOfSpeech></PartsOfSpeech>
    <PhonologicalFeatureSystem>
      <SymbolicFeature id="seg6"><Name>SegId</Name>
        <Symbols><Symbol id="idA">a</Symbol><Symbol id="idT">t</Symbol><Symbol id="idK">k</Symbol></Symbols>
      </SymbolicFeature>
    </PhonologicalFeatureSystem>
    <CharacterDefinitionTable id="table1">
      <Name>Orthography</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="seg1"><Representations><Representation>a</Representation></Representations>
          <FeatureValue feature="seg6" symbolValues="idA" /></SegmentDefinition>
        <SegmentDefinition id="segT"><Representations><Representation>t</Representation></Representations>
          <FeatureValue feature="seg6" symbolValues="idT" /></SegmentDefinition>
        <SegmentDefinition id="segK"><Representations><Representation>k</Representation></Representations>
          <FeatureValue feature="seg6" symbolValues="idK" /></SegmentDefinition>
      </SegmentDefinitions>
      <BoundaryDefinitions>
        <BoundaryDefinition id="bdry1"><Representations><Representation>+</Representation></Representations></BoundaryDefinition>
      </BoundaryDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses></NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="table1">
        <Name>main</Name>
        <LexicalEntries>
          <LexicalEntry id="root" partOfSpeech="n"><MorphemeId>root-id</MorphemeId>
            <Allomorphs><Allomorph id="root-form"><PhoneticShape>kat</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>kat</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

fn scratch_dir() -> std::path::PathBuf {
    let case = NEXT_CASE.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "pangloss-authored-generate-{}-{case}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).expect("create temporary fixture directory");
    dir
}

#[test]
fn authored_generate_request_uses_exact_snapshot_msa_owners() {
    let dir = scratch_dir();
    let grammar = dir.join("kat-snapshot.json");
    let request = dir.join("request.json");
    fs::write(
        &grammar,
        include_str!("../../../../docs/divergences/evidence/071/kat-snapshot.json"),
    )
    .expect("write committed synthetic snapshot fixture");
    fs::write(&request, REQUEST).expect("write versioned authored generation request");

    let output = Command::new(env!("CARGO_BIN_EXE_pangloss"))
        .args([
            "generate",
            grammar.to_str().expect("grammar path is UTF-8"),
            "--request",
            request.to_str().expect("request path is UTF-8"),
        ])
        .output()
        .expect("run pangloss generate");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "authored request failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    // The fixture's ordinary generator currently emits no form for this chain.
    assert!(stdout.is_empty());
    assert!(
        !stderr.contains("generate complete"),
        "ordinary generation has no completion witness: {stderr}"
    );

    fs::write(&request, ROOT_ONLY_REQUEST).expect("write root-only authored request");
    let root_output = Command::new(env!("CARGO_BIN_EXE_pangloss"))
        .args([
            "generate",
            grammar.to_str().expect("grammar path is UTF-8"),
            "--request",
            request.to_str().expect("request path is UTF-8"),
        ])
        .output()
        .expect("run pangloss generate for the root-only request");
    let root_stdout = String::from_utf8_lossy(&root_output.stdout);
    let root_stderr = String::from_utf8_lossy(&root_output.stderr);
    assert!(
        root_output.status.success(),
        "root-only authored request failed\nstdout:\n{root_stdout}\nstderr:\n{root_stderr}"
    );
    assert_eq!(root_stdout, "kad\n");
}

#[test]
fn authored_generate_request_generates_known_root_and_affix_surface() {
    let acceptance_dir =
        std::env::var_os("PANGLOSS_GENERATE_ACCEPTANCE_DIR").map(std::path::PathBuf::from);
    let (grammar, request) = if acceptance_dir.is_some() {
        (
            std::env::var_os("PANGLOSS_GENERATE_ACCEPTANCE_SOURCE")
                .map(std::path::PathBuf::from)
                .expect("acceptance source path is configured"),
            std::env::var_os("PANGLOSS_GENERATE_ACCEPTANCE_REQUEST")
                .map(std::path::PathBuf::from)
                .expect("acceptance request path is configured"),
        )
    } else {
        let dir = scratch_dir();
        let grammar = dir.join("fixture.fwdata");
        let request = dir.join("request.json");
        fs::write(
            &grammar,
            include_str!("../../../../docs/divergences/evidence/071/kat-fixture-with-k.fwdata"),
        )
        .expect("write committed FieldWorks source fixture");
        fs::write(&request, FW_DATA_ROOT_AND_AFFIX_REQUEST)
            .expect("write exact authored root and affix request");
        (grammar, request)
    };

    let output = Command::new(env!("CARGO_BIN_EXE_pangloss"))
        .args([
            "generate",
            grammar.to_str().expect("grammar path is UTF-8"),
            "--request",
            request.to_str().expect("request path is UTF-8"),
        ])
        .output()
        .expect("run pangloss generate");
    if let Some(dir) = &acceptance_dir {
        fs::create_dir_all(dir).expect("create acceptance output directory");
        fs::write(dir.join("tam-chair-pl-stdout.txt"), &output.stdout)
            .expect("save raw acceptance stdout");
        fs::write(dir.join("tam-chair-pl-stderr.txt"), &output.stderr)
            .expect("save raw acceptance stderr");
        fs::write(
            dir.join("tam-chair-pl-exit-code.txt"),
            output
                .status
                .code()
                .map_or_else(|| "signal\n".to_string(), |code| format!("{code}\n")),
        )
        .expect("save acceptance process exit code");
        fs::write(
            dir.join("tam-chair-pl-binary-path.txt"),
            format!("{}\n", env!("CARGO_BIN_EXE_pangloss")),
        )
        .expect("save managed test binary path");
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "authored fwdata request failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    if acceptance_dir.is_some() {
        assert_eq!(stdout, "கதிரைகள்\n");
    } else {
        assert_eq!(stdout, "rannas\n");
    }
    assert!(!stderr.contains("generate complete"));
}

#[test]
fn authored_generate_request_refuses_oversized_allomorph_product_before_enumeration() {
    let dir = scratch_dir();
    let grammar = dir.join("grammar.fwdata");
    let request = dir.join("request.json");
    let source =
        include_str!("../../../../docs/divergences/evidence/071/kat-fixture-with-k.fwdata");
    let source = source
        .replacen(
            "<AUni ws=\"fx\">xxx</AUni>",
            "<AUni ws=\"fx\">ranni</AUni>",
            1,
        )
        .replacen(
            "guid=\"00000000-0000-0000-0000-00000000abcd\" t=\"r\"",
            "guid=\"d7f713e8-e8cf-11d3-9764-00c04f186933\" t=\"r\"",
            1,
        );
    assert!(source.contains("<AUni ws=\"fx\">ranni</AUni>"));
    assert!(!source.contains("guid=\"00000000-0000-0000-0000-00000000abcd\""));
    fs::write(&grammar, source).expect("write two-allomorph fixture copy");

    // Repeating this two-allomorph authored root 63 times creates a 2^64 product without expanding it.
    let root = r#"{"entryGuid":"00000000-0000-0000-0000-000000000040","msaGuid":"00000000-0000-0000-0000-000000000042","msaClass":"MoStemMsa"}"#;
    let morphemes = std::iter::repeat_n(root, 63).collect::<Vec<_>>().join(",");
    let request_json = format!(r#"{{"schemaVersion":1,"root":{root},"morphemes":[{morphemes}]}}"#);
    assert!(request_json.len() < 64 * 1024);
    fs::write(&request, request_json).expect("write bounded authored request");

    let output = Command::new(env!("CARGO_BIN_EXE_pangloss"))
        .args([
            "generate",
            grammar.to_str().expect("grammar path is UTF-8"),
            "--request",
            request.to_str().expect("request path is UTF-8"),
        ])
        .output()
        .expect("run pangloss generate");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success(), "unexpected success: {stdout}");
    assert!(stdout.is_empty(), "refusal emitted forms: {stdout}");
    assert!(
        stderr.contains("GenerateRequestResourceLimit"),
        "missing typed resource refusal: {stderr}"
    );
    assert!(
        stderr.contains(
            "allomorph seed product exceeds the finite 50000000-unit generation allowance"
        ),
        "resource refusal did not state its bound: {stderr}"
    );
}

#[test]
fn authored_generate_request_does_not_fall_back_to_hc_xml_ids() {
    let dir = scratch_dir();
    let grammar = dir.join("grammar.xml");
    let request = dir.join("request.json");
    fs::write(&grammar, MINI_HC_XML).expect("write HC XML fixture");
    fs::write(&request, REQUEST).expect("write request");

    let request_output = Command::new(env!("CARGO_BIN_EXE_pangloss"))
        .args([
            "generate",
            grammar.to_str().expect("grammar path is UTF-8"),
            "--request",
            request.to_str().expect("request path is UTF-8"),
        ])
        .output()
        .expect("run request mode against HC XML");
    let request_stdout = String::from_utf8_lossy(&request_output.stdout);
    let request_stderr = String::from_utf8_lossy(&request_output.stderr);
    assert!(!request_output.status.success());
    assert!(request_stdout.is_empty());
    assert!(request_stderr.contains("AuthoredIdentityUnavailable"));
    assert!(request_stderr.contains("positional <MorphemeId>"));

    let positional_output = Command::new(env!("CARGO_BIN_EXE_pangloss"))
        .args([
            "generate",
            grammar.to_str().expect("grammar path is UTF-8"),
            "root-id",
        ])
        .output()
        .expect("run existing positional mode against HC XML");
    let positional_stdout = String::from_utf8_lossy(&positional_output.stdout);
    let positional_stderr = String::from_utf8_lossy(&positional_output.stderr);
    assert!(
        positional_output.status.success(),
        "positional HC XML mode failed\nstdout:\n{positional_stdout}\nstderr:\n{positional_stderr}"
    );
    assert_eq!(positional_stdout, "kat\n");
    assert!(!positional_stderr.contains("generate complete"));
}

#[test]
fn authored_generate_request_rejects_mixed_selector_modes() {
    let output = Command::new(env!("CARGO_BIN_EXE_pangloss"))
        .args([
            "generate",
            "unused.xml",
            "--request",
            "unused.json",
            "root-id",
        ])
        .output()
        .expect("run mixed selector invocation");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(stderr.contains("GenerateRequestMixedSelectors"));
}
