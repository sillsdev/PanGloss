//! Normalizes `pg-parse`'s own analysis output into [`crate::model`] values, so an HC-Rust run and
//! a `xample-projector parse` capture become comparable through the same [`XampleResult`] shape.
//!
//! This calls `pg_parse::identity::AnalysisIdentity::project` for the stable-key projection rather
//! than re-deriving it: that module already resolves `WordAnalysis::morpheme_ids`/`pos_id` (dense,
//! compiler-assigned ordinals) against `Grammar::morphemes`/the part-of-speech symbol table into
//! the same stable keys the LibLCM/XAMPLE path calls a GUID (see that module's own doc).

use crate::model::{AnalysisSignature, XampleResult};
use pg_grammar::model::Grammar;
use pg_parse::identity::{AnalysisIdentity, IdentityError};
use pg_parse::{ParseOutcome, WordAnalysis};
use std::collections::BTreeMap;
use std::fmt;
use unicode_normalization::UnicodeNormalization;

#[derive(Debug)]
pub enum HcNormalizationError {
    /// `ParseOutcome::guessed`: the analysis's root is fabricated, not an authored morpheme, so it
    /// has no stable key an oracle comparison could ever match against.
    GuessedAnalysesNotComparable,
    /// Defensive: `AnalysisIdentity::project` documents `None` as arising only when
    /// `WordAnalysis::guessed`, which is already refused above — reaching this means that
    /// invariant broke, not that the grammar did anything.
    UnexpectedGuessedSlot,
    Identity(IdentityError),
}

impl fmt::Display for HcNormalizationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HcNormalizationError::GuessedAnalysesNotComparable => write!(
                f,
                "a guessed-root analysis has no authored identity to compare against an oracle"
            ),
            HcNormalizationError::UnexpectedGuessedSlot => write!(
                f,
                "a non-guessed analysis carried a guessed-root identity slot (internal fault)"
            ),
            HcNormalizationError::Identity(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for HcNormalizationError {}

/// Normalize a whole word's [`ParseOutcome`] into the same multiset shape a `parse` capture reads
/// into (`crate::reader::read_parse_response`), so the two are directly comparable. `word` is the
/// literal surface both engines were asked to parse -- used for every analysis's `surface_nfd`
/// rather than any per-analysis resynthesis `outcome` itself carries, per
/// [`crate::model::AnalysisSignature::surface_nfd`]'s own doc ("the NFD-normalized surface form
/// both engines were asked to parse"). This matters beyond phrasing: a resynthesized surface can
/// legitimately carry an engine's own internal morph-boundary marker for a multi-affix chain
/// (measured against `edge-cases/deep-optional-affix-nesting`'s 12-slot case) that the literal
/// input word never had, which would fabricate a divergence against XAMPLE's own clean surface
/// report for the identical input.
pub fn xample_result_from_hc_outcome(
    outcome: &ParseOutcome,
    grammar: &Grammar,
    word: &str,
) -> Result<XampleResult, HcNormalizationError> {
    if outcome.guessed {
        return Err(HcNormalizationError::GuessedAnalysesNotComparable);
    }
    let surface_nfd: String = word.nfd().collect();
    let mut analyses: BTreeMap<AnalysisSignature, usize> = BTreeMap::new();
    for analysis in &outcome.structured {
        let signature = signature_from_word_analysis(analysis, &surface_nfd, grammar)?;
        *analyses.entry(signature).or_insert(0) += 1;
    }
    let reached_max_analyses = outcome.capped.then_some(outcome.structured.len());
    let engine_error = if outcome.invalid_shape {
        Some("invalid shape: surface word did not segment".to_string())
    } else if outcome.timed_out {
        Some("word timeout exceeded".to_string())
    } else {
        None
    };
    Ok(XampleResult {
        analyses,
        reached_max_analyses,
        engine_error,
    })
}

fn signature_from_word_analysis(
    analysis: &WordAnalysis,
    surface_nfd: &str,
    grammar: &Grammar,
) -> Result<AnalysisSignature, HcNormalizationError> {
    let identity =
        AnalysisIdentity::project(analysis, grammar).map_err(HcNormalizationError::Identity)?;
    // identity.root_index is dropped: the XAMPLE side has no root-position field either, so two HC reduplication analyses differing only in root attachment would collapse to one signature -- a comparability ceiling, not a bug.
    let mut msa_ids = Vec::with_capacity(identity.morphemes.len());
    for key in identity.morphemes {
        msa_ids.push(key.ok_or(HcNormalizationError::UnexpectedGuessedSlot)?);
    }
    let morphemes = analysis
        .morpheme_ids
        .iter()
        .map(|&ordinal| gloss_of(grammar, ordinal))
        .collect();
    Ok(AnalysisSignature {
        morphemes,
        msa_ids,
        category_id: identity.category,
        surface_nfd: surface_nfd.to_string(),
    })
}

/// A morpheme's display gloss, empty (never the stable key) when the grammar names none.
fn gloss_of(grammar: &Grammar, ordinal: u32) -> String {
    grammar
        .morphemes
        .get(ordinal as usize)
        .and_then(|m| m.gloss.clone())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests;
