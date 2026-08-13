use std::collections::{BTreeSet, HashSet};

use pg_evidence::{
    ConstructIdKind, ConstructKey, EvidenceContext, IdentityError, ObservationKey,
    EVIDENCE_CONTEXT_VERSION,
};

fn rule(id: &str) -> ConstructKey {
    ConstructKey::new(
        "fieldworks",
        "morphological_rule",
        id,
        ConstructIdKind::FieldworksGuid,
    )
    .expect("valid source rule")
}

#[test]
fn typed_keys_have_stable_wire_identity_ordering_and_hashing() {
    let construct = rule("11111111-1111-1111-1111-111111111111");
    let observation = ObservationKey::new(
        construct.clone(),
        "apply",
        "physical_rule_executions",
        1,
        "corpus_sum",
    )
    .expect("valid observation key");

    assert_eq!(
        serde_json::to_value(&construct).unwrap(),
        serde_json::json!({
            "namespace": "fieldworks",
            "kind": "morphological_rule",
            "id": "11111111-1111-1111-1111-111111111111",
            "id_kind": "fieldworks_guid"
        })
    );
    assert_eq!(construct.namespace(), "fieldworks");
    assert_eq!(construct.kind(), "morphological_rule");
    assert_eq!(construct.id(), "11111111-1111-1111-1111-111111111111");
    assert_eq!(construct.id_kind(), ConstructIdKind::FieldworksGuid);
    assert_eq!(observation.construct(), &construct);
    assert_eq!(observation.operation(), "apply");
    assert_eq!(observation.metric(), "physical_rule_executions");
    assert_eq!(observation.definition_version(), 1);
    assert_eq!(observation.aggregation(), "corpus_sum");
    assert_eq!(
        serde_json::to_value(&observation).unwrap(),
        serde_json::json!({
            "construct": {
                "namespace": "fieldworks",
                "kind": "morphological_rule",
                "id": "11111111-1111-1111-1111-111111111111",
                "id_kind": "fieldworks_guid"
            },
            "operation": "apply",
            "metric": "physical_rule_executions",
            "definition_version": 1,
            "aggregation": "corpus_sum"
        })
    );
    assert_eq!(
        serde_json::from_value::<ObservationKey>(serde_json::to_value(&observation).unwrap())
            .unwrap(),
        observation
    );

    let mut ordered = BTreeSet::new();
    ordered.insert(observation.clone());
    ordered.insert(
        ObservationKey::new(
            rule("22222222-2222-2222-2222-222222222222"),
            "apply",
            "physical_rule_executions",
            1,
            "corpus_sum",
        )
        .unwrap(),
    );
    assert_eq!(ordered.len(), 2);

    let mut hashed = HashSet::new();
    assert!(hashed.insert(observation.clone()));
    assert!(!hashed.insert(observation));
}

#[test]
fn compiler_assigned_constructs_are_not_misrepresented_as_authored_source_ids() {
    let key = ConstructKey::new(
        "pangloss",
        "stratum",
        "7",
        ConstructIdKind::CompilerAssigned,
    )
    .unwrap();
    assert_eq!(key.id_kind(), ConstructIdKind::CompilerAssigned);
    assert_eq!(
        serde_json::to_value(&key).unwrap()["id_kind"],
        serde_json::json!("compiler_assigned")
    );
}

#[test]
fn empty_identity_components_are_rejected_instead_of_minting_ambiguous_keys() {
    for (namespace, kind, id) in [
        ("", "rule", "id"),
        ("fieldworks", "", "id"),
        ("fieldworks", "rule", ""),
        ("   ", "rule", "id"),
    ] {
        assert_eq!(
            ConstructKey::new(namespace, kind, id, ConstructIdKind::AuthoredId),
            Err(IdentityError::EmptyComponent)
        );
    }

    assert_eq!(
        ObservationKey::new(rule("rule-guid"), "", "work", 1, "sum"),
        Err(IdentityError::EmptyComponent)
    );
    assert_eq!(
        ObservationKey::new(rule("rule-guid"), "apply", "work", 0, "sum"),
        Err(IdentityError::InvalidDefinitionVersion)
    );
}

#[test]
fn evidence_context_round_trips_without_requiring_optional_configuration() {
    let context = EvidenceContext {
        context_version: EVIDENCE_CONTEXT_VERSION,
        lang_project_guid: Some("aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa".into()),
        source_sha256: Some("sha256:source".into()),
        model_fingerprint: Some("sha256:model".into()),
        importer_version: Some("pg-fwdata/1".into()),
        compiler_version: Some("pangloss/0.1.0".into()),
        pipeline: Some("foma-confirm".into()),
        backend: Some("auto".into()),
        profile: None,
        config_digest: None,
    };

    let json = serde_json::to_string(&context).unwrap();
    assert_eq!(
        serde_json::from_str::<EvidenceContext>(&json).unwrap(),
        context
    );

    let no_config = EvidenceContext::default();
    assert_eq!(no_config.context_version, EVIDENCE_CONTEXT_VERSION);
    assert_eq!(no_config.profile, None);
    assert_eq!(no_config.config_digest, None);
}

#[test]
fn sparse_evidence_context_json_means_unknown_not_required_configuration() {
    let sparse: EvidenceContext = serde_json::from_str("{}").unwrap();
    assert_eq!(sparse, EvidenceContext::default());

    let version_only: EvidenceContext = serde_json::from_str(r#"{"context_version":1}"#).unwrap();
    assert_eq!(version_only, EvidenceContext::default());
}

#[test]
fn serde_rejects_invalid_identity_components_and_unknown_fields() {
    assert!(serde_json::from_value::<ConstructKey>(serde_json::json!({
        "namespace": " ",
        "kind": "rule",
        "id": "id",
        "id_kind": "authored_id"
    }))
    .is_err());

    let construct = serde_json::to_value(rule("rule-guid")).unwrap();
    assert!(serde_json::from_value::<ObservationKey>(serde_json::json!({
        "construct": construct,
        "operation": " ",
        "metric": "work",
        "definition_version": 1,
        "aggregation": "sum"
    }))
    .is_err());

    let construct = serde_json::to_value(rule("rule-guid")).unwrap();
    assert!(serde_json::from_value::<ObservationKey>(serde_json::json!({
        "construct": construct,
        "operation": "apply",
        "metric": "work",
        "definition_version": 0,
        "aggregation": "sum"
    }))
    .is_err());

    assert!(serde_json::from_value::<ConstructKey>(serde_json::json!({
        "namespace": "fieldworks",
        "kind": "rule",
        "id": "id",
        "id_kind": "authored_id",
        "unknown": true
    }))
    .is_err());

    let observation = serde_json::to_value(
        ObservationKey::new(rule("rule-guid"), "apply", "work", 1, "sum").unwrap(),
    )
    .unwrap();
    let mut observation = observation.as_object().unwrap().clone();
    observation.insert("unknown".into(), serde_json::json!(true));
    assert!(serde_json::from_value::<ObservationKey>(observation.into()).is_err());

    assert!(
        serde_json::from_value::<EvidenceContext>(serde_json::json!({
            "unknown": true
        }))
        .is_err()
    );
}
