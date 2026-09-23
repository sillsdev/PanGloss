//! The shared walk + shared fold `crate::witnessed_coverage` and `crate::faithfulness_coverage`
//! were each hand-rolling: one `discover()` + panic-hook-silenced fixture loop, one
//! `(CharacteristicKind, EmissionStrategy)` double-loop fold, one `strategy_index` helper -- all
//! three byte-identical in shape across the two modules before this seam existed. Design C of
//! `docs/research/coverage-verdict-seam-design.md`; each instrument's own compile/containment
//! measurement stays exactly where it was, called from inside the closure it hands to
//! `collect_observations`, and `build_report` never sees that closure at all.
//!
//! # What is deliberately NOT here
//! No `run_sweep`, no `CoverageObserver`, no `FixtureContext` laziness -- that is Design A, the
//! design doc's own follow-on for a third full-sweep instrument that does not exist yet (Section 8).
//! This module is a strict subset A can grow from later, not a step toward it.

use std::collections::BTreeSet;
use std::panic;

use pg_grammar::model::Grammar;

use crate::capability::CharacteristicKind;
use crate::enumerate::EmissionStrategy;
use crate::strategy_coverage::ALL_STRATEGIES;

/// Why a measurement never happened. Two causes, deliberately not one bag of strings: a Selector
/// refusal is a fact about the Backend/grammar pair that cannot change without a capability or
/// Selector change, so it can never be closed by "try harder"; every other non-attempt (an
/// oracle-preparation fault, a truncated proposal set, an empty corpus) is a fault a fix CAN close.
/// Conflating the two is exactly how a capability-refused Backend gets counted, unintentionally, in
/// the same backlog as a real defect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotAttemptedReason {
    /// `crate::backend_selection::select_backends` refused this backend, so nothing was measured.
    RefusedBySelector,
    /// The oracle itself yielded nothing comparable, so no backend can miss anything here.
    NoComparableWords,
    /// Evaluation stopped before per-word evidence existed; carries the certification that stopped it.
    EvaluationIncomplete(String),
    /// The fixture could not be prepared at all; carries the fault.
    OracleFault(String),
    /// The fixture offers nothing to compare before any measurement runs (no words, no character table).
    FixtureNotComparable(String),
}

impl NotAttemptedReason {
    /// Stable label for reports; the payload-carrying variants render their own detail.
    pub fn label(&self) -> &'static str {
        match self {
            Self::RefusedBySelector => "refused-by-selector",
            Self::NoComparableWords => "no comparable words after oracle exclusions",
            Self::EvaluationIncomplete(_) => "evaluation did not reach comparable words",
            Self::OracleFault(_) => "oracle preparation faulted",
            Self::FixtureNotComparable(_) => "fixture offers nothing to compare",
        }
    }
}

impl std::fmt::Display for NotAttemptedReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EvaluationIncomplete(detail)
            | Self::OracleFault(detail)
            | Self::FixtureNotComparable(detail) => {
                write!(f, "{}: {detail}", self.label())
            }
            _ => f.write_str(self.label()),
        }
    }
}

/// One (kind, strategy) -- or (fixture, strategy) -- pair's outcome, generic over what a FAILURE
/// carries as evidence. `Failure` is the only place `crate::witnessed_coverage::BackendOutcome` and
/// `crate::faithfulness_coverage::ContainmentOutcome` differ; the three-way shape is identical.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict<Failure> {
    Held,
    Failed(Failure),
    NotAttempted(NotAttemptedReason),
}

/// The fold contract: whatever an instrument's own enum is called, this is recoverable from it at
/// zero cost, which is what lets a per-pair enum plug into `build_report` without becoming, or
/// losing information to, `Verdict` itself.
pub trait MeasuredOutcome {
    type Failure: Clone;
    fn classify(&self) -> Verdict<Self::Failure>;
}

/// Any instrument's per-unit observation: a labelled unit, the constructs it exhibits, and one
/// measured outcome per strategy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Observation<E> {
    /// Caller-supplied identity (a `pg_conformance_fixtures::FixtureRef::label`, typically).
    pub label: String,
    /// Every distinct `CharacteristicKind` the unit exhibits, in `CharacteristicKind::ALL` order.
    pub kinds: Vec<CharacteristicKind>,
    /// One entry per `crate::strategy_coverage::ALL_STRATEGIES` entry, in that constant's order.
    pub outcomes: Vec<(EmissionStrategy, E)>,
}

impl<E> Observation<E> {
    pub fn outcome_for(&self, strategy: EmissionStrategy) -> Option<&E> {
        self.outcomes
            .iter()
            .find(|(s, _)| *s == strategy)
            .map(|(_, outcome)| outcome)
    }
}

/// Position in `crate::strategy_coverage::ALL_STRATEGIES`, giving `EmissionStrategy` a total order
/// it does not derive. The one function `witnessed_coverage.rs:605` and `faithfulness_coverage.rs:699`
/// each defined identically before this seam existed.
pub fn strategy_index(strategy: EmissionStrategy) -> usize {
    ALL_STRATEGIES
        .iter()
        .position(|&s| s == strategy)
        .unwrap_or_else(|| panic!("{strategy:?} is missing from ALL_STRATEGIES"))
}

/// Walks every already-discovered fixture once, silencing panic spam, and asks `observe_one` to
/// turn each loadable grammar into one observation. A fixture `load_grammar` returns `None` for
/// contributes nothing to the result (same as both hand-rolled walks this replaces); anything
/// `observe_one` itself needs to guard against (a mid-measurement panic, a fixture-specific skip)
/// stays the caller's own responsibility, exactly as it is today -- this function owns only the
/// parts that were duplicated byte-for-byte: the hook dance and the load-or-skip branch.
///
/// Generic over the fixture type `F` rather than `pg_conformance_fixtures::FixtureRef` directly:
/// that crate is a dev-dependency of this one (tests/examples only), so this module -- part of the
/// library proper -- cannot name its type. The caller (a `tests/*.rs` file, which already depends
/// on it) supplies `fixtures` pre-discovered and `load_grammar` as the one line that knows how.
///
/// Generic over the observation type `O` rather than fixed to `Observation<E>`: an instrument whose
/// per-unit shape carries more than one outcome vector (`crate::faithfulness_coverage::
/// FixtureContainmentObservation`'s soundness side-vector, see its own doc) still shares this walk;
/// only `build_report` requires the narrower `Observation<E>` shape.
pub fn collect_observations<F, O>(
    fixtures: &[F],
    load_grammar: impl Fn(&F) -> Option<Grammar>,
    observe_one: impl Fn(&F, &Grammar) -> O,
) -> (usize, Vec<O>) {
    let discovered = fixtures.len();

    let default_hook = panic::take_hook();
    panic::set_hook(Box::new(|_| {}));
    let mut observations = Vec::new();
    for fixture in fixtures {
        let Some(grammar) = load_grammar(fixture) else {
            continue;
        };
        observations.push(observe_one(fixture, &grammar));
    }
    panic::set_hook(default_hook);
    (discovered, observations)
}

/// The shared fold: every field a `MeasuredOutcome`-driven account needs, over the fixtures that
/// actually exhibit each construct. This is the literal deletion of both `build_report` bodies
/// `docs/research/coverage-verdict-seam-design.md` names in `witnessed_coverage.rs` and
/// `faithfulness_coverage.rs` -- each module's own report type is now built FROM this one, adding
/// only what genuinely differs (a declarative cannot-represent overlay on one side, a soundness
/// axis on the other).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverageMatrix<F> {
    /// What the caller's discovery claimed for this run, verbatim.
    pub scope: String,
    /// Units discovered, before any of them was loaded.
    pub discovered: usize,
    /// Units that loaded and were observed -- the real denominator of the collection.
    pub observed: Vec<String>,
    /// Every kind at least one observed unit exhibits.
    pub kinds_exhibited: Vec<CharacteristicKind>,
    /// Strategies with at least one Held or Failed verdict credited anywhere.
    pub backends_active: Vec<EmissionStrategy>,
    /// Pairs for which every exhibiting unit's verdict held, once any failure among them is ruled out.
    pub held: Vec<(CharacteristicKind, EmissionStrategy)>,
    /// Pairs for which at least one exhibiting unit's verdict failed -- a failure among exhibiting
    /// units always wins over a held one, never the reverse.
    pub failed: Vec<(CharacteristicKind, EmissionStrategy)>,
    /// One example per `Self::failed` entry: `(kind, strategy, unit label, failure evidence)`.
    pub failed_examples: Vec<(CharacteristicKind, EmissionStrategy, String, F)>,
    /// Pairs exhibited by at least one unit, but for which no exhibiting unit's measurement ever held or failed.
    pub not_attempted: Vec<(CharacteristicKind, EmissionStrategy)>,
    /// One example reason per `Self::not_attempted` entry, already formatted as `"{label}: {reason}"`.
    pub not_attempted_examples: Vec<(CharacteristicKind, EmissionStrategy, String)>,
}

/// Folds `observations` into the shared matrix. `scope` and `discovered` are the caller's
/// denominator claim: this function cannot see what the caller chose to walk, so it never invents
/// either. A kind no observation exhibits contributes nothing here -- that is the declarative
/// overlay `witnessed_coverage` layers on top for its own full-grid gap inventory, not a fact this
/// fold can state on its own.
pub fn build_report<E: MeasuredOutcome>(
    scope: &str,
    discovered: usize,
    observations: &[Observation<E>],
) -> CoverageMatrix<E::Failure> {
    let mut kinds_exhibited_set: BTreeSet<CharacteristicKind> = BTreeSet::new();
    let mut backends_active_set: BTreeSet<usize> = BTreeSet::new();
    for observation in observations {
        kinds_exhibited_set.extend(observation.kinds.iter().copied());
        for (strategy, outcome) in &observation.outcomes {
            if matches!(outcome.classify(), Verdict::Held | Verdict::Failed(_)) {
                backends_active_set.insert(strategy_index(*strategy));
            }
        }
    }

    let mut held = Vec::new();
    let mut failed = Vec::new();
    let mut failed_examples = Vec::new();
    let mut not_attempted = Vec::new();
    let mut not_attempted_examples = Vec::new();

    for &kind in CharacteristicKind::ALL {
        let exhibiting: Vec<&Observation<E>> = observations
            .iter()
            .filter(|observation| observation.kinds.contains(&kind))
            .collect();
        if exhibiting.is_empty() {
            continue;
        }
        for &strategy in ALL_STRATEGIES {
            let mut first_failure: Option<(String, E::Failure)> = None;
            let mut any_held = false;
            let mut reasons: Vec<String> = Vec::new();
            for observation in &exhibiting {
                match observation.outcome_for(strategy).map(MeasuredOutcome::classify) {
                    Some(Verdict::Failed(failure)) => {
                        if first_failure.is_none() {
                            first_failure = Some((observation.label.clone(), failure));
                        }
                    }
                    Some(Verdict::Held) => any_held = true,
                    Some(Verdict::NotAttempted(reason)) => {
                        reasons.push(format!("{}: {reason}", observation.label))
                    }
                    None => {}
                }
            }
            if let Some((label, failure)) = first_failure {
                failed.push((kind, strategy));
                failed_examples.push((kind, strategy, label, failure));
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

    CoverageMatrix {
        scope: scope.to_string(),
        discovered,
        observed: observations.iter().map(|o| o.label.clone()).collect(),
        kinds_exhibited: CharacteristicKind::ALL
            .iter()
            .copied()
            .filter(|kind| kinds_exhibited_set.contains(kind))
            .collect(),
        backends_active: backends_active_set
            .iter()
            .map(|&index| ALL_STRATEGIES[index])
            .collect(),
        held,
        failed,
        failed_examples,
        not_attempted,
        not_attempted_examples,
    }
}
