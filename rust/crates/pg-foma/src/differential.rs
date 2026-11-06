//! Occurrence-preserving proposal recall and confirmed multiset comparison against HC.
use std::collections::BTreeMap;

use pg_grammar::model::Grammar;
use pg_parse::identity::AnalysisIdentity;
use pg_parse::{Morpher, WordAnalysis};
use serde::{Deserialize, Serialize};

use crate::composite::{FomaAnalyzer, ProfiledFomaApplyOutcomeWithCandidates};
use pg_foma_runtime::compose_budget::ApplyBudget;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdentityMultiplicity {
    pub identity: AnalysisIdentity,
    pub count: usize,
}

pub fn multiset(
    analyses: &[WordAnalysis],
    grammar: &Grammar,
) -> Result<Vec<IdentityMultiplicity>, String> {
    let mut counts = BTreeMap::new();
    for analysis in analyses {
        let identity =
            AnalysisIdentity::project(analysis, grammar).map_err(|error| error.to_string())?;
        *counts.entry(identity).or_insert(0) += 1;
    }
    Ok(counts
        .into_iter()
        .map(|(identity, count)| IdentityMultiplicity { identity, count })
        .collect())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidateProjection {
    pub morphemes: Vec<Option<String>>,
    pub root_index: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DifferentialRow {
    pub word_id: usize,
    pub word: String,
    pub hc_status: String,
    pub fst_status: String,
    pub raw_candidates: Option<usize>,
    pub unique_candidates: Option<usize>,
    pub candidates: Option<Vec<CandidateProjection>>,
    pub oracle: Vec<IdentityMultiplicity>,
    pub hc_morph_occurrences: Vec<Vec<pg_parse::MorphOccurrence>>,
    pub confirmed_morph_occurrences: Vec<Vec<pg_parse::MorphOccurrence>>,
    pub confirmed: Vec<IdentityMultiplicity>,
    pub proposal_misses: Option<Vec<AnalysisIdentity>>,
    pub spurious_confirmed: Option<Vec<AnalysisIdentity>>,
    pub confirmed_multiset_equal: Option<bool>,
    pub incomplete: bool,
}

impl DifferentialRow {
    /// An unimplemented construction has no candidate relation to measure.
    pub fn not_implemented(word_id: usize, word: &str, reason: &str) -> Self {
        Self {
            word_id,
            word: word.to_string(),
            hc_status: "not_run_unimplemented_construction".to_string(),
            fst_status: reason.to_string(),
            raw_candidates: None,
            unique_candidates: None,
            candidates: None,
            oracle: Vec::new(),
            hc_morph_occurrences: Vec::new(),
            confirmed_morph_occurrences: Vec::new(),
            confirmed: Vec::new(),
            proposal_misses: None,
            spurious_confirmed: None,
            confirmed_multiset_equal: None,
            incomplete: true,
        }
    }
}

/// No set-only parity shortcut: multiplicity changes are observable even with full proposal recall.
pub fn compare_multisets(
    oracle: &[IdentityMultiplicity],
    confirmed: &[IdentityMultiplicity],
) -> bool {
    oracle == confirmed
}

pub fn measure_word(
    grammar: &Grammar,
    morpher: &Morpher,
    analyzer: Option<&mut FomaAnalyzer<'_>>,
    word_id: usize,
    word: &str,
    apply_budget: &ApplyBudget,
    compile_refusal: Option<&str>,
) -> Result<DifferentialRow, String> {
    let hc = morpher.parse_word(word);
    let hc_status = if hc.capped {
        "capped"
    } else if hc.timed_out {
        "timed_out"
    } else if hc.invalid_shape {
        "invalid_shape"
    } else {
        "complete"
    };
    let mut row = DifferentialRow {
        word_id,
        word: word.to_string(),
        hc_status: hc_status.to_string(),
        fst_status: compile_refusal.unwrap_or("not_run").to_string(),
        raw_candidates: None,
        unique_candidates: None,
        candidates: None,
        oracle: multiset(&hc.structured, grammar)?,
        hc_morph_occurrences: hc
            .structured
            .iter()
            .map(|analysis| analysis.morph_occurrences.clone())
            .collect(),
        confirmed_morph_occurrences: Vec::new(),
        confirmed: Vec::new(),
        proposal_misses: None,
        spurious_confirmed: None,
        confirmed_multiset_equal: None,
        incomplete: true,
    };
    let Some(analyzer) = analyzer else {
        return Ok(row);
    };
    match analyzer.analyze_word_with_diagnostics_budgeted_with_candidates(word, apply_budget) {
        ProfiledFomaApplyOutcomeWithCandidates::Incomplete {
            dimension,
            value,
            limit,
            diagnostics,
        } => {
            row.fst_status = format!("apply_limit:{}:{value}/{limit}", dimension.label());
            row.raw_candidates = Some(diagnostics.raw_paths);
            row.unique_candidates = Some(diagnostics.proposal.unique_candidates);
        }
        ProfiledFomaApplyOutcomeWithCandidates::Complete(result) => {
            row.raw_candidates = Some(result.diagnostics.raw_paths);
            row.unique_candidates = Some(result.candidates.len());
            row.confirmed = multiset(&result.outcome.structured, grammar)?;
            row.confirmed_morph_occurrences = result
                .outcome
                .structured
                .iter()
                .map(|analysis| analysis.morph_occurrences.clone())
                .collect();
            if let Some(error) = result.outcome.peel_chain_depth_error {
                row.fst_status = format!("peel_incomplete:{error}");
                return Ok(row);
            }
            row.fst_status = "complete".to_string();
            // An invalid shape is an explicit input mismatch, never an empty successful oracle row.
            if hc_status != "complete" {
                return Ok(row);
            }
            let mut projections = Vec::with_capacity(result.candidates.len());
            for candidate in &result.candidates {
                let keys = candidate
                    .morphemes
                    .iter()
                    .map(|id| {
                        grammar
                            .morphemes
                            .get(id.0 as usize)
                            .map(|m| Some(m.xml_key.clone()))
                            .ok_or_else(|| {
                                format!("candidate references missing morpheme {}", id.0)
                            })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                projections.push(CandidateProjection {
                    morphemes: keys,
                    root_index: candidate.root_index,
                });
            }
            row.proposal_misses = Some(
                row.oracle
                    .iter()
                    .filter(|entry| {
                        !projections.iter().any(|candidate| {
                            candidate.morphemes == entry.identity.morphemes
                                && candidate.root_index == entry.identity.root_index
                        })
                    })
                    .map(|entry| entry.identity.clone())
                    .collect(),
            );
            row.candidates = Some(projections);
            row.spurious_confirmed = Some(
                row.confirmed
                    .iter()
                    .filter(|entry| {
                        !row.oracle
                            .iter()
                            .any(|expected| expected.identity == entry.identity)
                    })
                    .map(|entry| entry.identity.clone())
                    .collect(),
            );
            row.confirmed_multiset_equal = Some(compare_multisets(&row.oracle, &row.confirmed));
            row.incomplete = false;
        }
    }
    Ok(row)
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DifferentialSummary {
    pub words: usize,
    pub complete_rows: usize,
    pub incomplete_rows: usize,
    pub raw_candidates: usize,
    pub unique_candidates: usize,
    pub oracle_identities: usize,
    pub proposal_misses: usize,
    pub spurious_confirmed: usize,
    pub multiset_equal_rows: usize,
    pub multiset_different_rows: usize,
}

impl DifferentialSummary {
    pub fn record(&mut self, row: &DifferentialRow) {
        self.words += 1;
        self.raw_candidates += row.raw_candidates.unwrap_or(0);
        self.unique_candidates += row.unique_candidates.unwrap_or(0);
        if row.incomplete {
            self.incomplete_rows += 1;
            return;
        }
        self.complete_rows += 1;
        self.oracle_identities += row.oracle.len();
        self.proposal_misses += row.proposal_misses.as_ref().map_or(0, Vec::len);
        self.spurious_confirmed += row.spurious_confirmed.as_ref().map_or(0, Vec::len);
        match row.confirmed_multiset_equal {
            Some(true) => self.multiset_equal_rows += 1,
            Some(false) => self.multiset_different_rows += 1,
            None => unreachable!("a complete differential row has a multiset verdict"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn multiplicity_difference_is_visible_when_identity_sets_agree() {
        let identity = AnalysisIdentity {
            morphemes: vec![Some("root".into())],
            root_index: 0,
            category: None,
        };
        let oracle = vec![IdentityMultiplicity {
            identity: identity.clone(),
            count: 2,
        }];
        let actual = vec![IdentityMultiplicity { identity, count: 1 }];
        assert!(!compare_multisets(&oracle, &actual));
    }
    #[test]
    fn incomplete_row_contributes_no_parity_or_recall_claim() {
        let row = DifferentialRow {
            word_id: 0,
            word: "a".into(),
            hc_status: "complete".into(),
            fst_status: "refused".into(),
            raw_candidates: None,
            unique_candidates: None,
            candidates: None,
            oracle: vec![],
            hc_morph_occurrences: vec![],
            confirmed_morph_occurrences: vec![],
            confirmed: vec![],
            proposal_misses: None,
            spurious_confirmed: None,
            confirmed_multiset_equal: None,
            incomplete: true,
        };
        let mut summary = DifferentialSummary::default();
        summary.record(&row);
        assert_eq!(summary.incomplete_rows, 1);
        assert_eq!(summary.complete_rows, 0);
        assert_eq!(summary.multiset_equal_rows, 0);
    }
}
