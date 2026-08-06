//! Corpus-gated (needs `samples/data/indonesian-hc.xml`, gitignored): compares the FFI batch path against `Morpher::parse_word` for all 121 words to isolate transport bugs from the engine-parity gap.

mod support;

use std::ffi::c_void;

use pangloss_ffi::{
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
#[ignore = "needs local gitignored corpus data (samples/data/indonesian-hc.xml); run with --include-ignored"]
fn ffi_batch_matches_in_process_for_full_corpus() {
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

    // Independent in-process grammar load under the SAME step-cap/memo config the FFI handle uses, so a config mismatch can't masquerade as an encoding bug.
    let grammar = pg_grammar::load(&xml).expect("load indonesian grammar in-process");
    let morpher = pg_parse::Morpher::new(&grammar, DEFAULT_STEP_CAP).with_memo(DEFAULT_MEMO);

    let handle = load_handle(&xml);

    // hc_parse_batch exercises the real rayon path, not hc_parse_word's single-threaded one (covered separately below).
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

    // Byte-stability: an identical repeated call must produce a byte-identical buffer, catching an incomplete tiebreaker in the canonical sort.
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

    // Encodes the in-process result through the SAME public encoder the FFI uses (`encode_single`), not a hand-rolled reimplementation that could itself disagree.
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
#[ignore = "needs local gitignored corpus data (samples/data/indonesian-hc.xml); run with --include-ignored"]
fn ffi_single_word_matches_in_process_for_full_corpus() {
    let Some(xml) = support::load_xml("indonesian-hc.xml") else {
        eprintln!("skipping: indonesian-hc.xml not present on disk");
        return;
    };
    let Some(words) = support::load_words("indonesian-words.txt") else {
        eprintln!("skipping: indonesian-words.txt not present on disk");
        return;
    };

    let grammar = pg_grammar::load(&xml).expect("load indonesian grammar in-process");
    let morpher = pg_parse::Morpher::new(&grammar, DEFAULT_STEP_CAP).with_memo(DEFAULT_MEMO);
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
