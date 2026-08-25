//! Test-support-only executable for `tests/worker_execution_limits_contract.rs`: a minimal process wrapper around `run_worker_child` so tests can exercise supervisor timeout and crash behavior with a real child process. Never part of any shipped product surface. Two env vars, read only by this binary's `main` (never by production code): `PANGLOSS_WORKER_TEST_SLEEP_MS` sleeps before reading the request frame, so a test can arm a shorter deadline and observe a real `Child::kill()`; `PANGLOSS_WORKER_TEST_CRASH=1` exits immediately with a distinctive code before writing a result frame, so a test can observe a real `ChildCrashed` classification.

// Cargo auto-discovers this bin target for every check target including wasm32, so give wasm32 a trivial no-op main rather than pulling the non-wasm-only pg_foma::worker dependency into that build.
#[cfg(target_arch = "wasm32")]
fn main() {}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    if std::env::var("PANGLOSS_WORKER_TEST_CRASH").as_deref() == Ok("1") {
        std::process::exit(97);
    }
    if let Ok(ms) = std::env::var("PANGLOSS_WORKER_TEST_SLEEP_MS") {
        if let Ok(ms) = ms.parse::<u64>() {
            std::thread::sleep(std::time::Duration::from_millis(ms));
        }
    }

    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    if let Err(e) = pg_foma::worker::run_worker_child(stdin.lock(), stdout.lock()) {
        eprintln!("worker_test_child: I/O error: {e}");
        std::process::exit(2);
    }
}
