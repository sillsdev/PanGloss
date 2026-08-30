//! Joins `crate::strategy_coverage`'s hand-curated, never-measured table against a real
//! per-fixture measurement (`examples/conf_matrix.rs`'s own instrument;
//! `docs/research/strategy-coverage-join-report.md` records one such run). Until this module
//! existed the table was compared against nothing, the same accounting hole
//! `crate::strategy_coverage` itself closed for per-strategy coverage: authored, load-bearing,
//! never checked.
//!
//! # Why the join is not one-to-one, and which direction is sound
//! The table is keyed by `(EmissionStrategy, CharacteristicKind)`; a measured fixture is keyed by
//! `(fixture, EmissionStrategy)`, and one fixture ordinarily exercises several
//! `CharacteristicKind`s (its authored `exercises:` tags, `crate::conformance_coverage::
//! construct_ids_for`'s own vocabulary -- reused here, never re-derived). A fixture's aggregate
//! per-strategy outcome therefore cannot be attributed to any ONE of the kinds it exercises, so
//! only one direction of comparison is airtight:
//!
//! - A [`crate::strategy_coverage::StrategyRepresentation::CannotRepresent`] row claims a
//!   compiler proposes NOTHING for the construct. If ANY fixture exercising it is measured EXACT
//!   (`crate::backend_optimizer::Certification::FullHcConfirmed` -- every comparable word in the
//!   fixture matched the live oracle) on that strategy, the row is
//!   [`JoinVerdict::Contradicted`](crate::strategy_coverage_join::JoinVerdict::Contradicted): "every word matched" already includes whichever word carried
//!   the tag, so the compiler demonstrably proposed (and confirmed) that construct at least once.
//!   No attribution is needed for this direction to be sound.
//! - The reverse never holds. A `Represents`/`RepresentsWithKnownGap` row claims a compiler CAN
//!   propose a construct, but a fixture measured NOT exact on that strategy may be failing on a
//!   *different* construct the same fixture also exercises. Absence of an exact witness is
//!   therefore only [`JoinVerdict::Unsupported`](crate::strategy_coverage_join::JoinVerdict::Unsupported), never a refutation.
//!
//! [`classify`](crate::strategy_coverage_join::classify) is the pure reduction of that asymmetry; [`classify_with_witnesses`](crate::strategy_coverage_join::classify_with_witnesses) is the same
//! reduction over real fixture-label evidence so a report can NAME which fixture did the work.
//! Neither touches a fixture, a grammar, or the compiler -- [`measure_fixture_exact`](crate::strategy_coverage_join::measure_fixture_exact) is the one
//! function here that does, and it exists only because no prior helper exposed "is this fixture's
//! confirmed output oracle-exact under this strategy" as a callable fact
//! (`examples/conf_matrix.rs` computes the same thing inline, with no library seam a second
//! caller could reuse).

use std::collections::HashSet;

use crate::backend_optimizer::Certification;
use crate::backend_runtime::{evaluate_plans_observed_with_cache, RunEvaluationCache, RuntimeBudget};
use crate::capability::CharacteristicKind;
use crate::conformance_coverage::construct_ids_for;
use crate::enumerate::{enumerate_default, CandidateRole, EmissionStrategy, LoweredCandidate};
use crate::grammar_semantics::GrammarSemantics;
use crate::junctions::PhonologyProbe;
use crate::lowering_adapter::LoweringAdapter;
use crate::strategy_coverage::StrategyRepresentation;
use pg_grammar::model::Grammar;

/// One table row's outcome against the measurement. See this module's own doc for which
/// directions are sound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinVerdict {
    /// Consistent with the table's claim: a `CannotRepresent` row with no exact exhibiting
    /// fixture, or a `Represents`/`RepresentsWithKnownGap` row WITH one.
    Agreed,
    /// A `CannotRepresent` row refuted by an exact exhibiting fixture -- the sound direction.
    /// Never produced for `Represents`/`RepresentsWithKnownGap`.
    Contradicted,
    /// A `Represents`/`RepresentsWithKnownGap` row with no exact exhibiting fixture -- suggestive
    /// only; a different construct the same fixture(s) exercise may be the real cause.
    Unsupported,
    /// No discovered fixture exercises this construct at all, so neither direction can be
    /// checked.
    NoEvidence,
}

/// Every `CharacteristicKind` at least one of whose `construct_ids_for` ids appears in
/// `exercised` -- the reverse of `construct_ids_for`, reused rather than re-derived (see this
/// module's own top-doc). `CharacteristicKind::ALL` is short enough that a linear scan per call
/// costs nothing next to the compile+oracle work every caller does around it.
pub fn kinds_exercised_by(exercised: &HashSet<&str>) -> HashSet<CharacteristicKind> {
    CharacteristicKind::ALL
        .iter()
        .copied()
        .filter(|&kind| {
            construct_ids_for(kind)
                .iter()
                .any(|id| exercised.contains(id))
        })
        .collect()
}

/// The join's pure core: reduces one table row's representation plus two measurement facts
/// (did any fixture exercise the construct; did any of those measure exact) to a verdict. Free of
/// fixture I/O so it stays cheaply unit-testable.
pub fn classify(
    representation: StrategyRepresentation,
    any_exhibiting_fixture: bool,
    any_exact_exhibiting_fixture: bool,
) -> JoinVerdict {
    if !any_exhibiting_fixture {
        return JoinVerdict::NoEvidence;
    }
    match representation {
        StrategyRepresentation::CannotRepresent => {
            if any_exact_exhibiting_fixture {
                JoinVerdict::Contradicted
            } else {
                JoinVerdict::Agreed
            }
        }
        StrategyRepresentation::Represents | StrategyRepresentation::RepresentsWithKnownGap => {
            if any_exact_exhibiting_fixture {
                JoinVerdict::Agreed
            } else {
                JoinVerdict::Unsupported
            }
        }
    }
}

/// One fixture's measured exactness, named so a report can cite it as a witness. Owns its label
/// (rather than borrowing) so a caller can build one straight from `FixtureRef::label`'s owned
/// `String` without threading a second lifetime through this whole module for a handful of short
/// strings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixtureExactness {
    pub label: String,
    pub exact: bool,
}

/// [`classify`] plus the fixture label(s) that justify it: the exact ones if any exist (positive
/// evidence, or the `Contradicted` witness), else every exhibiting fixture (named as consistent,
/// non-proving evidence).
pub fn classify_with_witnesses(
    representation: StrategyRepresentation,
    exhibiting: &[FixtureExactness],
) -> (JoinVerdict, Vec<String>) {
    let exact_labels: Vec<String> = exhibiting
        .iter()
        .filter(|f| f.exact)
        .map(|f| f.label.clone())
        .collect();
    let verdict = classify(
        representation,
        !exhibiting.is_empty(),
        !exact_labels.is_empty(),
    );
    let witnesses = if exact_labels.is_empty() {
        exhibiting.iter().map(|f| f.label.clone()).collect()
    } else {
        exact_labels
    };
    (verdict, witnesses)
}

/// Measures whether `words` under `grammar` certify
/// `Certification::FullHcConfirmed` (every comparable word matched the live oracle) when compiled
/// with `strategy` -- the same public evaluation path `examples/conf_matrix.rs` drives, extracted
/// here as a reusable fact rather than re-derived by a second caller. `grammar.char_tables` must
/// be non-empty and `words` non-empty; a grammar/corpus this fails to prepare against measures as
/// not exact rather than panicking, matching every other "could not measure" path in this crate's
/// own measurement instruments.
pub fn measure_fixture_exact(grammar: &Grammar, words: &[String], strategy: EmissionStrategy) -> bool {
    let semantics = GrammarSemantics::derive(grammar);
    let phonology = PhonologyProbe::new_with_semantics(&semantics);
    let baseline_plan = enumerate_default(grammar, semantics.prules_in_order(), phonology.as_ref());
    let Ok(mut cache) = RunEvaluationCache::prepare(grammar, words, RuntimeBudget::default()) else {
        return false;
    };
    let candidate = LoweredCandidate {
        label: "strategy-coverage-join",
        plan: baseline_plan,
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
    matches!(
        observed[0].evaluation.certification,
        Certification::FullHcConfirmed { .. }
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_exhibiting_fixture_is_no_evidence_regardless_of_representation() {
        for rep in [
            StrategyRepresentation::Represents,
            StrategyRepresentation::RepresentsWithKnownGap,
            StrategyRepresentation::CannotRepresent,
        ] {
            assert_eq!(classify(rep, false, false), JoinVerdict::NoEvidence, "{rep:?}");
        }
    }

    #[test]
    fn cannot_represent_with_an_exact_witness_is_contradicted() {
        assert_eq!(
            classify(StrategyRepresentation::CannotRepresent, true, true),
            JoinVerdict::Contradicted
        );
    }

    #[test]
    fn cannot_represent_with_no_exact_witness_is_agreed() {
        assert_eq!(
            classify(StrategyRepresentation::CannotRepresent, true, false),
            JoinVerdict::Agreed
        );
    }

    #[test]
    fn represents_with_an_exact_witness_is_agreed() {
        assert_eq!(
            classify(StrategyRepresentation::Represents, true, true),
            JoinVerdict::Agreed
        );
        assert_eq!(
            classify(StrategyRepresentation::RepresentsWithKnownGap, true, true),
            JoinVerdict::Agreed
        );
    }

    #[test]
    fn represents_with_no_exact_witness_is_unsupported_never_contradicted() {
        assert_eq!(
            classify(StrategyRepresentation::Represents, true, false),
            JoinVerdict::Unsupported
        );
        assert_eq!(
            classify(StrategyRepresentation::RepresentsWithKnownGap, true, false),
            JoinVerdict::Unsupported
        );
    }

    fn exactness(label: &str, exact: bool) -> FixtureExactness {
        FixtureExactness { label: label.to_string(), exact }
    }

    #[test]
    fn witnesses_prefer_exact_fixtures_and_name_the_contradiction() {
        let exhibiting = [exactness("refuses-here", false), exactness("works-here", true)];
        let (verdict, witnesses) =
            classify_with_witnesses(StrategyRepresentation::CannotRepresent, &exhibiting);
        assert_eq!(verdict, JoinVerdict::Contradicted);
        assert_eq!(witnesses, vec!["works-here".to_string()]);
    }

    #[test]
    fn witnesses_name_every_exhibiting_fixture_when_none_is_exact() {
        let exhibiting = [exactness("a", false), exactness("b", false)];
        let (verdict, witnesses) =
            classify_with_witnesses(StrategyRepresentation::Represents, &exhibiting);
        assert_eq!(verdict, JoinVerdict::Unsupported);
        assert_eq!(witnesses, vec!["a".to_string(), "b".to_string()]);
    }

    /// Feeding in every id a kind maps to must recover that kind (no second hand-copied mapping).
    #[test]
    fn kinds_exercised_by_recovers_every_kind_from_its_own_construct_ids() {
        for &kind in CharacteristicKind::ALL {
            let ids: HashSet<&str> = construct_ids_for(kind).iter().copied().collect();
            if ids.is_empty() {
                continue; // Unmappable kinds (none today) have nothing to recover from.
            }
            let recovered = kinds_exercised_by(&ids);
            assert!(recovered.contains(&kind), "{kind:?} not recovered from its own ids {ids:?}");
        }
    }

    #[test]
    fn kinds_exercised_by_is_empty_for_an_unknown_id() {
        let ids: HashSet<&str> = ["not-a-real-construct-id"].into_iter().collect();
        assert!(kinds_exercised_by(&ids).is_empty());
    }
}
