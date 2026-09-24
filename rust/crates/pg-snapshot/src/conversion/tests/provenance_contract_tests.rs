use super::super::*;

#[test]
fn object_identity_carries_the_given_guid() {
    let key = InventoryKey::object(InventoryKind::Entry, "guid-1");
    assert_eq!(
        key,
        InventoryKey {
            kind: InventoryKind::Entry,
            identity: InventoryIdentity::Object {
                guid: "guid-1".to_string()
            },
        }
    );
}

#[test]
fn attachment_identity_carries_owner_target_and_role() {
    let key = InventoryKey::attachment(InventoryKind::Environment, "owner", "target", "role");
    assert_eq!(
        key,
        InventoryKey {
            kind: InventoryKind::Environment,
            identity: InventoryIdentity::Attachment {
                owner_guid: "owner".to_string(),
                target_guid: "target".to_string(),
                role: "role".to_string(),
            },
        }
    );
}

#[test]
fn expansion_identity_carries_owner_members_and_role() {
    let key = InventoryKey::expansion(
        InventoryKind::Template,
        "owner",
        vec!["m1".to_string(), "m2".to_string()],
        "slots",
    );
    assert_eq!(
        key,
        InventoryKey {
            kind: InventoryKind::Template,
            identity: InventoryIdentity::Expansion {
                owner_guid: "owner".to_string(),
                member_guids: vec!["m1".to_string(), "m2".to_string()],
                role: "slots".to_string(),
            },
        }
    );
}

#[test]
fn setting_identity_carries_only_a_name() {
    let key = InventoryKey::setting(InventoryKind::ParserSetting, "activeParser");
    assert_eq!(
        key,
        InventoryKey {
            kind: InventoryKind::ParserSetting,
            identity: InventoryIdentity::Setting {
                name: "activeParser".to_string()
            },
        }
    );
}

#[test]
fn inventory_key_btreeset_round_trips_through_json_in_sorted_order() {
    let mut set = BTreeSet::new();
    set.insert(InventoryKey::object(InventoryKind::Sense, "zzz"));
    set.insert(InventoryKey::object(InventoryKind::Entry, "aaa"));
    set.insert(InventoryKey::setting(InventoryKind::ParserSetting, "x"));

    let sorted_before: Vec<_> = set.iter().cloned().collect();
    let json = serde_json::to_string(&set).expect("BTreeSet<InventoryKey> must serialize");
    let round_tripped: BTreeSet<InventoryKey> =
        serde_json::from_str(&json).expect("must deserialize back");
    let sorted_after: Vec<_> = round_tripped.iter().cloned().collect();

    assert_eq!(sorted_before, sorted_after);
    assert_eq!(set, round_tripped);
}

#[test]
fn synthetic_provenance_is_current_schema_version_and_synthetic_status() {
    let provenance = ConversionProvenance::synthetic();
    assert_eq!(
        provenance.schema_version,
        CONVERSION_PROVENANCE_SCHEMA_VERSION
    );
    assert_eq!(
        provenance.source_inventory_status,
        SourceInventoryStatus::Synthetic
    );
}

#[test]
fn validate_accepts_version_zero_with_unknown_status() {
    let provenance = ConversionProvenance {
        schema_version: 0,
        source_inventory_status: SourceInventoryStatus::Unknown,
        ..ConversionProvenance::default()
    };
    assert!(provenance.validate().is_ok());
}

#[test]
fn validate_accepts_version_one_with_a_resolved_status() {
    let provenance = ConversionProvenance {
        schema_version: 1,
        source_inventory_status: SourceInventoryStatus::ImportedComplete,
        ..ConversionProvenance::default()
    };
    assert!(provenance.validate().is_ok());
}

#[test]
fn validate_rejects_version_zero_with_a_resolved_status() {
    let provenance = ConversionProvenance {
        schema_version: 0,
        source_inventory_status: SourceInventoryStatus::ImportedComplete,
        ..ConversionProvenance::default()
    };
    assert!(matches!(
        provenance.validate(),
        Err(ProvenanceError::VersionZeroRequiresUnknownStatus { .. })
    ));
}

#[test]
fn validate_rejects_version_one_with_unknown_status() {
    let provenance = ConversionProvenance {
        schema_version: 1,
        source_inventory_status: SourceInventoryStatus::Unknown,
        ..ConversionProvenance::default()
    };
    assert!(matches!(
        provenance.validate(),
        Err(ProvenanceError::VersionOneForbidsUnknownStatus)
    ));
}

#[test]
fn validate_rejects_an_unrecognized_schema_version_regardless_of_status() {
    let provenance = ConversionProvenance {
        schema_version: 2,
        source_inventory_status: SourceInventoryStatus::ImportedComplete,
        ..ConversionProvenance::default()
    };
    assert!(matches!(
        provenance.validate(),
        Err(ProvenanceError::UnsupportedSchemaVersion { found: 2 })
    ));
}

#[test]
fn invariants_hold_for_a_well_formed_sequence() {
    let mut r = SelectionRecorder::default();
    let key = InventoryKey::object(InventoryKind::Entry, "g1");
    r.authored(key.clone());
    r.considered(key.clone());
    r.selected(key.clone());
    r.represented(key);
    assert!(r.check_invariants().is_ok());
}

#[test]
fn invariants_reject_selected_without_considered() {
    let mut r = SelectionRecorder::default();
    let key = InventoryKey::object(InventoryKind::Entry, "g1");
    r.authored(key.clone());
    r.selected(key);
    assert!(r.check_invariants().is_err());
}

#[test]
fn invariants_reject_considered_without_authored_or_synthesized() {
    let mut r = SelectionRecorder::default();
    let key = InventoryKey::object(InventoryKind::Entry, "g1");
    r.considered(key);
    assert!(r.check_invariants().is_err());
}

#[test]
fn invariants_reject_represented_and_rejected_together() {
    let mut r = SelectionRecorder::default();
    let key = InventoryKey::object(InventoryKind::Entry, "g1");
    r.authored(key.clone());
    r.considered(key.clone());
    r.selected(key.clone());
    r.represented(key.clone());
    r.rejected(
        key,
        ConversionIssue {
            code: crate::ImportWarningCode::Unregistered("test.code".to_string()),
            class: IssueClass::UnrepresentableForHc,
            source: None,
            fatal: false,
            message: "test".to_string(),
        },
    );
    assert!(r.check_invariants().is_err());
}

#[test]
fn revoke_represented_moves_the_key_to_rejected_and_off_represented() {
    let mut r = SelectionRecorder::default();
    let key = InventoryKey::object(InventoryKind::Msa, "g1");
    r.authored(key.clone());
    r.considered(key.clone());
    r.selected(key.clone());
    r.represented(key.clone());
    r.revoke_represented(
        key.clone(),
        ConversionIssue {
            code: crate::ImportWarningCode::Unregistered("test.revoked".to_string()),
            class: IssueClass::UnreachableInGrammar,
            source: None,
            fatal: false,
            message: "test".to_string(),
        },
    );
    assert!(!r.inventory.represented.contains(&key));
    assert!(r.inventory.rejected.contains(&key));
    assert!(r.check_invariants().is_ok());
}

#[test]
#[should_panic(expected = "key was not represented")]
fn revoke_represented_panics_on_a_key_that_was_never_represented() {
    let mut r = SelectionRecorder::default();
    let key = InventoryKey::object(InventoryKind::Msa, "g1");
    r.authored(key.clone());
    r.considered(key.clone());
    r.selected(key.clone());
    r.revoke_represented(
        key,
        ConversionIssue {
            code: crate::ImportWarningCode::Unregistered("test.revoked".to_string()),
            class: IssueClass::UnreachableInGrammar,
            source: None,
            fatal: false,
            message: "test".to_string(),
        },
    );
}

#[test]
fn record_text_use_is_independent_of_the_stage_invariants() {
    let mut r = SelectionRecorder::default();
    r.record_text_use(
        SourceRef {
            kind: crate::FwClass::MoForm,
            id: "allo-1".to_string(),
        },
        "quma",
    );
    assert_eq!(
        r.text_uses(),
        &[(
            SourceRef {
                kind: crate::FwClass::MoForm,
                id: "allo-1".to_string(),
            },
            "quma".to_string(),
        )]
    );
    // Recording usage touches no stage set, so an otherwise-empty recorder still satisfies its own invariants.
    assert!(r.check_invariants().is_ok());
}

#[test]
fn noted_issue_does_not_mark_a_retained_source_object_rejected() {
    let mut r = SelectionRecorder::default();
    r.noted(ConversionIssue {
        code: crate::ImportWarningCode::Unregistered("test.retained-object-note".to_string()),
        class: IssueClass::MigrationDifference,
        source: Some(SourceRef {
            kind: crate::FwClass::PhPhoneme,
            id: "phoneme-1".to_string(),
        }),
        fatal: false,
        message: "a retained phoneme has an ignored feature value".to_string(),
    });

    assert!(r.check_invariants().is_ok());
    let (inventory, issues) = r.finish();
    assert!(inventory.rejected.is_empty());
    assert_eq!(issues.len(), 1);
}

#[test]
fn synthesized_satisfies_the_authored_or_synthesized_requirement() {
    let mut r = SelectionRecorder::default();
    let key = InventoryKey::object(InventoryKind::Msa, "synth-1");
    r.synthesized(key.clone());
    r.considered(key.clone());
    r.selected(key.clone());
    r.represented(key);
    assert!(r.check_invariants().is_ok());
}

#[test]
fn from_stage_derives_exactly_the_three_loss_categories() {
    let clean = InventoryKey::object(InventoryKind::Entry, "clean");
    let omit_a = InventoryKey::object(InventoryKind::Msa, "omit-a");
    let omit_b = InventoryKey::object(InventoryKind::Msa, "omit-b");
    let unclassified_a = InventoryKey::object(InventoryKind::Allomorph, "unclassified-a");
    let unclassified_b = InventoryKey::object(InventoryKind::Allomorph, "unclassified-b");
    let synth_only_a = InventoryKey::object(InventoryKind::NaturalClass, "synth-only-a");
    let synth_only_b = InventoryKey::object(InventoryKind::NaturalClass, "synth-only-b");

    let inventory = ConversionInventory {
        authored: [clean.clone()].into_iter().collect(),
        considered: [clean.clone(), omit_a.clone(), omit_b.clone()]
            .into_iter()
            .collect(),
        selected: [clean.clone(), omit_a.clone(), omit_b.clone()]
            .into_iter()
            .collect(),
        represented: [
            clean.clone(),
            unclassified_a.clone(),
            unclassified_b.clone(),
        ]
        .into_iter()
        .collect(),
        rejected: BTreeSet::new(),
        synthesized: [synth_only_a.clone(), synth_only_b.clone()]
            .into_iter()
            .collect(),
    };

    let delta = InventoryDelta::from_stage(inventory.clone(), Vec::new());

    assert_eq!(
        delta.silently_omitted,
        [omit_a.clone(), omit_b.clone()].into_iter().collect()
    );
    assert_eq!(
        delta.unclassified,
        [unclassified_a.clone(), unclassified_b.clone()]
            .into_iter()
            .collect()
    );
    assert_eq!(
        delta.synthesized_only,
        [synth_only_a.clone(), synth_only_b.clone()]
            .into_iter()
            .collect()
    );
    assert!(!delta.silently_omitted.contains(&clean));
    assert!(!delta.unclassified.contains(&clean));
    assert!(!delta.synthesized_only.contains(&clean));
    assert_eq!(delta.inventory, inventory);
    assert!(delta.issues.is_empty());
}
