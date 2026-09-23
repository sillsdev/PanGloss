use super::*;

const SIMPLE: &str = include_str!("../../tests/data/captured-parse-simple.json");
const MANY: &str = include_str!("../../tests/data/captured-parse-many.json");
const DUPLICATE: &str = include_str!("../../tests/data/synthetic-duplicate-analyses.json");
const CAPPED: &str = include_str!("../../tests/data/synthetic-capped-result.json");
const HVO_SHAPED: &str = include_str!("../../tests/data/synthetic-hvo-shaped-msa-guid.json");

fn word_result<'a>(parsed: &'a ParsedParseResponse, word: &str) -> &'a XampleResult {
    &parsed
        .words
        .iter()
        .find(|(w, _)| w == word)
        .unwrap_or_else(|| panic!("no '{word}' entry in parsed response"))
        .1
}

#[test]
fn one_analysis_word_reads_as_a_single_member_multiset() {
    let parsed = read_parse_response(SIMPLE).expect("captured fixture must parse");
    let k = word_result(&parsed, "k");
    assert_eq!(k.analyses.len(), 1);
    assert_eq!(k.analyses.values().sum::<usize>(), 1);
    assert_eq!(k.reached_max_analyses, None);
    assert_eq!(k.engine_error, None);
    let (signature, count) = k.analyses.iter().next().unwrap();
    assert_eq!(*count, 1);
    assert_eq!(
        signature.msa_ids,
        vec!["d71a9c35-5dd1-4659-997a-b3ad043e06f2".to_string()]
    );
    assert_eq!(signature.morphemes, vec!["K".to_string()]);
    assert_eq!(signature.surface_nfd, "k");
}

#[test]
fn many_analyses_word_keeps_every_distinct_signature() {
    let parsed = read_parse_response(SIMPLE).expect("captured fixture must parse");
    let xk = word_result(&parsed, "xk");
    assert_eq!(
        xk.analyses.len(),
        12,
        "12 distinct prefix choices, none deduplicated"
    );
    assert_eq!(xk.analyses.values().sum::<usize>(), 12);
    assert!(xk.analyses.values().all(|&count| count == 1));
}

#[test]
fn a_much_larger_many_analyses_capture_reads_completely() {
    // Real captured XAmple output; 924 = C(12,6), the grammar's own optional-prefix combinatorics.
    let parsed = read_parse_response(MANY).expect("captured fixture must parse");
    let xx = word_result(&parsed, "xxxxxxk");
    assert_eq!(xx.analyses.len(), 924);
    assert_eq!(xx.analyses.values().sum::<usize>(), 924);
    assert_eq!(xx.reached_max_analyses, None);
}

#[test]
fn real_engine_error_is_captured_alongside_empty_analyses() {
    let parsed = read_parse_response(SIMPLE).expect("captured fixture must parse");
    let bad = word_result(&parsed, "xk k");
    assert!(bad.analyses.is_empty());
    let err = bad
        .engine_error
        .as_deref()
        .expect("real captured engine error");
    assert!(
        err.contains("multiple root elements"),
        "unexpected error text: {err}"
    );
}

#[test]
fn synthesized_duplicate_analyses_are_counted_not_collapsed() {
    let parsed = read_parse_response(DUPLICATE).expect("synthetic fixture must parse");
    let word = word_result(&parsed, "dup");
    assert_eq!(word.analyses.len(), 1, "one distinct signature");
    assert_eq!(
        word.analyses.values().sum::<usize>(),
        2,
        "reported twice by the engine"
    );
}

#[test]
fn synthesized_capped_result_reports_reached_max_analyses() {
    // Synthesized because reachedMaxAnalyses never fired against the live witness: --max-analyses 1..2000 against 'xxxxxxk' (924 real analyses) always returned exactly the requested count with reachedMaxAnalyses still false.
    let parsed = read_parse_response(CAPPED).expect("synthetic fixture must parse");
    let word = word_result(&parsed, "capped");
    assert_eq!(word.reached_max_analyses, Some(2));
    assert_eq!(word.analyses.values().sum::<usize>(), 2);
}

#[test]
fn unsupported_schema_version_is_refused() {
    let text = SIMPLE.replacen("\"schemaVersion\": 1", "\"schemaVersion\": 2", 1);
    let err = read_parse_response(&text).expect_err("schemaVersion 2 must be refused");
    assert!(matches!(
        err,
        ReadError::UnsupportedSchemaVersion { found: 2 }
    ));
}

#[test]
fn wrong_mode_is_refused() {
    let text = SIMPLE.replacen("\"mode\": \"parse\"", "\"mode\": \"project\"", 1);
    let err = read_parse_response(&text).expect_err("mode 'project' must be refused");
    assert!(matches!(err, ReadError::WrongMode { found } if found == "project"));
}

#[test]
fn missing_required_field_is_refused() {
    let value: serde_json::Value = serde_json::from_str(SIMPLE).unwrap();
    let mut object = value.as_object().unwrap().clone();
    object.remove("database");
    let text = serde_json::to_string(&object).unwrap();
    let err =
        read_parse_response(&text).expect_err("a response missing 'database' must be refused");
    assert!(matches!(err, ReadError::Malformed(_)));
}

#[test]
fn unrecognized_key_is_refused() {
    let value: serde_json::Value = serde_json::from_str(SIMPLE).unwrap();
    let mut object = value.as_object().unwrap().clone();
    object.insert(
        "somethingNewAndUnknown".to_string(),
        serde_json::json!(true),
    );
    let text = serde_json::to_string(&object).unwrap();
    let err =
        read_parse_response(&text).expect_err("an unrecognized top-level key must be refused");
    assert!(matches!(err, ReadError::Malformed(_)));
}

#[test]
fn morph_missing_msa_guid_is_refused_not_defaulted() {
    let value: serde_json::Value = serde_json::from_str(SIMPLE).unwrap();
    let mut root = value.as_object().unwrap().clone();
    let words = root.get_mut("words").unwrap().as_array_mut().unwrap();
    let k_word = words
        .iter_mut()
        .find(|w| w["word"] == "k")
        .expect("fixture has a 'k' word entry");
    k_word["analyses"][0]["morphemes"][0]
        .as_object_mut()
        .unwrap()
        .remove("msaGuid");
    let text = serde_json::to_string(&root).unwrap();
    let err = read_parse_response(&text).expect_err("a morph with no msaGuid must be refused");
    assert!(matches!(err, ReadError::MissingMsaGuid { .. }));
}

#[test]
fn hvo_shaped_msa_guid_is_refused_not_trusted() {
    let err = read_parse_response(HVO_SHAPED).expect_err("an hvo-shaped msaGuid must be refused");
    assert!(matches!(err, ReadError::MalformedMsaGuid { ref value, .. } if value == "123456"));
}
