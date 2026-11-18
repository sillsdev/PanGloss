//! Engine selection through the shipped batch command, including its feature-off contract.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

const BIN: &str = env!("CARGO_BIN_EXE_pangloss");

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../conformance-staging/edge-cases/loader-isactive/grammar.xml")
}

fn batch(dir: &Path, extra: &[&str]) -> Output {
    Command::new(BIN)
        .arg("batch")
        .arg(fixture())
        .arg(dir.join("words.txt"))
        .arg(dir.join("out.tsv"))
        .args(extra)
        .output()
        .expect("run shipped batch command")
}

fn rows(dir: &Path) -> Vec<Vec<String>> {
    fs::read_to_string(dir.join("out.tsv"))
        .unwrap()
        .lines()
        .map(|line| line.split('\t').map(str::to_string).collect::<Vec<_>>())
        .filter(|fields| fields.len() > 3)
        .collect()
}

#[test]
fn engine_help_exposes_the_fst_containment_limit() {
    let output = Command::new(BIN).arg("--help").output().unwrap();
    let help = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    for text in [
        "--engine hc|fst",
        "foma-tools",
        "no per-word deadline or process RSS bound",
        "external watchdog with a deadline",
        "memory ceiling",
    ] {
        assert!(help.contains(text), "missing {text}: {help}");
    }
    let output = Command::new(BIN).arg("--describe").output().unwrap();
    assert!(output.status.success());
    let described: Value = serde_json::from_slice(&output.stdout).unwrap();
    let batch = described["commands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|command| command["name"] == "batch")
        .unwrap();
    let engine = batch["flags"]
        .as_array()
        .unwrap()
        .iter()
        .find(|flag| flag["name"] == "--engine")
        .unwrap();
    let summary = engine["summary"].as_str().unwrap();
    assert!(summary.contains("no per-word deadline or process RSS bound"));
    assert!(summary.contains("external watchdog"));
}

#[test]
fn explicit_hc_adds_only_completion_metadata_in_both_writer_modes() {
    for threads in ["1", "2"] {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("words.txt"), "kat\nsod\ntat\n").unwrap();
        let plain = batch(dir.path(), &["--threads", threads]);
        assert!(
            plain.status.success(),
            "{}",
            String::from_utf8_lossy(&plain.stderr)
        );
        let default = rows(dir.path());
        assert!(default.iter().all(|row| row.len() == 5));
        for syntax in [vec!["--engine=hc"], vec!["--engine", "hc"]] {
            let mut args = vec!["--threads", threads];
            args.extend(syntax);
            let explicit = batch(dir.path(), &args);
            assert!(
                explicit.status.success(),
                "{}",
                String::from_utf8_lossy(&explicit.stderr)
            );
            let selected = rows(dir.path());
            assert_eq!(selected.len(), default.len());
            for (plain, explicit) in default.iter().zip(selected) {
                assert_eq!(explicit.len(), 6);
                for index in [0, 1, 3, 4] {
                    assert_eq!(plain[index], explicit[index]);
                }
                let completion: Value = serde_json::from_str(&explicit[5]).unwrap();
                assert_eq!(completion["engine"], "hc");
                assert_eq!(completion["capped"], false);
                assert_eq!(completion["timedOut"], false);
            }
        }
    }
}

#[test]
fn engine_values_and_feature_off_are_explicit_errors() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("out.tsv"), "preserve me").unwrap();
    for value in ["foma", "typo", ""] {
        let output = batch(dir.path(), &[&format!("--engine={value}")]);
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("expected hc|fst"));
    }
    #[cfg(not(feature = "foma-tools"))]
    {
        let output = batch(dir.path(), &["--engine=fst"]);
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("built without foma-tools"));
    }
    assert_eq!(
        fs::read_to_string(dir.path().join("out.tsv")).unwrap(),
        "preserve me"
    );
}

#[test]
fn hc_completion_flags_follow_actual_stops_and_keep_cost_columns_last() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("words.txt"), "kat\n").unwrap();
    for (control, value, status, flag) in [
        ("--work-cap", "1", "CAP", "capped"),
        ("--word-timeout-ms", "0", "TIMEOUT", "timedOut"),
    ] {
        let output = batch(
            dir.path(),
            &[
                "--engine=hc",
                "--threads=1",
                control,
                value,
                "--guess",
                "--partials=1",
                "--cost-column",
            ],
        );
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let rows = rows(dir.path());
        assert_eq!(rows[0].len(), 10);
        assert_eq!(rows[0][3], status);
        assert_eq!(rows[0][5], "false");
        let completion: Value = serde_json::from_str(&rows[0][7]).unwrap();
        assert_eq!(completion[flag], true);
        assert!(rows[0][8].parse::<usize>().is_ok());
        assert!(rows[0][9].parse::<u64>().is_ok());
    }
}

#[cfg(feature = "foma-tools")]
const FST_ARGS: &[&str] = &[
    "--engine=fst",
    "--threads=1",
    "--work-cap=unbounded",
    "--word-mem-cap-mib=0",
    "--capture-cap-mib=unbounded",
    "--guess=false",
];

#[cfg(feature = "foma-tools")]
fn fst_refuses_or_matches_hc(category: &str, name: &str, positive: &str) {
    let fixture = pg_conformance_fixtures::require_fixture(category, name);
    let dir = tempfile::tempdir().unwrap();
    let cases = fixture.load_words_yaml();
    assert!(!cases.words.is_empty());
    let words = dir.path().join("words.txt");
    fs::write(
        &words,
        cases
            .words
            .iter()
            .map(|case| case.word.as_str())
            .collect::<Vec<_>>()
            .join("\n"),
    )
    .unwrap();
    let mut expected = Vec::new();
    for engine in ["hc", "fst"] {
        let output_dir = dir.path().join(engine);
        fs::create_dir(&output_dir).unwrap();
        let path = output_dir.join("out.tsv");
        let output = Command::new(BIN)
            .arg("batch")
            .arg(fixture.grammar_path())
            .arg(&words)
            .arg(&path)
            .args(&FST_ARGS[1..])
            .arg(format!("--engine={engine}"))
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        if engine == "fst" && !output.status.success() {
            assert!(stderr.contains("FST compile refusal"), "{stderr}");
            assert!(
                stderr.contains("CapabilityRefused") || stderr.contains("ConstructionRefused"),
                "{stderr}"
            );
            assert!(!path.exists(), "refused grammar wrote batch output");
            assert!(output.stdout.is_empty());
            assert!(!stderr.contains("PARSEELAPSED"), "{stderr}");
            return;
        }
        assert!(output.status.success(), "{engine}: {stderr}");
        let actual = rows(&output_dir);
        assert_eq!(actual.len(), cases.words.len());
        for row in &actual {
            assert_eq!(row[3], "ok", "incomplete {engine} row: {row:?}");
            let completion: Value = serde_json::from_str(&row[5]).unwrap();
            assert_eq!(completion["engine"], engine);
            for flag in [
                "capped",
                "timedOut",
                "captureCapped",
                "wordMemCapped",
                "invalidShape",
            ] {
                assert_eq!(completion[flag], false, "{row:?}");
            }
        }
        let signatures: Vec<_> = actual.iter().map(|row| row[4].clone()).collect();
        if engine == "hc" {
            assert!(signatures
                .iter()
                .any(|signature| signature.contains(positive)));
            expected = signatures;
        } else {
            assert_eq!(
                expected, signatures,
                "{category}/{name}: FST lost or added analyses"
            );
        }
    }
}

#[test]
#[cfg(feature = "foma-tools")]
fn fst_admission_final_template_partial_discriminators_refuses_or_matches_hc() {
    fst_refuses_or_matches_hc(
        "edge-cases",
        "final-template-partial-discriminators",
        "DAK+NONFINTAGA+GATE1+FINTAGA|daknagafa",
    );
}

#[test]
#[cfg(feature = "foma-tools")]
fn fst_admission_metathesis_phase_isolation_refuses_or_matches_hc() {
    fst_refuses_or_matches_hc(
        "languages",
        "metathesis-phase-isolation",
        "RDPL+HASA|hasaasa",
    );
}

#[test]
#[cfg(feature = "foma-tools")]
fn fst_admission_backend_ordered_generic_refuses_or_matches_hc() {
    fst_refuses_or_matches_hc("edge-cases", "backend-ordered-generic", "RDP+HASA|haasa");
}

#[test]
#[cfg(feature = "foma-tools")]
fn fst_batch_matches_hc_on_a_conformance_fixture_and_resumes() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("words.txt"), "kat\nsod\ntat\n🦀\n").unwrap();
    let hc = batch(dir.path(), &["--threads=1", "--engine=hc"]);
    assert!(
        hc.status.success(),
        "{}",
        String::from_utf8_lossy(&hc.stderr)
    );
    let expected = rows(dir.path());
    assert_ne!(expected[0][4], "-");
    let analyses = dir.path().join("analyses.jsonl");
    let mut args = FST_ARGS.to_vec();
    args.extend(["--analyses", analyses.to_str().unwrap()]);
    let fst = batch(dir.path(), &args);
    assert!(
        fst.status.success(),
        "{}",
        String::from_utf8_lossy(&fst.stderr)
    );
    let actual = rows(dir.path());
    assert_eq!(actual.len(), expected.len());
    for (hc, fst) in expected.iter().zip(&actual) {
        assert_eq!(fst.len(), 6);
        for index in [0, 1, 3, 4] {
            assert_eq!(hc[index], fst[index]);
        }
        let completion: Value = serde_json::from_str(&fst[5]).unwrap();
        assert_eq!(completion["engine"], "fst");
        assert_eq!(completion["capped"], false);
        assert_eq!(completion["timedOut"], false);
    }
    let sidecar: Vec<Value> = fs::read_to_string(analyses)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(sidecar.len(), 4);
    assert_eq!(sidecar[0]["completion"]["engine"], "fst");
    assert!(!sidecar[0]["unavailable"].as_array().unwrap().is_empty());
    let prefix = fs::read_to_string(dir.path().join("out.tsv")).unwrap();
    let mut args = FST_ARGS.to_vec();
    args.extend(["--start=2", "--no-flush"]);
    assert!(batch(dir.path(), &args).status.success());
    assert!(fs::read_to_string(dir.path().join("out.tsv"))
        .unwrap()
        .starts_with(&prefix));
    for (resumed, original) in rows(dir.path())[4..].iter().zip(&actual[2..]) {
        for index in [0, 1, 3, 4, 5] {
            assert_eq!(resumed[index], original[index]);
        }
    }
    let mut args = FST_ARGS.to_vec();
    args.push("--start=999");
    assert!(!batch(dir.path(), &args).status.success());
}

#[test]
#[cfg(feature = "foma-tools")]
fn fst_refuses_unhonoured_controls_before_touching_output() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("out.tsv"), "preserve me").unwrap();
    for option in [
        "--stats",
        "--stats-compact",
        "--cache-pack",
        "--cache=cache.db",
        "--stats-manifest=manifest.json",
        "--partials=1",
        "--prefilter=on",
        "--prefilter=auto",
        "--word-timeout-ms=1",
        "--work-cap=1",
        "--word-mem-cap-mib=1",
        "--capture-cap-mib=1",
        "--work-columns",
        "--cost-column",
        "--guess",
        "--always-enforce-final-templates",
        "--threads=2",
    ] {
        let mut args = FST_ARGS.to_vec();
        args.push(option);
        let output = batch(dir.path(), &args);
        assert!(!output.status.success(), "accepted {option}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("--engine=fst"), "{option}: {stderr}");
        assert_eq!(
            fs::read_to_string(dir.path().join("out.tsv")).unwrap(),
            "preserve me"
        );
    }
}

#[test]
#[cfg(feature = "foma-tools")]
fn fst_unadmitted_grammar_prints_typed_refusal_without_fallback_or_truncation() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("out.tsv"), "preserve me").unwrap();
    fs::write(dir.path().join("words.txt"), "a\n").unwrap();
    let grammar = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../pg-parse/tests/data/alpha-optional-environment.xml");
    let output = Command::new(BIN)
        .arg("batch")
        .arg(grammar)
        .arg(dir.path().join("words.txt"))
        .arg(dir.path().join("out.tsv"))
        .args(FST_ARGS)
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("FST compile refusal"), "{stderr}");
    assert!(
        stderr.contains("CapabilityRefused") || stderr.contains("ConstructionRefused"),
        "{stderr}"
    );
    assert!(
        !stderr.contains("PARSEELAPSED"),
        "refused grammar was parsed: {stderr}"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("out.tsv")).unwrap(),
        "preserve me"
    );
}

#[test]
#[cfg(feature = "foma-tools")]
fn fst_affix_batch_matches_complete_hc_signatures_and_sidecar_availability() {
    let fixture = pg_conformance_fixtures::require_fixture(
        "edge-cases",
        "shared-template-unconstrained-suffix",
    );
    let dir = tempfile::tempdir().unwrap();
    let cases = fixture.load_words_yaml();
    fs::write(
        dir.path().join("words.txt"),
        cases
            .words
            .iter()
            .map(|entry| entry.word.as_str())
            .collect::<Vec<_>>()
            .join("\n"),
    )
    .unwrap();
    let mut results = Vec::new();
    let mut sidecars = Vec::new();
    for engine in ["hc", "fst"] {
        let analyses = dir.path().join(format!("{engine}.jsonl"));
        let output = Command::new(BIN)
            .arg("batch")
            .arg(fixture.grammar_path())
            .arg(dir.path().join("words.txt"))
            .arg(dir.path().join("out.tsv"))
            .args([
                "--threads=1",
                "--work-cap=unbounded",
                "--word-mem-cap-mib=0",
                "--capture-cap-mib=unbounded",
                "--guess=false",
                "--engine",
                engine,
                "--analyses",
            ])
            .arg(&analyses)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{engine}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let rows = rows(dir.path());
        assert_eq!(rows.len(), cases.words.len());
        assert!(rows.iter().all(|row| row[3] == "ok"));
        assert!(rows.iter().any(|row| row[4].contains('+')));
        results.push(rows.iter().map(|row| row[4].clone()).collect::<Vec<_>>());
        sidecars.push(
            fs::read_to_string(analyses)
                .unwrap()
                .lines()
                .map(|line| {
                    let row: Value = serde_json::from_str(line).unwrap();
                    let mut analyses: Vec<_> = row["analyses"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|analysis| serde_json::to_string(analysis).unwrap())
                        .collect();
                    analyses.sort();
                    let mut unavailable: Vec<_> = row["unavailable"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|reason| reason.as_str().unwrap().to_string())
                        .collect();
                    unavailable.sort();
                    (analyses, unavailable)
                })
                .collect::<Vec<_>>(),
        );
    }
    assert_eq!(results[0], results[1]);
    assert_eq!(sidecars[0], sidecars[1]);
}
