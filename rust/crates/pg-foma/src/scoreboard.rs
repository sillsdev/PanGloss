//! Per-(fixture, backend) measurement, extracted out of `examples/conf_matrix.rs` so a second
//! caller (a CI gate, `pangloss coverage`, ...) can reuse it instead of re-deriving the same
//! evaluation inline -- the gap `strategy_coverage_join`'s own top-doc names: "examples/
//! conf_matrix.rs computes the same thing inline, with no library seam a second caller could
//! reuse."
//!
//! [`CellOutcome`](crate::scoreboard::CellOutcome) types what used to be a printed string. Every
//! cell lands in exactly one of its four variants;
//! [`measure`](crate::scoreboard::measure) is the one function that decides which, and it is also
//! the only place `crate::strategy_coverage_join::envelope_refusal_predicates` is consulted for
//! this measurement, so a `CellOutcome::Refused`'s predicate list can never drift from what that
//! function reports elsewhere.
//!
//! # This module names no fixture-loading type
//! [`measure`](crate::scoreboard::measure) and
//! [`unmeasurable`](crate::scoreboard::unmeasurable) take a plain `label: &str`, an already-loaded
//! `&Grammar`, and already-selected `words: &[String]` -- never
//! `pg_conformance_fixtures::FixtureRef` or `WordsYaml`. The Compiler measures a (grammar, words)
//! pair; discovering fixtures, loading their `words.yaml`, subsampling (the example's own
//! `MAX_WORDS_PER_FIXTURE`), and excluding an `expect_crash` fixture by name are all a CALLER'S
//! concern (`examples/conf_matrix.rs`, `tests/backend_scoreboard_gate.rs`), which may depend on
//! `pg-conformance-fixtures` freely because fixture discovery is how a caller finds grammars, not
//! something the compiler needs to know about.
//!
//! # `IdentityDivergence` is exposed, not recomputed
//! `evaluate_plans_observed_with_cache` already threads a per-run
//! [`IdentityDivergence`](crate::parity::IdentityDivergence) through `RunEvaluationCache`;
//! `examples/conf_matrix.rs` used to subtract the running total before/after each strategy purely
//! to print two of its seven fields (`oracle_only_identities`, `candidate_only_identities`) and
//! then discard the rest. [`CellMeasurement::divergence`](crate::scoreboard::CellMeasurement)
//! carries the same subtracted delta whole, so a caller can see `candidate_only_identities` (a
//! soundness violation -- expected zero) without a second subtraction living in a second file.

use pg_grammar::model::Grammar;

use crate::backend_optimizer::Certification;
use crate::backend_runtime::{
    evaluate_plans_observed_with_cache, RunEvaluationCache, RuntimeBudget,
};
use crate::capability::PredicateId;
use crate::enumerate::{enumerate_default, CandidateRole, EmissionStrategy, LoweredCandidate};
use crate::grammar_semantics::GrammarSemantics;
use crate::junctions::PhonologyProbe;
use crate::lowering_adapter::LoweringAdapter;
use crate::parity::IdentityDivergence;
use crate::strategy_coverage::ALL_STRATEGIES;
use crate::strategy_coverage_join::envelope_refusal_predicates;

/// Safety margin above any fixture on disk; a caller subsamples to this before calling [`measure`]
/// and reports the subsampling itself (this module no longer sees the pre-subsample total).
pub const MAX_WORDS_PER_FIXTURE: usize = 200;

/// One (fixture, backend) cell's typed outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CellOutcome {
    /// `Certification::FullHcConfirmed`: every comparable word's confirmed output matched the
    /// oracle's identity set exactly.
    OracleExact,
    /// The candidate's network built and produced comparable per-word evidence, but at least one
    /// word's confirmed output was not oracle-exact. `recall_deficit` is this cell's own
    /// [`IdentityDivergence::oracle_only_identities`] -- identities the oracle has that the
    /// confirmed output does not.
    CompilesButMisses { recall_deficit: u64 },
    /// The candidate's network never built:
    /// `Certification::{CapabilityRejected,BuildFailed,Unsupported}`. `predicates` names the
    /// capability envelope's own declared refusal for this (grammar, strategy) pair, from
    /// [`envelope_refusal_predicates`] -- empty when the envelope has no report for this strategy
    /// or admits it, which can happen here because this measurement calls the backend directly and
    /// bypasses `crate::backend_selection::select_backends`.
    Refused {
        reason: String,
        predicates: Vec<PredicateId>,
    },
    /// The candidate's network built but no comparable per-word evidence came back (e.g.
    /// `Truncated`/`ResourceBreach`/`IdentityMismatch` certifications, or some other evaluation
    /// outcome short of per-word evidence). `reason` names which.
    Unmeasurable { reason: String },
}

/// One backend's full measurement for one fixture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellMeasurement {
    pub strategy: EmissionStrategy,
    pub outcome: CellOutcome,
    pub certification_debug: String,
    /// This cell's [`IdentityDivergence`] delta -- see this module's own doc for why it is exposed
    /// rather than recomputed by a second caller. `None` exactly when [`CellOutcome::Unmeasurable`]
    /// carries no comparable per-word evidence to derive a delta from.
    pub divergence: Option<IdentityDivergence>,
    /// Raw pre-confirm proposals not surviving into the confirmed output -- legal under ADR-0001,
    /// informational only. `None` alongside `divergence: None`.
    pub legal_overgeneration: Option<u64>,
    pub words_measured: Option<usize>,
}

impl CellMeasurement {
    pub fn compiled(&self) -> bool {
        !matches!(self.outcome, CellOutcome::Refused { .. })
    }

    pub fn exact(&self) -> bool {
        matches!(self.outcome, CellOutcome::OracleExact)
    }
}

/// One fixture's full measurement across every strategy in [`ALL_STRATEGIES`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScoredFixture {
    pub label: String,
    pub measured_words: usize,
    /// Words the oracle itself excluded, named rather than dropped silently -- see
    /// `RunEvaluationCache::corpus_evidence`.
    pub excluded_words: Vec<(String, String)>,
    pub cells: Vec<CellMeasurement>,
    /// `None` means the (grammar, words) pair itself could not be prepared at all (no character
    /// table, no words, or an oracle preparation fault) -- distinct from a pair that WAS prepared
    /// and measured zero exact backends. When `None`, every cell in `cells` is
    /// `CellOutcome::Unmeasurable` with the same reason.
    pub exact_count: Option<usize>,
}

/// A fixture-shaped row for a (label, reason) pair that could not even be attempted -- e.g. a
/// caller's own grammar load failed before a `Grammar` value existed to pass to [`measure`]. The
/// only way to build a whole-row `Unmeasurable` result without duplicating [`measure`]'s own
/// per-strategy shape at each call site.
pub fn unmeasurable(label: &str, reason: &str) -> ScoredFixture {
    let cells = ALL_STRATEGIES
        .iter()
        .map(|&strategy| CellMeasurement {
            strategy,
            outcome: CellOutcome::Unmeasurable {
                reason: reason.to_string(),
            },
            certification_debug: "n/a".to_string(),
            divergence: None,
            legal_overgeneration: None,
            words_measured: None,
        })
        .collect();
    ScoredFixture {
        label: label.to_string(),
        measured_words: 0,
        excluded_words: Vec::new(),
        cells,
        exact_count: None,
    }
}

/// Measures one already-loaded `(grammar, words)` pair against every strategy in
/// [`ALL_STRATEGIES`]. `words` is exactly what gets run -- a caller that needs to subsample a
/// larger declared corpus does so before calling this (see [`MAX_WORDS_PER_FIXTURE`]).
pub fn measure(label: &str, grammar: &Grammar, words: &[String]) -> ScoredFixture {
    if grammar.char_tables.is_empty() {
        return unmeasurable(label, "grammar has no character table");
    }
    if words.is_empty() {
        return unmeasurable(label, "no words to measure");
    }

    let semantics = GrammarSemantics::derive(grammar);
    let phonology = PhonologyProbe::new_with_semantics(&semantics);
    let baseline_plan = enumerate_default(grammar, semantics.prules_in_order(), phonology.as_ref());

    let mut cache = match RunEvaluationCache::prepare(grammar, words, RuntimeBudget::default()) {
        Ok(cache) => cache,
        Err(fault) => {
            return unmeasurable(label, &format!("oracle preparation faulted: {fault}"));
        }
    };

    let corpus_evidence = cache.corpus_evidence(words);
    let excluded_words: Vec<(String, String)> = corpus_evidence
        .exclusions
        .iter()
        .map(|e| (e.word.clone(), e.reason.clone()))
        .collect();

    let mut prev_divergence = cache.identity_divergence();
    let mut cells = Vec::with_capacity(ALL_STRATEGIES.len());
    let mut exact_count = 0usize;

    for &strategy in ALL_STRATEGIES {
        let candidate = LoweredCandidate {
            label: "conf-matrix",
            plan: baseline_plan.clone(),
            adapter: LoweringAdapter::for_strategy(strategy),
            role: if strategy == EmissionStrategy::PlanComposed {
                CandidateRole::Baseline
            } else {
                CandidateRole::Alternative
            },
        };

        let observed = evaluate_plans_observed_with_cache(
            grammar,
            std::slice::from_ref(&candidate),
            words,
            RuntimeBudget::default(),
            &mut cache,
        );
        let obs = &observed[0];

        let now_divergence = cache.identity_divergence();
        let delta = subtract_divergence(now_divergence, prev_divergence);
        prev_divergence = now_divergence;

        let certification = &obs.evaluation.certification;
        let certification_debug = format!("{certification:?}");

        let cell = match compile_reason(certification) {
            Some(reason) => CellMeasurement {
                strategy,
                outcome: CellOutcome::Refused {
                    reason,
                    predicates: envelope_refusal_predicates(grammar, strategy),
                },
                certification_debug,
                divergence: None,
                legal_overgeneration: None,
                words_measured: None,
            },
            None => match &obs.words {
                None => CellMeasurement {
                    strategy,
                    outcome: CellOutcome::Unmeasurable {
                        reason: format!(
                            "evaluation did not reach comparable per-word evidence \
                             (certification={certification_debug})"
                        ),
                    },
                    certification_debug,
                    divergence: Some(delta),
                    legal_overgeneration: None,
                    words_measured: None,
                },
                Some(evidence) => {
                    let exact = matches!(certification, Certification::FullHcConfirmed { .. });
                    if exact {
                        exact_count += 1;
                    }
                    let legal_overgeneration: u64 =
                        evidence.iter().map(proposals_pruned_by_confirm).sum();
                    CellMeasurement {
                        strategy,
                        outcome: if exact {
                            CellOutcome::OracleExact
                        } else {
                            CellOutcome::CompilesButMisses {
                                recall_deficit: delta.oracle_only_identities,
                            }
                        },
                        certification_debug,
                        divergence: Some(delta),
                        legal_overgeneration: Some(legal_overgeneration),
                        words_measured: Some(evidence.len()),
                    }
                }
            },
        };
        cells.push(cell);
    }

    ScoredFixture {
        label: label.to_string(),
        measured_words: words.len(),
        excluded_words,
        cells,
        exact_count: Some(exact_count),
    }
}

/// `Some(reason)` iff the candidate's network never built (a `CapabilityRejected`/`BuildFailed`/`Unsupported` certification).
fn compile_reason(certification: &Certification) -> Option<String> {
    match certification {
        Certification::CapabilityRejected { reason }
        | Certification::BuildFailed { reason }
        | Certification::Unsupported { reason } => Some(reason.clone()),
        _ => None,
    }
}

fn subtract_divergence(after: IdentityDivergence, before: IdentityDivergence) -> IdentityDivergence {
    IdentityDivergence {
        occurrences_compared: after
            .occurrences_compared
            .saturating_sub(before.occurrences_compared),
        occurrences_not_compared: after
            .occurrences_not_compared
            .saturating_sub(before.occurrences_not_compared),
        oracle_identities: after
            .oracle_identities
            .saturating_sub(before.oracle_identities),
        candidate_identities: after
            .candidate_identities
            .saturating_sub(before.candidate_identities),
        oracle_only_identities: after
            .oracle_only_identities
            .saturating_sub(before.oracle_only_identities),
        candidate_only_identities: after
            .candidate_only_identities
            .saturating_sub(before.candidate_only_identities),
        occurrences_with_candidate_only: after
            .occurrences_with_candidate_only
            .saturating_sub(before.occurrences_with_candidate_only),
        oracle_admission_key_collisions: after
            .oracle_admission_key_collisions
            .saturating_sub(before.oracle_admission_key_collisions),
        candidate_admission_key_collisions: after
            .candidate_admission_key_collisions
            .saturating_sub(before.candidate_admission_key_collisions),
    }
}

/// Raw pre-confirm proposals (admission key: morpheme ids + root index) not surviving into the
/// confirmed output for one word. See docs/research/backend-measurement-instruments.md.
fn proposals_pruned_by_confirm(evidence: &crate::backend_runtime::WordEvidence) -> u64 {
    use std::collections::BTreeMap;
    let mut actual_keys: BTreeMap<(Vec<u32>, i32), usize> = BTreeMap::new();
    for a in &evidence.actual {
        *actual_keys
            .entry((a.morpheme_ids.clone(), a.root_morpheme_index))
            .or_default() += 1;
    }
    let mut proposed_keys: BTreeMap<(Vec<u32>, i32), usize> = BTreeMap::new();
    for c in &evidence.proposals {
        let key = (c.morphemes.iter().map(|m| m.0).collect(), c.root_index);
        *proposed_keys.entry(key).or_default() += 1;
    }
    let mut pruned = 0u64;
    for (key, proposed_count) in &proposed_keys {
        let actual_count = actual_keys.get(key).copied().unwrap_or(0);
        pruned = pruned.saturating_add((*proposed_count).saturating_sub(actual_count) as u64);
    }
    pruned
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compile_reason_is_none_for_full_hc_confirmed() {
        assert!(compile_reason(&Certification::FullHcConfirmed {
            words: 1,
            corpus_hash: "x".to_string()
        })
        .is_none());
    }

    #[test]
    fn compile_reason_names_every_refusal_variant() {
        for cert in [
            Certification::CapabilityRejected {
                reason: "r".to_string(),
            },
            Certification::BuildFailed {
                reason: "r".to_string(),
            },
            Certification::Unsupported {
                reason: "r".to_string(),
            },
        ] {
            assert_eq!(compile_reason(&cert), Some("r".to_string()));
        }
    }

    fn cell(outcome: CellOutcome) -> CellMeasurement {
        CellMeasurement {
            strategy: EmissionStrategy::PlanComposed,
            outcome,
            certification_debug: "n/a".to_string(),
            divergence: None,
            legal_overgeneration: None,
            words_measured: None,
        }
    }

    #[test]
    fn cell_compiled_and_exact_read_from_the_outcome_alone() {
        let refused = cell(CellOutcome::Refused {
            reason: "r".to_string(),
            predicates: Vec::new(),
        });
        assert!(!refused.compiled());
        assert!(!refused.exact());

        let exact = cell(CellOutcome::OracleExact);
        assert!(exact.compiled());
        assert!(exact.exact());
    }

    #[test]
    fn unmeasurable_fills_every_strategy_with_the_same_reason() {
        let row = unmeasurable("some-label", "no grammar to measure");
        assert_eq!(row.exact_count, None);
        assert_eq!(row.cells.len(), ALL_STRATEGIES.len());
        for cell in &row.cells {
            assert_eq!(
                cell.outcome,
                CellOutcome::Unmeasurable {
                    reason: "no grammar to measure".to_string()
                }
            );
        }
    }
}
