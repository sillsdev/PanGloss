use pg_snapshot::feature::{FeatureStructure, FeatureSystem, FeatureValueKind};
use pg_snapshot::Snapshot;
use rusqlite::{params, Transaction};
use std::collections::BTreeMap;

use crate::morphology::checked_guid;
use crate::FactsError;

pub(crate) type FeatureStructureIds = BTreeMap<(String, String, String), i64>;

pub(crate) fn insert_authored(
    tx: &Transaction<'_>,
    snapshot: &Snapshot,
) -> Result<FeatureStructureIds, FactsError> {
    insert_feature_system(tx, "phonological", &snapshot.feature_systems.phonological)?;
    insert_feature_system(
        tx,
        "morphosyntactic",
        &snapshot.feature_systems.morphosyntactic,
    )?;
    let mut next_id = 1_i64;
    let mut ids = FeatureStructureIds::new();
    for phoneme in &snapshot.phonology.phonemes {
        if let Some(fs) = &phoneme.features {
            let id = insert_structure(
                tx,
                StructureOwner {
                    system: "phonological",
                    kind: "phoneme",
                    guid: &phoneme.guid,
                    role: "features",
                },
                "root",
                fs,
                &mut next_id,
            )?;
            ids.insert(
                ("phoneme".into(), phoneme.guid.clone(), "features".into()),
                id,
            );
        }
    }
    for class in &snapshot.phonology.natural_classes {
        if let pg_snapshot::phonology::NaturalClass::Features { guid, features, .. } = class {
            let id = insert_structure(
                tx,
                StructureOwner {
                    system: "phonological",
                    kind: "naturalClass",
                    guid,
                    role: "features",
                },
                "root",
                features,
                &mut next_id,
            )?;
            ids.insert(("naturalClass".into(), guid.clone(), "features".into()), id);
        }
    }
    Ok(ids)
}

fn insert_feature_system(
    tx: &Transaction<'_>,
    system_name: &str,
    system: &FeatureSystem,
) -> Result<(), FactsError> {
    for feature in &system.closed_features {
        let guid = checked_guid(&feature.guid, "feature.guid")?;
        tx.execute(
            "INSERT INTO feature(guid, system, kind, name, abbreviation, feature_type_guid) VALUES (?1, ?2, 'closed', ?3, ?4, NULL)",
            params![guid, system_name, feature.name, feature.abbreviation],
        )?;
        for (ordinal, value) in feature.values.iter().enumerate() {
            let value_guid = checked_guid(&value.guid, "featureValue.guid")?;
            tx.execute(
                "INSERT INTO feature_value(guid, feature_guid, ordinal, name, abbreviation) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![value_guid, guid, ordinal as i64, value.name, value.abbreviation],
            )?;
        }
    }
    for feature in &system.complex_features {
        let guid = checked_guid(&feature.guid, "feature.guid")?;
        let feature_type_guid = feature
            .feature_type
            .as_deref()
            .map(|value| checked_guid(value, "feature.featureType"))
            .transpose()?;
        tx.execute(
            "INSERT INTO feature(guid, system, kind, name, abbreviation, feature_type_guid) VALUES (?1, ?2, 'complex', ?3, ?4, ?5)",
            params![guid, system_name, feature.name, feature.abbreviation, feature_type_guid],
        )?;
    }
    Ok(())
}

struct StructureOwner<'a> {
    system: &'a str,
    kind: &'a str,
    guid: &'a str,
    role: &'a str,
}

fn insert_structure(
    tx: &Transaction<'_>,
    owner: StructureOwner<'_>,
    path: &str,
    structure: &FeatureStructure,
    next_id: &mut i64,
) -> Result<i64, FactsError> {
    let id = *next_id;
    *next_id += 1;
    tx.execute(
        "INSERT INTO feature_structure(fs_id, system, owner_kind, owner_guid, role, path) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![id, owner.system, owner.kind, owner.guid, owner.role, path],
    )?;
    for (ordinal, value) in structure.values.iter().enumerate() {
        let feature_guid = &value.feature;
        match &value.value {
            FeatureValueKind::Closed { value } => {
                tx.execute(
                    "INSERT INTO feature_assignment(fs_id, ordinal, feature_guid, value_kind, value_guid, child_fs_id) VALUES (?1, ?2, ?3, 'closed', ?4, NULL)",
                    params![id, ordinal as i64, feature_guid, value],
                )?;
            }
            FeatureValueKind::Complex { value } => {
                let child_path = format!("{path}/{ordinal}");
                let child_id = insert_structure(
                    tx,
                    StructureOwner {
                        system: owner.system,
                        kind: owner.kind,
                        guid: owner.guid,
                        role: owner.role,
                    },
                    &child_path,
                    value,
                    next_id,
                )?;
                tx.execute(
                    "INSERT INTO feature_assignment(fs_id, ordinal, feature_guid, value_kind, value_guid, child_fs_id) VALUES (?1, ?2, ?3, 'complex', NULL, ?4)",
                    params![id, ordinal as i64, feature_guid, child_id],
                )?;
            }
        }
    }
    Ok(id)
}
