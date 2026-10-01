//! Feeding a just-parsed word's own `structured` analysis back into `hc_generate_words` must reproduce that same surface word — the FieldWorks-shaped use case of regenerating from a previously-obtained analysis without touching a raw `FeatureStruct`. Self-skips if the untracked corpus isn't present, and is unconditionally `#[ignore]`d so the default test run stays fast.

#[path = "support/mod.rs"]
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
#[ignore = "needs local gitignored corpus data (samples/data/indonesian-hc.xml); run with --include-ignored"]
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

    // Only round-trip words with exactly one surviving analysis, since an ambiguous word has no single "own analysis" to feed back.
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

const RICH_XML: &str = include_str!("../../../tools/fixtures/supplied-lexicon-host-smoke.xml");

type JsonOperation = unsafe extern "C" fn(*mut c_void, *const u8, usize, *mut HcResultBuf) -> i32;

fn json_operation(
    handle: *mut c_void,
    operation: JsonOperation,
    request: serde_json::Value,
) -> serde_json::Value {
    let bytes = serde_json::to_vec(&request).unwrap();
    let mut out = HcResultBuf::EMPTY;
    assert_eq!(
        unsafe { operation(handle, bytes.as_ptr(), bytes.len(), &mut out) },
        HC_OK
    );
    assert!(!out.data.is_null());
    let result =
        serde_json::from_slice(unsafe { std::slice::from_raw_parts(out.data, out.len) }).unwrap();
    unsafe { hc_buf_free(&mut out) };
    assert!(out.data.is_null());
    result
}

fn add_rich_root(handle: *mut c_void) -> serde_json::Value {
    let catalog = json_operation(
        handle,
        pangloss_ffi::hc_lexicon_catalog_json,
        serde_json::json!({}),
    );
    let result = json_operation(
        handle,
        pangloss_ffi::hc_lexicon_add_json,
        serde_json::json!({
            "stem": "milu", "gloss": "", "signatures": [catalog["value"]["signatures"][0]["id"]],
            "expectedRevision": catalog["value"]["revision"]
        }),
    );
    assert_eq!(result["ok"], true, "{result}");
    result["value"].clone()
}

#[test]
fn native_rich_analysis_contains_the_complete_generation_payload() {
    let handle = load_handle(RICH_XML);
    add_rich_root(handle);
    let outcome = json_operation(
        handle,
        pangloss_ffi::hc_analyze_word_json,
        serde_json::json!({"word":"milu"}),
    );
    assert_eq!(outcome["ok"], true);
    let analysis = outcome["value"]["structured"]
        .as_array()
        .unwrap()
        .iter()
        .find(|analysis| !analysis["suppliedRoot"].is_null())
        .expect("supplied analysis");
    for field in [
        "synFs",
        "mpr",
        "morphOccurrences",
        "guessedString",
        "morphemeRoots",
    ] {
        assert!(
            analysis.get(field).is_some(),
            "native analysis drops {field}: {analysis}"
        );
    }
    let complete: pg_parse::WordAnalysis = serde_json::from_value(analysis.clone()).unwrap();
    assert_eq!(serde_json::to_value(complete).unwrap(), *analysis);
    unsafe { hc_grammar_free(handle) };
}

#[test]
fn legacy_generation_refuses_runtime_sentinel_instead_of_claiming_success() {
    let handle = load_handle(RICH_XML);
    add_rich_root(handle);
    let analysis = parse_one(handle, "milu")
        .analyses
        .into_iter()
        .find(|analysis| analysis.morpheme_ids.contains(&u32::MAX))
        .expect("supplied sentinel");
    let mut out = HcResultBuf::EMPTY;
    let code = unsafe {
        hc_generate_words(
            handle,
            analysis.morpheme_ids.as_ptr(),
            analysis.morpheme_ids.len(),
            analysis.root_morpheme_index,
            &mut out,
        )
    };
    assert_eq!(
        code,
        pangloss_ffi::HC_ERR_INVALID_ARG,
        "legacy generation cannot carry supplied identity"
    );
    assert!(out.data.is_null());
    unsafe {
        hc_buf_free(&mut out);
        hc_grammar_free(handle)
    };
}

#[test]
fn native_rich_generation_roundtrips_owner_identities_and_affixes() {
    let handle = load_handle(RICH_XML);
    add_rich_root(handle);
    add_rich_root(handle);
    let exported = json_operation(
        handle,
        pangloss_ffi::hc_lexicon_export_json,
        serde_json::json!({}),
    );
    assert_eq!(exported["ok"], true);
    let grammar = std::sync::Arc::new(pg_grammar::load(RICH_XML).unwrap());
    let owner = pg_lexicon::SuppliedLexiconRuntime::with_policy(
        grammar,
        RICH_XML,
        pg_lexicon::AnalysisPolicy {
            step_cap: pangloss_ffi::DEFAULT_STEP_CAP,
        },
    )
    .unwrap();
    owner
        .import_document(serde_json::from_value(exported["value"].clone()).unwrap())
        .unwrap();
    let mut supplied_ids = std::collections::BTreeSet::new();
    let mut affixed = 0;
    for word in ["milu", "milusi"] {
        let native = json_operation(
            handle,
            pangloss_ffi::hc_analyze_word_json,
            serde_json::json!({"word":word}),
        );
        assert_eq!(native["ok"], true);
        let value = &native["value"];
        let structured: Vec<pg_parse::WordAnalysis> =
            serde_json::from_value(value["structured"].clone()).unwrap();
        let expected = owner.analyze_word(word, None);
        assert_eq!(
            structured, expected.structured,
            "full owner identity/multiplicity for {word}"
        );
        assert_eq!(value["capped"], expected.capped);
        assert_eq!(value["timedOut"], expected.timed_out);
        assert_eq!(value["invalidShape"], expected.invalid_shape);
        assert!(
            !structured.is_empty(),
            "nonempty fixture control for {word}"
        );
        for analysis in structured {
            if let Some(root) = &analysis.supplied_root {
                supplied_ids.insert(root.realization_id.clone());
                affixed += usize::from(analysis.morpheme_ids.len() > 1);
            }
            let owner_generated = owner
                .generate_analysis(pg_lexicon::GenerationRequest {
                    revision: expected.revision.clone(),
                    analysis: analysis.clone(),
                })
                .unwrap();
            let generated = json_operation(
                handle,
                pangloss_ffi::hc_generate_words_json,
                serde_json::json!({
                    "revision": value["revision"], "analysis": analysis
                }),
            );
            assert_eq!(generated["ok"], true, "{generated}");
            let result: pg_lexicon::GenerationResult =
                serde_json::from_value(generated["value"].clone()).unwrap();
            assert_eq!(result.analysis, analysis);
            assert_eq!(result.words, owner_generated.words);
            assert!(result.words.iter().any(|surface| surface == word));
            assert_eq!(
                serde_json::to_value(result.revision).unwrap(),
                value["revision"]
            );
            assert_eq!(generated["value"]["completion"], "notAssessed");
        }
    }
    assert_eq!(
        supplied_ids.len(),
        2,
        "distinct homographs retain identities"
    );
    assert!(
        affixed >= 2,
        "both supplied identities survive root-plus-affix transport"
    );
    let authored = parse_one(handle, "milusi")
        .analyses
        .into_iter()
        .find(|analysis| !analysis.morpheme_ids.contains(&u32::MAX))
        .expect("authored control");
    assert!(
        generate(handle, &authored.morpheme_ids, authored.root_morpheme_index)
            .iter()
            .any(|word| word == "milusi")
    );
    assert_eq!(pangloss_ffi::hc_abi_version(), 3);
    unsafe { hc_grammar_free(handle) };
}

#[test]
fn native_rich_generation_rejects_forged_stale_removed_and_guessed_requests() {
    let handle = load_handle(RICH_XML);
    add_rich_root(handle);
    let native = json_operation(
        handle,
        pangloss_ffi::hc_analyze_word_json,
        serde_json::json!({"word":"milu"}),
    );
    let analysis = native["value"]["structured"]
        .as_array()
        .unwrap()
        .iter()
        .find(|analysis| !analysis["suppliedRoot"].is_null())
        .unwrap()
        .clone();
    let request =
        serde_json::json!({"revision": native["value"]["revision"], "analysis": analysis});
    let mut forged = request.clone();
    forged["analysis"]["suppliedRoot"]["lexicalSpelling"] = "panu".into();
    let root_index = forged["analysis"]["rootMorphemeIndex"].as_u64().unwrap() as usize;
    forged["analysis"]["morphemeRoots"][root_index] = forged["analysis"]["suppliedRoot"].clone();
    let rejected = json_operation(handle, pangloss_ffi::hc_generate_words_json, forged);
    assert_eq!(rejected["ok"], false);
    assert_eq!(rejected["error"]["code"], "invalid_supplied_root");
    let mut guessed = request.clone();
    guessed["analysis"]["guessed"] = true.into();
    let rejected = json_operation(handle, pangloss_ffi::hc_generate_words_json, guessed);
    assert_eq!(rejected["error"]["code"], "unsupported_guessed_generation");
    let mut malformed = request.clone();
    malformed["analysis"]["morphemeRoots"] = serde_json::json!([]);
    let rejected = json_operation(handle, pangloss_ffi::hc_generate_words_json, malformed);
    assert_eq!(rejected["error"]["code"], "invalid_analysis");
    let mut unknown = request.clone();
    unknown["analysis"]["unknown"] = true.into();
    let rejected = json_operation(handle, pangloss_ffi::hc_generate_words_json, unknown);
    assert_eq!(rejected["error"]["code"], "invalid_json");
    let removed = json_operation(
        handle,
        pangloss_ffi::hc_lexicon_remove_json,
        serde_json::json!({
            "id": request["analysis"]["suppliedRoot"]["entryId"], "expectedRevision": request["revision"]
        }),
    );
    assert_eq!(removed["ok"], true);
    let stale = json_operation(
        handle,
        pangloss_ffi::hc_generate_words_json,
        request.clone(),
    );
    assert_eq!(stale["error"]["code"], "revision_conflict");
    let mut absent = request;
    absent["revision"] = removed["value"]["revision"].clone();
    let rejected = json_operation(handle, pangloss_ffi::hc_generate_words_json, absent);
    assert_eq!(rejected["error"]["code"], "invalid_supplied_root");
    unsafe { hc_grammar_free(handle) };
}
