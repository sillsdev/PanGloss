use super::*;
use crate::{hc_analyze_word_json, hc_buf_free, hc_lexicon_add_json, HcResultBuf, HC_OK};

const XML: &str = r#"<HermitCrabInput><Language><Name>BackendTest</Name><PartsOfSpeech><PartOfSpeech id="p"><Name>N</Name></PartOfSpeech></PartsOfSpeech><CharacterDefinitionTable id="t"><Name>T</Name><SegmentDefinitions><SegmentDefinition id="a"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions></CharacterDefinitionTable><Strata><Stratum characterDefinitionTable="t"><Name>S</Name><LexicalEntries><LexicalEntry id="official-a" partOfSpeech="p"><Allomorphs><Allomorph id="aa"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs></LexicalEntry></LexicalEntries></Stratum></Strata></Language></HermitCrabInput>"#;

#[test]
fn foma_initialization_failure_explicitly_falls_back_to_official_morpher_analysis() {
    let grammar = pg_grammar::load(XML).unwrap();
    let handle = GrammarHandle::new_with_foma_result(grammar, XML, Err("forced".into()));
    let result = handle.analyze_word("a", false);
    assert!(result
        .structured
        .iter()
        .any(|analysis| matches!(analysis.provenance, pg_parse::AnalysisProvenance::Grammar)));
    assert_eq!(handle.backend_kind(), "morpherFallback");
    assert_eq!(
        handle.runtime.analysis_policy(),
        pg_lexicon::AnalysisPolicy::native_abi_v1()
    );
}

#[test]
fn analyzer_panic_is_enveloped_and_the_same_handle_remains_usable() {
    let grammar = pg_grammar::load(XML).unwrap();
    let handle = GrammarHandle::new(grammar, XML);
    handle.force_next_foma_panic();
    let raw = Box::into_raw(handle).cast();

    let request = br#"{"word":"a"}"#;
    let mut out = HcResultBuf::EMPTY;
    assert_eq!(
        unsafe { hc_analyze_word_json(raw, request.as_ptr(), request.len(), &mut out) },
        HC_OK
    );
    let first: serde_json::Value =
        serde_json::from_slice(unsafe { std::slice::from_raw_parts(out.data, out.len) }).unwrap();
    assert_eq!(first["error"]["code"], "panic");
    unsafe { hc_buf_free(&mut out) };

    assert_eq!(
        unsafe { hc_analyze_word_json(raw, request.as_ptr(), request.len(), &mut out) },
        HC_OK
    );
    let second: serde_json::Value =
        serde_json::from_slice(unsafe { std::slice::from_raw_parts(out.data, out.len) }).unwrap();
    assert_eq!(second["ok"], true);
    assert_eq!(
        second["value"]["structured"][0]["provenance"]["kind"],
        "grammar"
    );
    unsafe {
        hc_buf_free(&mut out);
        crate::hc_grammar_free(raw)
    };
}

#[test]
fn injected_analyzer_panic_is_scoped_to_one_handle() {
    let first = GrammarHandle::new(pg_grammar::load(XML).unwrap(), XML);
    let second = GrammarHandle::new(pg_grammar::load(XML).unwrap(), XML);
    first.force_next_foma_panic();
    assert!(!second.analyze_word("a", false).structured.is_empty());
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(
        || first.analyze_word("a", false)
    ))
    .is_err());
    assert!(!first.analyze_word("a", false).structured.is_empty());
}

#[test]
fn mutation_panic_is_structured_and_same_handle_remains_mutable_and_analyzable() {
    let handle = GrammarHandle::new(pg_grammar::load(XML).unwrap(), XML);
    let signature = handle.runtime.catalog().signatures()[0].id.as_str();
    let request = serde_json::json!({"stem":"a", "gloss":"", "signatures":[signature]});
    let bytes = serde_json::to_vec(&request).unwrap();
    handle.runtime.force_next_mutation_panic_for_test();
    let raw = Box::into_raw(handle).cast();
    let mut out = HcResultBuf::EMPTY;
    assert_eq!(
        unsafe { hc_lexicon_add_json(raw, bytes.as_ptr(), bytes.len(), &mut out) },
        HC_OK
    );
    let first: serde_json::Value =
        serde_json::from_slice(unsafe { std::slice::from_raw_parts(out.data, out.len) }).unwrap();
    assert_eq!(first["error"]["code"], "panic");
    unsafe { hc_buf_free(&mut out) };

    assert_eq!(
        unsafe { hc_lexicon_add_json(raw, bytes.as_ptr(), bytes.len(), &mut out) },
        HC_OK
    );
    let second: serde_json::Value =
        serde_json::from_slice(unsafe { std::slice::from_raw_parts(out.data, out.len) }).unwrap();
    assert_eq!(second["ok"], true);
    unsafe { hc_buf_free(&mut out) };

    let analyze = br#"{"word":"a"}"#;
    assert_eq!(
        unsafe { hc_analyze_word_json(raw, analyze.as_ptr(), analyze.len(), &mut out) },
        HC_OK
    );
    let third: serde_json::Value =
        serde_json::from_slice(unsafe { std::slice::from_raw_parts(out.data, out.len) }).unwrap();
    assert_eq!(third["ok"], true);
    unsafe {
        hc_buf_free(&mut out);
        crate::hc_grammar_free(raw)
    };
}

#[cfg(all(test, not(target_arch = "wasm32")))]
#[test]
fn batch_confirmation_uses_requested_parallelism_outside_backend_lock() {
    let handle = GrammarHandle::new(pg_grammar::load(XML).unwrap(), XML);
    let words = vec!["a".to_string(); 8];
    let (sequential, sequential_probe) = handle
        .analyze_words_with_confirmation_concurrency_probe(&words, 1, false)
        .unwrap();
    assert_eq!(sequential_probe.max_active(), 1);
    let pools_before = handle.pool_build_count_for_test();
    let (outcomes, parallel_probe) = handle
        .analyze_words_with_confirmation_concurrency_probe(&words, 2, false)
        .unwrap();
    assert_eq!(outcomes.len(), words.len());
    assert_eq!(parallel_probe.max_active(), 2);
    assert_eq!(handle.pool_build_count_for_test() - pools_before, 1);
    assert!(outcomes
        .iter()
        .all(|(outcome, _)| !outcome.structured.is_empty()));
    for ((expected, _), (actual, _)) in sequential.iter().zip(&outcomes) {
        assert_eq!(expected.analyses, actual.analyses);
        assert_eq!(expected.structured, actual.structured);
        assert_eq!(expected.guessed, actual.guessed);
        assert_eq!(expected.capped, actual.capped);
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
#[test]
fn batch_pool_build_failure_maps_to_invalid_argument_and_handle_is_reusable() {
    let handle = GrammarHandle::new(pg_grammar::load(XML).unwrap(), XML);
    handle.force_next_pool_build_failure_for_test();
    let raw = Box::into_raw(handle).cast();
    let mut out = HcResultBuf::EMPTY;
    assert_eq!(
        unsafe { crate::hc_parse_batch(raw, std::ptr::null(), 0, 2, &mut out) },
        crate::HC_ERR_INVALID_ARG
    );
    assert!(out.data.is_null());
    assert_eq!(
        unsafe { crate::hc_parse_word(raw, b"a".as_ptr(), 1, &mut out) },
        HC_OK
    );
    unsafe {
        hc_buf_free(&mut out);
        crate::hc_grammar_free(raw)
    };
}
