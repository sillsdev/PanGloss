//! Authored allomorph identity, order, and writing-system forms.

use std::collections::BTreeMap;

use pg_grammar::compile::{AllomorphGateKind, AllomorphGateOutcome};
use pg_snapshot::lexicon::Allomorph;
use pg_snapshot::morphology::MorphType;
use pg_snapshot::Snapshot;
use rusqlite::{params, Transaction};

use crate::features::FeatureStructureIds;
use crate::morphology::checked_guid;
use crate::FactsError;

pub(crate) fn insert_allomorphs(
    tx: &Transaction<'_>,
    entry_guid: &str,
    allomorphs: &[Allomorph],
) -> Result<(), FactsError> {
    for (ordinal, allomorph) in allomorphs.iter().enumerate() {
        let allomorph_guid = checked_guid(&allomorph.guid, "allomorph.guid")?;
        let morph_type = enum_string(&allomorph.morph_type)?;
        let form_class = form_class(allomorph);
        let stem_name_guid = allomorph
            .stem_name
            .as_deref()
            .map(|guid| checked_guid(guid, "allomorph.stemName"))
            .transpose()?;
        tx.execute(
            "INSERT INTO allomorph(guid, entry_guid, ordinal, morph_type, form_class, is_abstract, stem_name_guid) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                allomorph_guid,
                entry_guid,
                ordinal as i64,
                morph_type,
                form_class,
                if allomorph.is_abstract { 1_i64 } else { 0_i64 },
                stem_name_guid
            ],
        )?;
        for (form_ordinal, form) in allomorph.forms.iter().enumerate() {
            tx.execute(
                "INSERT INTO allomorph_form(allomorph_guid, ordinal, writing_system, form) VALUES (?1, ?2, ?3, ?4)",
                params![allomorph_guid, form_ordinal as i64, form.ws, form.form],
            )?;
        }
    }
    Ok(())
}

/// Derived from the morph type, since the Snapshot omits the `MoForm` subclass.
fn form_class(allomorph: &Allomorph) -> &'static str {
    if allomorph.process.is_some() {
        return "process";
    }
    match allomorph.morph_type {
        MorphType::Stem
        | MorphType::BoundStem
        | MorphType::Root
        | MorphType::BoundRoot
        | MorphType::Clitic
        | MorphType::Particle
        | MorphType::Phrase
        | MorphType::DiscontigPhrase => "stem",
        MorphType::Prefix
        | MorphType::Suffix
        | MorphType::Infix
        | MorphType::Circumfix
        | MorphType::Proclitic
        | MorphType::Enclitic
        | MorphType::PrefixingInterfix
        | MorphType::InfixingInterfix
        | MorphType::SuffixingInterfix => "affix",
    }
}

/// Publishes each authored allomorph gate; a gate the compiler recorded nothing for is `owner_not_loaded`.
pub(crate) fn insert_gates(
    tx: &Transaction<'_>,
    snapshot: &Snapshot,
    outcomes: &[AllomorphGateOutcome],
    feature_structures: &FeatureStructureIds,
) -> Result<(), FactsError> {
    let outcome_by_gate: BTreeMap<(&str, AllomorphGateKind, u32), &AllomorphGateOutcome> = outcomes
        .iter()
        .map(|outcome| {
            (
                (
                    outcome.allomorph_guid.as_str(),
                    outcome.gate_kind,
                    outcome.ordinal,
                ),
                outcome,
            )
        })
        .collect();
    let effect_of = |allomorph: &str, kind: AllomorphGateKind, ordinal: u32| match outcome_by_gate
        .get(&(allomorph, kind, ordinal))
    {
        Some(outcome) => (
            outcome.effect.as_str(),
            outcome.reason_code.map(str::to_owned),
        ),
        None => ("owner_not_loaded", None),
    };
    for entry in &snapshot.lexicon.entries {
        for allomorph in &entry.allomorphs {
            let allomorph_guid = checked_guid(&allomorph.guid, "allomorph.guid")?;
            for (index, class) in allomorph.inflection_classes.iter().enumerate() {
                let ordinal = index as u32;
                let class_guid = checked_guid(class, "allomorph.inflectionClasses")?;
                let outcome =
                    effect_of(&allomorph.guid, AllomorphGateKind::InflectionClass, ordinal);
                insert_gate(
                    tx,
                    &allomorph_guid,
                    GateRow {
                        gate_kind: "inflection_class",
                        ordinal,
                        target_guid: Some(&class_guid),
                        fs_id: None,
                        outcome,
                    },
                )?;
            }
            if allomorph.ms_env_features.is_some() {
                let fs_id = feature_structures
                    .get(&(
                        "allomorph".into(),
                        allomorph_guid.clone(),
                        "required_features".into(),
                    ))
                    .copied()
                    .ok_or_else(|| {
                        FactsError::Serialization(
                            "required features have no feature structure".into(),
                        )
                    })?;
                // FieldWorks owns MsEnvFeatures only on affix allomorphs; the compiler never reads it elsewhere.
                let outcome = if form_class(allomorph) == "affix" {
                    effect_of(&allomorph.guid, AllomorphGateKind::RequiredFeatures, 0)
                } else {
                    ("not_attempted", Some("notAnAffixAllomorph".into()))
                };
                insert_gate(
                    tx,
                    &allomorph_guid,
                    GateRow {
                        gate_kind: "required_features",
                        ordinal: 0,
                        target_guid: None,
                        fs_id: Some(fs_id),
                        outcome,
                    },
                )?;
            }
            if let Some(category) = &allomorph.ms_env_part_of_speech {
                let category_guid = checked_guid(category, "allomorph.msEnvPartOfSpeech")?;
                // The compiler has no path that reads this gate, so it is decided here.
                insert_gate(
                    tx,
                    &allomorph_guid,
                    GateRow {
                        gate_kind: "required_category",
                        ordinal: 0,
                        target_guid: Some(&category_guid),
                        fs_id: None,
                        outcome: ("ignored", Some("msEnvPartOfSpeechNotRead".into())),
                    },
                )?;
            }
            if let Some(stem_name) = &allomorph.stem_name {
                let stem_name_guid = checked_guid(stem_name, "allomorph.stemName")?;
                let outcome = effect_of(&allomorph.guid, AllomorphGateKind::StemName, 0);
                insert_gate(
                    tx,
                    &allomorph_guid,
                    GateRow {
                        gate_kind: "stem_name",
                        ordinal: 0,
                        target_guid: Some(&stem_name_guid),
                        fs_id: None,
                        outcome,
                    },
                )?;
            }
        }
    }
    Ok(())
}

/// One `allomorph_gate` row before its allomorph guid is attached.
struct GateRow<'a> {
    gate_kind: &'a str,
    ordinal: u32,
    target_guid: Option<&'a str>,
    fs_id: Option<i64>,
    outcome: (&'a str, Option<String>),
}

fn insert_gate(
    tx: &Transaction<'_>,
    allomorph_guid: &str,
    row: GateRow<'_>,
) -> Result<(), FactsError> {
    let (parser_effect, reason_code) = row.outcome;
    tx.execute(
        "INSERT INTO allomorph_gate(allomorph_guid, gate_kind, ordinal, target_guid, fs_id, parser_effect, reason_code) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            allomorph_guid,
            row.gate_kind,
            i64::from(row.ordinal),
            row.target_guid,
            row.fs_id,
            parser_effect,
            reason_code
        ],
    )?;
    Ok(())
}

fn enum_string<T: serde::Serialize>(value: &T) -> Result<String, FactsError> {
    let value = serde_json::to_value(value)
        .map_err(|error| FactsError::Serialization(error.to_string()))?;
    value
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| FactsError::Serialization("expected a string enum value".into()))
}
