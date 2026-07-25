//! Test-support-only executable for `pg-foma`'s own `tests/worker_supervisor.rs` integration
//! tests: a minimal process wrapper around [`pg_foma::worker::run_worker_child`] so those tests can
//! spawn a REAL child process (via `std::process::Command`, exactly as `pg-cli`'s own hidden
//! `__compile-worker-child` subcommand will in production) and exercise
//! `pg_foma::worker::run_compile_worker`'s wall-timeout/kill/RSS-sampling loop end-to-end, without
//! depending on the separate `pg-cli` crate.
//!
//! Not part of any shipped product surface (never invoked by `pangloss`, `pg-ffi`, or `pg-wasm`) --
//! `cargo build --workspace`/`cargo test -p pg-foma` build it like any other binary target, but
//! nothing outside this crate's own tests spawns it.
//!
//! # Test-only hooks (read before treating any of these as production behavior)
//! Two environment variables let `tests/worker_supervisor.rs` deterministically exercise outcomes a
//! real adversarial grammar cannot cheaply and reliably reproduce in a fast test:
//! - `PANGLOSS_WORKER_TEST_SLEEP_MS`: if set, sleeps that many milliseconds BEFORE reading the
//!   request frame at all -- lets a test arm a real wall-clock deadline shorter than the sleep and
//!   observe the supervisor's actual `Child::kill()` path fire, rather than merely asserting on the
//!   (untested) code that would call it.
//! - `PANGLOSS_WORKER_TEST_CRASH=1`: if set, exits the process immediately with a distinctive
//!   non-zero code BEFORE writing any result frame -- lets a test observe the supervisor's
//!   `WorkerOutcome::ChildCrashed` classification (a child that exited abnormally with no valid
//!   result frame) against a real process exit, not a fabricated `ExitStatus`.
//!
//! Both are read only by this test-support binary's own `main`, never by
//! [`pg_foma::worker::run_worker_child`] itself (which has no env-var hook of its own) -- so the
//! production child path (`pg-cli`'s hidden subcommand, which calls `run_worker_child` directly)
//! is unaffected by either variable's presence.

// `pg_foma::worker` is `#[cfg(not(target_arch = "wasm32"))]`-gated (that module's own top doc);
// this bin target is auto-discovered by Cargo for EVERY target `cargo check -p pg-foma` builds
// for, including `wasm32-unknown-unknown` (this crate's own wasm32 gate, `README`/module docs).
// Give wasm32 a trivial no-op `main` so the package's wasm32 check stays green without pulling this
// test-support binary's own non-wasm-only dependency (`pg_foma::worker`) into that build.
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
