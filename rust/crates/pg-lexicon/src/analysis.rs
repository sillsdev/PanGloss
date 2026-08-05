//! Shared official-plus-runtime lexical analysis orchestration for native and WASM bindings.

use crate::{EntryAuthority, Revision, SuppliedLexiconRuntime, ValidationState};
use pg_parse::{AnalysisProvenance, ParseOptions, WordAnalysis};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Construction-time policy for every authoritative overlay-aware `Morpher` owned by a runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnalysisPolicy {
    pub step_cap: usize,
    pub memo: bool,
}

impl AnalysisPolicy {
    pub const fn browser_default() -> Self {
        Self {
            step_cap: 100_000,
            memo: true,
        }
    }

    pub const fn native_abi_v1() -> Self {
        Self {
            step_cap: 500_000,
            memo: true,
        }
    }
}

impl Default for AnalysisPolicy {
    fn default() -> Self {
        Self::browser_default()
    }
}

#[derive(Debug, Clone)]
pub struct OfficialOutcome {
    pub analyses: Vec<(String, String)>,
    pub structured: Vec<WordAnalysis>,
    pub candidates_generated: usize,
}

#[derive(Debug, Clone)]
pub struct UnifiedAnalysis {
    pub analyses: Vec<(String, String)>,
    pub structured: Vec<WordAnalysis>,
    pub capped: bool,
    pub invalid_shape: bool,
    pub timed_out: bool,
    pub guessed: bool,
    pub candidates_generated: usize,
    pub revision: Revision,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisCacheKey {
    pub revision: Revision,
    pub word: String,
}

#[derive(Default)]
pub struct AnalysisCache {
    entries: BTreeMap<AnalysisCacheKey, UnifiedAnalysis>,
}

impl AnalysisCache {
    pub fn get(&self, revision: &Revision, word: &str) -> Option<&UnifiedAnalysis> {
        self.entries.get(&AnalysisCacheKey {
            revision: revision.clone(),
            word: word.to_string(),
        })
    }
    pub fn insert(&mut self, value: UnifiedAnalysis, word: String) {
        let revision = value.revision.clone();
        self.entries.retain(|key, _| key.revision == revision);
        self.entries
            .insert(AnalysisCacheKey { revision, word }, value);
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl SuppliedLexiconRuntime {
    /// Unions a confirmed grammar-only proposer result with the overlay-aware authoritative engine,
    /// with the guesser retry left **off** (`guess_fallback: false` — see
    /// `Self::analyze_word_opts`). This is the shape every existing caller of this exact method
    /// name gets: in particular `hc_parse_word`/`hc_parse_batch`'s wire format
    /// (`pg-ffi::buffer::MAGIC`) has no `guessed` field at all, so those two entry points must
    /// never see a guessed analysis out of this call, on pain of returning a guess
    /// wire-indistinguishable from a confirmed analysis (the overclaim this method's `_opts`
    /// sibling exists to let a caller safely opt back into — see that method's own doc for why the
    /// retry is opt-in rather than unconditional).
    pub fn analyze_word(&self, word: &str, official: Option<OfficialOutcome>) -> UnifiedAnalysis {
        self.analyze_word_opts(word, official, false)
    }

    /// Unions a confirmed grammar-only proposer result with the overlay-aware authoritative engine.
    /// `official = None` means no proposer is available, so the engine supplies official analyses
    /// too. `Some`, including an empty confirmed result, means the proposer is authoritative for
    /// official roots; the engine contributes supplied roots only.
    ///
    /// `guess_fallback` controls whether a *total* miss (the complete union above comes back empty
    /// on a validly-shaped word) retries once through the guesser. This used to be unconditional
    /// (always on) here; it is now explicit opt-in, defaulting to off in `Self::analyze_word`,
    /// because a guessed analysis is only ever safe to hand back through a wire format that can
    /// *mark* it as guessed (`WordAnalysis::guessed`/`UnifiedAnalysis::guessed` are carried
    /// correctly regardless of this flag — see each field's own doc — but a caller encoding into a
    /// format with no `guessed` bit at all must keep this off, or it silently reintroduces a
    /// guessed analysis as if it were confirmed). Callers that can honestly express `guessed` in
    /// their own output (the `pg-ffi` JSON API, `pg-wasm`'s bindings) pass `true` explicitly.
    pub fn analyze_word_opts(
        &self,
        word: &str,
        official: Option<OfficialOutcome>,
        guess_fallback: bool,
    ) -> UnifiedAnalysis {
        let snapshot = self.snapshot();
        let morpher = self.morpher(&snapshot);
        let normal_options = ParseOptions::default();
        let normal = morpher.parse_word_opts(word, &normal_options);
        let overridden = active_override_ids(snapshot.entries());
        let overridden: Vec<&str> = overridden.iter().map(String::as_str).collect();
        let mut analyses = Vec::new();
        let mut structured = Vec::new();
        let mut candidates_generated = normal.candidates_generated;
        if let Some(official) = official {
            candidates_generated += official.candidates_generated;
            for (pair, analysis) in official.analyses.into_iter().zip(official.structured) {
                if !analysis_is_overridden(&self.grammar, &analysis, &overridden) {
                    push_unique(&mut analyses, &mut structured, pair, analysis);
                }
            }
            for (pair, analysis) in normal.analyses.into_iter().zip(normal.structured) {
                if !matches!(analysis.provenance, AnalysisProvenance::Grammar) {
                    push_unique(&mut analyses, &mut structured, pair, analysis);
                }
            }
        } else {
            for (pair, analysis) in normal.analyses.into_iter().zip(normal.structured) {
                push_unique(&mut analyses, &mut structured, pair, analysis);
            }
        }

        let mut guessed = false;
        let mut capped = normal.capped;
        let mut invalid_shape = normal.invalid_shape;
        let mut timed_out = normal.timed_out;
        if structured.is_empty() && !invalid_shape && guess_fallback {
            let guess_options = ParseOptions::default().with_guess_only(true);
            let fallback = morpher.parse_word_opts(word, &guess_options);
            candidates_generated += fallback.candidates_generated;
            analyses.clear();
            structured.clear();
            for (pair, analysis) in fallback.analyses.into_iter().zip(fallback.structured) {
                push_unique(&mut analyses, &mut structured, pair, analysis);
            }
            guessed = fallback.guessed;
            capped |= fallback.capped;
            invalid_shape |= fallback.invalid_shape;
            timed_out |= fallback.timed_out;
        }
        UnifiedAnalysis {
            analyses,
            structured,
            capped,
            invalid_shape,
            timed_out,
            guessed,
            candidates_generated,
            revision: snapshot.revision().clone(),
        }
    }
}

fn active_override_ids(entries: &[crate::SuppliedEntry]) -> Vec<String> {
    entries
        .iter()
        .filter(|entry| matches!(entry.state, ValidationState::Active))
        .filter_map(|entry| match &entry.authority {
            EntryAuthority::SuppliedOverride {
                official_entry_id, ..
            } => Some(official_entry_id.clone()),
            EntryAuthority::Supplied => None,
        })
        .collect()
}

fn push_unique(
    pairs: &mut Vec<(String, String)>,
    structured: &mut Vec<WordAnalysis>,
    pair: (String, String),
    analysis: WordAnalysis,
) {
    if structured
        .iter()
        .zip(pairs.iter())
        .any(|(existing_analysis, existing_pair)| {
            existing_analysis == &analysis && existing_pair == &pair
        })
    {
        return;
    }
    pairs.push(pair);
    structured.push(analysis);
}

fn analysis_is_overridden(
    grammar: &pg_grammar::model::Grammar,
    analysis: &WordAnalysis,
    overridden: &[&str],
) -> bool {
    let Ok(root_index) = usize::try_from(analysis.root_morpheme_index) else {
        return false;
    };
    let Some(&root_morpheme) = analysis.morpheme_ids.get(root_index) else {
        return false;
    };
    grammar.entries.iter().any(|entry| {
        entry.morpheme.0 == root_morpheme && overridden.contains(&entry.authored_id.as_str())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn named_analysis_policies_pin_host_contracts() {
        assert_eq!(
            AnalysisPolicy::browser_default(),
            AnalysisPolicy {
                step_cap: 100_000,
                memo: true
            }
        );
        assert_eq!(
            AnalysisPolicy::native_abi_v1(),
            AnalysisPolicy {
                step_cap: 500_000,
                memo: true
            }
        );
    }
    #[test]
    fn inactive_override_never_suppresses_official_identity() {
        let date = crate::LexicalDate::parse("2026-07-22 00:00:00.000").unwrap();
        let entry = crate::SuppliedEntry {
            id: crate::EntryId::from_bytes([1; 16]),
            stem: "x".into(),
            gloss: String::new(),
            signatures: vec![],
            date_created: date.clone(),
            date_modified: date,
            authority: EntryAuthority::SuppliedOverride {
                official_entry_id: "official".into(),
                note: None,
            },
            state: ValidationState::Inactive {
                diagnostics: vec!["missing signature".into()],
            },
        };
        assert!(active_override_ids(&[entry]).is_empty());
    }
}
