use super::*;

fn synthetic_required() -> RequiredRuntimeFeatures {
    RequiredRuntimeFeatures {
        payload_format_version: 1,
        runtime_operations: vec!["synthetic.reduplication.peel".to_string()],
        foma_feature_level: 2,
        hc_port_semver: (1, 3, 0),
        extensions: vec!["synthetic.extension.alpha".to_string()],
    }
}

fn synthetic_provided_superset() -> ProvidedRuntimeFeatures {
    ProvidedRuntimeFeatures {
        payload_format_versions: vec![1, 2],
        runtime_operations: vec![
            "synthetic.reduplication.peel".to_string(),
            "synthetic.other.op".to_string(),
        ],
        foma_feature_level: 3,
        hc_port_semver: (1, 4, 0),
        extensions: vec![
            "synthetic.extension.alpha".to_string(),
            "synthetic.extension.beta".to_string(),
        ],
    }
}

#[test]
fn required_subset_of_provided_is_satisfied() {
    assert!(synthetic_required().satisfied_by(&synthetic_provided_superset()));
}

#[test]
fn missing_runtime_operation_is_not_satisfied() {
    let required = synthetic_required();
    let provided = ProvidedRuntimeFeatures {
        runtime_operations: vec!["synthetic.unrelated.op".to_string()],
        ..synthetic_provided_superset()
    };
    assert!(!required.satisfied_by(&provided));
}

#[test]
fn missing_extension_is_not_satisfied() {
    let required = synthetic_required();
    let provided = ProvidedRuntimeFeatures {
        extensions: vec![],
        ..synthetic_provided_superset()
    };
    assert!(!required.satisfied_by(&provided));
}

#[test]
fn lower_foma_feature_level_is_not_satisfied() {
    let required = synthetic_required();
    let provided = ProvidedRuntimeFeatures {
        foma_feature_level: 1,
        ..synthetic_provided_superset()
    };
    assert!(!required.satisfied_by(&provided));
}

#[test]
fn older_hc_port_semver_is_not_satisfied() {
    let required = synthetic_required();
    let provided = ProvidedRuntimeFeatures {
        hc_port_semver: (1, 2, 9),
        ..synthetic_provided_superset()
    };
    assert!(!required.satisfied_by(&provided));
}

#[test]
fn missing_payload_format_version_is_not_satisfied() {
    let required = synthetic_required();
    let provided = ProvidedRuntimeFeatures {
        payload_format_versions: vec![2, 3],
        ..synthetic_provided_superset()
    };
    assert!(!required.satisfied_by(&provided));
}
