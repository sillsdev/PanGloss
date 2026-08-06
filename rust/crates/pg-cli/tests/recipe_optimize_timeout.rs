use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn repo_file(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("..")
        .join(relative)
}

#[test]
fn recipe_optimize_kills_slow_child_and_writes_non_certifying_status() {
    let tag = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let out = std::env::temp_dir().join(format!("pangloss-recipe-timeout-{tag}"));
    let status = Command::new(env!("CARGO_BIN_EXE_pangloss"))
        .args([
            "recipe-optimize",
            "missing.xml",
            "missing.words",
            out.to_str().unwrap(),
            "--elapsed-ns",
            "10000000",
        ])
        .env("PANGLOSS_RECIPE_OPTIMIZE_TEST_SLEEP_MS", "5000")
        .status()
        .unwrap();
    assert!(!status.success());
    // No wall-clock assertion needed: a status.json reading budget-exhausted is only reachable via the deadline-kill path, since letting the 5000ms sleep finish would instead return a bare exit error with no status.json/partial-report.json at all.
    let status_json = fs::read_to_string(out.join("status.json")).unwrap();
    assert!(status_json.contains("budget-exhausted"));
    assert!(status_json.contains("\"certifying\": false"));
    let partial: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(out.join("partial-report.json")).unwrap())
            .unwrap();
    assert_eq!(partial["termination"], "budget-exhausted");
    // Names WHICH kill path fired: the memory-limit path also writes budget-exhausted, so this catches the deadline check silently being replaced by the memory guard.
    assert_eq!(partial["reason"], "elapsed deadline exceeded");
    assert_eq!(partial["certifying"], false);
    assert!(partial["winner"].is_null());
    assert_eq!(partial["candidates"].as_array().unwrap().len(), 0);
    assert!(partial["search"]["unexplored"].is_null());
    let _ = fs::remove_dir_all(out);
}

#[test]
fn recipe_optimize_deadline_banks_complete_rows_but_not_a_malformed_tail() {
    let tag = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("pangloss-recipe-banking-{tag}"));
    let out = root.join("out");
    let words = root.join("words.txt");
    fs::create_dir_all(&root).unwrap();
    fs::write(&words, "k\n").unwrap();
    let grammar = repo_file("conformance-staging/edge-cases/recipe-template-generic/grammar.xml");
    let progress = out.join("progress.jsonl");
    let mut child = Command::new(env!("CARGO_BIN_EXE_pangloss"))
        .args([
            "recipe-optimize",
            grammar.to_str().unwrap(),
            words.to_str().unwrap(),
            out.to_str().unwrap(),
            "--candidates",
            "1",
            "--evaluations",
            "2",
            "--elapsed-ns",
            "5000000000",
        ])
        .env(
            "PANGLOSS_RECIPE_OPTIMIZE_TEST_SLEEP_AFTER_PROGRESS_MS",
            "30000",
        )
        .spawn()
        .unwrap();

    let mut saw_complete_row = false;
    for _ in 0..200 {
        if fs::read_to_string(&progress)
            .map(|contents| contents.contains('\n'))
            .unwrap_or(false)
        {
            saw_complete_row = true;
            break;
        }
        if child.try_wait().unwrap().is_some() {
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    assert!(
        saw_complete_row,
        "the child must durably append a completed candidate before the deadline kill"
    );
    let mut tail = fs::OpenOptions::new().append(true).open(&progress).unwrap();
    writeln!(tail, "not-json").unwrap();
    tail.write_all(br#"{"id":"partial""#).unwrap();
    tail.flush().unwrap();

    let status = child.wait().unwrap();
    assert!(!status.success());
    let partial: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(out.join("partial-report.json")).unwrap())
            .unwrap();
    assert_eq!(partial["termination"], "budget-exhausted");
    assert_eq!(partial["reason"], "elapsed deadline exceeded");
    assert_eq!(partial["certifying"], false);
    assert!(partial["winner"].is_null());
    let candidates = partial["candidates"].as_array().unwrap();
    assert_eq!(candidates.len(), 1);
    assert!(candidates[0]["realized_strategy"]
        .as_str()
        .is_some_and(|strategy| !strategy.is_empty()));
    assert!(candidates[0]["id"]
        .as_str()
        .is_some_and(|id| !id.is_empty()));
    assert!(
        candidates
            .iter()
            .all(|candidate| candidate["id"] != "partial"),
        "malformed and truncated progress rows must not be banked"
    );
    let _ = fs::remove_dir_all(root);
}
