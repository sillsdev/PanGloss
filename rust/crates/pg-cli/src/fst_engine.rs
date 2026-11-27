//! Shared compile/apply seam for CLI consumers of the admitted FST pipeline.

use pg_foma::composite::{FomaAnalyzer, FomaApplyOutcome};
use pg_foma_backend::compose_budget::ApplyBudget;
use pg_grammar::model::Grammar;
use pg_parse::{Morpher, ParseOutcome};

pub(crate) struct Session<'g> {
    grammar: &'g Grammar,
    analyzer: FomaAnalyzer<'g>,
    apply_budget: ApplyBudget,
}

impl<'g> Session<'g> {
    /// Compile with typed admission; callers must reject HC controls whose stop flags confirmation cannot propagate.
    pub(crate) fn compile(grammar: &'g Grammar) -> Result<Self, String> {
        let proposer = pg_foma::analyzer::compile_proposer(grammar)
            .map_err(|error| format!("FST compile refusal: {error:?}: {error}"))?;
        let morpher = Morpher::new(grammar, usize::MAX)
            .with_word_mem_cap(None)
            .with_capture_cap(None)
            .with_guess_roots(false);
        let analyzer = FomaAnalyzer::from_cached_with_morpher(
            grammar,
            proposer,
            pg_foma::peel::ReduplicationPeeler::new(grammar),
            pg_foma::confirm::build_morpheme_owners(grammar),
            morpher,
            pg_foma::candidate_filter::shadow::CandidateFilterSettings::off(),
        );
        Ok(Self {
            grammar,
            analyzer,
            apply_budget: pg_foma_backend::backend_runtime::RuntimeBudget::default()
                .resolved_apply_budget(),
        })
    }

    /// Incomplete proposal/peel work has no publishable analyses and carries a cap flag.
    pub(crate) fn analyze(&mut self, word: &str) -> ParseOutcome {
        let mut outcome = ParseOutcome {
            partials: Vec::new(),
            analyses: Vec::new(),
            structured: Vec::new(),
            capped: false,
            capture_capped: false,
            word_mem_capped: false,
            invalid_shape: false,
            work_steps: 0,
            timed_out: false,
            guessed: false,
            candidates_generated: 0,
            cost: None,
        };
        if let Some(stratum) = self.grammar.strata.last() {
            let table = &self.grammar.char_tables[stratum.table.0 as usize];
            if pg_rules::shape_feat::segment_with_features(self.grammar, table, word).is_err() {
                outcome.invalid_shape = true;
                return outcome;
            }
        }
        match self
            .analyzer
            .analyze_word_budgeted(word, &self.apply_budget)
        {
            FomaApplyOutcome::Incomplete {
                dimension,
                value,
                limit,
            } => {
                eprintln!(
                    "FST apply incomplete: {}: {value}/{limit}",
                    dimension.label()
                );
                outcome.capped = true;
            }
            FomaApplyOutcome::Complete(result) => {
                if let Some(error) = result.peel_chain_depth_error {
                    eprintln!("FST peel incomplete: {error}");
                    outcome.capped = true;
                } else {
                    outcome.analyses = result.analyses;
                    outcome.structured = result.structured;
                    outcome.candidates_generated = result.candidates_generated;
                }
            }
        }
        outcome
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fst_session_matches_hc_identity_multisets_for_affix_conformance_cases() {
        let fixture = pg_conformance_fixtures::require_fixture(
            "edge-cases",
            "shared-template-unconstrained-suffix",
        );
        let grammar = fixture.load_grammar();
        let morpher = Morpher::new(&grammar, usize::MAX)
            .with_capture_cap(None)
            .with_word_mem_cap(None)
            .with_guess_roots(false);
        let mut session = Session::compile(&grammar).unwrap();
        let cases = fixture.load_words_yaml();
        assert!(!cases.words.is_empty());
        let mut identities = 0;
        for case in cases.words {
            let hc = morpher.parse_word(&case.word);
            let fst = session.analyze(&case.word);
            assert!(!hc.capped && !hc.timed_out && !hc.invalid_shape);
            assert!(!fst.capped && !fst.timed_out && !fst.invalid_shape);
            let expected = pg_foma::differential::multiset(&hc.structured, &grammar).unwrap();
            identities += expected.len();
            assert_eq!(
                expected,
                pg_foma::differential::multiset(&fst.structured, &grammar).unwrap(),
                "{}",
                case.word
            );
        }
        assert!(identities > 1);
    }

    #[test]
    fn fst_proposal_caps_never_publish_partial_or_empty_complete_analyses() {
        let grammar = pg_grammar::load(crate::tests::MINI_GRAMMAR_XML).unwrap();
        let mut session = Session::compile(&grammar).unwrap();
        assert!(!session.analyze("kat").analyses.is_empty());
        for budget in [
            ApplyBudget::with_caps(Some(0), None),
            ApplyBudget::with_caps(None, Some(0)),
        ] {
            session.apply_budget = budget;
            let outcome = session.analyze("kat");
            assert!(outcome.capped);
            assert!(!outcome.timed_out);
            assert!(outcome.analyses.is_empty());
            assert!(outcome.structured.is_empty());
            assert_eq!(crate::outcome_status(&outcome), "CAP");
        }
    }
}
