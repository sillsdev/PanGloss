use super::*;

const ALL: [EmissionStrategy; 1] = [EmissionStrategy::TunedSurfaceProbed];

#[test]
fn strategy_serde_uses_the_owned_label_and_rejects_removed_routes() {
    assert_eq!(
        serde_json::to_string(&EmissionStrategy::TunedSurfaceProbed).unwrap(),
        r#""tuned-surface-probed""#
    );
    for strategy in ALL {
        let wire = serde_json::to_string(&strategy).unwrap();
        assert_eq!(
            serde_json::from_str::<EmissionStrategy>(&wire).unwrap(),
            strategy
        );
    }
    for label in ["unknown", "plan-composed", "templated-underlying-tokens"] {
        assert!(serde_json::from_value::<EmissionStrategy>(serde_json::json!(label)).is_err());
    }
}

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

/// The labels of the two removed backends no longer resolve, so a stale route string or wire frame fails loudly instead of selecting the one remaining backend.
#[test]
fn from_label_rejects_removed_backend_labels() {
    assert_eq!(EmissionStrategy::from_label("plan-composed"), None);
    assert_eq!(
        EmissionStrategy::from_label("templated-underlying-tokens"),
        None
    );
}
