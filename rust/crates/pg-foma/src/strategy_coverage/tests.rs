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
    assert_eq!(
        labels,
        vec![
            "plan-composed",
            "tuned-surface-probed",
            "templated-underlying-tokens"
        ]
    );
}

/// Pinned so a regression in `uflexc`'s `affix_allomorphs()`-based walk is a reviewed table edit.
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

/// If the strategies never disagreed, this table would be a strategy-indexed copy of one answer.
#[test]
fn at_least_one_kind_is_answered_differently_by_two_strategies() {
    let disagreements: Vec<CharacteristicKind> = CharacteristicKind::ALL
        .iter()
        .copied()
        .filter(|&kind| {
            let first = representation_of(ALL_STRATEGIES[0], kind).representation;
            ALL_STRATEGIES
                .iter()
                .any(|&s| representation_of(s, kind).representation != first)
        })
        .collect();
    assert!(
        !disagreements.is_empty(),
        "no construct is answered differently by two strategies -- the table would be \
         strategy-blind in effect"
    );
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
fn unrepresentable_kinds_names_every_hole_and_leaves_the_mainline_clear() {
    assert_eq!(
        unrepresentable_kinds(EmissionStrategy::PlanComposed),
        vec![
            // uflexc lists Role::Process in its own skipped set; no lexc line is emitted at all.
            CharacteristicKind::ProcessMorphology
        ]
    );
    assert_eq!(
        unrepresentable_kinds(EmissionStrategy::TemplatedUnderlyingTokens),
        vec![
            // A root whose own spelling the final table lacks is filed `unsegmentable-root` and skipped.
            CharacteristicKind::CrossTableRespelling,
            CharacteristicKind::ProcessMorphology
        ],
        "the templated emitter's own doc says it has no composite pipeline, so it cannot \
         realize an in-place mutation"
    );
    assert!(unrepresentable_kinds(EmissionStrategy::TunedSurfaceProbed).is_empty());
}

#[test]
fn strategies_that_represent_excludes_every_holed_compiler() {
    assert_eq!(
        strategies_that_represent(CharacteristicKind::ProcessMorphology),
        vec![EmissionStrategy::TunedSurfaceProbed]
    );
    assert_eq!(
        strategies_that_represent(CharacteristicKind::Affixation).len(),
        ALL_STRATEGIES.len()
    );
}
