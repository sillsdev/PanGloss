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
    batch_output: Output,
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
        let batch_output = succeeds(run(&args.iter().map(PathBuf::as_path).collect::<Vec<_>>()));
        Self {
            _dir: dir,
            snapshot,
            context,
            cache,
            manifest,
            facts,
            batch_output,
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
        assert!(manifest.get("profile").is_none());
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
fn frozen_word_only_stats_publish_word_reports_and_refuse_facts_ingestion() {
    let measurement = Measurement::new(&["--stats-profile", "word-only"]);
    let manifest = measurement.manifest();
    assert_eq!(manifest["profile"], "word-only");
    assert_eq!(
        manifest["cache"]["schema_version"],
        pg_stats::WORD_ONLY_SCHEMA_VERSION
    );
    assert_eq!(manifest["completion"]["complete"], 1);
    assert!(!String::from_utf8_lossy(&measurement.batch_output.stdout)
        .contains("FINAL_TEMPLATE_STATS_NEWLY_ANALYZED"));
    let output = succeeds(run(&[
        &text("stats"),
        &measurement.snapshot,
        &text("--cache"),
        &measurement.cache,
        &text("--group"),
        &text("word"),
        &text("--format"),
        &text("jsonl"),
    ]));
    let rows: Vec<Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["profile"], "word-only");
    assert_eq!(
        rows[0]["unavailable_views"],
        json!([
            "object",
            "allomorph",
            "morpheme",
            "group",
            "never-fires",
            "gate",
            "stratum"
        ])
    );
    assert_eq!(rows[1]["form"], "a");
    assert!(rows[1]["elapsed_ns"].as_u64().unwrap() > 0);
    assert!(rows[1]["work"].as_u64().unwrap() > 0);

    let report_args = [
        text("stats"),
        measurement.snapshot.clone(),
        text("--cache"),
        measurement.cache.clone(),
    ];
    let mut empty = report_args.to_vec();
    empty.extend(
        [
            "--group",
            "word",
            "--word",
            "synthetic-absent",
            "--format",
            "jsonl",
        ]
        .map(text),
    );
    let output = succeeds(run(&empty.iter().map(PathBuf::as_path).collect::<Vec<_>>()));
    let meta: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        meta["totals"],
        json!({"rows": 0, "rows_shown": 0, "time_ns": 0, "work": 0})
    );
    assert_eq!(meta["profile"], "word-only");
    for (group, exit) in [("gate", 3), ("object", 3), ("invalid", 1)] {
        let mut args = report_args.to_vec();
        args.extend([text("--group"), text(group), text("--format=jsonl")]);
        let result = run(&args.iter().map(PathBuf::as_path).collect::<Vec<_>>());
        assert_eq!(result.status.code(), Some(exit));
        assert!(result.stdout.is_empty());
        if exit == 3 {
            let error = String::from_utf8_lossy(&result.stderr);
            assert!(error.contains("missing_object_facts"), "{error}");
        }
    }
    let refused = measurement.read_facts();
    assert!(!refused.status.success());
    let error = String::from_utf8_lossy(&refused.stderr);
    assert!(error.contains("missing_object_facts"), "{error}");
    assert!(!measurement.facts.exists());
}

#[test]
fn separate_default_stats_paths_and_cross_profile_refusal_preserve_outputs() {
    let measurement = Measurement::new(&[]);
    let dir = measurement._dir.path();
    let data = dir.join("data");
    let words = dir.join("words.txt");
    let tsv = dir.join("batch.tsv");
    let command = |args: &[&Path]| {
        Command::new(env!("CARGO_BIN_EXE_pangloss"))
            .env("XDG_DATA_HOME", &data)
            .env("LOCALAPPDATA", &data)
            .args(args)
            .output()
            .unwrap()
    };
    let batch: [&Path; 6] = [
        &text("batch"),
        &measurement.snapshot,
        &words,
        &tsv,
        &text("--threads=1"),
        &text("--stats"),
    ];
    let mut scoped = batch.to_vec();
    let scoped_profile = text("--stats-profile=word-only");
    scoped.push(&scoped_profile);
    succeeds(command(&scoped));
    succeeds(command(&batch));

    let mut projects = std::fs::read_dir(data.join("PanGloss/stats")).unwrap();
    let project = projects.next().unwrap().unwrap().path();
    assert!(projects.next().is_none());
    let full = project.join("cache.sqlite3");
    let word_only = project.join("word-only.sqlite3");
    for (path, expected) in [
        (&full, pg_stats::SCHEMA_VERSION),
        (&word_only, pg_stats::WORD_ONLY_SCHEMA_VERSION),
    ] {
        let conn = rusqlite::Connection::open(path).unwrap();
        let stored: i64 = conn
            .query_row("SELECT schema_version FROM run", [], |row| row.get(0))
            .unwrap();
        assert_eq!(stored, expected);
    }
    let report = succeeds(command(&[
        &text("stats"),
        &measurement.snapshot,
        &text("--stats-profile=word-only"),
        &text("--group=word"),
        &text("--format=jsonl"),
    ]));
    let meta: Value =
        serde_json::from_slice(report.stdout.split(|&b| b == b'\n').next().unwrap()).unwrap();
    assert_eq!(meta["profile"], "word-only");

    for (path, other) in [(&full, "word-only"), (&word_only, "full")] {
        let restored = dir.join(format!("restored-{other}.sqlite"));
        succeeds(run(&[
            &text("stats-cache"),
            &text("unpack"),
            &text("--cache"),
            path,
            &text("--out"),
            &restored,
        ]));
        let before = std::fs::read(&restored).unwrap();
        let previous_tsv = std::fs::read(&tsv).unwrap();
        let mut crossed = batch.to_vec();
        let profile = text(&format!("--stats-profile={other}"));
        let cache_flag = text("--cache");
        crossed.extend([profile.as_path(), cache_flag.as_path(), restored.as_path()]);
        let refused = command(&crossed);
        assert!(!refused.status.success());
        assert!(String::from_utf8_lossy(&refused.stderr).contains("profile, requested"));
        assert_eq!(std::fs::read(&tsv).unwrap(), previous_tsv);
        assert_eq!(std::fs::read(&restored).unwrap(), before);
    }
}

#[test]
fn attribution_reports_refuse_when_a_word_owns_a_scoped_run_in_a_full_cache() {
    let measurement = Measurement::new(&[]);
    let conn = rusqlite::Connection::open(&measurement.cache).unwrap();
    conn.execute_batch(
        "INSERT INTO run
         SELECT run_id + 1, schema_version, counter_semantics, build_info, fwdata_path,
                grammar_hash, engine, options_hash, options_json, created_utc, 0, 0, work_cap
         FROM run;
         UPDATE run SET schema_version = 15 WHERE run_id = (SELECT MIN(run_id) FROM run);
         DELETE FROM fact;
         DELETE FROM gate_fact;
         UPDATE word SET gates_measured = 0;",
    )
    .unwrap();
    assert!(matches!(
        pg_stats::per_object_report(&conn, &Default::default()),
        Err(pg_stats::StatsError::WordOnlyFactsUnavailable)
    ));
    let report = |group| {
        run(&[
            &text("stats"),
            &measurement.snapshot,
            &text("--cache"),
            &measurement.cache,
            &text("--group"),
            &text(group),
            &text("--format=jsonl"),
            &text("--word=a"),
        ])
    };
    for group in ["gate", "object"] {
        let refused = report(group);
        assert_eq!(refused.status.code(), Some(3));
        assert!(refused.stdout.is_empty());
    }
    succeeds(report("word"));
}

#[test]
fn empty_initialized_caches_emit_jsonl_metadata() {
    let dir = tempfile::tempdir().unwrap();
    for (profile, open) in [
        ("full", pg_stats::StatsCache::open as fn(&Path, &str) -> _),
        ("word-only", pg_stats::StatsCache::open_word_only),
    ] {
        let cache = dir.path().join(format!("{profile}.sqlite"));
        drop(open(&cache, "synthetic").unwrap());
        let out = dir.path().join(format!("{profile}.jsonl"));
        let output = succeeds(run(&[
            &text("stats"),
            &text("grammar.xml"),
            &text("--group=word"),
            &text("--format=jsonl"),
            &text("--cache"),
            &cache,
            &text("--out"),
            &out,
        ]));
        assert!(output.stdout.is_empty());
        let meta: Value = serde_json::from_slice(&std::fs::read(out).unwrap()).unwrap();
        assert_eq!(meta["meta"], true);
        assert_eq!(meta["orientation"], "word");
        assert_eq!(meta["empty_reason"], "no_runs");
        assert_eq!(meta["profile"].as_str().unwrap_or("full"), profile);
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
