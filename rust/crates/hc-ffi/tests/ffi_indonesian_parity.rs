//! The tightest gate for M8 (plan brief): transport fidelity, not the C# golden. This test calls
//! the real `extern "C"` entry points — exactly as an external caller would, through
//! `hermit_crab::{hc_grammar_load, hc_parse_word, hc_parse_batch, hc_buf_free, hc_grammar_free}`,
//! never reaching into `hc-ffi` internals — and decodes the returned buffer with the crate's own
//! public reference decoder (`hermit_crab::decode`). It compares the result, analysis-for-
//! analysis, against `hc_parse::Morpher::parse_word`'s in-process result for **all 121 words** of
//! the Indonesian corpus (not just the 68 that match the C# golden — see MEMORY
//! `rust-parity-facts`). Any divergence here is an FFI/buffer-encoding bug, not a pre-existing
//! engine parity gap, because both sides run the identical engine — the whole point is isolating
//! transport bugs (UTF-8 handling, morpheme-id marshalling, buffer framing, canonical-order
//! sorting) from that separate, already-tracked parity gap.
//!
//! Self-skips (rather than fails) if the untracked sample corpus isn't present on disk, matching
//! the existing convention in `hc-grammar`'s tests (plan §8: "corpora stay untracked local files
//! with self-skipping tests").

mod support;

use std::ffi::c_void;

use hermit_crab::{
    decode, encode_single, hc_buf_free, hc_grammar_free, hc_grammar_load, hc_parse_batch,
    hc_parse_word, DecodedWord, HcError, HcResultBuf, HcStr, DEFAULT_MEMO, DEFAULT_STEP_CAP, HC_OK,
};

fn load_handle(xml: &str) -> *mut c_void {
    let mut handle: *mut c_void = std::ptr::null_mut();
    let mut err = HcError::EMPTY;
    let code = unsafe { hc_grammar_load(xml.as_ptr(), xml.len(), &mut handle, &mut err) };
    assert_eq!(code, HC_OK, "hc_grammar_load failed: code={code}");
    unsafe { hc_buf_free(&mut err.message) };
    assert!(!handle.is_null());
    handle
}

/// Decode a `hc_parse_word` single-word buffer's one entry.
fn decode_one(bytes: &[u8]) -> DecodedWord {
    let mut v = decode(bytes).expect("decode single-word buffer");
    assert_eq!(v.len(), 1);
    v.pop().unwrap()
}

#[test]
fn ffi_batch_matches_in_process_for_full_indonesian_corpus() {
    let Some(xml) = support::load_xml("indonesian-hc.xml") else {
        eprintln!("skipping: indonesian-hc.xml not present on disk");
        return;
    };
    let Some(words) = support::load_words("indonesian-words.txt") else {
        eprintln!("skipping: indonesian-words.txt not present on disk");
        return;
    };
    assert_eq!(
        words.len(),
        121,
        "test data assumption stale: expected 121 Indonesian words"
    );

    // In-process baseline: an independent grammar load (hc-ffi's internal Grammar is private to
    // the crate), under the SAME step-cap/memo config the FFI handle builds internally, so a
    // config mismatch can't masquerade as an encoding bug.
    let grammar = hc_grammar::load(&xml).expect("load indonesian grammar in-process");
    let morpher = hc_parse::Morpher::new(&grammar, DEFAULT_STEP_CAP).with_memo(DEFAULT_MEMO);

    let handle = load_handle(&xml);

    // FFI path: one hc_parse_batch call over the whole corpus — exercises the real rayon path,
    // not just hc_parse_word's single-threaded one (that's covered separately, below).
    let hcstrs: Vec<HcStr> = words
        .iter()
        .map(|w| HcStr {
            ptr: w.as_ptr(),
            len: w.len(),
        })
        .collect();
    let mut out = HcResultBuf::EMPTY;
    let code = unsafe { hc_parse_batch(handle, hcstrs.as_ptr(), hcstrs.len(), 4, &mut out) };
    assert_eq!(code, HC_OK, "hc_parse_batch failed: code={code}");
    let ffi_bytes = unsafe { std::slice::from_raw_parts(out.data, out.len) }.to_vec();
    let ffi_decoded = decode(&ffi_bytes).expect("decode FFI batch buffer");
    assert_eq!(ffi_decoded.len(), 121);
    unsafe { hc_buf_free(&mut out) };

    // Byte-stability: re-run the identical call in the same process and require a byte-identical
    // buffer. Guards against HashMap-iteration-order nondeterminism (documented in MEMORY
    // rust-parity-facts for step-cap-truncated words) leaking through the canonical sort — if the
    // sort's tiebreaker were incomplete, this would be the assertion that catches it.
    let mut out2 = HcResultBuf::EMPTY;
    let code2 = unsafe { hc_parse_batch(handle, hcstrs.as_ptr(), hcstrs.len(), 4, &mut out2) };
    assert_eq!(code2, HC_OK);
    let ffi_bytes2 = unsafe { std::slice::from_raw_parts(out2.data, out2.len) }.to_vec();
    unsafe { hc_buf_free(&mut out2) };
    assert_eq!(
        ffi_bytes, ffi_bytes2,
        "hc_parse_batch output must be byte-identical across repeated calls in one process"
    );

    unsafe { hc_grammar_free(handle) };

    // Analysis-for-analysis comparison: for each word, run Morpher::parse_word directly
    // in-process and encode it through the SAME public encoder the FFI uses (`encode_single`) —
    // "same encoder, two callers", not a hand-rolled reimplementation of the wire format that
    // could itself disagree with the real one.
    let mut mismatches = Vec::new();
    for (i, word) in words.iter().enumerate() {
        let outcome = morpher.parse_word(word);
        let expected = decode_one(&encode_single(&outcome));
        if ffi_decoded[i] != expected {
            mismatches.push(word.clone());
        }
    }
    assert!(
        mismatches.is_empty(),
        "{}/121 words diverged between the FFI batch path and in-process parse_word: {:?}",
        mismatches.len(),
        mismatches
    );
    eprintln!("ffi_batch_matches_in_process: 121/121 Indonesian words match (transport fidelity confirmed)");
}

#[test]
fn ffi_single_word_matches_in_process_for_full_indonesian_corpus() {
    let Some(xml) = support::load_xml("indonesian-hc.xml") else {
        eprintln!("skipping: indonesian-hc.xml not present on disk");
        return;
    };
    let Some(words) = support::load_words("indonesian-words.txt") else {
        eprintln!("skipping: indonesian-words.txt not present on disk");
        return;
    };

    let grammar = hc_grammar::load(&xml).expect("load indonesian grammar in-process");
    let morpher = hc_parse::Morpher::new(&grammar, DEFAULT_STEP_CAP).with_memo(DEFAULT_MEMO);
    let handle = load_handle(&xml);

    let mut mismatches = Vec::new();
    for word in &words {
        let mut out = HcResultBuf::EMPTY;
        let code = unsafe { hc_parse_word(handle, word.as_ptr(), word.len(), &mut out) };
        assert_eq!(code, HC_OK, "hc_parse_word({word:?}) failed: code={code}");
        let ffi_bytes = unsafe { std::slice::from_raw_parts(out.data, out.len) }.to_vec();
        let ffi_word = decode_one(&ffi_bytes);
        unsafe { hc_buf_free(&mut out) };

        let outcome = morpher.parse_word(word);
        let expected = decode_one(&encode_single(&outcome));
        if ffi_word != expected {
            mismatches.push(word.clone());
        }
    }
    unsafe { hc_grammar_free(handle) };

    assert!(
        mismatches.is_empty(),
        "{}/121 words diverged between hc_parse_word and in-process parse_word: {:?}",
        mismatches.len(),
        mismatches
    );
    eprintln!("ffi_single_word_matches_in_process: 121/121 Indonesian words match");
}
