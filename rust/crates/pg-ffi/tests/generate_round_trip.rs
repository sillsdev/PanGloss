//! `hc_generate_words` round-trip gate (W7): for a real grammar, feeding a just-parsed word's own
//! `structured` analysis (root + other-morpheme ordinals, exactly what `hc_parse_word`/
//! `hc_parse_batch` already emit — see `buffer` module docs) back into `hc_generate_words` must
//! reproduce that same surface word among the generated set. This is the FieldWorks-shaped use
//! case the ABI is for: regenerate from a previously-obtained analysis without ever touching a raw
//! `FeatureStruct`.
//!
//! Self-skips if the untracked Indonesian corpus isn't present on disk, matching every other
//! corpus-backed test in this crate (plan §8: "corpora stay untracked local files with self-
//! skipping tests").

mod support;

use std::ffi::c_void;

use pangloss_ffi::{
    decode, hc_buf_free, hc_generate_words, hc_grammar_free, hc_grammar_load, hc_parse_word,
    DecodedWord, HcError, HcResultBuf, HC_OK,
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

fn parse_one(handle: *mut c_void, word: &str) -> DecodedWord {
    let mut out = HcResultBuf::EMPTY;
    let code = unsafe { hc_parse_word(handle, word.as_ptr(), word.len(), &mut out) };
    assert_eq!(code, HC_OK, "hc_parse_word({word:?}) failed: code={code}");
    let bytes = unsafe { std::slice::from_raw_parts(out.data, out.len) }.to_vec();
    let mut decoded = decode(&bytes).expect("decode single-word buffer");
    unsafe { hc_buf_free(&mut out) };
    assert_eq!(decoded.len(), 1);
    decoded.pop().unwrap()
}

fn generate(handle: *mut c_void, morpheme_ids: &[u32], root_morpheme_index: i32) -> Vec<String> {
    let mut out = HcResultBuf::EMPTY;
    let code = unsafe {
        hc_generate_words(
            handle,
            morpheme_ids.as_ptr(),
            morpheme_ids.len(),
            root_morpheme_index,
            &mut out,
        )
    };
    assert_eq!(code, HC_OK, "hc_generate_words failed: code={code}");
    let bytes = unsafe { std::slice::from_raw_parts(out.data, out.len) }.to_vec();
    let words =
        pangloss_ffi::decode_generated_words(&bytes).expect("decode generated-words buffer");
    unsafe { hc_buf_free(&mut out) };
    words
}

#[test]
fn regenerating_a_parsed_words_own_analysis_reproduces_it() {
    let Some(xml) = support::load_xml("indonesian-hc.xml") else {
        eprintln!("skipping: indonesian-hc.xml not present on disk");
        return;
    };
    let Some(words) = support::load_words("indonesian-words.txt") else {
        eprintln!("skipping: indonesian-words.txt not present on disk");
        return;
    };

    let handle = load_handle(&xml);

    // Only round-trip words with EXACTLY one surviving analysis -- an ambiguous word has no single
    // "own analysis" to feed back, and this test is about the round trip, not enumerating every
    // ambiguity (that's `ffi_indonesian_parity.rs`'s job).
    let mut checked = 0usize;
    let mut mismatches = Vec::new();
    for word in words.iter().take(60) {
        let decoded = parse_one(handle, word);
        if decoded.invalid_shape || decoded.analyses.len() != 1 {
            continue;
        }
        let analysis = &decoded.analyses[0];
        let generated = generate(handle, &analysis.morpheme_ids, analysis.root_morpheme_index);
        checked += 1;
        if !generated.iter().any(|g| g == word) {
            mismatches.push((word.clone(), generated));
        }
    }
    unsafe { hc_grammar_free(handle) };

    assert!(
        checked >= 10,
        "test data assumption stale: expected at least 10 unambiguous words in the first 60"
    );
    assert!(
        mismatches.is_empty(),
        "{}/{checked} unambiguous words did not regenerate themselves: {mismatches:?}",
        mismatches.len()
    );
    eprintln!("generate_round_trip: {checked}/{checked} unambiguous Indonesian words regenerate themselves");
}
