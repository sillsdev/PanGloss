use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

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
    // No wall-clock assertion, and none is needed -- the artifacts below already prove the child was
    // KILLED rather than waited out, more strictly than a stopwatch could.
    //
    // If the supervisor had let the 5000ms sleep finish, `try_wait` would have returned the child's
    // own non-success exit (it goes on to fail loading `missing.xml`), and that path returns
    // `recipe worker exited with ...` WITHOUT writing `status.json` or `partial-report.json` at all.
    // So reading a `status.json` that says `budget-exhausted` is only possible on the deadline-kill
    // path. This used to also assert `start.elapsed() < 2s`, which measured the machine rather than
    // the supervisor: it failed at 3.59s purely because a concurrent build held the CPU at 100% and
    // builds here run `BelowNormal` so interactive daemons stay responsive.
    let status_json = fs::read_to_string(out.join("status.json")).unwrap();
    assert!(status_json.contains("budget-exhausted"));
    assert!(status_json.contains("\"certifying\": false"));
    let partial: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(out.join("partial-report.json")).unwrap())
            .unwrap();
    assert_eq!(partial["termination"], "budget-exhausted");
    // WHICH kill path, not merely that one fired. The memory-limit path also writes
    // `budget-exhausted`, so without this the test would pass if the deadline check silently stopped
    // working and the memory guard happened to trip instead -- a gap the old timing assertion did not
    // cover either.
    assert_eq!(partial["reason"], "elapsed deadline exceeded");
    assert_eq!(partial["certifying"], false);
    assert!(partial["winner"].is_null());
    assert_eq!(partial["candidates"].as_array().unwrap().len(), 0);
    assert!(partial["search"]["unexplored"].is_null());
    let _ = fs::remove_dir_all(out);
}
