use super::*;

const ALL: [EmissionStrategy; 3] = [
    EmissionStrategy::PlanComposed,
    EmissionStrategy::TunedSurfaceProbed,
    EmissionStrategy::TemplatedUnderlyingTokens,
];

/// `from_label` must invert `label` for every variant, or a route string round-trip silently drifts.
#[test]
fn label_and_from_label_round_trip() {
    for strategy in ALL {
        assert_eq!(
            EmissionStrategy::from_label(strategy.label()),
            Some(strategy)
        );
    }
}

/// An unrecognized route string must never resolve to a strategy by accident.
#[test]
fn from_label_rejects_unknown_strings() {
    assert_eq!(EmissionStrategy::from_label("not-a-real-strategy"), None);
}
