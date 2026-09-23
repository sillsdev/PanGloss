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
    UnsupportedSchemaVersion {
        found: u64,
    },
    WrongMode {
        found: String,
    },
    /// A morph entry named no `msaGuid` — the one field this crate's identity model cannot do
    /// without (`crate::model::AnalysisSignature` docs).
    MissingMsaGuid {
        word: String,
        analysis_index: usize,
        morph_index: usize,
    },
    /// `msaGuid` was present but not guid-shaped (a bare hvo, or an unresolved
    /// `"lexEntryHvo.refIndex.msaHvo"` DbRef leaking through) — never trusted as an identity, even
    /// though a caller reading only for absence (`MissingMsaGuid`) would miss it.
    MalformedMsaGuid {
        word: String,
        analysis_index: usize,
        morph_index: usize,
        value: String,
    },
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
            ReadError::MissingMsaGuid {
                word,
                analysis_index,
                morph_index,
            } => write!(
                f,
                "word '{word}', analysis #{analysis_index}, morph #{morph_index}: no msaGuid \
                 (this reader has no display-text fallback for morpheme identity)"
            ),
            ReadError::MalformedMsaGuid {
                word,
                analysis_index,
                morph_index,
                value,
            } => write!(
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
    #[allow(dead_code)]
    // Not consumed by this crate's model; kept so an unrecognized-key check still validates the rest of a real response.
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
    #[allow(dead_code)]
    // Unread here, but must stay a modeled field: deny_unknown_fields would otherwise refuse every real morph object that carries it.
    form: Option<String>,
    #[serde(rename = "msaGuid")]
    msa_guid: Option<String>,
    #[serde(rename = "morphnameOrGloss")]
    morphname_or_gloss: Option<String>,
    #[serde(rename = "type")]
    #[allow(dead_code)]
    // Unread here, but must stay a modeled field: deny_unknown_fields would otherwise refuse every real morph object that carries it.
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
    is_guid(value)
        || value
            .split_once('#')
            .is_some_and(|(a, b)| is_guid(a) && is_guid(b))
}

/// Parse one `parse --out <response.json>` document.
pub fn read_parse_response(json_text: &str) -> Result<ParsedParseResponse, ReadError> {
    let raw: RawResponse = serde_json::from_str(json_text)?;
    if raw.schema_version != SUPPORTED_SCHEMA_VERSION {
        return Err(ReadError::UnsupportedSchemaVersion {
            found: raw.schema_version,
        });
    }
    if raw.mode != EXPECTED_MODE {
        return Err(ReadError::WrongMode { found: raw.mode });
    }

    let mut words = Vec::with_capacity(raw.words.len());
    for raw_word in raw.words {
        let result = read_word(&raw_word)?;
        words.push((raw_word.word, result));
    }
    Ok(ParsedParseResponse {
        database: raw.database,
        words,
    })
}

fn read_word(raw_word: &RawWord) -> Result<XampleResult, ReadError> {
    let mut analyses: BTreeMap<AnalysisSignature, usize> = BTreeMap::new();
    for (analysis_index, raw_analysis) in raw_word.analyses.iter().enumerate() {
        let signature = read_analysis(&raw_word.word, analysis_index, raw_analysis)?;
        *analyses.entry(signature).or_insert(0) += 1;
    }
    let reached_max_analyses = raw_word
        .reached_max_analyses
        .then_some(raw_word.analyses.len());
    Ok(XampleResult {
        analyses,
        reached_max_analyses,
        engine_error: raw_word.engine_error.clone(),
    })
}

fn read_analysis(
    word: &str,
    analysis_index: usize,
    raw: &RawAnalysis,
) -> Result<AnalysisSignature, ReadError> {
    let mut morphemes = Vec::with_capacity(raw.morphemes.len());
    let mut msa_ids = Vec::with_capacity(raw.morphemes.len());
    for (morph_index, morph) in raw.morphemes.iter().enumerate() {
        let msa_guid = morph
            .msa_guid
            .clone()
            .ok_or_else(|| ReadError::MissingMsaGuid {
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
mod tests;
