//! Authored allomorph identity, order, and writing-system forms.

use std::collections::{BTreeMap, BTreeSet};

use pg_grammar::compile::{AllomorphGateKind, AllomorphGateOutcome};
use pg_snapshot::lexicon::{AffixProcess, Allomorph, EntryRef, RuleMapping};
use pg_snapshot::morphology::MorphType;
use pg_snapshot::phonology::PhonContext;
use pg_snapshot::Snapshot;
use rusqlite::{params, Transaction};

use crate::features::FeatureStructureIds;
use crate::morphology::checked_guid;
use crate::phonology::{insert_context, insert_pattern_root};
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
        if let Some(process) = &allomorph.process {
            insert_process(tx, &allomorph_guid, process)?;
        }
    }
    Ok(())
}

/// Parts are 1-based as `CopyFromInput` refers to them; a concrete part gets a pattern root.
fn insert_process(
    tx: &Transaction<'_>,
    allomorph_guid: &str,
    process: &AffixProcess,
) -> Result<(), FactsError> {
    for (index, context) in process.input.iter().enumerate() {
        let part = index as i64 + 1;
        let is_variable = matches!(context, PhonContext::Variable);
        let pattern_root_id = if is_variable {
            None
        } else {
            let root = insert_pattern_root(
                tx,
                "affixProcess",
                allomorph_guid,
                "process_input",
                index,
                "authored",
            )?;
            insert_context(tx, root, None, 0, context)?;
            Some(root)
        };
        tx.execute(
            "INSERT INTO affix_process_input(allomorph_guid, part, is_variable, pattern_root_id) VALUES (?1, ?2, ?3, ?4)",
            params![allomorph_guid, part, i64::from(is_variable), pattern_root_id],
        )?;
    }
    for (ordinal, step) in process.output.iter().enumerate() {
        let (kind, part, natural_class, text): (&str, Option<i64>, Option<&str>, Option<&str>) =
            match step {
                RuleMapping::InsertNaturalClass { natural_class } => {
                    ("insert_class", None, Some(natural_class), None)
                }
                RuleMapping::CopyFromInput { part } => ("copy", Some(i64::from(*part)), None, None),
                RuleMapping::InsertSegments { text } => ("insert_segments", None, None, Some(text)),
                RuleMapping::ModifyFromInput {
                    part,
                    natural_class,
                } => ("modify", Some(i64::from(*part)), Some(natural_class), None),
            };
        let natural_class = natural_class
            .map(|guid| checked_guid(guid, "affixProcess.output.naturalClass"))
            .transpose()?;
        tx.execute(
            "INSERT INTO affix_process_output(allomorph_guid, ordinal, kind, part, natural_class_guid, text) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![allomorph_guid, ordinal as i64, kind, part, natural_class, text],
        )?;
    }
    Ok(())
}

/// Derived from the morph type, since the Snapshot omits the `MoForm` subclass.
pub(crate) fn form_class(allomorph: &Allomorph) -> &'static str {
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

/// Whether the compiler reads an allomorph's `positions`; affix paths chain them after `environments`.
/// Pinned by `root_position_sharing_an_invalid_environment_exports_as_not_attempted`.
pub(crate) fn reads_positions(allomorph: &Allomorph) -> bool {
    form_class(allomorph) == "affix"
}

/// Publishes each authored allomorph gate; a gate the compiler recorded nothing for is `owner_not_loaded`.
pub(crate) fn insert_gates(
    tx: &Transaction<'_>,
    snapshot: &Snapshot,
    outcomes: &[AllomorphGateOutcome],
    feature_structures: &FeatureStructureIds,
) -> Result<(), FactsError> {
    let gates = GateEffects::new(outcomes);
    let effect_of = |allomorph: &str, kind: AllomorphGateKind, ordinal: u32| {
        gates.effect(allomorph, kind, ordinal)
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
                        outcome: (
                            MS_ENV_PART_OF_SPEECH_EFFECT,
                            Some("msEnvPartOfSpeechNotRead".into()),
                        ),
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

/// Publishes variant links and their types; complex-form links are not walked by the compiler.
pub(crate) fn insert_entry_variants(
    tx: &Transaction<'_>,
    snapshot: &Snapshot,
) -> Result<(), FactsError> {
    let mut entry_guids = BTreeSet::new();
    let mut sense_guids = BTreeSet::new();
    for entry in &snapshot.lexicon.entries {
        entry_guids.insert(checked_guid(&entry.guid, "lexicon.entries.guid")?);
        for sense in &entry.senses {
            sense_guids.insert(checked_guid(&sense.guid, "sense.guid")?);
        }
    }
    let infl_type_guids = snapshot
        .morphology
        .lex_entry_infl_types
        .iter()
        .map(|infl_type| checked_guid(&infl_type.guid, "lexEntryInflType.guid"))
        .collect::<Result<BTreeSet<_>, _>>()?;
    for entry in &snapshot.lexicon.entries {
        let variant_entry_guid = checked_guid(&entry.guid, "lexicon.entries.guid")?;
        for entry_ref in &entry.entry_refs {
            let EntryRef::Variant {
                guid,
                component_lexemes,
                variant_entry_types,
            } = entry_ref
            else {
                continue;
            };
            let ref_guid = checked_guid(guid, "entryRef.guid")?;
            for (ordinal, component) in component_lexemes.iter().enumerate() {
                let component_guid = checked_guid(component, "entryRef.componentLexeme")?;
                let component_kind = if entry_guids.contains(&component_guid) {
                    "entry"
                } else if sense_guids.contains(&component_guid) {
                    "sense"
                } else {
                    "unresolved"
                };
                tx.execute(
                    "INSERT INTO entry_variant(variant_entry_guid, ref_guid, ordinal, component_guid, component_kind) VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![
                        variant_entry_guid,
                        ref_guid,
                        ordinal as i64,
                        component_guid,
                        component_kind
                    ],
                )?;
            }
            for (ordinal, type_guid) in variant_entry_types.iter().enumerate() {
                let type_guid = checked_guid(type_guid, "entryRef.variantEntryType")?;
                let is_infl_type = i64::from(infl_type_guids.contains(&type_guid));
                tx.execute(
                    "INSERT INTO entry_variant_type(ref_guid, ordinal, type_guid, is_infl_type) VALUES (?1, ?2, ?3, ?4)",
                    params![ref_guid, ordinal as i64, type_guid, is_infl_type],
                )?;
            }
        }
    }
    Ok(())
}

/// The `parser_effect` of an `MsEnvPartOfSpeech` gate: no compiler path reads it.
pub(crate) const MS_ENV_PART_OF_SPEECH_EFFECT: &str = "ignored";

/// The compiler's recorded gate outcomes, looked up the way the gate table and the reference index need them.
pub(crate) struct GateEffects<'a> {
    by_gate: BTreeMap<(&'a str, AllomorphGateKind, u32), &'a AllomorphGateOutcome>,
}

impl<'a> GateEffects<'a> {
    pub(crate) fn new(outcomes: &'a [AllomorphGateOutcome]) -> Self {
        Self {
            by_gate: outcomes
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
                .collect(),
        }
    }

    /// The effect and reason the compiler recorded; a gate it never read is `owner_not_loaded`.
    pub(crate) fn effect(
        &self,
        allomorph: &str,
        kind: AllomorphGateKind,
        ordinal: u32,
    ) -> (&'static str, Option<String>) {
        match self.by_gate.get(&(allomorph, kind, ordinal)) {
            Some(outcome) => (
                outcome.effect.as_str(),
                outcome.reason_code.map(str::to_owned),
            ),
            None => ("owner_not_loaded", None),
        }
    }
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
