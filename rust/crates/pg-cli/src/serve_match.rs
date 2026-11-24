//! `pangloss serve` op `match`: applies HermitCrab pattern text over a word list with Rust HC's own pattern matcher. Wire shape and limits: `docs/formats/serve-protocol.md`.

use pg_fst::work::WorkBudget;
use pg_grammar::chardef::CharDefId;
use pg_grammar::model::{Grammar, TableId};
use pg_parse::Morpher;
use pg_rules::pattern_match::{
    compile_match, compile_match_engine, has_stray_boundary, segment_byte_spans,
};
use pg_rules::pattern_text::{boundary_char_def, parse_pattern, PatternError};
use pg_rules::shape_feat::segment_with_features_load as segment_with_features;
use serde_json::{json, Map, Value};
use std::time::Duration;

const VERSION: u32 = 1;
const MAX_PATTERN_BYTES: usize = 4096;
const DEFAULT_MAX_WORDS: usize = 100_000;
const HARD_MAX_WORDS: usize = 1_000_000;
const DEFAULT_MAX_MATCHES_PER_WORD: usize = 1000;
const HARD_MAX_MATCHES_PER_WORD: usize = 100_000;
const MAX_MATCHES_TOTAL: usize = 1_000_000;

const SUPPORTED_CONSTRUCTS: [&str; 9] = [
    "naturalClass",
    "literal",
    "optional",
    "repeat",
    "wordEdge",
    "morphemeBoundary",
    "environment",
    "featureBundle",
    "alphaVariable",
];
const GATED_CONSTRUCTS: [&str; 2] = ["featureValue", "alphaAcrossFeatures"];
/// Every error code this op can emit.
const ERROR_CODES: [&str; 11] = [
    "bad_request",
    "pattern_parse",
    "unknown_natural_class",
    "unknown_feature",
    "unsupported_construct",
    "boundary_needs_marked_input",
    "unknown_boundary_marker",
    "too_many_words",
    "words_file_unreadable",
    "pattern_compile_failed",
    "internal",
];
/// Every `reason` an `unsegmentable` word can carry.
const WORD_REASONS: [&str; 3] = [
    "no_character_definition",
    "boundary_in_unmarked_word",
    "unexpected_boundary",
];
const STATUSES: [&str; 3] = ["ok", "unsegmentable", "incomplete"];
const STOP_REASONS: [&str; 4] = ["match_cap", "total_match_cap", "work_limit", "deadline"];
/// Per-word matcher work units when the request names none; the server's own `--work-cap` is the ceiling.
pub(crate) const DEFAULT_MAX_WORK: usize = 50_000_000;
/// Per-word wall clock when neither the request nor `--word-timeout-ms` names one.
pub(crate) const DEFAULT_WORD_TIMEOUT_MS: u64 = 10_000;
pub(crate) const MAX_WORD_TIMEOUT_MS: u64 = 600_000;
/// What a segment is tested by: `engine` as rule application does (features), `identity` by exact segment and class membership.
const SEGMENT_TESTS: [&str; 2] = ["engine", "identity"];
const DEFAULT_SEGMENT_TEST: &str = "engine";
const KNOWN_FIELDS: [&str; 11] = [
    "id",
    "op",
    "pattern",
    "words",
    "wordsFile",
    "boundaryMarker",
    "maxWords",
    "maxMatchesPerWord",
    "maxWork",
    "wordTimeoutMs",
    "segmentTest",
];

/// The `interfaces.serveOps.match` capability object; derived from the same constants the handler uses.
pub(crate) fn describe() -> Value {
    json!({
        "version": VERSION,
        "constructs": {"supported": SUPPORTED_CONSTRUCTS, "gated": GATED_CONSTRUCTS},
        "errors": ERROR_CODES,
        "statuses": STATUSES,
        "stopReasons": STOP_REASONS,
        "request": &KNOWN_FIELDS[2..],
        "wordReasons": WORD_REASONS,
        "segmentTests": SEGMENT_TESTS,
        "defaultSegmentTest": DEFAULT_SEGMENT_TEST,
        "zeroWidth": "reported: a pattern that can match the empty string reports an empty match at every position its anchors allow",
        "offsets": "utf8ByteOffsetsInNfdText",
        "limits": {
            "maxPatternBytes": MAX_PATTERN_BYTES,
            "defaultMaxWords": DEFAULT_MAX_WORDS,
            "maxWords": HARD_MAX_WORDS,
            "defaultMaxMatchesPerWord": DEFAULT_MAX_MATCHES_PER_WORD,
            "maxMatchesPerWord": HARD_MAX_MATCHES_PER_WORD,
            "maxMatchesTotal": MAX_MATCHES_TOTAL,
            "defaultMaxWork": DEFAULT_MAX_WORK,
            "defaultWordTimeoutMs": DEFAULT_WORD_TIMEOUT_MS,
            "maxWordTimeoutMs": MAX_WORD_TIMEOUT_MS,
        },
    })
}

pub(crate) fn error(id: &Value, code: &str, message: String, extra: Value) -> Value {
    debug_assert!(ERROR_CODES.contains(&code), "undeclared error code {code}");
    let mut v = json!({"id": id, "type": "error", "code": code, "message": message});
    if let (Some(map), Some(more)) = (v.as_object_mut(), extra.as_object()) {
        map.extend(more.clone());
    }
    v
}

fn pattern_error(id: &Value, e: PatternError) -> Value {
    match e {
        PatternError::Parse {
            token,
            offset,
            expected,
        } => error(
            id,
            "pattern_parse",
            format!(
                "unexpected {token:?} at offset {offset}; expected {}",
                expected.join(", ")
            ),
            json!({"token": token, "offset": offset, "expected": expected}),
        ),
        PatternError::UnknownNaturalClass { name, offset, near } => error(
            id,
            "unknown_natural_class",
            format!("no natural class named {name:?}"),
            json!({"name": name, "offset": offset, "near": near}),
        ),
        PatternError::UnknownFeature { name, offset, near } => error(
            id,
            "unknown_feature",
            format!("no phonological feature named {name:?}"),
            json!({"name": name, "offset": offset, "near": near}),
        ),
        PatternError::Unsupported { construct, offset } => error(
            id,
            "unsupported_construct",
            format!("{construct} is not served yet"),
            json!({"construct": construct, "offset": offset}),
        ),
        PatternError::BoundaryNeedsMarkedInput { offset } => error(
            id,
            "boundary_needs_marked_input",
            "a pattern with '+' needs words with marked boundaries; send \"boundaryMarker\"".into(),
            json!({"offset": offset}),
        ),
    }
}

fn opt_usize(
    id: &Value,
    object: &Map<String, Value>,
    key: &str,
    default: usize,
    hard: usize,
) -> Result<usize, Value> {
    match object.get(key) {
        None => Ok(default),
        Some(v) => match v.as_u64() {
            Some(n) if n as usize <= hard && (n >= 1 || default == 0) => Ok(n as usize),
            _ => Err(error(
                id,
                "bad_request",
                format!("\"{key}\" must be an integer from 1 to {hard}"),
                json!({}),
            )),
        },
    }
}

/// The per-word work units and wall clock a request names with `maxWork` and `wordTimeoutMs`, bounded by the server's own `--work-cap` and `--word-timeout-ms`. Shared by every op that reports `work_limit` and `deadline`.
pub(crate) fn request_budget(
    id: &Value,
    object: &Map<String, Value>,
    morpher: &Morpher<'_>,
) -> Result<(usize, Duration), Value> {
    let server_cap = morpher.work_cap().max(1);
    let max_work = opt_usize(
        id,
        object,
        "maxWork",
        DEFAULT_MAX_WORK.min(server_cap),
        server_cap,
    )?;
    let word_timeout =
        match opt_usize(id, object, "wordTimeoutMs", 0, MAX_WORD_TIMEOUT_MS as usize)? {
            0 => morpher
                .word_timeout()
                .unwrap_or(Duration::from_millis(DEFAULT_WORD_TIMEOUT_MS)),
            n => Duration::from_millis(n as u64),
        };
    Ok((max_work, word_timeout))
}

/// Answers one `match` request (the request line is already known to be a JSON object with a string `id`).
pub(crate) fn handle(
    g: &Grammar,
    morpher: &Morpher<'_>,
    id: &Value,
    object: &Map<String, Value>,
) -> Value {
    let bad = |m: &str| error(id, "bad_request", m.to_string(), json!({}));
    if let Some(unknown) = object.keys().find(|k| !KNOWN_FIELDS.contains(&k.as_str())) {
        return bad(&format!("unknown match field: {unknown}"));
    }
    let Some(pattern_text) = object.get("pattern").and_then(Value::as_str) else {
        return bad("\"pattern\" must be a string");
    };
    if pattern_text.len() > MAX_PATTERN_BYTES {
        return bad(&format!("\"pattern\" exceeds {MAX_PATTERN_BYTES} bytes"));
    }
    let marker = match object.get("boundaryMarker") {
        None => None,
        Some(Value::String(s)) if !s.is_empty() => Some(s.as_str()),
        Some(_) => return bad("\"boundaryMarker\" must be a non-empty string"),
    };
    let max_words = match opt_usize(id, object, "maxWords", DEFAULT_MAX_WORDS, HARD_MAX_WORDS) {
        Ok(n) => n,
        Err(e) => return e,
    };
    let per_word = match opt_usize(
        id,
        object,
        "maxMatchesPerWord",
        DEFAULT_MAX_MATCHES_PER_WORD,
        HARD_MAX_MATCHES_PER_WORD,
    ) {
        Ok(n) => n,
        Err(e) => return e,
    };
    let identity = match object.get("segmentTest") {
        None => false,
        Some(Value::String(s)) if s == "engine" => false,
        Some(Value::String(s)) if s == "identity" => true,
        Some(_) => return bad("\"segmentTest\" must be \"engine\" or \"identity\""),
    };
    let (max_work, word_timeout) = match request_budget(id, object, morpher) {
        Ok(b) => b,
        Err(e) => return e,
    };
    let words: Vec<String> = match (object.get("words"), object.get("wordsFile")) {
        (Some(_), Some(_)) => return bad("send \"words\" or \"wordsFile\", not both"),
        (None, None) => return bad("\"words\" or \"wordsFile\" is required"),
        (Some(Value::Array(items)), None) => {
            if items.len() > max_words {
                return too_many(id, items.len(), max_words);
            }
            let mut words = Vec::with_capacity(items.len());
            for item in items {
                match item.as_str() {
                    Some(s) => words.push(s.to_string()),
                    None => return bad("every entry of \"words\" must be a string"),
                }
            }
            words
        }
        (Some(_), None) => return bad("\"words\" must be an array of strings"),
        (None, Some(Value::String(path))) => {
            let text = match std::fs::read_to_string(path) {
                Ok(t) => t,
                Err(e) => {
                    return error(
                        id,
                        "words_file_unreadable",
                        format!("{path}: {e}"),
                        json!({"path": path}),
                    )
                }
            };
            let words: Vec<String> = text
                .lines()
                .map(|l| l.trim_end_matches('\r'))
                .filter(|l| !l.is_empty())
                .map(str::to_string)
                .collect();
            if words.len() > max_words {
                return too_many(id, words.len(), max_words);
            }
            words
        }
        (None, Some(_)) => return bad("\"wordsFile\" must be a string"),
    };

    let Some(stratum) = g.strata.last() else {
        return error(id, "internal", "grammar has no strata".into(), json!({}));
    };
    let table_id: TableId = stratum.table;
    let table = &g.char_tables[table_id.0 as usize];
    let boundary: Option<CharDefId> = match marker {
        None => None,
        Some(m) => match boundary_char_def(table, m) {
            Some(cd) => Some(cd),
            None => {
                return error(
                    id,
                    "unknown_boundary_marker",
                    format!("{m:?} is not a boundary character definition of the surface table"),
                    json!({"boundaryMarker": m}),
                )
            }
        },
    };
    let parsed = match parse_pattern(g, table, pattern_text, boundary) {
        Ok(p) => p,
        Err(e) => return pattern_error(id, e),
    };
    let compile = if identity {
        compile_match
    } else {
        compile_match_engine
    };
    let compiled = match compile(g, table_id, &parsed) {
        Ok(c) => c,
        Err(e) => return error(id, "pattern_compile_failed", e.0, json!({})),
    };

    let mut stop_reason: Option<&str> = None;
    let mut total = 0usize;
    let mut out_words = Vec::with_capacity(words.len());
    for word in &words {
        if total >= MAX_MATCHES_TOTAL {
            stop_reason.get_or_insert("total_match_cap");
            out_words.push(json!({
                "word": word, "status": "incomplete", "stop": "total_match_cap", "matches": []
            }));
            continue;
        }
        let shape = match segment_with_features(g, table, word) {
            Ok(s) => s,
            Err(e) => {
                let offset = word
                    .char_indices()
                    .nth(e.position)
                    .map_or(word.len(), |(b, _)| b);
                out_words.push(json!({
                    "word": word, "status": "unsegmentable", "matches": [],
                    "at": {"charIndex": e.position, "offset": offset},
                    "reason": "no_character_definition",
                }));
                continue;
            }
        };
        if has_stray_boundary(&shape, boundary) {
            let reason = if boundary.is_some() {
                "unexpected_boundary"
            } else {
                "boundary_in_unmarked_word"
            };
            out_words.push(json!({
                "word": word, "status": "unsegmentable", "matches": [], "reason": reason
            }));
            continue;
        }
        let nfd = pg_grammar::nfd::nfd(word);
        let Some(seg_spans) = segment_byte_spans(table, &shape, &nfd) else {
            return error(
                id,
                "internal",
                format!("cannot place segments of {word:?}"),
                json!({}),
            );
        };
        let n = seg_spans.len();
        let pos = |i: usize| seg_spans.get(i).map_or(nfd.len(), |s| s.0);
        let budget = WorkBudget::new(max_work).with_timeout(Some(word_timeout));
        let mut matches = {
            let _scope = budget.enter();
            compiled.matches(g, &shape)
        };
        if budget.stopped() || budget.retained_capped() {
            let stop = if budget.timed_out() {
                "deadline"
            } else {
                "work_limit"
            };
            stop_reason.get_or_insert(stop);
            out_words.push(json!({
                "word": word, "status": "incomplete", "stop": stop, "matches": []
            }));
            continue;
        }
        let room = MAX_MATCHES_TOTAL - total;
        let all = matches.len();
        let stop = if all > room && room <= per_word {
            Some("total_match_cap")
        } else if all > per_word {
            Some("match_cap")
        } else {
            None
        };
        matches.truncate(per_word.min(room));
        total += matches.len();
        let rendered: Vec<Value> = matches
            .iter()
            .map(|m| {
                let start = pos(m.start);
                let end = if m.end > m.start {
                    seg_spans[m.end - 1].1
                } else {
                    start
                };
                json!({
                    "segments": [m.start, m.end],
                    "offsets": [start, end],
                    "left": [0, m.start],
                    "right": [m.end, n],
                })
            })
            .collect();
        let mut entry = json!({"word": word, "status": "ok", "matches": rendered});
        if let Some(stop) = stop {
            entry["status"] = json!("incomplete");
            entry["stop"] = json!(stop);
            stop_reason.get_or_insert(stop);
        }
        if nfd != *word {
            entry["normalized"] = json!(nfd);
        }
        out_words.push(entry);
    }
    let mut result = json!({
        "id": id, "type": "result", "op": "match",
        "complete": stop_reason.is_none(),
        "stopReason": stop_reason,
        "words": out_words,
    });
    if let Some(m) = marker {
        result["boundaryMarker"] = json!(m);
    }
    result
}

fn too_many(id: &Value, got: usize, max: usize) -> Value {
    error(
        id,
        "too_many_words",
        format!("{got} words exceed the limit of {max}"),
        json!({"words": got, "maxWords": max}),
    )
}
