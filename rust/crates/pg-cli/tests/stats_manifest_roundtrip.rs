use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::{json, Value};

fn run(args: &[&Path]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_pangloss"))
        .args(args)
        .output()
        .unwrap()
}

fn text(value: &str) -> PathBuf {
    PathBuf::from(value)
}

fn succeeds(output: Output) -> Output {
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

struct Measurement {
    _dir: tempfile::TempDir,
    snapshot: PathBuf,
    context: PathBuf,
    cache: PathBuf,
    manifest: PathBuf,
    facts: PathBuf,
}

impl Measurement {
    fn new(options: &[&str]) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("project.fwdata");
        let snapshot = dir.path().join("snapshot.json");
        let words = dir.path().join("words.txt");
        let context = dir.path().join("context.json");
        let cache = dir.path().join("cache.sqlite");
        let manifest = dir.path().join("manifest.json");
        let facts = dir.path().join("facts.sqlite");
        let source = include_str!("../../pg-fwdata/tests/data/fixture.fwdata");
        let dangling = "<objsur guid=\"00000000-0000-0000-0000-0000000000ff\" t=\"r\" />";
        assert_eq!(source.matches(dangling).count(), 1);
        std::fs::write(&project, source.replace(dangling, "")).unwrap();
        std::fs::write(&words, "a\n").unwrap();
        std::fs::write(
            &context,
            br#"{"format":"pangloss-facts-context","version":1,"baselineToken":{"id":"manifest-roundtrip"},"inputKind":"baseline","dryRunDigest":null}"#,
        )
        .unwrap();
        succeeds(run(&[&text("import"), &project, &snapshot]));
        let mut args = vec![
            text("batch"),
            snapshot.clone(),
            words,
            dir.path().join("batch.tsv"),
            text("--threads"),
            text("1"),
            text("--stats"),
            text("--cache"),
            cache.clone(),
            text("--stats-manifest"),
            manifest.clone(),
        ];
        args.extend(options.iter().map(|option| text(option)));
        succeeds(run(&args.iter().map(PathBuf::as_path).collect::<Vec<_>>()));
        Self {
            _dir: dir,
            snapshot,
            context,
            cache,
            manifest,
            facts,
        }
    }

    fn read_facts(&self) -> Output {
        run(&[
            &text("facts"),
            &self.snapshot,
            &text("--out"),
            &self.facts,
            &text("--context"),
            &self.context,
            &text("--stats"),
            &self.cache,
            &text("--stats-manifest"),
            &self.manifest,
            &text("--light"),
            &text("--json"),
        ])
    }

    fn manifest(&self) -> Value {
        serde_json::from_slice(&std::fs::read(&self.manifest).unwrap()).unwrap()
    }

    fn write_manifest(&self, value: &Value) {
        std::fs::write(&self.manifest, serde_json::to_vec(value).unwrap()).unwrap();
    }
}

#[test]
fn real_batch_manifest_roundtrips_through_facts_and_stats_replay() {
    for options in [
        vec![],
        vec!["--word-mem-cap-mib", "0"],
        vec!["--word-mem-cap-mib", "8", "--stats-compact"],
        vec!["--word-mem-cap-mib", "0", "--cache-pack"],
        vec![
            "--stats-compact",
            "--cache-pack",
            "--guess",
            "--work-cap",
            "unbounded",
            "--word-timeout-ms",
            "10000",
            "--always-enforce-final-templates",
        ],
    ] {
        let measurement = Measurement::new(&options);
        let manifest = measurement.manifest();
        let expected_cap = match options
            .windows(2)
            .find(|pair| pair[0] == "--word-mem-cap-mib")
        {
            Some(pair) if pair[1] == "0" => Value::Null,
            Some(pair) => json!(pair[1].parse::<usize>().unwrap() * 1024 * 1024),
            None => json!(pg_parse::morpher::DEFAULT_WORD_MEM_CAP_BYTES),
        };
        assert_eq!(manifest["batch"]["word_mem_cap"], expected_cap);
        let run_options: Value =
            serde_json::from_str(manifest["run"]["options_json"].as_str().unwrap()).unwrap();
        assert_eq!(run_options["word_mem_cap"], expected_cap);
        let result = succeeds(measurement.read_facts());
        let response: Value = serde_json::from_slice(&result.stdout).unwrap();
        assert_eq!(response["compileStatus"], "completed");
        let db = rusqlite::Connection::open(&measurement.facts).unwrap();
        let batch: String = db
            .query_row("SELECT batch_options_json FROM stats_run", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&batch).unwrap(),
            manifest["batch"]
        );
        let count: i64 = db
            .query_row("SELECT count(*) FROM stats_word", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
        if manifest["cache"]["schema_version"] == pg_stats::SCHEMA_VERSION {
            let subrules: i64 = db
                .query_row(
                    "SELECT count(*) FROM stats_allomorph WHERE key LIKE '%#subrule:%'",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert!(
                subrules > 0,
                "full caches must exercise compound-subrule locators"
            );
        }
        let replay = succeeds(run(&[
            &text("stats"),
            &measurement.snapshot,
            &text("--cache"),
            &measurement.cache,
            &text("--group"),
            &text("object"),
            &text("--format"),
            &text("jsonl"),
        ]));
        assert!(!replay.stdout.is_empty());
    }
}

#[test]
fn facts_rejects_changed_cap_and_unknown_writer_fields() {
    let measurement = Measurement::new(&[]);
    let original = measurement.manifest();
    for (field, value, diagnostic) in [
        ("word_mem_cap", json!(1), "stats batch options do not match"),
        (
            "word_mem_cap",
            Value::Null,
            "stats batch options do not match",
        ),
        ("unexpected", json!(true), "unknown field `unexpected`"),
        ("word_mem_cap", json!("unbounded"), "invalid type"),
    ] {
        let mut manifest = original.clone();
        manifest["batch"][field] = value;
        measurement.write_manifest(&manifest);
        let result = measurement.read_facts();
        assert!(!result.status.success());
        let stderr = String::from_utf8_lossy(&result.stderr);
        assert!(stderr.contains(diagnostic), "{stderr}");
        assert!(!measurement.facts.exists());
    }
}

#[test]
fn batch_and_facts_without_stats_preserve_the_optional_measurement_contract() {
    let measurement = Measurement::new(&[]);
    let dir = measurement._dir.path();
    succeeds(run(&[
        &text("batch"),
        &measurement.snapshot,
        &dir.join("words.txt"),
        &dir.join("plain.tsv"),
        &text("--threads"),
        &text("1"),
        &text("--word-mem-cap-mib"),
        &text("0"),
    ]));
    let plain_facts = dir.join("plain.sqlite");
    succeeds(run(&[
        &text("facts"),
        &measurement.snapshot,
        &text("--out"),
        &plain_facts,
        &text("--context"),
        &measurement.context,
        &text("--json"),
    ]));
    let db = rusqlite::Connection::open(plain_facts).unwrap();
    let count: i64 = db
        .query_row("SELECT count(*) FROM stats_run", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 0);

    for (options, diagnostic) in [
        (vec![], "--stats-manifest requires --stats"),
        (
            vec![text("--stats")],
            "--stats-manifest requires an explicit --cache",
        ),
    ] {
        let unpublished = dir.join("unpublished.json");
        let mut args = vec![
            text("batch"),
            measurement.snapshot.clone(),
            dir.join("words.txt"),
            dir.join("refused.tsv"),
            text("--stats-manifest"),
            unpublished.clone(),
        ];
        args.extend(options);
        let output = run(&args.iter().map(PathBuf::as_path).collect::<Vec<_>>());
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains(diagnostic));
        assert!(!unpublished.exists());
        assert!(!dir.join("refused.tsv").exists());
    }
}
