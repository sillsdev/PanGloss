//! Exercises the additive `hc_parse_word_opts`/`hc_parse_batch_opts` FFI entry points against the real `extern "C"` boundary, using the same synthetic lexical-pattern grammar as the library/CLI guesser tests, so all three surfaces provably test the same engine behavior; self-contained and synthetic, so it runs in the default suite.

use std::ffi::c_void;

use pangloss_ffi::{
    decode, decode_guess, encode_single, encode_single_guess, hc_analyze_word_json, hc_buf_free,
    hc_grammar_free, hc_grammar_load, hc_lexicon_add_json, hc_lexicon_catalog_json,
    hc_lexicon_export_json, hc_lexicon_import_json, hc_lexicon_remove_json, hc_parse_batch,
    hc_parse_batch_opts, hc_parse_word, hc_parse_word_opts, DecodedWordGuess, HcError, HcResultBuf,
    HcStr, DEFAULT_STEP_CAP, HC_OK,
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
          <LexicalEntry id="00000000-0000-0000-0000-000000000000" partOfSpeech="posV">
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

fn parse_opts_batch(handle: *mut c_void, words: &[&str], guess_root: i32) -> Vec<DecodedWordGuess> {
    let hcstrs: Vec<_> = words
        .iter()
        .map(|word| HcStr {
            ptr: word.as_ptr(),
            len: word.len(),
        })
        .collect();
    let mut out = HcResultBuf::EMPTY;
    let code = unsafe {
        hc_parse_batch_opts(
            handle,
            hcstrs.as_ptr(),
            hcstrs.len(),
            2,
            guess_root,
            &mut out,
        )
    };
    assert_eq!(code, HC_OK, "hc_parse_batch_opts failed: code={code}");
    let bytes = unsafe { std::slice::from_raw_parts(out.data, out.len) }.to_vec();
    let decoded = decode_guess(&bytes).expect("decode batch opts");
    unsafe { hc_buf_free(&mut out) };
    decoded
}

fn json_call(
    call: unsafe extern "C" fn(*mut c_void, *const u8, usize, *mut HcResultBuf) -> i32,
    handle: *mut c_void,
    request: &str,
) -> serde_json::Value {
    let mut out = HcResultBuf::EMPTY;
    let code = unsafe { call(handle, request.as_ptr(), request.len(), &mut out) };
    assert_eq!(code, HC_OK, "JSON call failed: code={code}");
    let bytes = unsafe { std::slice::from_raw_parts(out.data, out.len) };
    let value: serde_json::Value = serde_json::from_slice(bytes).expect("JSON response");
    unsafe { hc_buf_free(&mut out) };
    assert_eq!(value["ok"], true, "JSON request failed: {value}");
    value["value"].clone()
}

fn add_supplied_root(handle: *mut c_void, stem: &str) -> String {
    let catalog = json_call(hc_lexicon_catalog_json, handle, "{}");
    let signature = catalog["signatures"][0]["id"]
        .as_str()
        .expect("catalog signature id");
    let request = serde_json::json!({
        "stem": stem,
        "gloss": "",
        "signatures": [signature],
        "expectedRevision": catalog["revision"],
    })
    .to_string();
    json_call(hc_lexicon_add_json, handle, &request)["value"]["id"]
        .as_str()
        .expect("new supplied root id")
        .to_string()
}

fn assert_wire_matches_owner(
    wire: &DecodedWordGuess,
    owner: &pg_lexicon::UnifiedAnalysis,
    word: &str,
) {
    let expected_outcome = pg_parse::ParseOutcome {
        analyses: owner.analyses.clone(),
        structured: owner.structured.clone(),
        capped: owner.capped,
        invalid_shape: owner.invalid_shape,
        steps: 0,
        timed_out: owner.timed_out,
        guessed: owner.guessed,
        candidates_generated: owner.candidates_generated,
    };
    let mut expected =
        decode_guess(&encode_single_guess(&expected_outcome)).expect("encode direct owner outcome");
    assert_eq!(expected.len(), 1);
    assert_eq!(wire, &expected.pop().unwrap(), "word={word:?}");
}

fn assert_opts_owner_matrix(handle: *mut c_void, words: &[&str]) {
    let exported = json_call(hc_lexicon_export_json, handle, "{}");
    let document: pg_lexicon::LexiconDocument =
        serde_json::from_value(exported).expect("decode published lexicon document");
    let grammar = std::sync::Arc::new(pg_grammar::load(GRAMMAR_XML).expect("load owner grammar"));
    let owner = pg_lexicon::SuppliedLexiconRuntime::with_policy(
        grammar,
        GRAMMAR_XML,
        pg_lexicon::AnalysisPolicy {
            step_cap: DEFAULT_STEP_CAP,
        },
    )
    .expect("construct independent owner");
    owner
        .import_document(document)
        .expect("import published lexicon snapshot");

    for guess_root in [0, 1] {
        let batch = parse_opts_batch(handle, words, guess_root);
        assert_eq!(batch.len(), words.len());
        for (index, word) in words.iter().enumerate() {
            let single = parse_opts_one(handle, word, guess_root);
            assert_eq!(
                batch[index], single,
                "word={word:?}, guess_root={guess_root}"
            );
            let owner_outcome = owner.analyze_word_opts(word, None, guess_root != 0);
            assert_wire_matches_owner(&single, &owner_outcome, word);
        }
    }
}

/// In-process baseline: an independent grammar load + `Morpher::parse_word_opts`, encoded through the same public encoder the FFI entry point uses ("same encoder, two callers"), never a hand-rolled reimplementation that could disagree with the real one.
fn in_process_one(word: &str, guess_root: bool) -> DecodedWordGuess {
    let grammar = pg_grammar::load(GRAMMAR_XML).expect("load grammar in-process");
    let morpher = pg_parse::Morpher::new(&grammar, usize::MAX);
    let opts = pg_parse::ParseOptions::default().with_guess_root(guess_root);
    let outcome = morpher.parse_word_opts(word, &opts);
    let mut decoded = decode_guess(&encode_single_guess(&outcome)).expect("decode");
    assert_eq!(decoded.len(), 1);
    decoded.pop().unwrap()
}

/// Gate 4 + gate 1: `guess_root == 0` through the real FFI boundary must be byte-for-byte identical to the in-process `Morpher::parse_word_opts` with `guess_root: false`.
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

/// Gate 2: `guess_root != 0` through the FFI marks the result guessed (word-level and per-analysis), matching the in-process engine.
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

/// Gate 3 (negative control): ordinary lexical root "kad" is never marked guessed, on or off, through the FFI boundary.
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

/// Options endpoints preserve owner identities across supplied-root CRUD and guess modes.
#[test]
fn opts_single_and_batch_follow_owner_across_add_override_and_remove() {
    const OFFICIAL_ID: &str = "00000000-0000-0000-0000-000000000000";
    let handle = load_handle();
    let words = ["ga", "kad", "gag", "gagd"];

    let added_id = add_supplied_root(handle, "ga");
    let added_owner = json_call(
        hc_analyze_word_json,
        handle,
        &serde_json::json!({"word": "ga"}).to_string(),
    );
    let added_analysis = added_owner["structured"]
        .as_array()
        .unwrap()
        .iter()
        .find(|analysis| !analysis["suppliedRoot"].is_null())
        .expect("JSON owner transports active supplied root");
    assert_eq!(added_analysis["provenance"]["kind"], "supplied");
    assert_eq!(added_analysis["suppliedRoot"]["entryId"], added_id);
    assert_eq!(added_analysis["suppliedRoot"]["lexicalSpelling"], "ga");
    assert_eq!(
        added_analysis["suppliedRoot"]["authority"]["kind"],
        "supplied"
    );
    assert_opts_owner_matrix(handle, &words);

    let _generated_override_id = add_supplied_root(handle, "kad");
    let override_id = pg_lexicon::EntryId::from_dotnet_guid_string(OFFICIAL_ID)
        .expect("authored GUID has a canonical PGL entry ID")
        .as_str()
        .to_string();
    let mut document = json_call(hc_lexicon_export_json, handle, "{}");
    let entry = document["entries"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|entry| entry["stem"] == "kad")
        .expect("new override entry");
    entry["id"] = serde_json::json!(override_id);
    entry["authority"] = serde_json::json!({
        "suppliedOverride": {"officialEntryId": OFFICIAL_ID, "note": null}
    });
    let imported = json_call(
        hc_lexicon_import_json,
        handle,
        &serde_json::json!({"document": document}).to_string(),
    );
    assert_eq!(imported["changed"], true);
    let override_owner = json_call(
        hc_analyze_word_json,
        handle,
        &serde_json::json!({"word": "kad"}).to_string(),
    );
    let override_analysis = override_owner["structured"]
        .as_array()
        .unwrap()
        .iter()
        .find(|analysis| !analysis["suppliedRoot"].is_null())
        .expect("JSON owner transports override root");
    assert_eq!(override_analysis["provenance"]["kind"], "suppliedOverride");
    assert_eq!(
        override_analysis["provenance"]["overriddenGrammarEntryId"],
        OFFICIAL_ID
    );
    assert_eq!(override_analysis["suppliedRoot"]["entryId"], override_id);
    assert_eq!(override_analysis["suppliedRoot"]["lexicalSpelling"], "kad");
    assert_eq!(
        override_analysis["suppliedRoot"]["authority"]["kind"],
        "suppliedOverride"
    );
    assert!(!override_owner["structured"]
        .as_array()
        .unwrap()
        .iter()
        .any(|analysis| analysis["provenance"]["kind"] == "grammar"));
    assert_opts_owner_matrix(handle, &words);

    for entry_id in [&added_id, &override_id] {
        let removed = json_call(
            hc_lexicon_remove_json,
            handle,
            &serde_json::json!({"id": entry_id}).to_string(),
        );
        assert_eq!(removed["changed"], true);
    }
    let removed_owner = json_call(
        hc_analyze_word_json,
        handle,
        &serde_json::json!({"word": "ga"}).to_string(),
    );
    assert!(removed_owner["structured"]
        .as_array()
        .unwrap()
        .iter()
        .all(|analysis| analysis["suppliedRoot"].is_null()));
    assert_opts_owner_matrix(handle, &words);

    unsafe { hc_grammar_free(handle) };
}

/// `hc_parse_batch_opts` agrees with the single-word entry point, per word, in original request order, proving the parallel dispatch path carries `guess_root` correctly too.
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

/// The pre-existing `hc_parse_word` entry point (and its wire format) is untouched by this addition: it still returns `HC_OK` and a well-formed old-format buffer.
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

// Guess-off analyses must never be an unmarked guess: the MAGIC wire format `hc_parse_word`/`hc_parse_batch` use has no guessed bit at all, so a guessed analysis would be byte-indistinguishable from a confirmed one.

/// Gate: `hc_parse_word` AND `hc_parse_batch` return zero analyses for a guess-only word, not an unmarked guess.
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

    // Batch: mix a real word in with the two guess-only ones, proving the guard is selective, not a blanket wipe.
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

/// Gate: `hc_parse_word`'s bytes for a word with real (non-guessed) analyses are unaffected by the `guess_fallback` plumbing: encode the in-process `Morpher::parse_word` result through the same `encode_single` writer the FFI entry point uses and require byte-identical output.
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
    let morpher = pg_parse::Morpher::new(&grammar, DEFAULT_STEP_CAP);
    let outcome = morpher.parse_word("kad");
    let expected_bytes = encode_single(&outcome);

    assert_eq!(
        ffi_bytes, expected_bytes,
        "hc_parse_word's bytes for a word with real analyses must be byte-identical to the \
         in-process Morpher encoding -- the guess-off fix must not change this word's output at all"
    );
}
