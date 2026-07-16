//! The upstream foma regression suite, ported from `foma/tests/run.sh`.
//!
//! The `.foma` scripts under `tests/data/` are verbatim copies of the upstream
//! regression inputs (each pins a specific historical crash/leak). run.sh runs
//! them through `foma -q -f <script>` and checks exit status (and, for the error
//! case, that a syntax error is reported). We reproduce each of those six checks
//! against OUR foma binary here.
//!
//! These are script-level regressions, not single-function verifications, so
//! they carry NO `[spec:…/test]` facets — they are valuable as-is, exactly as
//! the upstream suite.

use std::path::PathBuf;
use std::process::{Command, Output};

fn data(name: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("tests");
    p.push("data");
    p.push(name);
    p
}

/// `foma -q -f <data/name>` → captured output + status.
fn run_script(name: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_foma"))
        .arg("-q")
        .arg("-f")
        .arg(data(name))
        .output()
        .expect("failed to spawn foma binary")
}

// run.sh line 4: `foma -q -f test-leaky-redefine.foma || exit 1`
// (defining the same name twice must not crash — a leak regression).
#[test]
fn leaky_redefine_exits_zero() {
    assert!(run_script("test-leaky-redefine.foma").status.success());
}

// run.sh line 5: `foma -q -f test-segfault-eliminate.foma || exit 1`
// (eliminate flags on a 0-path transducer used to segfault).
#[test]
fn segfault_eliminate_exits_zero() {
    assert!(run_script("test-segfault-eliminate.foma").status.success());
}

// run.sh lines 6-9: `foma -q -f test-error-rendering.foma 2>&1 | grep -q 'syntax
// error'` — the run must REPORT a syntax error (for `regex ||;`).
//
// DEVIATION from run.sh: our parser (nfst-xre) emits the diagnostic
// "Syntax error: …" (capital S), rather than the lowercase "syntax error" the
// upstream grep matches. The diagnostic is now a `tracing::error!` event routed
// to stderr by the CLI subscriber (rendered as "ERROR Syntax error: …"); we
// assert it case-insensitively contains "syntax error". The whole -f run still
// exits 0.
#[test]
fn error_rendering_reports_syntax_error() {
    let out = run_script("test-error-rendering.foma");
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        combined.contains("Syntax error"),
        "expected a 'Syntax error' diagnostic, got: {combined:?}"
    );
    // Matches run.sh's grep 'syntax error' case-insensitively.
    assert!(combined.to_lowercase().contains("syntax error"));
    // -f always exits 0 regardless of the parse error.
    assert!(out.status.success());
}

// run.sh line 10: `foma -q -f test-segfault-long-name > /dev/null || exit 1`
// (a >40-char defined name written via `save defined` — the fixed-size name
// field overflow regression). Note: no ".foma" extension upstream.
#[test]
fn segfault_long_name_exits_zero() {
    assert!(run_script("test-segfault-long-name").status.success());
}

// run.sh line 11: `foma -q -f test-segfault-empty-fst.foma > /dev/null || exit 1`
// (`save defined` with an implicitly-defined empty net regression).
#[test]
fn segfault_empty_fst_exits_zero() {
    assert!(run_script("test-segfault-empty-fst.foma").status.success());
}

// run.sh line 12: `foma -q -f test-leaky-test.foma > /dev/null || exit 1`
// (a battery of `test …` / `print shortest-string` commands must not crash).
#[test]
fn leaky_test_exits_zero() {
    assert!(run_script("test-leaky-test.foma").status.success());
}
