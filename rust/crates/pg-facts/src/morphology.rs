use pg_snapshot::feature::{FeatureStructure, FeatureValue, FeatureValueKind};
use pg_snapshot::lexicon::{LexEntry, Msa};
use pg_snapshot::morphology::PartOfSpeech;
use pg_snapshot::{
    canonical_guid, InventoryIdentity, InventoryKind, LoadDecision, LoadDisposition,
    LoadPipelineStage, Snapshot,
};
use rusqlite::{params, Transaction};
use serde::Deserialize;
use std::collections::BTreeMap;

use crate::FactsError;

type TemplateSlotPlacementMap = BTreeMap<(String, String, i64), (String, i64, i64)>;

pub(crate) fn validate_authored_guids(snapshot: &Snapshot) -> Result<(), FactsError> {
    validate_pos_guids(
        &snapshot.morphology.parts_of_speech,
        "morphology.partsOfSpeech",
    )?;
    for (entry_index, entry) in snapshot.lexicon.entries.iter().enumerate() {
        checked_guid(&entry.guid, &format!("lexicon.entries[{entry_index}].guid"))?;
        for (allomorph_index, allomorph) in entry.allomorphs.iter().enumerate() {
            let path = format!("lexicon.entries[{entry_index}].allomorphs[{allomorph_index}]");
            checked_guid(&allomorph.guid, &format!("{path}.guid"))?;
            if let Some(stem_name_guid) = &allomorph.stem_name {
                checked_guid(stem_name_guid, &format!("{path}.stemName"))?;
            }
        }
        for (msa_index, msa) in entry.msas.iter().enumerate() {
            checked_guid(
                msa.guid(),
                &format!("lexicon.entries[{entry_index}].msas[{msa_index}].guid"),
            )?;
        }
        for (sense_index, sense) in entry.senses.iter().enumerate() {
            checked_guid(
                &sense.guid,
                &format!("lexicon.entries[{entry_index}].senses[{sense_index}].guid"),
            )?;
        }
    }
    crate::adhoc::validate_authored_guids(snapshot)?;
    Ok(())
}

fn validate_pos_guids(items: &[PartOfSpeech], path: &str) -> Result<(), FactsError> {
    for (index, pos) in items.iter().enumerate() {
        let here = format!("{path}[{index}]");
        checked_guid(&pos.guid, &format!("{here}.guid"))?;
        if let Some(guid) = &pos.default_inflection_class {
            checked_guid(guid, &format!("{here}.defaultInflectionClass"))?;
        }
        for (class_index, class) in pos.inflection_classes.iter().enumerate() {
            validate_class_guid(class, &format!("{here}.inflectionClasses[{class_index}]"))?;
        }
        for (slot_index, slot) in pos.affix_slots.iter().enumerate() {
            checked_guid(&slot.guid, &format!("{here}.affixSlots[{slot_index}].guid"))?;
        }
        for (template_index, template) in pos.affix_templates.iter().enumerate() {
            let template_path = format!("{here}.affixTemplates[{template_index}]");
            checked_guid(&template.guid, &format!("{template_path}.guid"))?;
            for (slot_index, slot_guid) in template.prefix_slots.iter().enumerate() {
                checked_guid(
                    slot_guid,
                    &format!("{template_path}.prefixSlots[{slot_index}]"),
                )?;
            }
            for (slot_index, slot_guid) in template.suffix_slots.iter().enumerate() {
                checked_guid(
                    slot_guid,
                    &format!("{template_path}.suffixSlots[{slot_index}]"),
                )?;
            }
        }
        validate_pos_guids(&pos.children, &format!("{here}.children"))?;
    }
    Ok(())
}

fn validate_class_guid(class: &pg_snapshot::InflectionClass, path: &str) -> Result<(), FactsError> {
    checked_guid(&class.guid, &format!("{path}.guid"))?;
    for (index, child) in class.children.iter().enumerate() {
        validate_class_guid(child, &format!("{path}.children[{index}]"))?;
    }
    Ok(())
}

pub(crate) fn checked_guid(value: &str, path: &str) -> Result<String, FactsError> {
    canonical_guid(value).ok_or_else(|| {
        FactsError::InvalidSnapshot(format!(
            "{path} is not a lowercase-hyphenated GUID: {value:?}"
        ))
    })
}

pub(crate) fn insert_authored(
    tx: &Transaction<'_>,
    snapshot: &Snapshot,
    decisions: &[LoadDecision],
    compile_completed: bool,
) -> Result<(), FactsError> {
    tx.execute(
        "INSERT INTO project(singleton, name) VALUES (1, ?1)",
        [&snapshot.project.name],
    )?;
    let mut writing_systems = std::collections::BTreeSet::new();
    for tag in snapshot
        .project
        .vernacular_writing_systems
        .iter()
        .chain(&snapshot.project.analysis_writing_systems)
        .chain(
            snapshot
                .lexicon
                .entries
                .iter()
                .flat_map(|entry| entry.citation_form.iter().map(|form| &form.ws)),
        )
        .chain(snapshot.lexicon.entries.iter().flat_map(|entry| {
            entry.senses.iter().flat_map(|sense| {
                sense
                    .gloss
                    .iter()
                    .chain(&sense.definition)
                    .map(|form| &form.ws)
            })
        }))
        .chain(
            snapshot
                .lexicon
                .entries
                .iter()
                .flat_map(|entry| entry.allomorphs.iter())
                .flat_map(|allomorph| allomorph.forms.iter().map(|form| &form.ws)),
        )
        .chain(
            snapshot
                .phonology
                .phonemes
                .iter()
                .flat_map(|phoneme| phoneme.representations.iter().map(|form| &form.ws)),
        )
        .chain(
            snapshot
                .phonology
                .boundary_markers
                .iter()
                .flat_map(|boundary| boundary.representations.iter().map(|form| &form.ws)),
        )
    {
        writing_systems.insert(tag.as_str());
    }
    if let Some(groups) = &snapshot.morphology.adhoc_prohibition_groups {
        for form in groups
            .iter()
            .flat_map(|group| group.name.iter().chain(&group.description))
        {
            writing_systems.insert(form.ws.as_str());
        }
    }
    for tag in writing_systems {
        tx.execute("INSERT INTO writing_system(tag) VALUES (?1)", [tag])?;
    }
    for (role, systems) in [
        ("vernacular", &snapshot.project.vernacular_writing_systems),
        ("analysis", &snapshot.project.analysis_writing_systems),
    ] {
        for (ordinal, tag) in systems.iter().enumerate() {
            tx.execute(
                "INSERT INTO project_writing_system(role, ordinal, writing_system_tag) VALUES (?1, ?2, ?3)",
                params![role, ordinal as i64, tag],
            )?;
        }
    }
    for (ordinal, character) in snapshot.project.exemplar_characters.iter().enumerate() {
        tx.execute(
            "INSERT INTO project_exemplar_character(ordinal, character) VALUES (?1, ?2)",
            params![ordinal as i64, character],
        )?;
    }
    insert_categories(tx, &snapshot.morphology.parts_of_speech, None)?;
    insert_templates(
        tx,
        &snapshot.morphology.parts_of_speech,
        decisions,
        compile_completed,
    )?;
    for (entry_ordinal, entry) in snapshot.lexicon.entries.iter().enumerate() {
        insert_entry(tx, entry, entry_ordinal)?;
    }
    crate::adhoc::insert_authored(tx, snapshot)
}

fn insert_categories(
    tx: &Transaction<'_>,
    items: &[PartOfSpeech],
    parent_guid: Option<&str>,
) -> Result<(), FactsError> {
    for (ordinal, pos) in items.iter().enumerate() {
        let guid = checked_guid(&pos.guid, "category.guid")?;
        tx.execute(
            "INSERT INTO category(guid, parent_guid, sibling_ordinal, name, abbreviation, default_inflection_class_guid) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                guid,
                parent_guid,
                ordinal as i64,
                pos.name,
                pos.abbreviation,
                pos.default_inflection_class
                    .as_deref()
                    .map(|class_guid| checked_guid(class_guid, "category.defaultInflectionClass"))
                    .transpose()?
            ],
        )?;
        for (feature_ordinal, feature_guid) in pos.inflectable_features.iter().enumerate() {
            let feature_guid = checked_guid(feature_guid, "category.inflectableFeature")?;
            tx.execute(
                "INSERT INTO category_feature(category_guid, ordinal, feature_guid) VALUES (?1, ?2, ?3)",
                params![guid, feature_ordinal as i64, feature_guid],
            )?;
        }
        insert_classes(tx, &pos.inflection_classes, &guid, None)?;
        insert_categories(tx, &pos.children, Some(&guid))?;
    }
    Ok(())
}

fn insert_classes(
    tx: &Transaction<'_>,
    classes: &[pg_snapshot::InflectionClass],
    owner_category_guid: &str,
    parent_guid: Option<&str>,
) -> Result<(), FactsError> {
    for (ordinal, class) in classes.iter().enumerate() {
        let guid = checked_guid(&class.guid, "inflectionClass.guid")?;
        tx.execute(
            "INSERT INTO inflection_class(guid, owner_category_guid, parent_guid, sibling_ordinal, name, abbreviation) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                guid,
                owner_category_guid,
                parent_guid,
                ordinal as i64,
                class.name,
                class.abbreviation
            ],
        )?;
        insert_classes(tx, &class.children, owner_category_guid, Some(&guid))?;
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TemplateSlotPlacement {
    side: String,
    ordinal: i64,
    slot_guid: String,
    compiled_order: i64,
    surface_ordinal: i64,
}

fn insert_templates(
    tx: &Transaction<'_>,
    items: &[PartOfSpeech],
    decisions: &[LoadDecision],
    compile_completed: bool,
) -> Result<(), FactsError> {
    let placements = if compile_completed {
        template_slot_placements(decisions)?
    } else {
        BTreeMap::new()
    };
    let inserted = insert_pos_templates(tx, items, &placements)?;
    if inserted != placements.len() {
        return Err(FactsError::Serialization(
            "compiler template slot placement has no authored template row".into(),
        ));
    }
    Ok(())
}

fn insert_pos_templates(
    tx: &Transaction<'_>,
    items: &[PartOfSpeech],
    placements: &TemplateSlotPlacementMap,
) -> Result<usize, FactsError> {
    let mut inserted = 0;
    for pos in items {
        let category_guid = checked_guid(&pos.guid, "affixTemplate.categoryGuid")?;
        for slot in &pos.affix_slots {
            let slot_guid = checked_guid(&slot.guid, "affixSlot.guid")?;
            tx.execute(
                "INSERT INTO affix_slot(guid, category_guid, name, optional) VALUES (?1, ?2, ?3, ?4)",
                params![
                    slot_guid,
                    category_guid,
                    slot.name,
                    if slot.optional { 1_i64 } else { 0_i64 }
                ],
            )?;
        }
        for template in &pos.affix_templates {
            let template_guid = checked_guid(&template.guid, "affixTemplate.guid")?;
            tx.execute(
                "INSERT INTO affix_template(guid, category_guid, name, disabled, is_final) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    template_guid,
                    category_guid,
                    template.name,
                    if template.disabled { 1_i64 } else { 0_i64 },
                    if template.is_final { 1_i64 } else { 0_i64 }
                ],
            )?;
            for (side, slot_guids) in [
                ("prefix", &template.prefix_slots),
                ("suffix", &template.suffix_slots),
            ] {
                for (ordinal, slot_guid) in slot_guids.iter().enumerate() {
                    let slot_guid = checked_guid(slot_guid, "templateSlot.slotGuid")?;
                    let ordinal = i64::try_from(ordinal).map_err(|error| {
                        FactsError::Serialization(format!(
                            "template slot ordinal overflow: {error}"
                        ))
                    })?;
                    let key = (template_guid.clone(), side.to_string(), ordinal);
                    let (compiled_order, surface_ordinal) = match placements.get(&key) {
                        Some((mapped_slot, order, surface)) if mapped_slot == &slot_guid => {
                            inserted += 1;
                            (Some(*order), Some(*surface))
                        }
                        Some(_) => {
                            return Err(FactsError::Serialization(format!(
                                "compiler slot mapping disagrees with authored template {template_guid}"
                            )))
                        }
                        None => (None, None),
                    };
                    tx.execute(
                        "INSERT INTO template_slot(template_guid, side, ordinal, slot_guid, compiled_order, surface_ordinal) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                        params![template_guid, side, ordinal, slot_guid, compiled_order, surface_ordinal],
                    )?;
                }
            }
        }
        inserted += insert_pos_templates(tx, &pos.children, placements)?;
    }
    Ok(inserted)
}

fn template_slot_placements(
    decisions: &[LoadDecision],
) -> Result<TemplateSlotPlacementMap, FactsError> {
    let mut placements = BTreeMap::new();
    for decision in decisions {
        if decision.pipeline_stage != LoadPipelineStage::Compile
            || decision.subject.kind != InventoryKind::TemplateSlot
        {
            continue;
        }
        let Some(value) = &decision.effective_value_json else {
            continue;
        };
        if decision.disposition != LoadDisposition::Represented || decision.loaded != Some(true) {
            return Err(FactsError::Serialization(
                "compiler template slot placement is not a represented decision".into(),
            ));
        }
        let (owner_guid, target_guid) = match &decision.subject.identity {
            InventoryIdentity::Attachment {
                owner_guid,
                target_guid,
                role,
            } if role == "slot" => (owner_guid, target_guid),
            _ => {
                return Err(FactsError::Serialization(
                    "compiler template slot placement has no slot attachment identity".into(),
                ))
            }
        };
        let placement: TemplateSlotPlacement = serde_json::from_str(value)
            .map_err(|error| FactsError::Serialization(error.to_string()))?;
        if !matches!(placement.side.as_str(), "prefix" | "suffix")
            || placement.ordinal < 0
            || placement.compiled_order < 0
        {
            return Err(FactsError::Serialization(
                "compiler emitted an invalid template slot placement".into(),
            ));
        }
        let owner_guid = checked_guid(owner_guid, "compiler template guid")?;
        let target_guid = checked_guid(target_guid, "compiler template slot guid")?;
        let slot_guid = checked_guid(&placement.slot_guid, "compiler template slot value")?;
        if target_guid != slot_guid {
            return Err(FactsError::Serialization(
                "compiler template slot identity disagrees with its placement".into(),
            ));
        }
        let key = (owner_guid, placement.side, placement.ordinal);
        if placements
            .insert(
                key,
                (
                    slot_guid,
                    placement.compiled_order,
                    placement.surface_ordinal,
                ),
            )
            .is_some()
        {
            return Err(FactsError::Serialization(
                "compiler emitted duplicate template slot placements".into(),
            ));
        }
    }
    Ok(placements)
}

fn insert_entry(
    tx: &Transaction<'_>,
    entry: &LexEntry,
    entry_ordinal: usize,
) -> Result<(), FactsError> {
    let entry_guid = checked_guid(&entry.guid, "lexicon.entries.guid")?;
    let morph_type = enum_string(&entry.lexeme_morph_type)?;
    tx.execute(
        "INSERT INTO lex_entry(guid, source_ordinal, lexeme_morph_type) VALUES (?1, ?2, ?3)",
        params![entry_guid, entry_ordinal as i64, morph_type],
    )?;
    crate::lexicon::insert_allomorphs(tx, &entry_guid, &entry.allomorphs)?;
    for (ordinal, form) in entry.citation_form.iter().enumerate() {
        tx.execute(
            "INSERT INTO entry_citation_form(entry_guid, ordinal, writing_system, form) VALUES (?1, ?2, ?3, ?4)",
            params![entry_guid, ordinal as i64, form.ws, form.form],
        )?;
    }
    for msa in &entry.msas {
        insert_msa(tx, &entry_guid, msa)?;
    }
    for sense in &entry.senses {
        let sense_guid = checked_guid(&sense.guid, "lexicon.entries.senses.guid")?;
        let msa_guid = sense
            .msa
            .as_deref()
            .map(|guid| checked_guid(guid, "lexicon.entries.senses.msa"))
            .transpose()?;
        tx.execute(
            "INSERT INTO sense(sense_guid, entry_guid, msa_guid) VALUES (?1, ?2, ?3)",
            params![sense_guid, entry_guid, msa_guid],
        )?;
        for (kind, forms) in [("gloss", &sense.gloss), ("definition", &sense.definition)] {
            for (ordinal, form) in forms.iter().enumerate() {
                tx.execute(
                    "INSERT INTO sense_text(sense_guid, kind, ordinal, writing_system, text) VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![sense_guid, kind, ordinal as i64, form.ws, form.form],
                )?;
            }
        }
    }
    Ok(())
}

fn insert_msa(tx: &Transaction<'_>, entry_guid: &str, msa: &Msa) -> Result<(), FactsError> {
    let msa_guid = checked_guid(msa.guid(), "lexicon.entries.msas.guid")?;
    let kind = match msa {
        Msa::Stem { .. } => "stem",
        Msa::Inflectional { .. } => "inflectional",
        Msa::Derivational { .. } => "derivational",
        Msa::Unclassified { .. } => "unclassified",
    };
    tx.execute(
        "INSERT INTO msa(msa_guid, entry_guid, kind) VALUES (?1, ?2, ?3)",
        params![msa_guid, entry_guid, kind],
    )?;
    match msa {
        Msa::Stem {
            part_of_speech,
            inflection_class,
            features,
            exception_features,
            from_parts_of_speech,
            slots,
            ..
        } => {
            insert_optional_category(tx, &msa_guid, "pos", part_of_speech.as_deref())?;
            for (ordinal, guid) in from_parts_of_speech.iter().enumerate() {
                insert_category(tx, &msa_guid, "clitic_from", ordinal, guid)?;
            }
            insert_optional_class(tx, &msa_guid, "class", inflection_class.as_deref())?;
            for (ordinal, guid) in exception_features.iter().enumerate() {
                insert_exception(tx, &msa_guid, "required", ordinal, guid)?;
            }
            for (ordinal, guid) in slots.iter().enumerate() {
                insert_slot(tx, &msa_guid, "clitic_slot", ordinal, guid)?;
            }
            insert_feature_structure(tx, &msa_guid, "features", features.as_ref())?;
        }
        Msa::Inflectional {
            part_of_speech,
            slots,
            features,
            exception_features,
            ..
        } => {
            insert_optional_category(tx, &msa_guid, "pos", part_of_speech.as_deref())?;
            for (ordinal, guid) in slots.iter().enumerate() {
                insert_slot(tx, &msa_guid, "slot", ordinal, guid)?;
            }
            for (ordinal, guid) in exception_features.iter().enumerate() {
                insert_exception(tx, &msa_guid, "required", ordinal, guid)?;
            }
            insert_feature_structure(tx, &msa_guid, "features", features.as_ref())?;
        }
        Msa::Derivational {
            from_part_of_speech,
            to_part_of_speech,
            from_features,
            to_features,
            from_inflection_class,
            to_inflection_class,
            from_exception_features,
            to_exception_features,
            from_stem_name,
            ..
        } => {
            insert_optional_category(tx, &msa_guid, "from_pos", from_part_of_speech.as_deref())?;
            insert_optional_category(tx, &msa_guid, "to_pos", to_part_of_speech.as_deref())?;
            insert_optional_class(
                tx,
                &msa_guid,
                "from_class",
                from_inflection_class.as_deref(),
            )?;
            insert_optional_class(tx, &msa_guid, "to_class", to_inflection_class.as_deref())?;
            if let Some(guid) = from_stem_name {
                let guid = checked_guid(guid, "MSA stem-name reference")?;
                tx.execute(
                    "INSERT INTO msa_stem_name(msa_guid, role, stem_name_guid) VALUES (?1, 'from_stem_name', ?2)",
                    params![msa_guid, guid],
                )?;
            }
            for (ordinal, guid) in from_exception_features.iter().enumerate() {
                insert_exception(tx, &msa_guid, "from_required", ordinal, guid)?;
            }
            for (ordinal, guid) in to_exception_features.iter().enumerate() {
                insert_exception(tx, &msa_guid, "to_required", ordinal, guid)?;
            }
            insert_feature_structure(tx, &msa_guid, "from_features", from_features.as_ref())?;
            insert_feature_structure(tx, &msa_guid, "to_features", to_features.as_ref())?;
        }
        Msa::Unclassified { part_of_speech, .. } => {
            insert_optional_category(tx, &msa_guid, "pos", part_of_speech.as_deref())?;
        }
    }
    Ok(())
}

fn insert_optional_category(
    tx: &Transaction<'_>,
    msa_guid: &str,
    role: &str,
    guid: Option<&str>,
) -> Result<(), FactsError> {
    if let Some(guid) = guid {
        insert_category(tx, msa_guid, role, 0, guid)?;
    }
    Ok(())
}

fn insert_category(
    tx: &Transaction<'_>,
    msa_guid: &str,
    role: &str,
    ordinal: usize,
    guid: &str,
) -> Result<(), FactsError> {
    let guid = checked_guid(guid, "MSA category reference")?;
    tx.execute(
        "INSERT INTO msa_category(msa_guid, role, ordinal, category_guid) VALUES (?1, ?2, ?3, ?4)",
        params![msa_guid, role, ordinal as i64, guid],
    )?;
    Ok(())
}

fn insert_optional_class(
    tx: &Transaction<'_>,
    msa_guid: &str,
    role: &str,
    guid: Option<&str>,
) -> Result<(), FactsError> {
    if let Some(guid) = guid {
        let guid = checked_guid(guid, "MSA inflection-class reference")?;
        tx.execute(
            "INSERT INTO msa_inflection_class(msa_guid, role, class_guid) VALUES (?1, ?2, ?3)",
            params![msa_guid, role, guid],
        )?;
    }
    Ok(())
}

fn insert_slot(
    tx: &Transaction<'_>,
    msa_guid: &str,
    role: &str,
    ordinal: usize,
    guid: &str,
) -> Result<(), FactsError> {
    let guid = checked_guid(guid, "MSA slot reference")?;
    tx.execute(
        "INSERT INTO msa_slot(msa_guid, role, ordinal, slot_guid) VALUES (?1, ?2, ?3, ?4)",
        params![msa_guid, role, ordinal as i64, guid],
    )?;
    Ok(())
}

fn insert_exception(
    tx: &Transaction<'_>,
    msa_guid: &str,
    role: &str,
    ordinal: usize,
    guid: &str,
) -> Result<(), FactsError> {
    let guid = checked_guid(guid, "MSA exception-feature reference")?;
    tx.execute(
        "INSERT INTO msa_exception_feature(msa_guid, role, ordinal, target_guid) VALUES (?1, ?2, ?3, ?4)",
        params![msa_guid, role, ordinal as i64, guid],
    )?;
    Ok(())
}

fn insert_feature_structure(
    tx: &Transaction<'_>,
    msa_guid: &str,
    role: &str,
    structure: Option<&pg_snapshot::feature::FeatureStructure>,
) -> Result<(), FactsError> {
    if let Some(structure) = structure {
        let structure = normalize_feature_structure(structure)?;
        let value = serde_json::to_value(structure)
            .map_err(|error| FactsError::Serialization(error.to_string()))?;
        let json = pg_assess::canonicalize(&value)
            .map_err(|error| FactsError::Serialization(error.to_string()))?;
        tx.execute(
            "INSERT INTO msa_feature_structure(msa_guid, role, feature_structure_json) VALUES (?1, ?2, ?3)",
            params![msa_guid, role, json],
        )?;
    }
    Ok(())
}

fn normalize_feature_structure(
    structure: &FeatureStructure,
) -> Result<FeatureStructure, FactsError> {
    let values = structure
        .values
        .iter()
        .map(|item| {
            Ok(FeatureValue {
                feature: checked_guid(&item.feature, "MSA feature reference")?,
                value: match &item.value {
                    FeatureValueKind::Closed { value } => FeatureValueKind::Closed {
                        value: checked_guid(value, "MSA feature-value reference")?,
                    },
                    FeatureValueKind::Complex { value } => FeatureValueKind::Complex {
                        value: normalize_feature_structure(value)?,
                    },
                },
            })
        })
        .collect::<Result<Vec<_>, FactsError>>()?;
    Ok(FeatureStructure { values })
}

fn enum_string<T: serde::Serialize>(value: &T) -> Result<String, FactsError> {
    let value = serde_json::to_value(value)
        .map_err(|error| FactsError::Serialization(error.to_string()))?;
    value
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| FactsError::Serialization("expected a string enum value".into()))
}
