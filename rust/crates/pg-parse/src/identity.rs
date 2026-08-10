//! Structured analysis identity — a self-contained value of stable source keys.
//!
//! ADR 0006. An identity carries the keys themselves, never a reference resolved against a compiled
//! model, so an analysis stays comparable after the grammar that produced it deleted the morpheme,
//! renamed it, or stopped compiling entirely. `crate::WordAnalysis`'s own `morpheme_ids` and
//! `pos_id` are dense compiler-assigned ordinals that shift whenever authored content is added or
//! reordered (`Grammar::morphemes` is in document order, and so is the part-of-speech symbol
//! table), which is exactly why they cannot serve as identity across two compilations.
//!
//! `guessed` is deliberately absent. It is an annotation reported alongside an identity, not part
//! of it — `CONTEXT.md`'s "Analysis annotation", and
//! `define-grammar-coverage-contract`'s "compared and reported separately for Rust-to-Rust
//! results". Two analyses whose identities match but whose `guessed` differs are the same analysis
//! observed differently.
//!
//! # Why this module lives in `pg-parse` and not in `pg-assess`
//!
//! It was written for `pg-assess` and lived there until the backend runtime needed the SAME
//! projection to express its parity relation. `pg-foma` is the engine and `pg-assess` is the
//! assessment/reporting layer, so `pg-foma -> pg-assess` is a backwards dependency and forking the
//! projection into `pg-foma` would leave two definitions of "the same analysis" free to drift —
//! the one failure this module exists to prevent. This module imports only `pg_grammar::model` and
//! this crate's own `crate::WordAnalysis`, which it is the natural owner of, and BOTH `pg-foma`
//! and `pg-assess` already depend on `pg-parse`. `pg-assess` re-exports it (`pub use
//! pg_parse::identity`), so its public API, schemas, and call sites are unchanged.

use crate::WordAnalysis;
use pg_grammar::model::{Grammar, MorphemeId, SynFeatureKind};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// One morpheme slot of an identity: its stable source key, or `None` for the fabricated root of a
/// guessed analysis, which by construction has no authored source (`MorphemeId::GUESSED` has no
/// `Grammar::morphemes` row at all).
pub type MorphemeKey = Option<String>;

/// The versioned identity profile this crate implements.
pub const IDENTITY_PROFILE: &str = "pangloss.machine-word-analysis/v1";

/// A complete structured analysis identity.
///
/// The serde representation and `Self::to_canonical_value` are deliberately the same shape: a
/// suite's expectations are deserialized identities, and they must digest identically to identities
/// the parser produced or every expectation would silently miss.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AnalysisIdentity {
    /// Ordered stable morpheme keys — `MorphemeInfo::xml_key`, which is the MSA GUID on the
    /// LibLCM path and the `id` attribute on HC XML.
    pub morphemes: Vec<MorphemeKey>,
    /// The root's position within `morphemes`.
    pub root_index: i32,
    /// The stable part-of-speech symbol id, absent when the analysis carries no category.
    pub category: Option<String>,
}

/// Why an analysis could not be projected to stable keys.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdentityError {
    /// A morpheme ordinal has no row in `Grammar::morphemes` — an internal fault, never the ordinary "this grammar deleted a morpheme" case, which identities-as-values makes invisible here.
    UnresolvedMorpheme { ordinal: u32 },
    /// A part-of-speech ordinal has no symbol in the model's POS table.
    UnresolvedCategory { ordinal: u32 },
}

impl std::fmt::Display for IdentityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IdentityError::UnresolvedMorpheme { ordinal } => write!(
                f,
                "morpheme ordinal {ordinal} has no row in this model; an analysis referencing a \
                 morpheme its own model lacks is an internal fault, not a grammar edit"
            ),
            IdentityError::UnresolvedCategory { ordinal } => write!(
                f,
                "part-of-speech ordinal {ordinal} has no symbol in this model's category table"
            ),
        }
    }
}

impl std::error::Error for IdentityError {}

impl AnalysisIdentity {
    /// Project one analysis against the model that produced it.
    pub fn project(analysis: &WordAnalysis, grammar: &Grammar) -> Result<Self, IdentityError> {
        let mut morphemes = Vec::with_capacity(analysis.morpheme_ids.len());
        for &ordinal in &analysis.morpheme_ids {
            if ordinal == MorphemeId::GUESSED.0 {
                // The fabricated root of a guessed parse; never index `Grammar::morphemes` with this sentinel.
                morphemes.push(None);
                continue;
            }
            let info = grammar
                .morphemes
                .get(ordinal as usize)
                .ok_or(IdentityError::UnresolvedMorpheme { ordinal })?;
            morphemes.push(Some(info.xml_key.clone()));
        }

        let category = match analysis.pos_id {
            None => None,
            Some(ordinal) => Some(
                category_key(grammar, ordinal)
                    .ok_or(IdentityError::UnresolvedCategory { ordinal })?,
            ),
        };

        Ok(AnalysisIdentity {
            morphemes,
            root_index: analysis.root_morpheme_index,
            category,
        })
    }

    /// The canonical JSON form hashed by `identity_digest` and embedded in every projection.
    ///
    /// Produced through serde rather than hand-built, so a deserialized expectation and a projected
    /// analysis cannot drift into digesting differently.
    pub fn to_canonical_value(&self) -> Value {
        serde_json::to_value(self).expect("an identity is strings, an integer, and nulls")
    }
}

/// Resolves a dense part-of-speech ordinal to its stable symbol id — the ordinal shifts whenever a part of speech is added upstream, but the symbol id does not.
fn category_key(grammar: &Grammar, ordinal: u32) -> Option<String> {
    let feature = grammar
        .syn_features
        .features
        .get(grammar.syn_features.pos.0 as usize)?;
    match &feature.kind {
        SynFeatureKind::Symbolic { symbols, .. } => {
            symbols.get(ordinal as usize).map(|(id, _)| id.clone())
        }
        SynFeatureKind::Complex => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn identity(
        morphemes: Vec<MorphemeKey>,
        root_index: i32,
        category: Option<&str>,
    ) -> AnalysisIdentity {
        AnalysisIdentity {
            morphemes,
            root_index,
            category: category.map(str::to_string),
        }
    }

    #[test]
    fn identity_ordering_is_total_and_stable() {
        // Analysis sets are sorted before digesting, so `Ord` must be a total order over every field, including the guessed-slot `None`.
        let mut ids = [
            identity(vec![Some("b".into())], 0, None),
            identity(vec![Some("a".into())], 1, Some("noun")),
            identity(vec![Some("a".into())], 0, Some("verb")),
            identity(vec![None], 0, None),
        ];
        ids.sort();
        assert_eq!(ids[0], identity(vec![None], 0, None));
        assert_eq!(ids[1], identity(vec![Some("a".into())], 0, Some("verb")));
        assert_eq!(ids[2], identity(vec![Some("a".into())], 1, Some("noun")));
        assert_eq!(ids[3], identity(vec![Some("b".into())], 0, None));
    }

    #[test]
    fn guessed_slot_and_absent_category_are_distinguishable() {
        // `None` in `morphemes` means "fabricated root"; `None` in `category` means "no POS" — they must not collapse into one another.
        let guessed = identity(vec![None], 0, Some("noun"));
        let uncategorized = identity(vec![Some("m".into())], 0, None);
        assert_ne!(
            guessed.to_canonical_value(),
            uncategorized.to_canonical_value()
        );
    }

    #[test]
    fn canonical_value_names_every_field() {
        let v =
            identity(vec![Some("guid-a".into()), None], 1, Some("guid-noun")).to_canonical_value();
        assert_eq!(v["morphemes"][0], json!("guid-a"));
        assert_eq!(v["morphemes"][1], Value::Null);
        assert_eq!(v["rootIndex"], json!(1));
        assert_eq!(v["category"], json!("guid-noun"));
    }

    #[test]
    fn category_absent_serializes_as_null_not_missing() {
        // A missing key and an explicit null canonicalize differently, so the projection must be consistent about which it emits.
        let v = identity(vec![Some("m".into())], 0, None).to_canonical_value();
        assert_eq!(v.get("category"), Some(&Value::Null));
    }
}
