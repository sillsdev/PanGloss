//! Per-STRATEGY FAITHFULNESS coverage, proven by PROPOSAL CONTAINMENT -- not by compiling alone.
//!
//! # The gap this closes
//! `crate::witnessed_coverage`'s own doc states plainly what a compile-only witness does NOT
//! establish: that a backend's proposer represents a construct faithfully, rather than compiling
//! while silently skipping the construct's material. The pipeline this crate feeds is
//! FST-proposes + HermitCrab-confirms (`crate::confirm`, driven through
//! `crate::composite::FomaAnalyzer`), so the honest faithfulness question is CONTAINMENT: for a
//! fixture exhibiting a construct, does the backend's final proposal set contain every analysis
//! full Rust HermitCrab finds for that fixture's words? If an emitter skipped the construct's
//! material, the proposal set under-generates and containment fails -- a fact a compile-only witness
//! cannot see, because the network still builds.
//!
//! `crate::backend_runtime::word_proposal_containment` is the comparison; it is the same relation
//! `tests/cross_compiler_equivalence_gate.rs` already checked, by hand, for one pinned fixture. This
//! module runs it over every discovered fixture and folds the result into the same
//! `(capability::CharacteristicKind, enumerate::EmissionStrategy)` shape
//! `crate::witnessed_coverage` reports in, so the two accounts read side by side.
//!
//! # What a HELD verdict does and does not establish
//! HELD means every comparable word in every fixture exhibiting the construct offered at least as
//! many of each oracle identity as the oracle found. It is containment, not equality: a backend
//! that over-generates wildly still reads HELD here -- `crate::backend_runtime::check_proposal_ratio`
//! is the separate, existing guard against that, and is orthogonal to this account.
//!
//! # NOT ATTEMPTED is not a pass
//! A pair reads `ContainmentOutcome::NotAttempted` whenever the comparison never ran at all: the
//! selector refused the backend, oracle preparation faulted, or the evaluation never reached a
//! comparable word (a resource breach, a step-capped oracle, an empty corpus). `FaithfulnessReport`
//! keeps this class distinct from HELD for the same reason `crate::witnessed_coverage` keeps
//! `BackendOutcome::RefusedBySelector` distinct from `Compiled`: silently reading "never checked" as
//! "checked and fine" is the exact trap a faithfulness account exists to close.
//!
//! # Denominator, always
//! Same discipline as `crate::witnessed_coverage`: `FaithfulnessReport` carries the claimed
//! conformance scope, the fixture counts, and which backends actually had at least one comparison
//! attempted, printed above the totals.

use std::collections::BTreeSet;

use pg_grammar::model::Grammar;

use crate::backend_runtime::{
    word_proposal_containment, RunEvaluationCache, RuntimeBudget, WordEvidence,
};
use crate::backend_selection::{select_backends, BackendReport};
use crate::capability::CharacteristicKind;
use crate::enumerate::{enumerate_default, CandidateRole, EmissionStrategy, LoweredCandidate};
use crate::grammar_semantics::GrammarSemantics;
use crate::junctions::PhonologyProbe;
use crate::lowering_adapter::LoweringAdapter;
use crate::strategy_coverage::ALL_STRATEGIES;

/// One (fixture, backend) pair's containment outcome. Only `Held`/`Failed` come from a real
/// comparison; `NotAttempted` covers every reason the comparison could not run at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContainmentOutcome {
    /// Every comparable word's proposal set contained every oracle identity at the oracle's own multiplicity.
    Held,
    /// At least one comparable word's proposal set was missing an oracle identity or offered it too few times; carries the first such gap, human-readable.
    Failed { word: String, detail: String },
    /// The comparison never ran for this fixture/backend pair; names why.
    NotAttempted { reason: String },
}

impl ContainmentOutcome {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Held => "held",
            Self::Failed { .. } => "failed",
            Self::NotAttempted { .. } => "not-attempted",
        }
    }
}

/// One fixture's contribution: the constructs it exhibits, and what happened when each backend's
/// proposal set was compared against the same fixture's full-HC oracle result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixtureContainmentObservation {
    /// Caller-supplied identity (a `pg_conformance_fixtures::FixtureRef::label`, typically).
    pub label: String,
    /// Every distinct `CharacteristicKind` this fixture's grammar exhibits, in `CharacteristicKind::ALL` order.
    pub kinds: Vec<CharacteristicKind>,
    /// One entry per `crate::strategy_coverage::ALL_STRATEGIES` entry, in that constant's order.
    pub outcomes: Vec<(EmissionStrategy, ContainmentOutcome)>,
}

impl FixtureContainmentObservation {
    pub fn outcome_for(&self, strategy: EmissionStrategy) -> Option<&ContainmentOutcome> {
        self.outcomes
            .iter()
            .find(|(s, _)| *s == strategy)
            .map(|(_, outcome)| outcome)
    }

    fn not_attempted(label: &str, kinds: Vec<CharacteristicKind>, reason: String) -> Self {
        Self {
            label: label.to_string(),
            kinds,
            outcomes: ALL_STRATEGIES
                .iter()
                .map(|&strategy| {
                    (
                        strategy,
                        ContainmentOutcome::NotAttempted {
                            reason: reason.clone(),
                        },
                    )
                })
                .collect(),
        }
    }
}

/// Characterizes `grammar`, asks `crate::backend_selection` which backends may run it, then for
/// each SELECTED backend runs the real propose+observe pipeline
/// (`crate::backend_runtime::evaluate_plans_observed_with_cache`) over `words` and checks
/// `crate::backend_runtime::word_proposal_containment` for every comparable word.
///
/// One `RunEvaluationCache` is shared across every selected backend for this fixture, exactly as
/// `tests/cross_compiler_equivalence_gate.rs` shares one cache across its three pinned candidates --
/// the oracle is prepared once, never once per backend.
pub fn observe_fixture_containment(
    label: &str,
    grammar: &Grammar,
    words: &[String],
) -> FixtureContainmentObservation {
    let semantics = GrammarSemantics::derive(grammar);
    let observed: BTreeSet<CharacteristicKind> = semantics
        .characteristics()
        .observations()
        .iter()
        .map(|o| o.kind)
        .collect();
    let kinds: Vec<CharacteristicKind> = CharacteristicKind::ALL
        .iter()
        .copied()
        .filter(|kind| observed.contains(kind))
        .collect();

    if words.is_empty() {
        return FixtureContainmentObservation::not_attempted(
            label,
            kinds,
            "fixture has no words".to_string(),
        );
    }
    let Some(_) = grammar.char_tables.first() else {
        return FixtureContainmentObservation::not_attempted(
            label,
            kinds,
            "grammar has no character table".to_string(),
        );
    };

    let selection = select_backends(&semantics);
    let mut not_attempted: Vec<(EmissionStrategy, ContainmentOutcome)> = Vec::new();
    let mut selected_strategies: Vec<EmissionStrategy> = Vec::new();
    for &strategy in ALL_STRATEGIES {
        let representable = selection
            .report_for(strategy)
            .is_some_and(BackendReport::can_represent);
        if representable {
            selected_strategies.push(strategy);
        } else {
            not_attempted.push((
                strategy,
                ContainmentOutcome::NotAttempted {
                    reason: "refused-by-selector".to_string(),
                },
            ));
        }
    }
    if selected_strategies.is_empty() {
        return FixtureContainmentObservation {
            label: label.to_string(),
            kinds,
            outcomes: not_attempted,
        };
    }

    let phonology = PhonologyProbe::new_with_semantics(&semantics);
    let baseline_plan = enumerate_default(grammar, semantics.prules_in_order(), phonology.as_ref());
    let plans: Vec<LoweredCandidate> = selected_strategies
        .iter()
        .map(|&strategy| LoweredCandidate {
            label: "faithfulness-containment-sweep",
            plan: baseline_plan.clone(),
            adapter: LoweringAdapter::for_strategy(strategy),
            // Only the plan-composing adapter reads this shared baseline plan, so it alone is baseline.
            role: if strategy == EmissionStrategy::PlanComposed {
                CandidateRole::Baseline
            } else {
                CandidateRole::Alternative
            },
        })
        .collect();

    let mut cache = match RunEvaluationCache::prepare(grammar, words, RuntimeBudget::default()) {
        Ok(cache) => cache,
        Err(fault) => {
            let reason = format!("oracle preparation faulted: {fault}");
            let mut outcomes: Vec<(EmissionStrategy, ContainmentOutcome)> = selected_strategies
                .iter()
                .map(|&strategy| {
                    (
                        strategy,
                        ContainmentOutcome::NotAttempted {
                            reason: reason.clone(),
                        },
                    )
                })
                .collect();
            outcomes.extend(not_attempted);
            return FixtureContainmentObservation {
                label: label.to_string(),
                kinds,
                outcomes,
            };
        }
    };

    let observed = crate::backend_runtime::evaluate_plans_observed_with_cache(
        grammar,
        &plans,
        words,
        RuntimeBudget::default(),
        &mut cache,
    );

    let mut outcomes: Vec<(EmissionStrategy, ContainmentOutcome)> = plans
        .iter()
        .zip(&observed)
        .map(|(plan, observation)| {
            let strategy = plan.strategy();
            let Some(evidence) = &observation.words else {
                return (
                    strategy,
                    ContainmentOutcome::NotAttempted {
                        reason: format!(
                            "evaluation did not reach comparable words: {:?}",
                            observation.evaluation.certification
                        ),
                    },
                );
            };
            (strategy, containment_outcome_for_evidence(evidence))
        })
        .collect();
    outcomes.extend(not_attempted);

    FixtureContainmentObservation {
        label: label.to_string(),
        kinds,
        outcomes,
    }
}

/// Classifies one backend's already-observed evidence for one fixture: `Held` if every comparable
/// word's proposal set contained the oracle's identities at the oracle's own multiplicity, `Failed`
/// at the first word that did not, or `NotAttempted` if there is no comparable evidence at all (an
/// empty corpus after oracle exclusions -- an evaluation that failed outright never reaches this
/// function; `observe_fixture_containment` classifies that case itself, from the certification,
/// before calling this).
///
/// Exposed so a falsification test can classify a deliberately mutated evidence vector directly,
/// without needing an injection point inside `observe_fixture_containment`'s own compile/evaluate
/// pipeline -- the same reason `word_proposal_containment` is exposed rather than inlined.
pub fn containment_outcome_for_evidence(evidence: &[WordEvidence]) -> ContainmentOutcome {
    if evidence.is_empty() {
        return ContainmentOutcome::NotAttempted {
            reason: "no comparable words after oracle exclusions".to_string(),
        };
    }
    for word in evidence {
        if let Err(gap) = word_proposal_containment(word) {
            return ContainmentOutcome::Failed {
                word: gap.word.clone(),
                detail: gap.to_string(),
            };
        }
    }
    ContainmentOutcome::Held
}

/// How strictly `FaithfulnessReport::check` reads the failure inventory.
///
/// **THE PLACE THIS ACCOUNT BECOMES STRICT.** Mirrors
/// `crate::witnessed_coverage::CompletenessRequirement`: the lenient reading is what
/// `tests/faithfulness_coverage_gate.rs` asserts today, reporting every failure rather than
/// building on it, because turning it build-breaking before a real gap is triaged would turn `main`
/// red for every unrelated change. Flipping the gate to `Self::NoFailures` is a one-word edit at
/// that test's own `REQUIREMENT` constant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaithfulnessRequirement {
    /// The run must have compared something for real: at least one fixture observed, at least one
    /// containment comparison actually attempted (held or failed, never only not-attempted), and at
    /// least two distinct backends exercised. Failures are reported, never failed on.
    NonVacuity,
    /// `Self::NonVacuity` plus a ceiling on the failure inventory: a ratchet, not a target.
    ///
    /// `NonVacuity` and `NoFailures` are an all-or-nothing pair, and the inventory has never been
    /// empty, so the gate has sat at "report everything, fail on nothing" and a NEW under-generating
    /// backend would have joined the list silently. Holding the line at today's count refuses that
    /// while still not demanding the backlog be cleared first. Lower it whenever a cause is fixed;
    /// raising it is a decision, and one this type makes visible rather than automatic.
    NoMoreThan { failures: usize },
    /// `Self::NonVacuity` plus an empty failure inventory.
    NoFailures,
}

/// The full account over every discovered fixture, stated alongside the denominator it was
/// collected over.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FaithfulnessReport {
    /// What `pg_conformance_fixtures`'s scope variable claimed for this run, verbatim.
    pub scope: String,
    /// Fixtures the caller discovered, before any of them was loaded.
    pub fixtures_discovered: usize,
    /// Fixtures that loaded and were observed -- the real denominator of the collection.
    pub fixtures_observed: Vec<String>,
    /// Every kind at least one observed fixture exhibits.
    pub kinds_exhibited: Vec<CharacteristicKind>,
    /// Backends that had at least one containment comparison actually attempted (held or failed).
    pub backends_exercised: Vec<EmissionStrategy>,
    /// Pairs for which every exhibiting fixture's containment comparison held.
    pub held: Vec<(CharacteristicKind, EmissionStrategy)>,
    /// Pairs for which at least one exhibiting fixture's containment comparison failed -- the
    /// headline number.
    pub failed: Vec<(CharacteristicKind, EmissionStrategy)>,
    /// One example per `Self::failed` entry: `(kind, strategy, fixture label, missing-analysis detail)`.
    pub failure_examples: Vec<(CharacteristicKind, EmissionStrategy, String, String)>,
    /// Pairs exhibited by at least one fixture, but for which no comparison was ever attempted on
    /// that backend (refused, oracle fault, no comparable words).
    pub not_attempted: Vec<(CharacteristicKind, EmissionStrategy)>,
    /// One example reason per `Self::not_attempted` entry.
    pub not_attempted_examples: Vec<(CharacteristicKind, EmissionStrategy, String)>,
}

impl FaithfulnessReport {
    pub fn held_for(&self, strategy: EmissionStrategy) -> Vec<CharacteristicKind> {
        self.held
            .iter()
            .filter(|(_, s)| *s == strategy)
            .map(|(kind, _)| *kind)
            .collect()
    }

    pub fn failed_for(&self, strategy: EmissionStrategy) -> Vec<CharacteristicKind> {
        self.failed
            .iter()
            .filter(|(_, s)| *s == strategy)
            .map(|(kind, _)| *kind)
            .collect()
    }

    /// The requirement's verdict: `Ok` or every violated clause, named.
    pub fn check(&self, requirement: FaithfulnessRequirement) -> Result<(), Vec<String>> {
        let mut violations = Vec::new();
        if self.fixtures_observed.is_empty() {
            violations.push(format!(
                "no fixture was observed at all (scope={}, discovered={})",
                self.scope, self.fixtures_discovered
            ));
        }
        if self.held.is_empty() && self.failed.is_empty() {
            violations.push(
                "no containment comparison was ever attempted -- every pair is not-attempted, so \
                 this run measured nothing"
                    .to_string(),
            );
        }
        if self.backends_exercised.len() < 2 {
            violations.push(format!(
                "only {} backend(s) had a containment comparison attempted ({:?}) -- a \
                 single-backend run cannot distinguish one compiler's faithfulness from three",
                self.backends_exercised.len(),
                self.backends_exercised
                    .iter()
                    .map(|s| s.label())
                    .collect::<Vec<_>>()
            ));
        }
        if let FaithfulnessRequirement::NoMoreThan { failures } = requirement {
            if self.failed.len() > failures {
                violations.push(format!(
                    "{} (kind, backend) pair(s) FAILED proposal containment, above the ratchet of \
                     {failures}; a backend started missing an analysis the oracle finds",
                    self.failed.len()
                ));
            }
        }
        if requirement == FaithfulnessRequirement::NoFailures && !self.failed.is_empty() {
            violations.push(format!(
                "{} (kind, backend) pair(s) FAILED proposal containment",
                self.failed.len()
            ));
        }
        if violations.is_empty() {
            Ok(())
        } else {
            Err(violations)
        }
    }

    /// The human-readable account: denominator first, then totals, then the failure inventory --
    /// mirrors `crate::witnessed_coverage::CompletenessReport::render`'s shape so the two reports
    /// read side by side.
    pub fn render(&self) -> String {
        let mut out = String::new();
        out.push_str("faithfulness-coverage (proposal containment vs. full-HC oracle)\n");
        out.push_str("=== denominator ===\n");
        out.push_str(&format!("conformance scope claimed: {}\n", self.scope));
        out.push_str(&format!(
            "fixtures discovered: {}; observed (loaded + characterized): {}\n",
            self.fixtures_discovered,
            self.fixtures_observed.len()
        ));
        out.push_str(&format!(
            "backends with at least one containment comparison attempted: {}\n",
            if self.backends_exercised.is_empty() {
                "NONE".to_string()
            } else {
                self.backends_exercised
                    .iter()
                    .map(|s| s.label())
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        ));
        out.push_str(&format!(
            "constructs exhibited by at least one observed fixture: {} of {}\n",
            self.kinds_exhibited.len(),
            CharacteristicKind::ALL.len()
        ));

        out.push_str("=== totals ===\n");
        out.push_str(&format!(
            "pairs exhibited: {} held; {} FAILED; {} not-attempted\n",
            self.held.len(),
            self.failed.len(),
            self.not_attempted.len()
        ));
        for &strategy in ALL_STRATEGIES {
            out.push_str(&format!(
                "  {}: {} held / {} FAILED / {} not-attempted\n",
                strategy.label(),
                self.held_for(strategy).len(),
                self.failed_for(strategy).len(),
                self.not_attempted
                    .iter()
                    .filter(|(_, s)| *s == strategy)
                    .count(),
            ));
        }

        out.push_str("=== failure inventory (the headline) ===\n");
        if self.failure_examples.is_empty() {
            out.push_str("(none)\n");
        }
        for (kind, strategy, fixture, detail) in &self.failure_examples {
            out.push_str(&format!(
                "  FAILED {kind:?} x {} -- e.g. {fixture}: {detail}\n",
                strategy.label()
            ));
        }

        if !self.not_attempted_examples.is_empty() {
            out.push_str(&format!(
                "=== not-attempted ({}) ===\n",
                self.not_attempted_examples.len()
            ));
            for (kind, strategy, reason) in &self.not_attempted_examples {
                out.push_str(&format!("  {kind:?} x {}: {reason}\n", strategy.label()));
            }
        }
        out
    }
}

/// Folds observations into the account. `scope` and `fixtures_discovered` are the caller's
/// denominator claim: this function cannot see what the caller chose to walk, so it never invents
/// either.
pub fn build_report(
    scope: &str,
    fixtures_discovered: usize,
    observations: &[FixtureContainmentObservation],
) -> FaithfulnessReport {
    let mut kinds_exhibited_set: BTreeSet<CharacteristicKind> = BTreeSet::new();
    let mut backends_exercised_set: BTreeSet<usize> = BTreeSet::new();
    for observation in observations {
        kinds_exhibited_set.extend(observation.kinds.iter().copied());
        for (strategy, outcome) in &observation.outcomes {
            if matches!(
                outcome,
                ContainmentOutcome::Held | ContainmentOutcome::Failed { .. }
            ) {
                backends_exercised_set.insert(strategy_index(*strategy));
            }
        }
    }

    let mut held = Vec::new();
    let mut failed = Vec::new();
    let mut failure_examples = Vec::new();
    let mut not_attempted = Vec::new();
    let mut not_attempted_examples = Vec::new();

    for &kind in CharacteristicKind::ALL {
        let exhibiting: Vec<&FixtureContainmentObservation> = observations
            .iter()
            .filter(|observation| observation.kinds.contains(&kind))
            .collect();
        if exhibiting.is_empty() {
            continue;
        }
        for &strategy in ALL_STRATEGIES {
            let mut first_failure: Option<(String, String, String)> = None;
            let mut any_held = false;
            let mut reasons: Vec<String> = Vec::new();
            for observation in &exhibiting {
                match observation.outcome_for(strategy) {
                    Some(ContainmentOutcome::Failed { word, detail }) => {
                        if first_failure.is_none() {
                            first_failure =
                                Some((observation.label.clone(), word.clone(), detail.clone()));
                        }
                    }
                    Some(ContainmentOutcome::Held) => any_held = true,
                    Some(ContainmentOutcome::NotAttempted { reason }) => {
                        reasons.push(format!("{}: {reason}", observation.label))
                    }
                    None => {}
                }
            }
            if let Some((fixture, word, detail)) = first_failure {
                failed.push((kind, strategy));
                failure_examples.push((
                    kind,
                    strategy,
                    fixture,
                    format!("word {word:?}: {detail}"),
                ));
            } else if any_held {
                held.push((kind, strategy));
            } else {
                not_attempted.push((kind, strategy));
                not_attempted_examples.push((
                    kind,
                    strategy,
                    reasons.into_iter().next().unwrap_or_default(),
                ));
            }
        }
    }

    FaithfulnessReport {
        scope: scope.to_string(),
        fixtures_discovered,
        fixtures_observed: observations.iter().map(|o| o.label.clone()).collect(),
        kinds_exhibited: CharacteristicKind::ALL
            .iter()
            .copied()
            .filter(|kind| kinds_exhibited_set.contains(kind))
            .collect(),
        backends_exercised: backends_exercised_set
            .iter()
            .map(|&index| ALL_STRATEGIES[index])
            .collect(),
        held,
        failed,
        failure_examples,
        not_attempted,
        not_attempted_examples,
    }
}

/// A degenerate observation: no comparison for this fixture was possible at all (a fixture that
/// failed to load, or whose characterization/evaluation panicked, caught by the caller). `kinds`
/// may be empty if it could not even be characterized.
pub fn unobservable_fixture(
    label: &str,
    kinds: Vec<CharacteristicKind>,
    reason: String,
) -> FixtureContainmentObservation {
    FixtureContainmentObservation::not_attempted(label, kinds, reason)
}

/// Position in `crate::strategy_coverage::ALL_STRATEGIES`, giving `EmissionStrategy` a total order it does not derive.
fn strategy_index(strategy: EmissionStrategy) -> usize {
    ALL_STRATEGIES
        .iter()
        .position(|&s| s == strategy)
        .unwrap_or_else(|| panic!("{strategy:?} is missing from ALL_STRATEGIES"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn observation(
        label: &str,
        kinds: &[CharacteristicKind],
        outcomes: &[(EmissionStrategy, ContainmentOutcome)],
    ) -> FixtureContainmentObservation {
        FixtureContainmentObservation {
            label: label.to_string(),
            kinds: kinds.to_vec(),
            outcomes: outcomes.to_vec(),
        }
    }

    /// A single held observation must classify as held, never as not-attempted or failed.
    #[test]
    fn a_held_pair_is_classified_held() {
        let report = build_report(
            "all",
            1,
            &[observation(
                "synthetic",
                &[CharacteristicKind::Affixation],
                &[
                    (EmissionStrategy::PlanComposed, ContainmentOutcome::Held),
                    (
                        EmissionStrategy::TunedSurfaceProbed,
                        ContainmentOutcome::Held,
                    ),
                    (
                        EmissionStrategy::TemplatedUnderlyingTokens,
                        ContainmentOutcome::NotAttempted {
                            reason: "refused-by-selector".to_string(),
                        },
                    ),
                ],
            )],
        );
        assert_eq!(
            report.held,
            vec![
                (
                    CharacteristicKind::Affixation,
                    EmissionStrategy::PlanComposed
                ),
                (
                    CharacteristicKind::Affixation,
                    EmissionStrategy::TunedSurfaceProbed
                ),
            ]
        );
        assert!(report.failed.is_empty());
        assert_eq!(
            report.not_attempted,
            vec![(
                CharacteristicKind::Affixation,
                EmissionStrategy::TemplatedUnderlyingTokens
            )]
        );
    }

    /// A failure among exhibiting fixtures must never be diluted by a passing neighbor.
    #[test]
    fn any_failure_among_exhibiting_fixtures_wins_over_held() {
        let report = build_report(
            "all",
            2,
            &[
                observation(
                    "held-fixture",
                    &[CharacteristicKind::Affixation],
                    &[(EmissionStrategy::PlanComposed, ContainmentOutcome::Held)],
                ),
                observation(
                    "failing-fixture",
                    &[CharacteristicKind::Affixation],
                    &[(
                        EmissionStrategy::PlanComposed,
                        ContainmentOutcome::Failed {
                            word: "kolo".to_string(),
                            detail: "missing identity".to_string(),
                        },
                    )],
                ),
            ],
        );
        assert_eq!(
            report.failed,
            vec![(
                CharacteristicKind::Affixation,
                EmissionStrategy::PlanComposed
            )]
        );
        assert!(report.held.is_empty());
        assert_eq!(report.failure_examples.len(), 1);
        assert_eq!(report.failure_examples[0].2, "failing-fixture");
    }

    /// A kind no observed fixture exhibits must not appear in held, failed, or not_attempted.
    #[test]
    fn an_unexhibited_kind_is_not_in_any_bucket() {
        let report = build_report(
            "all",
            1,
            &[observation(
                "synthetic",
                &[CharacteristicKind::Affixation],
                &[(EmissionStrategy::PlanComposed, ContainmentOutcome::Held)],
            )],
        );
        for &kind in CharacteristicKind::ALL {
            if kind == CharacteristicKind::Affixation {
                continue;
            }
            assert!(!report.held.iter().any(|(k, _)| *k == kind));
            assert!(!report.failed.iter().any(|(k, _)| *k == kind));
            assert!(!report.not_attempted.iter().any(|(k, _)| *k == kind));
        }
    }

    /// A run with zero observations must fail non-vacuity rather than report a clean sheet.
    #[test]
    fn a_vacuous_run_is_refused_by_the_non_vacuity_requirement() {
        let report = build_report("all", 0, &[]);
        let violations = report
            .check(FaithfulnessRequirement::NonVacuity)
            .expect_err("an empty collection must not pass");
        assert!(violations.iter().any(|v| v.contains("no fixture")));
        assert!(violations.iter().any(|v| v.contains("not-attempted")));
    }

    /// The strict requirement must reject exactly the failure inventory the lenient one tolerates.
    #[test]
    fn the_strict_requirement_rejects_a_failure_the_lenient_one_reports() {
        let report = build_report(
            "all",
            1,
            &[observation(
                "synthetic",
                &[CharacteristicKind::Affixation],
                &[(
                    EmissionStrategy::PlanComposed,
                    ContainmentOutcome::Failed {
                        word: "kolo".to_string(),
                        detail: "missing identity".to_string(),
                    },
                )],
            )],
        );
        // Pins that a single-backend failure still fails the strict requirement outright.
        let violations = report
            .check(FaithfulnessRequirement::NoFailures)
            .expect_err("a non-empty failure inventory must fail the strict requirement");
        assert!(violations
            .iter()
            .any(|v| v.contains("FAILED proposal containment")));
    }

    /// The ratchet must admit today's inventory and refuse one more -- a ceiling that cannot detect
    /// its own target would pass for every count and gate nothing.
    #[test]
    fn the_ratchet_admits_its_own_count_and_refuses_one_more() {
        let report = build_report(
            "all",
            1,
            &[observation(
                "synthetic",
                &[CharacteristicKind::Affixation],
                &[(
                    EmissionStrategy::PlanComposed,
                    ContainmentOutcome::Failed {
                        word: "kolo".to_string(),
                        detail: "missing identity".to_string(),
                    },
                )],
            )],
        );
        // Asserted on the RATCHET's own violation, not on overall success: this synthetic report
        // exercises one backend, so `check` reports a non-vacuity violation either way.
        let at_count = report
            .check(FaithfulnessRequirement::NoMoreThan { failures: 1 })
            .err()
            .unwrap_or_default();
        assert!(
            !at_count.iter().any(|v| v.contains("above the ratchet")),
            "a ratchet at the observed count must not fire: {at_count:?}"
        );
        let below = report
            .check(FaithfulnessRequirement::NoMoreThan { failures: 0 })
            .expect_err("one failure above the ratchet must be refused");
        assert!(below.iter().any(|v| v.contains("above the ratchet")));
    }
}
