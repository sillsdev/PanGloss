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
