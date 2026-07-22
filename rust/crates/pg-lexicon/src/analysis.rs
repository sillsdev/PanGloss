//! Shared official-plus-runtime lexical analysis orchestration for native and WASM bindings.

use crate::{EntryAuthority, Revision, SuppliedLexiconRuntime};
use pg_parse::{AnalysisProvenance, Morpher, ParseOptions, WordAnalysis};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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
    /// Unions a confirmed grammar-only proposer result with the overlay-aware authoritative engine.
    /// `official = None` means no proposer is available, so the engine supplies official analyses
    /// too. `Some`, including an empty confirmed result, means the proposer is authoritative for
    /// official roots; the engine contributes supplied roots only. Guessing is retried exactly once
    /// and only after that complete union is empty.
    pub fn analyze_word(&self, word: &str, official: Option<OfficialOutcome>) -> UnifiedAnalysis {
        let snapshot = self.snapshot();
        let morpher = Morpher::new_with_overlay(&self.grammar, 100_000, snapshot.overlay());
        let normal_options = ParseOptions::default();
        let normal = morpher.parse_word_opts(word, &normal_options);
        let overridden: Vec<&str> = snapshot
            .entries()
            .iter()
            .filter_map(|entry| match &entry.authority {
                EntryAuthority::SuppliedOverride {
                    official_entry_id, ..
                } => Some(official_entry_id.as_str()),
                EntryAuthority::Supplied => None,
            })
            .collect();

        let mut analyses = Vec::new();
        let mut structured = Vec::new();
        let mut candidates_generated = normal.candidates_generated;
        if let Some(official) = official {
            candidates_generated += official.candidates_generated;
            for (pair, analysis) in official.analyses.into_iter().zip(official.structured) {
                if !analysis_is_overridden(&self.grammar, &analysis, &overridden) {
                    analyses.push(pair);
                    structured.push(analysis);
                }
            }
            for (pair, analysis) in normal.analyses.into_iter().zip(normal.structured) {
                if !matches!(analysis.provenance, AnalysisProvenance::Grammar) {
                    analyses.push(pair);
                    structured.push(analysis);
                }
            }
        } else {
            analyses = normal.analyses;
            structured = normal.structured;
        }

        let mut guessed = false;
        let mut capped = normal.capped;
        let mut invalid_shape = normal.invalid_shape;
        let mut timed_out = normal.timed_out;
        if structured.is_empty() && !invalid_shape {
            let mut guess_options = ParseOptions::default();
            guess_options.guess_root = true;
            let fallback = morpher.parse_word_opts(word, &guess_options);
            candidates_generated += fallback.candidates_generated;
            analyses = fallback.analyses;
            structured = fallback.structured;
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
