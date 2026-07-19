//! Abort-safety test (plan §8 layer 7): a panic raised **inside a rayon worker thread, on the
//! real `hc_parse_batch` FFI call path**, must be caught by `catch_unwind` at the `extern "C"`
//! boundary, reported as `HC_ERR_PANIC`, and leave both the grammar handle and the process
//! usable for further calls.
//!
//! ## Why this specific mechanism
//! The task brief is explicit that a single-word-only repro is insufficient — rayon's
//! panic-propagation-through-`par_iter`/`.install()` is the actual thing under test, not just
//! "does `catch_unwind` exist somewhere". The injection point is `pg_parse::batch`'s
//! `TEST_PANIC_WORD` sentinel (see `pg-parse/src/batch.rs`), compiled in ONLY when pg-parse is
//! built with the `test-panic-hook` Cargo feature — which pg-ffi enables *exclusively* via its
//! own `[dev-dependencies]` entry (`Cargo.toml`), never `[dependencies]`. That means:
//! - `cargo test` (this file) gets the hook, because dev-dependencies are part of a test build.
//! - `cargo build` / `cargo build --release -p pg-ffi` (the shipped `pangloss_ffi.dll`) does
//!   **not** build `[dev-dependencies]` at all, so the hook is structurally absent from any
//!   artifact a host ever loads — "can't fire in production grammars" by construction, not by
//!   convention.
//!
//! If `pg_parse::batch::TEST_PANIC_WORD` fails to resolve, that itself is a loud signal the
//! feature-unification wiring is broken (a compile error, not a silently-skipped test) — see the
//! `Cargo.toml` comments on both crates for the mechanism.
//!
//! ## Guarding against a false pass
//! The most dangerous failure mode for an abort-safety test is a false pass: if the hook were
//! somehow inert, the sentinel word would just fail to parse like any other unrecognized word and
//! `hc_parse_batch` would return `HC_OK` — a test that only checked "the next call still works"
//! would pass without ever exercising `catch_unwind`. So the first assertion below is on the
//! *panicking call's own* return code, not just on a subsequent call.
//!
//! Test-timing policy (revised 2026-07-17): this test loads a real grammar from
//! `samples/data/indonesian-hc.xml` (gitignored), so per policy it is unconditionally
//! `#[ignore = "..."]`d even though it is fast and covers an important safety property — the
//! default local `cargo test --workspace --release` run must not depend on gitignored corpus data
//! at all. Run explicitly with `--include-ignored` (this is the one test in the crate covering
//! rayon-worker panic safety, so it is worth running locally before a release even though it is
//! out of the default run).

mod support;

use std::ffi::c_void;

use pangloss_ffi::{
    decode, hc_buf_free, hc_grammar_free, hc_grammar_load, hc_parse_batch, HcError, HcResultBuf,
    HcStr, HC_ERR_PANIC, HC_OK,
};

#[test]
#[ignore = "needs local gitignored corpus data (samples/data/indonesian-hc.xml); run with --include-ignored"]
fn batch_panic_is_caught_and_handle_stays_usable() {
    let Some(xml) = support::load_xml("indonesian-hc.xml") else {
        eprintln!("skipping: indonesian-hc.xml not present on disk");
        return;
    };
    let Some(mut words) = support::load_words("indonesian-words.txt") else {
        eprintln!("skipping: indonesian-words.txt not present on disk");
        return;
    };
    words.truncate(20); // small + fast; still enough words to keep a 4-thread pool genuinely busy

    let mut handle: *mut c_void = std::ptr::null_mut();
    let mut err = HcError::EMPTY;
    let code = unsafe { hc_grammar_load(xml.as_ptr(), xml.len(), &mut handle, &mut err) };
    assert_eq!(code, HC_OK, "hc_grammar_load failed: code={code}");
    unsafe { hc_buf_free(&mut err.message) };
    assert!(!handle.is_null());

    // Inject the sentinel into the middle of the batch, so real parsing work is genuinely running
    // on other rayon workers concurrently with the panicking task — this is not a batch of one.
    let mut poisoned: Vec<String> = words.clone();
    let mid = poisoned.len() / 2;
    poisoned.insert(mid, pg_parse::batch::TEST_PANIC_WORD.to_string());

    let hcstrs: Vec<HcStr> = poisoned
        .iter()
        .map(|w| HcStr {
            ptr: w.as_ptr(),
            len: w.len(),
        })
        .collect();
    let mut out = HcResultBuf::EMPTY;
    // max_threads = 4: genuinely engages rayon's scoped pool, not a trivially-sequential path.
    let code = unsafe { hc_parse_batch(handle, hcstrs.as_ptr(), hcstrs.len(), 4, &mut out) };

    // (1) The panicking call itself must surface the error. A crash here means the panic reached
    // the `extern "C"` frame uncaught (this test process would abort, not report a failed
    // assertion) — the strongest possible evidence catch_unwind is doing its job, since there is
    // no way to "fake" reaching this line after an actual uncaught unwind across an FFI frame.
    assert_eq!(
        code, HC_ERR_PANIC,
        "a panic inside a rayon worker task must surface as HC_ERR_PANIC at the FFI boundary, not crash the process or silently succeed"
    );
    // `out` must still be in the documented "valid, freeable, empty" state on the error path.
    assert!(
        out.data.is_null(),
        "HcResultBuf must be left empty on an error return"
    );
    unsafe { hc_buf_free(&mut out) }; // must be a safe no-op on an already-empty buffer

    // (2) The process is alive and the SAME handle still works for further calls — the grammar
    // tier and the handle's internal state were not corrupted by the caught unwind.
    let hcstrs_ok: Vec<HcStr> = words
        .iter()
        .map(|w| HcStr {
            ptr: w.as_ptr(),
            len: w.len(),
        })
        .collect();
    let mut out2 = HcResultBuf::EMPTY;
    let code2 =
        unsafe { hc_parse_batch(handle, hcstrs_ok.as_ptr(), hcstrs_ok.len(), 4, &mut out2) };
    assert_eq!(
        code2, HC_OK,
        "the handle must remain usable for further hc_parse_batch calls after a caught panic"
    );
    let decoded = decode(unsafe { std::slice::from_raw_parts(out2.data, out2.len) })
        .expect("decode post-panic batch buffer");
    assert_eq!(decoded.len(), words.len());
    unsafe { hc_buf_free(&mut out2) };

    unsafe { hc_grammar_free(handle) };

    eprintln!("abort-safety: panic inside hc_parse_batch's rayon pool caught cleanly; handle remained usable afterward");
}
