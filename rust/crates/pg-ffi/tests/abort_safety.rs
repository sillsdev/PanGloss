//! Abort-safety test: a panic inside a rayon worker on the real `hc_parse_batch` FFI call path must be caught by `catch_unwind` at the `extern "C"` boundary, reported as `HC_ERR_PANIC`, leaving the grammar handle and process usable; the injection sentinel is compiled in only under a dev-dependency feature, absent from any shipped artifact.

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

    // Inject the sentinel into the middle of the batch, so real parsing work runs concurrently on other rayon workers -- not a batch of one.
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

    // (1) The panicking call itself must surface the error; a crash here means the panic reached the extern "C" frame uncaught, aborting the test process instead.
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

    // (2) The process is alive and the same handle still works, so the caught unwind did not corrupt its internal state.
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
