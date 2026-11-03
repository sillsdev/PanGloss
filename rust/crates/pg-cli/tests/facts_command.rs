use std::path::Path;
use std::process::Command;

use pg_snapshot::phonology::Phonology;
use pg_snapshot::Snapshot;
use pg_snapshot::{Allomorph, BoundaryMarker, LexEntry, MorphType, Msa, Project, WsForm};

fn executable() -> &'static str {
    env!("CARGO_BIN_EXE_pangloss")
}

fn minimal_snapshot() -> Snapshot {
    let project = Project {
        vernacular_writing_systems: vec!["qaa".into()],
        ..Project::default()
    };
    Snapshot::new(
        project,
        pg_snapshot::FeatureSystems::default(),
        Phonology {
            boundary_markers: vec![BoundaryMarker {
                guid: "00000000-0000-0000-0000-000000000001".into(),
                name: "Morpheme boundary".into(),
                representations: vec![WsForm {
                    ws: "qaa".into(),
                    form: "+".into(),
                }],
            }],
            ..Phonology::default()
        },
        pg_snapshot::Morphology::default(),
        pg_snapshot::Lexicon::default(),
    )
}

fn write_context(path: &Path) {
    std::fs::write(
        path,
        br#"{"format":"pangloss-facts-context","version":1,"baselineToken":{"id":"cli-test"},"inputKind":"baseline","dryRunDigest":null}"#,
    )
    .unwrap();
}

#[test]
fn facts_command_writes_one_json_result_and_rejects_input_collision() {
    let temp = tempfile::tempdir().unwrap();
    let snapshot_path = temp.path().join("snapshot.json");
    let context_path = temp.path().join("context.json");
    let output_path = temp.path().join("facts.sqlite");
    let source = minimal_snapshot().to_json();
    std::fs::write(&snapshot_path, &source).unwrap();
    write_context(&context_path);

    let result = Command::new(executable())
        .arg("facts")
        .arg(&snapshot_path)
        .arg("--out")
        .arg(&output_path)
        .arg("--context")
        .arg(&context_path)
        .arg("--json")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let response: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(response["applicationId"], 1_346_848_321);
    assert_eq!(response["schemaVersion"], 8);
    assert_eq!(response["compileStatus"], "completed");
    assert!(response["outputBytes"].as_u64().unwrap() > 0);
    assert!(output_path.exists());

    let missing_stats_manifest = Command::new(executable())
        .arg("facts")
        .arg(&snapshot_path)
        .arg("--out")
        .arg(temp.path().join("missing-manifest.sqlite"))
        .arg("--context")
        .arg(&context_path)
        .arg("--stats")
        .arg(temp.path().join("stats.sqlite"))
        .output()
        .unwrap();
    assert!(!missing_stats_manifest.status.success());
    assert!(String::from_utf8_lossy(&missing_stats_manifest.stderr)
        .contains("--stats and --stats-manifest must be supplied together"));

    let source_before = std::fs::read(&snapshot_path).unwrap();
    let collision = Command::new(executable())
        .arg("facts")
        .arg(&snapshot_path)
        .arg("--out")
        .arg(&snapshot_path)
        .arg("--context")
        .arg(&context_path)
        .arg("--json")
        .output()
        .unwrap();
    assert!(!collision.status.success());
    assert!(String::from_utf8_lossy(&collision.stderr).contains("input_output_collision"));
    assert_eq!(source_before, std::fs::read(&snapshot_path).unwrap());

    let context_before = std::fs::read(&context_path).unwrap();
    let context_collision = Command::new(executable())
        .arg("facts")
        .arg(&snapshot_path)
        .arg("--out")
        .arg(&context_path)
        .arg("--context")
        .arg(&context_path)
        .arg("--json")
        .output()
        .unwrap();
    assert!(!context_collision.status.success());
    assert!(String::from_utf8_lossy(&context_collision.stderr).contains("input_output_collision"));
    assert_eq!(context_before, std::fs::read(&context_path).unwrap());
}

/// A snapshot with an entry but no conversion provenance, so the compile is refused.
fn refusable_snapshot() -> Snapshot {
    let mut snapshot = minimal_snapshot();
    snapshot.lexicon.entries.push(LexEntry {
        guid: "00000000-0000-0000-0000-0000000000e1".into(),
        citation_form: vec![WsForm {
            ws: "qaa".into(),
            form: "a".into(),
        }],
        lexeme_morph_type: MorphType::Root,
        allomorphs: vec![Allomorph {
            guid: "00000000-0000-0000-0000-0000000000a1".into(),
            morph_type: MorphType::Root,
            is_abstract: false,
            forms: vec![WsForm {
                ws: "qaa".into(),
                form: "a".into(),
            }],
            environments: Vec::new(),
            positions: Vec::new(),
            stem_name: None,
            inflection_classes: Vec::new(),
            ms_env_features: None,
            ms_env_part_of_speech: None,
            process: None,
        }],
        msas: vec![Msa::Stem {
            guid: "00000000-0000-0000-0000-0000000000b1".into(),
            part_of_speech: Some("00000000-0000-0000-0000-0000000000c1".into()),
            inflection_class: None,
            features: None,
            exception_features: Vec::new(),
            from_parts_of_speech: Vec::new(),
            slots: Vec::new(),
        }],
        senses: Vec::new(),
        entry_refs: Vec::new(),
    });
    snapshot.conversion_provenance = Default::default();
    snapshot
}

struct FactsRun {
    _temp: tempfile::TempDir,
    status_ok: bool,
    response: serde_json::Value,
    output: std::path::PathBuf,
}

fn run_facts(snapshot: &Snapshot, extra: &[&str]) -> FactsRun {
    let temp = tempfile::tempdir().unwrap();
    let snapshot_path = temp.path().join("snapshot.json");
    let context_path = temp.path().join("context.json");
    let output = temp.path().join("facts.sqlite");
    std::fs::write(&snapshot_path, snapshot.to_json()).unwrap();
    write_context(&context_path);
    let result = Command::new(executable())
        .arg("facts")
        .arg(&snapshot_path)
        .arg("--out")
        .arg(&output)
        .arg("--context")
        .arg(&context_path)
        .args(extra)
        .arg("--json")
        .output()
        .unwrap();
    let response = serde_json::from_slice(&result.stdout).unwrap_or(serde_json::Value::Null);
    FactsRun {
        status_ok: result.status.success(),
        response,
        output,
        _temp: temp,
    }
}

/// `artifact_meta.light_omissions` and the status of one section, read from the written file.
fn light_marks(path: &Path, section: &str) -> (Option<String>, String) {
    let db = rusqlite::Connection::open(path).unwrap();
    let has_column: i64 = db
        .query_row(
            "SELECT count(*) FROM pragma_table_info('artifact_meta') WHERE name='light_omissions'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let omissions = (has_column == 1).then(|| {
        db.query_row("SELECT light_omissions FROM artifact_meta", [], |row| {
            row.get::<_, String>(0)
        })
        .unwrap()
    });
    let status = db
        .query_row(
            "SELECT status FROM artifact_section WHERE section=?1",
            [section],
            |row| row.get::<_, String>(0),
        )
        .unwrap();
    (omissions, status)
}

#[test]
fn facts_light_flag_writes_a_declared_light_file_and_the_default_stays_full() {
    let full = run_facts(&minimal_snapshot(), &[]);
    let light = run_facts(&minimal_snapshot(), &["--light"]);
    assert!(full.status_ok && light.status_ok);
    assert_eq!(full.response["grammarHash"], light.response["grammarHash"]);
    assert_eq!(
        full.response["sourceSha256"],
        light.response["sourceSha256"]
    );
    let (full_omissions, full_inventory) = light_marks(&full.output, "conversion_inventory");
    let (light_omissions, light_inventory) = light_marks(&light.output, "conversion_inventory");
    assert_eq!(
        full_omissions, None,
        "a full file has no light_omissions column"
    );
    assert_eq!(full_inventory, "complete");
    let omissions = light_omissions.expect("a light file declares what it omits");
    assert!(omissions.contains("conversion_item") && omissions.contains("load_fact"));
    assert_eq!(
        light_inventory, "partial",
        "conversion_inventory covers the cut conversion_item rows"
    );
    let statuses = |response: &serde_json::Value| response["sections"].clone();
    assert_ne!(statuses(&full.response), statuses(&light.response));
}

#[test]
fn facts_light_flag_still_publishes_a_refused_compile_and_declares_it() {
    let full = run_facts(&refusable_snapshot(), &[]);
    let light = run_facts(&refusable_snapshot(), &["--light"]);
    assert!(
        !full.status_ok && !light.status_ok,
        "a refused compile exits unsuccessfully"
    );
    assert_eq!(full.response["compileStatus"], "refused");
    assert_eq!(light.response["compileStatus"], "refused");
    assert_eq!(full.response["grammarHash"], light.response["grammarHash"]);
    let (omissions, grammar) = light_marks(&light.output, "effective_grammar");
    assert!(omissions.is_some_and(|text| text.contains("conversion_item")));
    assert_eq!(
        grammar, "unavailable",
        "a refused compile keeps its status under --light"
    );
    assert_eq!(
        light_marks(&full.output, "effective_grammar").1,
        "unavailable"
    );
}

#[test]
fn facts_light_flag_does_not_waive_the_paired_stats_flags() {
    let run = run_facts(
        &minimal_snapshot(),
        &["--light", "--stats", "/nonexistent/stats.sqlite"],
    );
    assert!(!run.status_ok);
    assert!(
        !run.output.exists(),
        "a refused argument list writes nothing"
    );
}
