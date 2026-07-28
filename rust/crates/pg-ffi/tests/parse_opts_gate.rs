//! HC-rust port gap G3 closure gate (`docs/hermitcrab-rust-port-audit.md` sec 2/3 item 1;
//! `docs/p11-guesser-api-design.md`): exercises the additive `hc_parse_word_opts`/
//! `hc_parse_batch_opts` FFI entry points against the real `extern "C"` boundary (never reaching
//! into `pg-ffi` internals), using the same synthetic lexical-pattern grammar as
//! `conformance-staging/edge-cases/guesser-pattern-root-fallback/` and `pg-cli`'s own
//! `guess_tests` module, so all three surfaces (library, CLI, FFI) are provably testing the exact
//! same engine behavior. Unlike `ffi_transport_parity.rs`, this grammar is self-contained and
//! synthetic (no gitignored corpus dependency), so every test here runs in the default
//! `cargo test --workspace` suite — this IS the "FFI path agrees with the CLI/library path" gate.

use std::ffi::c_void;

use pangloss_ffi::{
    decode, decode_guess, encode_single, encode_single_guess, hc_buf_free, hc_grammar_free,
    hc_grammar_load, hc_parse_batch, hc_parse_batch_opts, hc_parse_word, hc_parse_word_opts,
    DecodedWordGuess, HcError, HcResultBuf, HcStr, DEFAULT_MEMO, DEFAULT_STEP_CAP, HC_OK,
};

const GRAMMAR_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>FfiGuessOptsProbe</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>Verb</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cG"><Representations><Representation>g</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cD"><Representations><Representation>d</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cK"><Representations><Representation>k</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
      <BoundaryDefinitions>
        <BoundaryDefinition id="cPlus"><Representations><Representation>+</Representation></Representations></BoundaryDefinition>
      </BoundaryDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRules="mrPast">
        <Name>Morphophonemic</Name>
        <MorphologicalRuleDefinitions>
          <MorphologicalRule id="mrPast" requiredPartsOfSpeech="posV">
            <Name>past_suffix</Name>
            <MorphemeId>PAST</MorphemeId>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subPast">
                <MorphologicalInput>
                  <PhoneticSequence id="stem">
                    <OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence>
                  </PhoneticSequence>
                </MorphologicalInput>
                <MorphologicalOutput>
                  <CopyFromInput index="stem" />
                  <InsertSegments><PhoneticShape>+d</PhoneticShape></InsertSegments>
                </MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
          </MorphologicalRule>
        </MorphologicalRuleDefinitions>
        <LexicalEntries>
          <LexicalEntry id="ePattern">
            <MorphemeId>PATTERN</MorphemeId>
            <Allomorphs><Allomorph id="aPattern"><PhoneticShape>[Any]*</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>pattern</Gloss>
          </LexicalEntry>
          <LexicalEntry id="eKad" partOfSpeech="posV">
            <MorphemeId>KAD</MorphemeId>
            <Allomorphs><Allomorph id="aKad"><PhoneticShape>kad</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>kad</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

fn load_handle() -> *mut c_void {
    let mut handle: *mut c_void = std::ptr::null_mut();
    let mut err = HcError::EMPTY;
    let code = unsafe {
        hc_grammar_load(
            GRAMMAR_XML.as_ptr(),
            GRAMMAR_XML.len(),
            &mut handle,
            &mut err,
        )
    };
    assert_eq!(code, HC_OK, "hc_grammar_load failed: code={code}");
    unsafe { hc_buf_free(&mut err.message) };
    assert!(!handle.is_null());
    handle
}

fn parse_opts_one(handle: *mut c_void, word: &str, guess_root: i32) -> DecodedWordGuess {
    let mut out = HcResultBuf::EMPTY;
    let code =
        unsafe { hc_parse_word_opts(handle, word.as_ptr(), word.len(), guess_root, &mut out) };
    assert_eq!(
        code, HC_OK,
        "hc_parse_word_opts({word:?}) failed: code={code}"
    );
    let bytes = unsafe { std::slice::from_raw_parts(out.data, out.len) }.to_vec();
    let mut decoded = decode_guess(&bytes).expect("decode single-word guess buffer");
    unsafe { hc_buf_free(&mut out) };
    assert_eq!(decoded.len(), 1);
    decoded.pop().unwrap()
}

/// In-process baseline: an independent grammar load + `pg_parse::Morpher::parse_word_opts`,
/// encoded through the exact same public encoder the FFI entry point uses
/// (`encode_single_guess`), then decoded with the same reference decoder -- "same encoder, two
/// callers", exactly `ffi_transport_parity.rs`'s own established pattern, not a hand-rolled
/// reimplementation that could itself disagree with the real one.
fn in_process_one(word: &str, guess_root: bool) -> DecodedWordGuess {
    let grammar = pg_grammar::load(GRAMMAR_XML).expect("load grammar in-process");
    let morpher = pg_parse::Morpher::new(&grammar, usize::MAX);
    let opts = pg_parse::ParseOptions::default().with_guess_root(guess_root);
    let outcome = morpher.parse_word_opts(word, &opts);
    let mut decoded = decode_guess(&encode_single_guess(&outcome)).expect("decode");
    assert_eq!(decoded.len(), 1);
    decoded.pop().unwrap()
}

/// Gate 4 (flag default is OFF) + gate 1 (guesser OFF reproduces the pre-existing empty result):
/// `guess_root == 0` through the real FFI boundary must be byte-for-byte identical (via the
/// decoded struct) to the in-process `Morpher::parse_word_opts` with `guess_root: false` --
/// proving the FFI's default path is genuinely a no-op wrapper, not just "usually agrees".
#[test]
fn guess_root_zero_matches_in_process_and_finds_nothing_for_the_pattern_only_word() {
    let handle = load_handle();
    for word in ["gag", "gagd"] {
        let ffi = parse_opts_one(handle, word, 0);
        let expected = in_process_one(word, false);
        assert_eq!(ffi, expected, "word={word:?}");
        assert_eq!(
            expected.analyses.len(),
            0,
            "word={word:?}: guess off must find nothing"
        );
        assert!(!expected.guessed);
    }
    unsafe { hc_grammar_free(handle) };
}

/// Gate 2: `guess_root != 0` through the FFI analyzes the out-of-lexicon words and marks the
/// result guessed (word-level AND per-analysis), matching the in-process engine exactly.
#[test]
fn guess_root_nonzero_matches_in_process_and_marks_pattern_only_words_guessed() {
    let handle = load_handle();

    let gag = parse_opts_one(handle, "gag", 1);
    let gag_expected = in_process_one("gag", true);
    assert_eq!(gag, gag_expected);
    assert!(gag.guessed);
    assert_eq!(gag.analyses.len(), 1);
    assert!(gag.analyses[0].guessed);
    assert_eq!(gag.analyses[0].morpheme_ids, vec![u32::MAX]);

    let gagd = parse_opts_one(handle, "gagd", 1);
    let gagd_expected = in_process_one("gagd", true);
    assert_eq!(gagd, gagd_expected);
    assert!(gagd.guessed);
    assert_eq!(gagd.analyses.len(), 2);
    assert!(gagd.analyses.iter().all(|a| a.guessed));

    unsafe { hc_grammar_free(handle) };
}

/// Gate 3 (negative control): the ordinary lexical root "kad" is never marked guessed, on or off,
/// through the FFI boundary -- matching the in-process engine and `pg-cli`'s own control test.
#[test]
fn ordinary_root_is_never_marked_guessed_through_ffi() {
    let handle = load_handle();
    for guess_root in [0, 1] {
        let ffi = parse_opts_one(handle, "kad", guess_root);
        let expected = in_process_one("kad", guess_root != 0);
        assert_eq!(ffi, expected, "guess_root={guess_root}");
        assert!(!ffi.guessed, "guess_root={guess_root}");
        assert_eq!(ffi.analyses.len(), 1);
        assert!(!ffi.analyses[0].guessed);
    }
    unsafe { hc_grammar_free(handle) };
}

/// The batch entry point (`hc_parse_batch_opts`) agrees with the single-word one, per word, in
/// original request order -- proving the parallel dispatch path carries `guess_root` correctly
/// too, not just the single-word one.
#[test]
fn batch_opts_agrees_with_word_opts_per_word_in_request_order() {
    let handle = load_handle();
    let words = ["kad", "gag", "gagd"];
    let hcstrs: Vec<HcStr> = words
        .iter()
        .map(|w| HcStr {
            ptr: w.as_ptr(),
            len: w.len(),
        })
        .collect();
    let mut out = HcResultBuf::EMPTY;
    let code =
        unsafe { hc_parse_batch_opts(handle, hcstrs.as_ptr(), hcstrs.len(), 2, 1, &mut out) };
    assert_eq!(code, HC_OK, "hc_parse_batch_opts failed: code={code}");
    let bytes = unsafe { std::slice::from_raw_parts(out.data, out.len) }.to_vec();
    let batch_decoded = decode_guess(&bytes).expect("decode batch guess buffer");
    unsafe { hc_buf_free(&mut out) };
    assert_eq!(batch_decoded.len(), 3);

    for (i, word) in words.iter().enumerate() {
        let single = parse_opts_one(handle, word, 1);
        assert_eq!(
            batch_decoded[i], single,
            "word={word:?}: batch and single-word guess-on results must agree"
        );
    }
    // Order pin: "gag"/"gagd" (index 1/2) are guessed, "kad" (index 0) is not.
    assert!(!batch_decoded[0].guessed);
    assert!(batch_decoded[1].guessed);
    assert!(batch_decoded[2].guessed);

    unsafe { hc_grammar_free(handle) };
}

/// The pre-existing `hc_parse_word` entry point (and its wire format) is untouched by this
/// addition: it still returns `HC_OK` and a well-formed (old-format) buffer for the same grammar
/// -- a coarse but real "nothing broke" check alongside `pg-ffi`'s own `buffer.rs` unit tests,
/// which pin the old format's exact bytes directly.
#[test]
fn pre_existing_hc_parse_word_still_works_unchanged_on_this_grammar() {
    let handle = load_handle();
    let mut out = HcResultBuf::EMPTY;
    let code = unsafe { hc_parse_word(handle, b"kad".as_ptr(), 3, &mut out) };
    assert_eq!(code, HC_OK);
    assert!(!out.data.is_null());
    assert!(
        pangloss_ffi::decode(unsafe { std::slice::from_raw_parts(out.data, out.len) }).is_some()
    );
    unsafe {
        hc_buf_free(&mut out);
        hc_grammar_free(handle);
    }
}

// -- Guess-off overclaim fix (2026-07-25) ----------------------------------------------------
//
// Before this fix, `hc_parse_word`/`hc_parse_batch` retried through `pg_lexicon`'s guesser
// unconditionally on a total analysis miss and encoded the result through the `MAGIC` wire
// format, which has no `guessed` bit at all -- a guessed analysis was byte-indistinguishable from
// a confirmed one for any caller of those two symbols. "gag"/"gagd" are this file's own
// guess-only words (see `guess_root_zero_matches_in_process_and_finds_nothing_for_the_pattern_
// only_word` above: the plain, guess-off engine finds nothing for them at all), making them the
// exact fixture this overclaim needs.

/// Gate: `hc_parse_word` AND `hc_parse_batch` return ZERO analyses for a guess-only word, not an
/// unmarked guess.
#[test]
fn hc_parse_word_and_batch_return_zero_analyses_for_a_guess_only_word() {
    let handle = load_handle();

    let mut out = HcResultBuf::EMPTY;
    let code = unsafe { hc_parse_word(handle, b"gag".as_ptr(), 3, &mut out) };
    assert_eq!(code, HC_OK);
    let decoded = decode(unsafe { std::slice::from_raw_parts(out.data, out.len) }).unwrap();
    assert_eq!(decoded.len(), 1);
    assert!(
        decoded[0].analyses.is_empty(),
        "hc_parse_word must never return a guessed analysis for a guess-only word: {:?}",
        decoded[0].analyses
    );
    assert!(
        !decoded[0].invalid_shape,
        "the word shape itself is valid, only guessing found it"
    );
    unsafe { hc_buf_free(&mut out) };

    // Batch: mix a real word in with the two guess-only ones, so the test also proves the guard
    // is selective (only the guess-only entries come back empty), not an accidental blanket wipe.
    let words = ["kad", "gag", "gagd"];
    let hcstrs: Vec<HcStr> = words
        .iter()
        .map(|w| HcStr {
            ptr: w.as_ptr(),
            len: w.len(),
        })
        .collect();
    let code = unsafe { hc_parse_batch(handle, hcstrs.as_ptr(), hcstrs.len(), 2, &mut out) };
    assert_eq!(code, HC_OK);
    let decoded = decode(unsafe { std::slice::from_raw_parts(out.data, out.len) }).unwrap();
    assert_eq!(decoded.len(), 3);
    assert!(
        !decoded[0].analyses.is_empty(),
        "kad has a real grammar analysis and must still be returned"
    );
    assert!(
        decoded[1].analyses.is_empty(),
        "gag is guess-only: hc_parse_batch must return zero analyses for it, not an unmarked guess"
    );
    assert!(
        decoded[2].analyses.is_empty(),
        "gagd is guess-only: hc_parse_batch must return zero analyses for it, not an unmarked guess"
    );
    unsafe { hc_buf_free(&mut out) };

    unsafe { hc_grammar_free(handle) };
}

/// Gate: `hc_parse_word`'s bytes for a word WITH real (non-guessed) analyses are exactly what they
/// always were, unaffected by the `guess_fallback` plumbing this fix threads through
/// `GrammarHandle`/`pg_lexicon`. Same "same encoder, two callers" proof `ffi_transport_parity.rs`
/// uses for the Indonesian corpus: encode the in-process `Morpher::parse_word` result through the
/// exact same `encode_single` writer the FFI entry point uses and require byte-identical output.
/// `guess_fallback` never influences this word at all (the retry only fires on a total miss, and
/// "kad" is a real lexical root), so this also stands as the "before vs after" proof the gate
/// asks for: the bytes match a computation this change cannot have touched.
#[test]
fn hc_parse_word_bytes_for_a_real_word_are_byte_identical_to_the_in_process_encoding() {
    let handle = load_handle();
    let mut out = HcResultBuf::EMPTY;
    let code = unsafe { hc_parse_word(handle, b"kad".as_ptr(), 3, &mut out) };
    assert_eq!(code, HC_OK);
    let ffi_bytes = unsafe { std::slice::from_raw_parts(out.data, out.len) }.to_vec();
    unsafe {
        hc_buf_free(&mut out);
        hc_grammar_free(handle);
    }

    let grammar = pg_grammar::load(GRAMMAR_XML).expect("load grammar in-process");
    let morpher = pg_parse::Morpher::new(&grammar, DEFAULT_STEP_CAP).with_memo(DEFAULT_MEMO);
    let outcome = morpher.parse_word("kad");
    let expected_bytes = encode_single(&outcome);

    assert_eq!(
        ffi_bytes, expected_bytes,
        "hc_parse_word's bytes for a word with real analyses must be byte-identical to the \
         in-process Morpher encoding -- the guess-off fix must not change this word's output at all"
    );
}
