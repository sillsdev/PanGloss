use std::collections::BTreeSet;

use pg_snapshot::morphology::{AdhocProhibition, AdhocProhibitionGroup};
use pg_snapshot::{Snapshot, WsForm};
use rusqlite::{params, Transaction};

use crate::morphology::checked_guid;
use crate::FactsError;

pub(crate) fn validate_authored_guids(snapshot: &Snapshot) -> Result<(), FactsError> {
    for (index, prohibition) in snapshot.morphology.adhoc_prohibitions.iter().enumerate() {
        let (guid, primary, others) = match prohibition {
            AdhocProhibition::Allomorph {
                guid,
                primary,
                others,
                ..
            }
            | AdhocProhibition::Morpheme {
                guid,
                primary,
                others,
                ..
            } => (guid, primary, others),
        };
        checked_guid(guid, &format!("morphology.adhocProhibitions[{index}].guid"))?;
        checked_guid(
            primary,
            &format!("morphology.adhocProhibitions[{index}].primary"),
        )?;
        for (other_index, target) in others.iter().enumerate() {
            checked_guid(
                target,
                &format!("morphology.adhocProhibitions[{index}].others[{other_index}]"),
            )?;
        }
    }
    if let Some(groups) = &snapshot.morphology.adhoc_prohibition_groups {
        for (index, group) in groups.iter().enumerate() {
            checked_guid(
                &group.guid,
                &format!("morphology.adhocProhibitionGroups[{index}].guid"),
            )?;
            validate_forms(&group.name, index, "name")?;
            validate_forms(&group.description, index, "description")?;
            for (member_index, member) in group.members.iter().enumerate() {
                checked_guid(
                    member,
                    &format!("morphology.adhocProhibitionGroups[{index}].members[{member_index}]"),
                )?;
            }
        }
    }
    Ok(())
}

fn validate_forms(forms: &[WsForm], group_index: usize, field: &str) -> Result<(), FactsError> {
    let mut writing_systems = BTreeSet::new();
    for form in forms {
        if !writing_systems.insert(form.ws.as_str()) {
            return Err(FactsError::InvalidSnapshot(format!(
                "morphology.adhocProhibitionGroups[{group_index}].{field} repeats writing system {:?}",
                form.ws
            )));
        }
    }
    Ok(())
}

pub(crate) fn insert_authored(tx: &Transaction<'_>, snapshot: &Snapshot) -> Result<(), FactsError> {
    for prohibition in &snapshot.morphology.adhoc_prohibitions {
        insert_prohibition(tx, prohibition)?;
    }
    let Some(groups) = &snapshot.morphology.adhoc_prohibition_groups else {
        return Ok(());
    };
    let mut groups: Vec<&AdhocProhibitionGroup> = groups.iter().collect();
    groups.sort_by(|left, right| left.guid.cmp(&right.guid));
    for group in &groups {
        let guid = checked_guid(&group.guid, "adhocGroup.guid")?;
        tx.execute("INSERT INTO adhoc_group(group_guid) VALUES (?1)", [&guid])?;
    }
    for group in groups {
        let guid = checked_guid(&group.guid, "adhocGroup.guid")?;
        insert_texts(tx, &guid, "name", &group.name)?;
        insert_texts(tx, &guid, "description", &group.description)?;
        let members: BTreeSet<_> = group
            .members
            .iter()
            .map(|member| checked_guid(member, "adhocGroup.member"))
            .collect::<Result<_, _>>()?;
        for member in members {
            tx.execute(
                "INSERT INTO adhoc_group_member(group_guid, member_guid) VALUES (?1, ?2)",
                params![guid, member],
            )?;
        }
    }
    Ok(())
}

fn insert_prohibition(
    tx: &Transaction<'_>,
    prohibition: &AdhocProhibition,
) -> Result<(), FactsError> {
    let (guid, kind, disabled, primary, others, adjacency) = match prohibition {
        AdhocProhibition::Allomorph {
            guid,
            disabled,
            primary,
            others,
            adjacency,
        } => (guid, "allomorph", disabled, primary, others, adjacency),
        AdhocProhibition::Morpheme {
            guid,
            disabled,
            primary,
            others,
            adjacency,
        } => (guid, "morpheme", disabled, primary, others, adjacency),
    };
    let guid = checked_guid(guid, "adhocProhibition.guid")?;
    let primary = checked_guid(primary, "adhocProhibition.primary")?;
    let adjacency = serde_json::to_value(adjacency)
        .map_err(|error| FactsError::Serialization(error.to_string()))?
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| FactsError::Serialization("expected adjacency enum text".into()))?;
    let target_kind = if kind == "allomorph" {
        "allomorph"
    } else {
        "msa"
    };
    tx.execute(
        "INSERT INTO adhoc_prohibition(prohibition_guid, kind, disabled, adjacency, primary_guid, target_kind) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            guid,
            kind,
            if *disabled { 1_i64 } else { 0_i64 },
            adjacency,
            primary,
            target_kind
        ],
    )?;
    for (ordinal, target) in others.iter().enumerate() {
        let target = checked_guid(target, "adhocProhibition.others")?;
        tx.execute(
            "INSERT INTO adhoc_other(prohibition_guid, ordinal, target_guid, target_kind) VALUES (?1, ?2, ?3, ?4)",
            params![guid, ordinal as i64, target, target_kind],
        )?;
    }
    Ok(())
}

fn insert_texts(
    tx: &Transaction<'_>,
    group_guid: &str,
    field: &str,
    forms: &[WsForm],
) -> Result<(), FactsError> {
    let mut forms: Vec<&WsForm> = forms.iter().collect();
    forms.sort_by(|left, right| left.ws.cmp(&right.ws));
    for form in forms {
        tx.execute(
            "INSERT INTO adhoc_group_text(group_guid, field, writing_system, text) VALUES (?1, ?2, ?3, ?4)",
            params![group_guid, field, form.ws, form.form],
        )?;
    }
    Ok(())
}
