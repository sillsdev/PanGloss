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
use crate::coverage_seam::{self, MeasuredOutcome, Observation, Verdict};
use crate::enumerate::{enumerate_default, CandidateRole, EmissionStrategy, LoweredCandidate};
use crate::grammar_semantics::GrammarSemantics;
use crate::junctions::PhonologyProbe;
use crate::lowering_adapter::LoweringAdapter;
use crate::strategy_coverage::ALL_STRATEGIES;

/// Why a containment comparison never ran, moved to `crate::coverage_seam` so
/// `crate::witnessed_coverage`'s `BackendOutcome` can spell the same "never measured" fact the same
/// way. Re-exported here so every existing caller of `faithfulness_coverage::NotAttemptedReason`
/// keeps resolving.
pub use crate::coverage_seam::NotAttemptedReason;

/// One (fixture, backend) pair's containment outcome. Only `Held`/`Failed` come from a real
/// comparison; `NotAttempted` covers every reason the comparison could not run at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContainmentOutcome {
    /// Every comparable word's proposal set contained every distinct oracle identity at least once
    /// (presence, not the oracle's own multiplicity -- see `backend_runtime::word_proposal_containment`'s doc).
    Held,
    /// At least one comparable word's proposal set never offered some oracle identity at all; carries the first such gap, human-readable.
    Failed { word: String, detail: String },
    /// The comparison never ran for this fixture/backend pair; names why.
    NotAttempted { reason: NotAttemptedReason },
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

/// The (word, detail) pair a containment failure carries, recovered losslessly from `Self::Failed`.
impl MeasuredOutcome for ContainmentOutcome {
    type Failure = (String, String);

    fn classify(&self) -> Verdict<Self::Failure> {
        match self {
            Self::Held => Verdict::Held,
            Self::Failed { word, detail } => Verdict::Failed((word.clone(), detail.clone())),
            Self::NotAttempted { reason } => Verdict::NotAttempted(reason.clone()),
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
    /// One entry per `crate::strategy_coverage::ALL_STRATEGIES` entry: this fixture's
    /// `crate::parity::IdentityDivergence::candidate_only_identities` for that backend's corpus
    /// pass, or `None` if nothing was compared (mirrors why `ContainmentOutcome::NotAttempted` is
    /// its own variant rather than a clean pass -- see `crate::backend_runtime::RuntimeEvaluation`).
    /// This is the SOUNDNESS half of the account: `Self::outcomes` is containment (recall) only, by
    /// design (see this module's doc), and never sees an over-generation that survived confirmation.
    pub soundness: Vec<(EmissionStrategy, Option<u64>)>,
}

impl FixtureContainmentObservation {
    pub fn outcome_for(&self, strategy: EmissionStrategy) -> Option<&ContainmentOutcome> {
        self.outcomes
            .iter()
            .find(|(s, _)| *s == strategy)
            .map(|(_, outcome)| outcome)
    }

    /// This fixture's candidate-only identity count for `strategy`, or `None` if nothing was
    /// compared (either because no entry exists for `strategy` at all, or because the comparison
    /// never ran) -- both collapse to `None` deliberately, since neither supports a claim either way.
    pub fn soundness_for(&self, strategy: EmissionStrategy) -> Option<u64> {
        self.soundness
            .iter()
            .find(|(s, _)| *s == strategy)
            .and_then(|(_, count)| *count)
    }

    fn not_attempted(label: &str, kinds: Vec<CharacteristicKind>, reason: NotAttemptedReason) -> Self {
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
            soundness: ALL_STRATEGIES.iter().map(|&strategy| (strategy, None)).collect(),
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
            NotAttemptedReason::FixtureNotComparable("fixture has no words".to_string()),
        );
    }
    let Some(_) = grammar.char_tables.first() else {
        return FixtureContainmentObservation::not_attempted(
            label,
            kinds,
            NotAttemptedReason::FixtureNotComparable("grammar has no character table".to_string()),
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
                    reason: NotAttemptedReason::RefusedBySelector,
                },
            ));
        }
    }
    if selected_strategies.is_empty() {
        let soundness = not_attempted.iter().map(|(s, _)| (*s, None)).collect();
        return FixtureContainmentObservation {
            label: label.to_string(),
            kinds,
            outcomes: not_attempted,
            soundness,
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
            let reason = NotAttemptedReason::OracleFault(fault.to_string());
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
            outcomes.extend(not_attempted.iter().cloned());
            let soundness = outcomes.iter().map(|(s, _)| (*s, None)).collect();
            return FixtureContainmentObservation {
                label: label.to_string(),
                kinds,
                outcomes,
                soundness,
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

    let mut outcomes: Vec<(EmissionStrategy, ContainmentOutcome)> = Vec::with_capacity(plans.len());
    // Parallel to `outcomes`, but the SOUNDNESS half of the account: `crate::backend_runtime::word_proposal_containment` (what `outcomes` is built from) checks recall only, by design, and cannot see a candidate-only identity that survived confirmation -- that fact lives on `RuntimeEvaluation::divergence`, which `outcomes`'s own containment check never reads.
    let mut soundness: Vec<(EmissionStrategy, Option<u64>)> = Vec::with_capacity(plans.len());
    for (plan, observation) in plans.iter().zip(&observed) {
        let strategy = plan.strategy();
        let divergence = observation.evaluation.divergence;
        soundness.push((
            strategy,
            (divergence.occurrences_compared > 0)
                .then_some(divergence.candidate_only_identities),
        ));
        let Some(evidence) = &observation.words else {
            outcomes.push((
                strategy,
                ContainmentOutcome::NotAttempted {
                    reason: NotAttemptedReason::EvaluationIncomplete(format!(
                        "{:?}",
                        observation.evaluation.certification
                    )),
                },
            ));
            continue;
        };
        outcomes.push((strategy, containment_outcome_for_evidence(evidence)));
    }
    outcomes.extend(not_attempted.iter().cloned());
    soundness.extend(not_attempted.iter().map(|(s, _)| (*s, None)));

    FixtureContainmentObservation {
        label: label.to_string(),
        kinds,
        outcomes,
        soundness,
    }
}

/// Classifies one backend's already-observed evidence for one fixture: `Held` if every comparable
/// word's proposal set contained every one of the oracle's distinct identities at least once,
/// `Failed` at the first word that did not, or `NotAttempted` if there is no comparable evidence at all (an
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
            reason: NotAttemptedReason::NoComparableWords,
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

/// How strictly `FaithfulnessReport::check_soundness` reads the over-generation inventory.
///
/// A separate axis from `FaithfulnessRequirement`, deliberately not folded into it:
/// `FaithfulnessRequirement`/`Self::held`/`Self::failed` are containment (recall) only, by this
/// module's own design (see its doc), and cannot see a candidate-only identity that survived
/// confirmation. `crate::parity::IdentityDivergence` is the one place that count exists, and this
/// is its gate. See `crate::parity::IdentityDivergence` for why the two directions are counted
/// separately rather than merged into one number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SoundnessRequirement {
    /// Ratchet, not a target: holds today's real count as the ceiling so a NEW over-generating
    /// backend fails the gate while a known backlog stays legible. Mirrors
    /// `FaithfulnessRequirement::NoMoreThan`.
    NoMoreThan { over_generations: usize },
    /// `Self::NoMoreThan { over_generations: 0 }`, named: the measured starting count IS zero (no
    /// backlog to hold), so there is nothing to ratchet down later -- a regression is the only way
    /// this fires. Mirrors `FaithfulnessRequirement::NoFailures`.
    NoOverGeneration,
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
    /// Pairs for which at least one exhibiting fixture's corpus pass counted a candidate-only
    /// identity -- the SOUNDNESS hazard `crate::parity::IdentityDivergence::candidate_only_identities`
    /// exists to catch. A distinct axis from `Self::failed` (the opposite-direction recall defect):
    /// a pair can appear in both, neither, or exactly one.
    pub over_generating: Vec<(CharacteristicKind, EmissionStrategy)>,
    /// One example per `Self::over_generating` entry: `(kind, strategy, fixture label, candidate-only identity count)`.
    pub over_generation_examples: Vec<(CharacteristicKind, EmissionStrategy, String, u64)>,
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

    /// The SOUNDNESS ratchet's verdict, separate from `Self::check`: relies on `Self::check`'s own
    /// non-vacuity clause (same `observed` pass feeds both accounts) rather than re-asserting it,
    /// so this checks only the over-generation ceiling itself.
    pub fn check_soundness(&self, requirement: SoundnessRequirement) -> Result<(), Vec<String>> {
        let over_generations = match requirement {
            SoundnessRequirement::NoMoreThan { over_generations } => over_generations,
            SoundnessRequirement::NoOverGeneration => 0,
        };
        if self.over_generating.len() > over_generations {
            Err(vec![format!(
                "{} (kind, backend) pair(s) counted a CANDIDATE-ONLY identity (a surviving \
                 over-generation) above the ratchet of {over_generations}; \
                 crate::parity::IdentityDivergence found something confirm should have pruned",
                self.over_generating.len()
            )])
        } else {
            Ok(())
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
        out.push_str(&format!(
            "soundness (candidate-only identities that survived confirmation): {} (kind, backend) pair(s) OVER-GENERATING\n",
            self.over_generating.len()
        ));

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

        out.push_str("=== over-generation inventory (soundness) ===\n");
        if self.over_generation_examples.is_empty() {
            out.push_str("(none)\n");
        }
        for (kind, strategy, fixture, count) in &self.over_generation_examples {
            out.push_str(&format!(
                "  OVER-GENERATING {kind:?} x {} -- e.g. {fixture}: {count} candidate-only identity(ies) survived confirmation\n",
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

/// Folds observations into the account, over the shared `crate::coverage_seam::build_report` fold
/// for the containment (recall) axis. `scope` and `fixtures_discovered` are the caller's
/// denominator claim: this function cannot see what the caller chose to walk, so it never invents
/// either.
///
/// The soundness (over-generation) axis is NOT part of the shared fold: it reads
/// `FixtureContainmentObservation::soundness`, a side-vector `crate::coverage_seam::Observation`
/// has no field for, since it answers a question `ContainmentOutcome`'s own `MeasuredOutcome::classify`
/// cannot see (this module's own doc explains why the two axes are independent).
pub fn build_report(
    scope: &str,
    fixtures_discovered: usize,
    observations: &[FixtureContainmentObservation],
) -> FaithfulnessReport {
    let generic: Vec<Observation<ContainmentOutcome>> = observations
        .iter()
        .map(|o| Observation {
            label: o.label.clone(),
            kinds: o.kinds.clone(),
            outcomes: o.outcomes.clone(),
        })
        .collect();
    let matrix = coverage_seam::build_report(scope, fixtures_discovered, &generic);

    let mut over_generating = Vec::new();
    let mut over_generation_examples = Vec::new();
    for &kind in CharacteristicKind::ALL {
        let exhibiting: Vec<&FixtureContainmentObservation> = observations
            .iter()
            .filter(|observation| observation.kinds.contains(&kind))
            .collect();
        if exhibiting.is_empty() {
            continue;
        }
        for &strategy in ALL_STRATEGIES {
            let mut first_over_generation: Option<(String, u64)> = None;
            for observation in &exhibiting {
                if first_over_generation.is_none() {
                    if let Some(count) = observation.soundness_for(strategy) {
                        if count > 0 {
                            first_over_generation = Some((observation.label.clone(), count));
                        }
                    }
                }
            }
            if let Some((fixture, count)) = first_over_generation {
                over_generating.push((kind, strategy));
                over_generation_examples.push((kind, strategy, fixture, count));
            }
        }
    }

    FaithfulnessReport {
        scope: matrix.scope,
        fixtures_discovered: matrix.discovered,
        fixtures_observed: matrix.observed,
        kinds_exhibited: matrix.kinds_exhibited,
        backends_exercised: matrix.backends_active,
        held: matrix.held,
        failed: matrix.failed,
        failure_examples: matrix
            .failed_examples
            .into_iter()
            .map(|(kind, strategy, fixture, (word, detail))| {
                (kind, strategy, fixture, format!("word {word:?}: {detail}"))
            })
            .collect(),
        not_attempted: matrix.not_attempted,
        not_attempted_examples: matrix.not_attempted_examples,
        over_generating,
        over_generation_examples,
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
    FixtureContainmentObservation::not_attempted(
        label,
        kinds,
        NotAttemptedReason::FixtureNotComparable(reason),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // Every strategy `None`; `observation_with_soundness` is the one that exercises the new axis.
    fn observation(
        label: &str,
        kinds: &[CharacteristicKind],
        outcomes: &[(EmissionStrategy, ContainmentOutcome)],
    ) -> FixtureContainmentObservation {
        observation_with_soundness(label, kinds, outcomes, &[])
    }

    fn observation_with_soundness(
        label: &str,
        kinds: &[CharacteristicKind],
        outcomes: &[(EmissionStrategy, ContainmentOutcome)],
        soundness: &[(EmissionStrategy, u64)],
    ) -> FixtureContainmentObservation {
        FixtureContainmentObservation {
            label: label.to_string(),
            kinds: kinds.to_vec(),
            outcomes: outcomes.to_vec(),
            soundness: ALL_STRATEGIES
                .iter()
                .map(|&strategy| {
                    (
                        strategy,
                        soundness
                            .iter()
                            .find(|(s, _)| *s == strategy)
                            .map(|(_, count)| *count),
                    )
                })
                .collect(),
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
                            reason: NotAttemptedReason::RefusedBySelector,
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

    /// The ratchet must admit today's count and refuse one more, or it gates nothing.
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
        // On the ratchet's own violation: one backend trips non-vacuity either way.
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

    // A HELD (recall) fixture can still be flagged over-generating: the two axes must not conflate.
    #[test]
    fn over_generation_is_counted_independently_of_containment() {
        let report = build_report(
            "all",
            1,
            &[observation_with_soundness(
                "over-generating-but-held",
                &[CharacteristicKind::Affixation],
                &[(EmissionStrategy::PlanComposed, ContainmentOutcome::Held)],
                &[(EmissionStrategy::PlanComposed, 3)],
            )],
        );
        assert_eq!(
            report.held,
            vec![(
                CharacteristicKind::Affixation,
                EmissionStrategy::PlanComposed
            )],
            "recall containment must still read HELD"
        );
        assert_eq!(
            report.over_generating,
            vec![(
                CharacteristicKind::Affixation,
                EmissionStrategy::PlanComposed
            )],
            "a candidate-only identity must be visible even though recall containment held"
        );
        assert_eq!(report.over_generation_examples.len(), 1);
        assert_eq!(report.over_generation_examples[0].2, "over-generating-but-held");
        assert_eq!(report.over_generation_examples[0].3, 3);
    }

    // Neither a zero-count reading nor an uncompared (`None`) one may read as an over-generation.
    #[test]
    fn zero_and_not_compared_soundness_readings_never_over_generate() {
        let report = build_report(
            "all",
            1,
            &[observation_with_soundness(
                "clean",
                &[CharacteristicKind::Affixation],
                &[(EmissionStrategy::PlanComposed, ContainmentOutcome::Held)],
                &[(EmissionStrategy::PlanComposed, 0)],
            )],
        );
        assert!(report.over_generating.is_empty());

        // `soundness: &[]` (no entry) makes every strategy `None` via `observation_with_soundness`'s own fill -- "not compared", not "clean".
        let uncompared = build_report(
            "all",
            1,
            &[observation(
                "uncompared",
                &[CharacteristicKind::Affixation],
                &[(EmissionStrategy::PlanComposed, ContainmentOutcome::Held)],
            )],
        );
        assert!(uncompared.over_generating.is_empty());
    }

    // FALSIFICATION: a forced over-generation must trip `check_soundness`; the ratchet at that exact count must not.
    #[test]
    fn the_soundness_ratchet_fires_on_a_forced_over_generation_and_admits_its_own_count() {
        let clean = build_report(
            "all",
            1,
            &[observation_with_soundness(
                "synthetic",
                &[CharacteristicKind::Affixation],
                &[(EmissionStrategy::PlanComposed, ContainmentOutcome::Held)],
                &[(EmissionStrategy::PlanComposed, 0)],
            )],
        );
        assert!(
            clean
                .check_soundness(SoundnessRequirement::NoMoreThan { over_generations: 0 })
                .is_ok(),
            "a clean run must not trip a zero ratchet"
        );
        assert!(
            clean.check_soundness(SoundnessRequirement::NoOverGeneration).is_ok(),
            "NoOverGeneration must agree with NoMoreThan{{0}} on a clean run"
        );

        // Forces the trigger the gate exists to catch: a candidate-only identity that survived confirmation.
        let sabotaged = build_report(
            "all",
            1,
            &[observation_with_soundness(
                "synthetic",
                &[CharacteristicKind::Affixation],
                &[(EmissionStrategy::PlanComposed, ContainmentOutcome::Held)],
                &[(EmissionStrategy::PlanComposed, 1)],
            )],
        );
        let violations = sabotaged
            .check_soundness(SoundnessRequirement::NoMoreThan { over_generations: 0 })
            .expect_err("a forced over-generation must be caught by the ratchet");
        assert!(violations.iter().any(|v| v.contains("CANDIDATE-ONLY")));
        assert!(
            sabotaged.check_soundness(SoundnessRequirement::NoOverGeneration).is_err(),
            "NoOverGeneration must also catch the same forced over-generation"
        );

        // The ratchet set to the observed count must admit it -- a ratchet that never admits its own count gates nothing either.
        assert!(sabotaged
            .check_soundness(SoundnessRequirement::NoMoreThan { over_generations: 1 })
            .is_ok());
    }
}
