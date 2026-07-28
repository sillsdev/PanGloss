use std::fs;
use std::process::Command;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[test]
fn recipe_optimize_kills_slow_child_and_writes_non_certifying_status() {
    let tag = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let out = std::env::temp_dir().join(format!("pangloss-recipe-timeout-{tag}"));
    let start = Instant::now();
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
    assert!(start.elapsed() < Duration::from_secs(2));
    let status_json = fs::read_to_string(out.join("status.json")).unwrap();
    assert!(status_json.contains("budget-exhausted"));
    assert!(status_json.contains("\"certifying\": false"));
    let partial: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(out.join("partial-report.json")).unwrap())
            .unwrap();
    assert_eq!(partial["termination"], "budget-exhausted");
    assert_eq!(partial["certifying"], false);
    assert!(partial["winner"].is_null());
    assert_eq!(partial["candidates"].as_array().unwrap().len(), 0);
    assert!(partial["search"]["unexplored"].is_null());
    let _ = fs::remove_dir_all(out);
}
