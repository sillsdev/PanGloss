use super::*;

/// The table is total: every `(strategy, kind)` pair has a row, and every row cites something.
#[test]
fn every_strategy_kind_pair_has_a_cited_row() {
    for &strategy in ALL_STRATEGIES {
        for &kind in CharacteristicKind::ALL {
            let row = representation_of(strategy, kind);
            assert_eq!(row.strategy, strategy);
            assert_eq!(row.kind, kind);
            assert!(
                !row.evidence.is_empty(),
                "{strategy:?} x {kind:?} has an empty evidence citation"
            );
        }
    }
}

/// `EmissionStrategy` has no reflection, so this pins the count and each label rather than deriving it.
#[test]
fn all_strategies_lists_every_emission_strategy() {
    let labels: Vec<&str> = ALL_STRATEGIES.iter().map(|s| s.label()).collect();
    assert_eq!(labels, vec!["tuned-surface-probed"]);
}

/// Pinned so a regression in realizational morphology is a reviewed table edit.
#[test]
fn realizational_morphology_is_representable_by_every_strategy_now() {
    for &strategy in ALL_STRATEGIES {
        assert_eq!(
            representation_of(strategy, CharacteristicKind::RealizationalMorphology).representation,
            StrategyRepresentation::Represents,
            "{strategy:?}"
        );
    }
}

/// Pinned so a regression in the bounded compound loop shows up here as a reviewed table edit.
#[test]
fn compounding_is_representable_by_every_strategy_now() {
    for &strategy in ALL_STRATEGIES {
        assert_eq!(
            representation_of(strategy, CharacteristicKind::Compounding).representation,
            StrategyRepresentation::Represents,
            "{strategy:?}"
        );
    }
}

// The premise `crate::capability::StrategyEnvelope::global` rests on: one compiler with no gap.
#[test]
fn some_strategy_represents_every_kind() {
    assert_eq!(
        strategies_representing_every_kind(),
        vec![EmissionStrategy::TunedSurfaceProbed],
        "the mainline compiler is the one with no recorded gap; if that changes, \
         capability::StrategyEnvelope::global's identity to the compiler-blind verdict changes \
         with it"
    );
}

#[test]
fn the_mainline_compiler_has_no_unrepresentable_kind() {
    assert!(unrepresentable_kinds(EmissionStrategy::TunedSurfaceProbed).is_empty());
}

#[test]
fn strategies_that_represent_lists_the_mainline_for_every_kind() {
    for &kind in CharacteristicKind::ALL {
        assert_eq!(
            strategies_that_represent(kind),
            vec![EmissionStrategy::TunedSurfaceProbed],
            "{kind:?}"
        );
    }
}
