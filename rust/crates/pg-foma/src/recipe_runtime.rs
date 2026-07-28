//! Production evaluator for recipe plans.

use crate::analyzer::FomaProposer;
use crate::build::build_controllable;
use crate::compose_budget::{ComposeBudget, ComposeError};
use crate::composite::FomaAnalyzer;
use crate::emit::surface_table;
use crate::enumerate::CandidatePlan;
use crate::recipe_optimizer::{Certification, Score};
use crate::replace::SegAlphabet;
use foma::options::FomaOptions;
use pg_grammar::model::{Grammar, PhonRuleDef};
use pg_parse::WordAnalysis;
use sha2::{Digest, Sha256};
use std::time::Instant;

fn elapsed_ns(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WordEvidence {
    pub word: String,
    pub expected: Vec<WordAnalysis>,
    pub actual: Vec<WordAnalysis>,
}

/// Compare analyses as multisets: order is irrelevant, multiplicity is not.
pub fn certify_word(
    word: impl Into<String>,
    expected: Vec<WordAnalysis>,
    actual: Vec<WordAnalysis>,
) -> Certification {
    let word = word.into();
    if expected.len() != actual.len() {
        return Certification::MultiplicityMismatch {
            word,
            expected: expected.len() as u64,
            actual: actual.len() as u64,
        };
    }
    let mut remaining = actual;
    for (i, e) in expected.iter().enumerate() {
        if let Some(p) = remaining.iter().position(|a| a == e) {
            remaining.remove(p);
        } else {
            return Certification::IdentityMismatch {
                word,
                detail: format!("analysis {i} has no matching actual analysis: {e:?}"),
            };
        }
    }
    Certification::FullHcConfirmed {
        words: 1,
        corpus_hash: "runtime".into(),
    }
}

fn corpus_hash(words: &[String]) -> String {
    let mut hash = Sha256::new();
    for word in words {
        hash.update((word.len() as u64).to_le_bytes());
        hash.update(word.as_bytes());
    }
    format!("{:x}", hash.finalize())
}
pub fn certify_corpus(
    expected: &[(String, Vec<WordAnalysis>)],
    actual: &[(String, Vec<WordAnalysis>)],
) -> Certification {
    if expected.len() != actual.len() {
        return Certification::Truncated {
            stage: "full-hc".into(),
        };
    }
    let mut failures = expected
        .iter()
        .zip(actual)
        .filter_map(
            |((expected_word, expected_analyses), (actual_word, actual_analyses))| {
                if expected_word != actual_word {
                    return Some(Certification::Truncated {
                        stage: "full-hc-word-order".into(),
                    });
                }
                let verdict = certify_word(
                    expected_word,
                    expected_analyses.clone(),
                    actual_analyses.clone(),
                );
                (!verdict.selectable()).then_some(verdict)
            },
        )
        .collect::<Vec<_>>();
    failures.sort_by_key(|verdict| {
        verdict
            .shortest_disagreement()
            .map(|word| (word.chars().count(), word.to_owned()))
    });
    failures
        .into_iter()
        .next()
        .unwrap_or(Certification::FullHcConfirmed {
            words: expected.len() as u64,
            corpus_hash: "runtime".into(),
        })
}
pub fn build_candidate(
    candidate: &CandidatePlan,
    opts: &FomaOptions,
    grammar: &Grammar,
    alphabet: &SegAlphabet<'_>,
    prules: &[&PhonRuleDef],
    budget: &ComposeBudget,
) -> Result<crate::gate::GatedCompileResult, ComposeError> {
    build_controllable(&candidate.plan, opts, grammar, alphabet, prules, budget)
}

#[derive(Debug, Clone, Copy, Default)]
pub struct RuntimeBudget {
    pub states: Option<u64>,
    pub arcs: Option<u64>,
    pub build: Option<u64>,
    pub apply: Option<u64>,
    pub proposals: Option<u64>,
    pub confirmation: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeEvaluation {
    pub certification: Certification,
    pub score: Score,
}

/// Evaluates every plan through build_controllable and the production propose→confirm pipeline.
/// The caller-provided order is preserved; therefore the baseline must be element zero.
pub fn evaluate_plans(
    grammar: &Grammar,
    plans: &[CandidatePlan],
    words: &[String],
    budget: RuntimeBudget,
) -> Vec<RuntimeEvaluation> {
    let alphabet = SegAlphabet::new(surface_table(grammar));
    let prules: Vec<&PhonRuleDef> = grammar
        .strata
        .iter()
        .flat_map(|s| &s.prules)
        .map(|id| &grammar.prules[id.0 as usize])
        .collect();
    let opts = FomaOptions::default();
    let compose = ComposeBudget::from_env().with_step_timeout(
        budget
            .build
            .filter(|limit| *limit != u64::MAX)
            .map(std::time::Duration::from_nanos),
    );
    let morpher = pg_parse::Morpher::new(grammar, usize::MAX);
    let expected: Vec<_> = words
        .iter()
        .map(|w| (w.clone(), morpher.parse_word(w).structured))
        .collect();
    let report = crate::emit::emit(grammar).report;
    plans
        .iter()
        .map(|candidate| {
            let t = Instant::now();
            let built = build_candidate(candidate, &opts, grammar, &alphabet, &prules, &compose);
            let build = elapsed_ns(t).max(1);
            let Ok(mut built) = built else {
                return RuntimeEvaluation {
                    certification: Certification::BuildFailed {
                        reason: "build failed".into(),
                    },
                    score: Score {
                        states: 0,
                        arcs: 0,
                        build,
                        apply: 0,
                        proposals: 0,
                        confirmation: 0,
                    },
                };
            };
            let Some(net) = built.net.take() else {
                return RuntimeEvaluation {
                    certification: Certification::Truncated {
                        stage: "empty-network".into(),
                    },
                    score: Score {
                        states: 0,
                        arcs: 0,
                        build,
                        apply: 0,
                        proposals: 0,
                        confirmation: 0,
                    },
                };
            };
            let score0 = (net.statecount as u64, net.arccount as u64);
            let mut analyzer = FomaAnalyzer::from_precompiled_proposer(
                grammar,
                FomaProposer::from_precompiled_network(&net, report.clone())
                    .with_segment_query_encoder(surface_table(grammar)),
            );
            let mut actual = Vec::new();
            let mut apply: u64 = 0;
            let mut proposals: u64 = 0;
            let mut confirmation: u64 = 0;
            for w in words {
                let t = Instant::now();
                let p = analyzer.analyze_word_with_diagnostics(w);
                apply = apply.saturating_add(elapsed_ns(t).max(1));
                proposals = proposals.saturating_add(p.outcome.candidates_generated as u64);
                confirmation = confirmation.saturating_add(p.diagnostics.confirmation_calls as u64);
                actual.push((w.clone(), p.outcome.structured));
            }
            let score = Score {
                states: score0.0,
                arcs: score0.1,
                build,
                apply,
                proposals,
                confirmation,
            };
            let breach = [
                ("states", score.states, budget.states),
                ("arcs", score.arcs, budget.arcs),
                ("build", build, budget.build),
                ("apply", apply, budget.apply),
                ("proposals", proposals, budget.proposals),
                ("confirmation", confirmation, budget.confirmation),
            ]
            .into_iter()
            .find(|(_, v, l)| l.is_some_and(|limit| *v > limit));
            let certification = match breach {
                Some((d, v, Some(l))) => Certification::ResourceBreach {
                    dimension: d.into(),
                    value: v,
                    limit: l,
                },
                _ => match certify_corpus(&expected, &actual) {
                    Certification::FullHcConfirmed {
                        words: word_count, ..
                    } => Certification::FullHcConfirmed {
                        words: word_count,
                        corpus_hash: corpus_hash(words),
                    },
                    failure => failure,
                },
            };
            RuntimeEvaluation {
                certification,
                score,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn wa(n: u32) -> WordAnalysis {
        WordAnalysis {
            morpheme_ids: vec![n],
            root_morpheme_index: 0,
            pos_id: None,
            syn_fs: pg_featstruct::FeatureStruct::EMPTY,
            mpr: pg_grammar::model::MprSet::EMPTY,
            guessed: false,
            provenance: pg_parse::AnalysisProvenance::Grammar,
            supplied_root: None,
            morpheme_roots: vec![None],
        }
    }
    #[test]
    fn reordered_equal_multiset() {
        assert!(certify_word("w", vec![wa(1), wa(2)], vec![wa(2), wa(1)]).selectable());
    }
    #[test]
    fn identity_mismatch() {
        assert!(matches!(
            certify_word("w", vec![wa(1)], vec![wa(2)]),
            Certification::IdentityMismatch { .. }
        ));
    }
    #[test]
    fn multiplicity_mismatch() {
        assert!(matches!(
            certify_word("w", vec![wa(1), wa(1)], vec![wa(1)]),
            Certification::MultiplicityMismatch { .. }
        ));
    }
    #[test]
    fn duplicate_corpus_words_preserve_occurrence_multiplicity() {
        let expected = vec![("w".into(), vec![wa(1)]), ("w".into(), vec![wa(2)])];
        assert!(certify_corpus(&expected, &expected).selectable());
        let changed = vec![("w".into(), vec![wa(1)]), ("w".into(), vec![wa(1)])];
        assert!(matches!(
            certify_corpus(&expected, &changed),
            Certification::IdentityMismatch { .. }
        ));
    }
    #[test]
    fn missing_truncated() {
        assert!(matches!(
            certify_corpus(&[("w".into(), vec![])], &[]),
            Certification::Truncated { .. }
        ));
    }
}
