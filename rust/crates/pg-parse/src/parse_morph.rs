//! Projects confirmed analyses into the source-identity morphology consumed by FieldWorks hosts.
//!
//! Dense compiler ids are accepted only as lookup handles. Every emitted source identity must come
//! from grammar metadata, and circumfix halves must have two independently ordered occurrences.

use crate::WordAnalysis;
use pg_grammar_model::model::{Grammar, MorphemeId, SourceMorphPlacement};
use serde::{Deserialize, Serialize};

/// The versioned FieldWorks-compatible morphology projection.
pub const PARSE_ANALYSIS_PROFILE: &str = "fieldworks-parse-analysis/v1";

/// One confirmed analysis in surface morphological order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ParseAnalysis {
    pub morphs: Vec<ParseMorph>,
}

/// One FieldWorks `ParseMorph`-shaped slot.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ParseMorph {
    /// The source MoForm GUID, not a rendered surface string.
    pub form: Option<String>,
    /// The source MSA GUID. A projected morph always has an authored identity.
    pub msa: Option<String>,
    /// The source LexEntryInflType GUID when this morph is variant-specific.
    pub infl_type: Option<String>,
    /// The exact fabricated-root string for a guessed morph.
    pub guessed_string: Option<String>,
}

/// Why a confirmed analysis cannot be represented with authoritative source identities.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParseProjectionError {
    MissingOccurrences {
        expected: usize,
    },
    UnresolvedMorpheme {
        ordinal: u32,
    },
    UnresolvedAllomorph {
        ordinal: u32,
    },
    MissingMsa {
        ordinal: u32,
    },
    MissingSourceForm {
        allomorph: u32,
        slot: usize,
    },
    UnsupportedSourceFormArity {
        allomorph: u32,
        count: usize,
    },
    MissingCircumfixOccurrence {
        allomorph: u32,
        occurrence: usize,
    },
    GuessedStringUnavailable,
    UnsupportedRuntimeRoot,
    NonCanonicalSourceGuid {
        kind: &'static str,
        ordinal: u32,
        slot: Option<usize>,
    },
}

impl std::fmt::Display for ParseProjectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingOccurrences { expected } => write!(
                f,
                "analysis has {expected} morpheme ids but no raw annotation occurrences"
            ),
            Self::UnresolvedMorpheme { ordinal } => {
                write!(f, "morpheme ordinal {ordinal} has no grammar row")
            }
            Self::UnresolvedAllomorph { ordinal } => {
                write!(f, "allomorph ordinal {ordinal} has no grammar source metadata")
            }
            Self::MissingMsa { ordinal } => write!(
                f,
                "morpheme ordinal {ordinal} has no authoritative source MSA GUID"
            ),
            Self::MissingSourceForm { allomorph, slot } => write!(
                f,
                "allomorph ordinal {allomorph} has no authoritative source form GUID for slot {slot}"
            ),
            Self::UnsupportedSourceFormArity { allomorph, count } => write!(
                f,
                "allomorph ordinal {allomorph} has unsupported source form arity {count}"
            ),
            Self::MissingCircumfixOccurrence {
                allomorph,
                occurrence,
            } => write!(
                f,
                "circumfix allomorph ordinal {allomorph} is missing occurrence {occurrence}"
            ),
            Self::GuessedStringUnavailable => {
                write!(f, "guessed morph has no exact guessed string")
            }
            Self::UnsupportedRuntimeRoot => write!(
                f,
                "runtime root has no authored MoForm or MSA identity"
            ),
            Self::NonCanonicalSourceGuid {
                kind,
                ordinal,
                slot,
            } => match slot {
                Some(slot) => write!(
                    f,
                    "{kind} identity at ordinal {ordinal}, slot {slot} is not a canonical GUID"
                ),
                None => write!(
                    f,
                    "{kind} identity at ordinal {ordinal} is not a canonical GUID"
                ),
            },
        }
    }
}

impl std::error::Error for ParseProjectionError {}

fn is_canonical_guid(value: &str) -> bool {
    if value.len() != 36 {
        return false;
    }
    let valid_shape = value.bytes().enumerate().all(|(index, byte)| {
        if matches!(index, 8 | 13 | 18 | 23) {
            byte == b'-'
        } else {
            byte.is_ascii_digit() || matches!(byte, b'a'..=b'f')
        }
    });
    valid_shape
        && value
            .bytes()
            .enumerate()
            .any(|(index, byte)| !matches!(index, 8 | 13 | 18 | 23) && byte != b'0')
}

fn require_guid(
    value: String,
    kind: &'static str,
    ordinal: u32,
    slot: Option<usize>,
) -> Result<String, ParseProjectionError> {
    if is_canonical_guid(&value) {
        Ok(value)
    } else {
        Err(ParseProjectionError::NonCanonicalSourceGuid {
            kind,
            ordinal,
            slot,
        })
    }
}

/// Projects one parser result into source GUIDs while preserving annotation order.
pub fn project_parse_analysis(
    analysis: &WordAnalysis,
    grammar: &Grammar,
) -> Result<ParseAnalysis, ParseProjectionError> {
    if analysis.morph_occurrences.is_empty() && !analysis.morpheme_ids.is_empty() {
        return Err(ParseProjectionError::MissingOccurrences {
            expected: analysis.morpheme_ids.len(),
        });
    }

    let mut occurrences = analysis.morph_occurrences.clone();
    occurrences.sort_by_key(|occurrence| occurrence.order);
    let mut used = vec![false; occurrences.len()];
    let mut events: Vec<(u32, usize, SourceMorphPlacement, ParseMorph)> = Vec::new();
    let mut ordinary_morphemes = Vec::new();

    for i in 0..occurrences.len() {
        if used[i] {
            continue;
        }
        let occurrence = &occurrences[i];
        if occurrence.morpheme_id == MorphemeId::GUESSED.0 || occurrence.allomorph_id == u32::MAX {
            return Err(ParseProjectionError::UnsupportedRuntimeRoot);
        }

        let source = grammar
            .allomorph_sources
            .get(occurrence.allomorph_id as usize)
            .ok_or(ParseProjectionError::UnresolvedAllomorph {
                ordinal: occurrence.allomorph_id,
            })?;
        if source.omitted {
            for (j, candidate) in occurrences.iter().enumerate().skip(i) {
                if candidate.allomorph_id == occurrence.allomorph_id
                    && candidate.morpheme_id == occurrence.morpheme_id
                {
                    used[j] = true;
                }
            }
            continue;
        }
        if source.form_guids.len() == 1 && ordinary_morphemes.contains(&occurrence.morpheme_id) {
            for (j, candidate) in occurrences.iter().enumerate().skip(i) {
                if candidate.allomorph_id == occurrence.allomorph_id
                    && candidate.morpheme_id == occurrence.morpheme_id
                {
                    used[j] = true;
                }
            }
            continue;
        }
        let info = grammar
            .morphemes
            .get(occurrence.morpheme_id as usize)
            .ok_or(ParseProjectionError::UnresolvedMorpheme {
                ordinal: occurrence.morpheme_id,
            })?;
        let msa = info
            .source_msa_guid
            .as_deref()
            .filter(|guid| !guid.is_empty())
            .map(str::to_string)
            .ok_or(ParseProjectionError::MissingMsa {
                ordinal: occurrence.morpheme_id,
            })?;
        let msa = require_guid(msa, "MSA", occurrence.morpheme_id, None)?;
        let infl_type = info
            .source_infl_type_guid
            .clone()
            .map(|guid| require_guid(guid, "InflType", occurrence.morpheme_id, None))
            .transpose()?;
        match source.form_guids.len() {
            0 => {
                return Err(ParseProjectionError::UnsupportedSourceFormArity {
                    allomorph: occurrence.allomorph_id,
                    count: 0,
                });
            }
            1 => {
                let form = source.form_guids[0].clone().ok_or(
                    ParseProjectionError::MissingSourceForm {
                        allomorph: occurrence.allomorph_id,
                        slot: 0,
                    },
                )?;
                let form = require_guid(form, "MoForm", occurrence.allomorph_id, Some(0))?;
                used[i] = true;
                ordinary_morphemes.push(occurrence.morpheme_id);
                for (j, candidate) in occurrences.iter().enumerate().skip(i + 1) {
                    if candidate.allomorph_id == occurrence.allomorph_id
                        && candidate.morpheme_id == occurrence.morpheme_id
                    {
                        used[j] = true;
                    }
                }
                events.push((
                    occurrence.order,
                    i,
                    source.placement,
                    ParseMorph {
                        form: Some(form),
                        msa: Some(msa),
                        infl_type,
                        guessed_string: None,
                    },
                ));
            }
            2 => {
                let j = occurrences
                    .iter()
                    .enumerate()
                    .skip(i + 1)
                    .find_map(|(j, candidate)| {
                        (!used[j]
                            && candidate.allomorph_id == occurrence.allomorph_id
                            && candidate.morpheme_id == occurrence.morpheme_id)
                            .then_some(j)
                    });
                let j = j.ok_or(ParseProjectionError::MissingCircumfixOccurrence {
                    allomorph: occurrence.allomorph_id,
                    occurrence: 1,
                })?;
                let first = source.form_guids[0].clone().ok_or(
                    ParseProjectionError::MissingSourceForm {
                        allomorph: occurrence.allomorph_id,
                        slot: 0,
                    },
                )?;
                let first = require_guid(first, "MoForm", occurrence.allomorph_id, Some(0))?;
                let second = source.form_guids[1].clone().ok_or(
                    ParseProjectionError::MissingSourceForm {
                        allomorph: occurrence.allomorph_id,
                        slot: 1,
                    },
                )?;
                let second = require_guid(second, "MoForm", occurrence.allomorph_id, Some(1))?;
                used[i] = true;
                used[j] = true;
                events.push((
                    occurrence.order,
                    i,
                    source.placement,
                    ParseMorph {
                        form: Some(first),
                        msa: Some(msa.clone()),
                        infl_type: infl_type.clone(),
                        guessed_string: None,
                    },
                ));
                events.push((
                    occurrences[j].order,
                    j,
                    source.placement,
                    ParseMorph {
                        form: Some(second),
                        msa: Some(msa),
                        infl_type,
                        guessed_string: None,
                    },
                ));
            }
            count => {
                return Err(ParseProjectionError::UnsupportedSourceFormArity {
                    allomorph: occurrence.allomorph_id,
                    count,
                });
            }
        }
    }

    events.sort_by_key(|(order, ordinal, _, _)| (*order, *ordinal));
    let mut morphs = Vec::with_capacity(events.len());
    for (_, _, placement, morph) in events {
        if placement == SourceMorphPlacement::InsertBeforeLast && !morphs.is_empty() {
            morphs.insert(morphs.len() - 1, morph);
        } else {
            morphs.push(morph);
        }
    }
    Ok(ParseAnalysis { morphs })
}

/// Keeps one projection error attached to each input analysis.
pub fn project_parse_analyses(
    analyses: &[WordAnalysis],
    grammar: &Grammar,
) -> Vec<Result<ParseAnalysis, ParseProjectionError>> {
    analyses
        .iter()
        .map(|analysis| project_parse_analysis(analysis, grammar))
        .collect()
}

#[cfg(test)]
mod tests;
