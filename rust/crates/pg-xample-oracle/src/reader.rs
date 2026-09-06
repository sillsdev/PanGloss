//! Reads `xample-projector parse`'s JSON response (`tools/xample-projector/ParseCommand.cs`,
//! `README.md`'s `parse` contract) into [`crate::model`] values.
//!
//! Every shape here mirrors that contract exactly and refuses anything it does not name:
//! `serde(deny_unknown_fields)` on every struct that models a real object in the response, a
//! required (non-`Option`) field for everything the contract always emits, and an explicit check
//! on `schemaVersion`/`mode` naming what was expected and what was found. A schema this reader
//! does not understand is a refusal, never a best-effort guess.

use crate::model::{AnalysisSignature, XampleResult};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fmt;

const SUPPORTED_SCHEMA_VERSION: u64 = 1;
const EXPECTED_MODE: &str = "parse";

/// Everything the `parse` response's `words[]` was read into, in document order.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedParseResponse {
    pub database: String,
    /// `(word, result)` per entry, in the response's own order. A `Vec`, not a `BTreeMap`, because
    /// the contract never promises `words[].word` is unique and collapsing on word text would
    /// silently drop a real repeated-word run.
    pub words: Vec<(String, XampleResult)>,
}

#[derive(Debug)]
pub enum ReadError {
    Malformed(serde_json::Error),
    UnsupportedSchemaVersion { found: u64 },
    WrongMode { found: String },
    /// A morph entry named no `msaGuid` — the one field this crate's identity model cannot do
    /// without (`crate::model::AnalysisSignature` docs).
    MissingMsaGuid { word: String, analysis_index: usize, morph_index: usize },
    /// `msaGuid` was present but not guid-shaped (a bare hvo, or an unresolved
    /// `"lexEntryHvo.refIndex.msaHvo"` DbRef leaking through) — never trusted as an identity, even
    /// though a caller reading only for absence (`MissingMsaGuid`) would miss it.
    MalformedMsaGuid { word: String, analysis_index: usize, morph_index: usize, value: String },
}

impl fmt::Display for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReadError::Malformed(e) => write!(f, "malformed parse response: {e}"),
            ReadError::UnsupportedSchemaVersion { found } => write!(
                f,
                "parse response schemaVersion {found} is not supported (this reader only \
                 understands {SUPPORTED_SCHEMA_VERSION})"
            ),
            ReadError::WrongMode { found } => write!(
                f,
                "expected a '{EXPECTED_MODE}' response, found mode '{found}'"
            ),
            ReadError::MissingMsaGuid { word, analysis_index, morph_index } => write!(
                f,
                "word '{word}', analysis #{analysis_index}, morph #{morph_index}: no msaGuid \
                 (this reader has no display-text fallback for morpheme identity)"
            ),
            ReadError::MalformedMsaGuid { word, analysis_index, morph_index, value } => write!(
                f,
                "word '{word}', analysis #{analysis_index}, morph #{morph_index}: msaGuid \
                 '{value}' is not guid-shaped (expected a guid or 'guid#guid')"
            ),
        }
    }
}

impl std::error::Error for ReadError {}

impl From<serde_json::Error> for ReadError {
    fn from(e: serde_json::Error) -> Self {
        ReadError::Malformed(e)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawResponse {
    #[serde(rename = "schemaVersion")]
    schema_version: u64,
    mode: String,
    database: String,
    #[serde(rename = "engineVersion")]
    #[allow(dead_code)] // Not consumed by this crate's model; kept so an unrecognized-key check still validates the rest of a real response.
    engine_version: Option<String>,
    #[allow(dead_code)]
    parameters: serde_json::Value,
    words: Vec<RawWord>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawWord {
    word: String,
    analyses: Vec<RawAnalysis>,
    #[serde(rename = "reachedMaxAnalyses")]
    reached_max_analyses: bool,
    #[serde(rename = "engineError")]
    engine_error: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawAnalysis {
    morphemes: Vec<RawMorph>,
    #[serde(rename = "categoryId")]
    category_id: Option<String>,
    #[serde(rename = "surfaceNfd")]
    surface_nfd: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawMorph {
    #[allow(dead_code)] // Unread here, but must stay a modeled field: deny_unknown_fields would otherwise refuse every real morph object that carries it.
    form: Option<String>,
    #[serde(rename = "msaGuid")]
    msa_guid: Option<String>,
    #[serde(rename = "morphnameOrGloss")]
    morphname_or_gloss: Option<String>,
    #[serde(rename = "type")]
    #[allow(dead_code)] // Unread here, but must stay a modeled field: deny_unknown_fields would otherwise refuse every real morph object that carries it.
    kind: Option<String>,
}

/// A trusted `msaGuid` shape: one bare 36-char guid, or two joined by `#` (`crate::model::AnalysisSignature`'s own doc names why).
fn is_guid_shaped_msa_id(value: &str) -> bool {
    fn is_guid(s: &str) -> bool {
        let bytes = s.as_bytes();
        bytes.len() == 36
            && bytes.iter().enumerate().all(|(i, &b)| {
                if matches!(i, 8 | 13 | 18 | 23) {
                    b == b'-'
                } else {
                    b.is_ascii_hexdigit()
                }
            })
    }
    is_guid(value) || value.split_once('#').is_some_and(|(a, b)| is_guid(a) && is_guid(b))
}

/// Parse one `parse --out <response.json>` document.
pub fn read_parse_response(json_text: &str) -> Result<ParsedParseResponse, ReadError> {
    let raw: RawResponse = serde_json::from_str(json_text)?;
    if raw.schema_version != SUPPORTED_SCHEMA_VERSION {
        return Err(ReadError::UnsupportedSchemaVersion { found: raw.schema_version });
    }
    if raw.mode != EXPECTED_MODE {
        return Err(ReadError::WrongMode { found: raw.mode });
    }

    let mut words = Vec::with_capacity(raw.words.len());
    for raw_word in raw.words {
        let result = read_word(&raw_word)?;
        words.push((raw_word.word, result));
    }
    Ok(ParsedParseResponse { database: raw.database, words })
}

fn read_word(raw_word: &RawWord) -> Result<XampleResult, ReadError> {
    let mut analyses: BTreeMap<AnalysisSignature, usize> = BTreeMap::new();
    for (analysis_index, raw_analysis) in raw_word.analyses.iter().enumerate() {
        let signature = read_analysis(&raw_word.word, analysis_index, raw_analysis)?;
        *analyses.entry(signature).or_insert(0) += 1;
    }
    let reached_max_analyses = raw_word.reached_max_analyses.then_some(raw_word.analyses.len());
    Ok(XampleResult { analyses, reached_max_analyses, engine_error: raw_word.engine_error.clone() })
}

fn read_analysis(
    word: &str,
    analysis_index: usize,
    raw: &RawAnalysis,
) -> Result<AnalysisSignature, ReadError> {
    let mut morphemes = Vec::with_capacity(raw.morphemes.len());
    let mut msa_ids = Vec::with_capacity(raw.morphemes.len());
    for (morph_index, morph) in raw.morphemes.iter().enumerate() {
        let msa_guid = morph.msa_guid.clone().ok_or_else(|| ReadError::MissingMsaGuid {
            word: word.to_string(),
            analysis_index,
            morph_index,
        })?;
        if !is_guid_shaped_msa_id(&msa_guid) {
            return Err(ReadError::MalformedMsaGuid {
                word: word.to_string(),
                analysis_index,
                morph_index,
                value: msa_guid,
            });
        }
        msa_ids.push(msa_guid);
        morphemes.push(morph.morphname_or_gloss.clone().unwrap_or_default());
    }
    Ok(AnalysisSignature {
        morphemes,
        msa_ids,
        category_id: raw.category_id.clone(),
        surface_nfd: raw.surface_nfd.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIMPLE: &str = include_str!("../tests/data/captured-parse-simple.json");
    const MANY: &str = include_str!("../tests/data/captured-parse-many.json");
    const DUPLICATE: &str = include_str!("../tests/data/synthetic-duplicate-analyses.json");
    const CAPPED: &str = include_str!("../tests/data/synthetic-capped-result.json");
    const HVO_SHAPED: &str = include_str!("../tests/data/synthetic-hvo-shaped-msa-guid.json");

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
        assert_eq!(signature.msa_ids, vec!["d71a9c35-5dd1-4659-997a-b3ad043e06f2".to_string()]);
        assert_eq!(signature.morphemes, vec!["K".to_string()]);
        assert_eq!(signature.surface_nfd, "k");
    }

    #[test]
    fn many_analyses_word_keeps_every_distinct_signature() {
        let parsed = read_parse_response(SIMPLE).expect("captured fixture must parse");
        let xk = word_result(&parsed, "xk");
        assert_eq!(xk.analyses.len(), 12, "12 distinct prefix choices, none deduplicated");
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
        let err = bad.engine_error.as_deref().expect("real captured engine error");
        assert!(err.contains("multiple root elements"), "unexpected error text: {err}");
    }

    #[test]
    fn synthesized_duplicate_analyses_are_counted_not_collapsed() {
        let parsed = read_parse_response(DUPLICATE).expect("synthetic fixture must parse");
        let word = word_result(&parsed, "dup");
        assert_eq!(word.analyses.len(), 1, "one distinct signature");
        assert_eq!(word.analyses.values().sum::<usize>(), 2, "reported twice by the engine");
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
        assert!(matches!(err, ReadError::UnsupportedSchemaVersion { found: 2 }));
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
        let err = read_parse_response(&text).expect_err("a response missing 'database' must be refused");
        assert!(matches!(err, ReadError::Malformed(_)));
    }

    #[test]
    fn unrecognized_key_is_refused() {
        let value: serde_json::Value = serde_json::from_str(SIMPLE).unwrap();
        let mut object = value.as_object().unwrap().clone();
        object.insert("somethingNewAndUnknown".to_string(), serde_json::json!(true));
        let text = serde_json::to_string(&object).unwrap();
        let err = read_parse_response(&text).expect_err("an unrecognized top-level key must be refused");
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
        let err =
            read_parse_response(HVO_SHAPED).expect_err("an hvo-shaped msaGuid must be refused");
        assert!(matches!(err, ReadError::MalformedMsaGuid { ref value, .. } if value == "123456"));
    }
}
