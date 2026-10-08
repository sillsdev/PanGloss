use std::collections::BTreeMap;

use pg_snapshot::lexicon::Msa;
use pg_snapshot::{InventoryKind, LoadDecision, LoadDisposition, LoadPipelineStage, Snapshot};

use crate::model::{Grammar, MorphRuleDef, PhonRuleDef, StratumId};

use super::issues::{CompiledAllomorphOrder, CompiledMapping};

pub(crate) fn publish(
    snapshot: &Snapshot,
    grammar: &Grammar,
    decisions: &[LoadDecision],
) -> (Vec<CompiledMapping>, Vec<CompiledAllomorphOrder>) {
    let mut mappings = BTreeMap::new();
    let mut orders = BTreeMap::new();

    for (index, entry) in grammar.entries.iter().enumerate() {
        let entry_id = crate::model::LexEntryId(index as u32);
        let morpheme = &grammar.morphemes[entry.morpheme.0 as usize];
        let bucket = bucket_name(grammar, morpheme.stratum);
        let output_key = entry_output_key(morpheme, &bucket);
        if let Some(guid) = entry.source_guid.as_deref() {
            mapping(
                &mut mappings,
                "entry",
                Some(guid),
                guid,
                "lexEntry",
                &output_key,
                "structural",
            );
        }
        if let Some(guid) = morpheme.source_msa_guid.as_deref() {
            mapping(
                &mut mappings,
                "msa",
                Some(guid),
                guid,
                "lexEntry",
                &output_key,
                "structural",
            );
        }
        add_root_order(
            grammar,
            entry_id,
            entry.source_guid.as_deref(),
            morpheme.source_msa_guid.as_deref(),
            &bucket,
            &output_key,
            &mut mappings,
            &mut orders,
        );
    }

    let entry_by_allomorph: BTreeMap<_, _> = snapshot
        .lexicon
        .entries
        .iter()
        .flat_map(|entry| {
            entry
                .allomorphs
                .iter()
                .map(move |allomorph| (allomorph.guid.as_str(), entry.guid.as_str()))
        })
        .collect();
    let mut entry_by_msa = BTreeMap::new();
    for entry in &snapshot.lexicon.entries {
        for msa in &entry.msas {
            entry_by_msa
                .entry(msa.guid())
                .or_insert(entry.guid.as_str());
        }
    }

    for rule in &grammar.mrules {
        match rule {
            MorphRuleDef::Compounding(compound) => {
                let output_key = format!("morph_rule:{}", compound.xml_id);
                if let Some(guid) = compound.source_guid.as_deref() {
                    mapping(
                        &mut mappings,
                        "compoundRule",
                        Some(guid),
                        guid,
                        "morphRule",
                        &output_key,
                        "structural",
                    );
                } else {
                    mapping(
                        &mut mappings,
                        "synthetic",
                        None,
                        &compound.xml_id,
                        "morphRule",
                        &output_key,
                        "synthetic",
                    );
                }
            }
            MorphRuleDef::AffixProcess(def) => {
                add_rule_lineage(
                    grammar,
                    def.morpheme,
                    &def.allomorphs,
                    &entry_by_allomorph,
                    &entry_by_msa,
                    &mut mappings,
                    &mut orders,
                );
            }
            MorphRuleDef::Realizational(def) => {
                add_rule_lineage(
                    grammar,
                    def.morpheme,
                    &def.allomorphs,
                    &entry_by_allomorph,
                    &entry_by_msa,
                    &mut mappings,
                    &mut orders,
                );
            }
        }
    }

    for class in &grammar.natural_classes {
        if snapshot
            .phonology
            .natural_classes
            .iter()
            .any(|source| natural_class_guid(source) == class.xml_id)
        {
            mapping(
                &mut mappings,
                "naturalClass",
                Some(&class.xml_id),
                &class.xml_id,
                "naturalClass",
                &format!("natural_class:{}", class.xml_id),
                "authored",
            );
        }
    }

    for (index, rule) in grammar.prules.iter().enumerate() {
        let (guid, output_key) = match rule {
            PhonRuleDef::Rewrite(rule) => (&rule.xml_id, format!("phonological_rule:{}", index)),
            PhonRuleDef::Metathesis(rule) => (&rule.xml_id, format!("phonological_rule:{}", index)),
        };
        if snapshot.phonology.rules.iter().any(|source| match source {
            pg_snapshot::phonology::PhonologicalRule::Rewrite(source) => source.guid == *guid,
            pg_snapshot::phonology::PhonologicalRule::Metathesis(source) => source.guid == *guid,
        }) {
            mapping(
                &mut mappings,
                "phonologicalRule",
                Some(guid),
                guid,
                "phonologicalRule",
                &output_key,
                "structural",
            );
        }
    }

    for (index, template) in grammar.templates.iter().enumerate() {
        if let Some(guid) = template.source_guid.as_deref() {
            mapping(
                &mut mappings,
                "template",
                Some(guid),
                guid,
                "template",
                &format!("template:{index}"),
                "structural",
            );
        }
    }

    add_unrepresented_root_order(snapshot, decisions, &mut orders);

    (
        mappings.into_values().collect(),
        orders.into_values().collect(),
    )
}

fn natural_class_guid(class: &pg_snapshot::phonology::NaturalClass) -> &str {
    match class {
        pg_snapshot::phonology::NaturalClass::Segments { guid, .. }
        | pg_snapshot::phonology::NaturalClass::Features { guid, .. } => guid,
    }
}

fn add_rule_lineage(
    grammar: &Grammar,
    morpheme_id: crate::model::MorphemeId,
    allomorphs: &[crate::model::AffixAllomorphDef],
    entry_by_allomorph: &BTreeMap<&str, &str>,
    entry_by_msa: &BTreeMap<&str, &str>,
    mappings: &mut BTreeMap<MappingKey, CompiledMapping>,
    orders: &mut BTreeMap<OrderKey, CompiledAllomorphOrder>,
) {
    let morpheme = &grammar.morphemes[morpheme_id.0 as usize];
    let bucket = bucket_name(grammar, morpheme.stratum);
    let output_key = rule_output_key(morpheme, &bucket);
    if let Some(guid) = morpheme.source_msa_guid.as_deref() {
        mapping(
            mappings,
            "msa",
            Some(guid),
            guid,
            "morphRule",
            &output_key,
            "structural",
        );
    }
    if let Some(guid) = morpheme.source_infl_type_guid.as_deref() {
        mapping(
            mappings,
            "ruleFeature",
            Some(guid),
            guid,
            "morphRule",
            &output_key,
            "structural",
        );
    }

    let owner_key = output_key.clone();
    let final_index = allomorphs.len().checked_sub(1);
    let source_entry_guid = morpheme
        .source_msa_guid
        .as_deref()
        .and_then(|guid| entry_by_msa.get(guid).copied());
    for (index, allomorph) in allomorphs.iter().enumerate() {
        let key = allomorph_output_key(&output_key, index as u16);
        let source = grammar
            .allomorph_sources
            .get(allomorph.id.0 as usize)
            .expect("allomorph source rows parallel the final allomorph registry");
        if source.form_guids.is_empty() {
            if let Some(guid) = morpheme.source_infl_type_guid.as_deref() {
                mapping(
                    mappings,
                    "ruleFeature",
                    Some(guid),
                    &format!("null-affix#{guid}"),
                    "allomorph",
                    &key,
                    "structural",
                );
            } else {
                mapping(
                    mappings,
                    "synthetic",
                    None,
                    &key,
                    "allomorph",
                    &key,
                    "synthetic",
                );
            }
        }
        for guid in source.form_guids.iter().flatten() {
            let allomorph_entry_guid = entry_by_allomorph
                .get(guid.as_str())
                .copied()
                .or(source_entry_guid);
            mapping(
                mappings,
                "allomorph",
                Some(guid),
                guid,
                "allomorph",
                &key,
                "structural",
            );
            if let Some(entry_guid) = entry_by_allomorph.get(guid.as_str()) {
                mapping(
                    mappings,
                    "entry",
                    Some(entry_guid),
                    entry_guid,
                    "morphRule",
                    &output_key,
                    "structural",
                );
            }
            push_order(
                orders,
                morpheme.source_msa_guid.as_deref(),
                &bucket,
                Some(guid),
                Some(&key),
                index as u32,
                final_index == Some(index) && allomorph.environments.is_empty(),
                &owner_key,
                allomorph_entry_guid,
            );
        }
        if source.form_guids.is_empty() {
            push_order(
                orders,
                morpheme.source_msa_guid.as_deref(),
                &bucket,
                None,
                Some(&key),
                index as u32,
                final_index == Some(index) && allomorph.environments.is_empty(),
                &owner_key,
                source_entry_guid,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn add_root_order(
    grammar: &Grammar,
    entry_id: crate::model::LexEntryId,
    source_entry_guid: Option<&str>,
    source_msa_guid: Option<&str>,
    bucket: &str,
    output_key: &str,
    mappings: &mut BTreeMap<MappingKey, CompiledMapping>,
    orders: &mut BTreeMap<OrderKey, CompiledAllomorphOrder>,
) {
    let entry = &grammar.entries[entry_id.0 as usize];
    let owner_key = output_key.to_string();
    let final_index = entry.allomorphs.len().checked_sub(1);
    for (index, allomorph) in entry.allomorphs.iter().enumerate() {
        let key = allomorph_output_key(output_key, index as u16);
        let source = grammar
            .allomorph_sources
            .get(allomorph.id.0 as usize)
            .expect("allomorph source rows parallel the final allomorph registry");
        for guid in source.form_guids.iter().flatten() {
            mapping(
                mappings,
                "allomorph",
                Some(guid),
                guid,
                "allomorph",
                &key,
                "structural",
            );
            push_order(
                orders,
                source_msa_guid,
                bucket,
                Some(guid),
                Some(&key),
                index as u32,
                final_index == Some(index) && allomorph.environments.is_empty(),
                &owner_key,
                source_entry_guid,
            );
        }
        if source.form_guids.is_empty() {
            push_order(
                orders,
                source_msa_guid,
                bucket,
                None,
                Some(&key),
                index as u32,
                final_index == Some(index) && allomorph.environments.is_empty(),
                &owner_key,
                source_entry_guid,
            );
        }
    }
}

fn add_unrepresented_root_order(
    snapshot: &Snapshot,
    decisions: &[LoadDecision],
    orders: &mut BTreeMap<OrderKey, CompiledAllomorphOrder>,
) {
    let entry_by_allomorph: BTreeMap<_, _> = snapshot
        .lexicon
        .entries
        .iter()
        .flat_map(|entry| {
            entry
                .allomorphs
                .iter()
                .map(move |allomorph| (allomorph.guid.as_str(), entry))
        })
        .collect();
    for decision in decisions.iter().filter(|decision| {
        decision.pipeline_stage == LoadPipelineStage::Compile
            && decision.subject.kind == InventoryKind::Allomorph
            && decision.disposition == LoadDisposition::NotConsidered
            && decision.context_key.starts_with("lexEntryForm:")
    }) {
        let guid = match &decision.subject.identity {
            pg_snapshot::InventoryIdentity::Object { guid } => guid.as_str(),
            _ => continue,
        };
        let Some(entry) = entry_by_allomorph.get(guid) else {
            continue;
        };
        let bucket = if decision.context_key.ends_with("clitics") {
            "Clitics"
        } else {
            "Morphology"
        };
        for msa in entry.msas.iter().filter_map(|msa| match msa {
            Msa::Stem { guid, .. } => Some(guid.as_str()),
            _ => None,
        }) {
            let owner_key = format!("lex_entry:{msa}@{bucket}");
            let order_key = order_key(&owner_key, Some(guid), None);
            if orders.contains_key(&order_key) {
                continue;
            }
            orders.insert(
                order_key,
                CompiledAllomorphOrder {
                    owner_key,
                    source_entry_guid: Some(entry.guid.clone()),
                    source_msa_guid: Some(msa.to_string()),
                    bucket: bucket.to_string(),
                    source_allomorph_guid: Some(guid.to_string()),
                    output_key: None,
                    compiled_order: None,
                    is_final_elsewhere_case: false,
                },
            );
        }
    }
}

fn mapping(
    out: &mut BTreeMap<MappingKey, CompiledMapping>,
    source_kind: &str,
    source_guid: Option<&str>,
    source_key: &str,
    output_kind: &str,
    output_key: &str,
    identity_quality: &str,
) {
    let source_guid = source_guid.map(str::to_string);
    let record = CompiledMapping {
        source_kind: source_kind.to_string(),
        source_guid,
        source_key: source_key.to_string(),
        output_kind: output_kind.to_string(),
        output_key: output_key.to_string(),
        identity_quality: identity_quality.to_string(),
    };
    let key = (
        record.source_kind.clone(),
        record.source_key.clone(),
        record.output_kind.clone(),
        record.output_key.clone(),
    );
    out.insert(key, record);
}

#[allow(clippy::too_many_arguments)]
fn push_order(
    orders: &mut BTreeMap<OrderKey, CompiledAllomorphOrder>,
    source_msa_guid: Option<&str>,
    bucket: &str,
    source_allomorph_guid: Option<&str>,
    output_key: Option<&str>,
    compiled_order: u32,
    is_final_elsewhere_case: bool,
    owner_key: &str,
    source_entry_guid: Option<&str>,
) {
    let key = order_key(owner_key, source_allomorph_guid, output_key);
    orders.insert(
        key,
        CompiledAllomorphOrder {
            owner_key: owner_key.to_string(),
            source_entry_guid: source_entry_guid.map(str::to_string),
            source_msa_guid: source_msa_guid.map(str::to_string),
            bucket: bucket.to_string(),
            source_allomorph_guid: source_allomorph_guid.map(str::to_string),
            output_key: output_key.map(str::to_string),
            compiled_order: Some(compiled_order),
            is_final_elsewhere_case,
        },
    );
}

fn order_key(owner_key: &str, source_guid: Option<&str>, output_key: Option<&str>) -> OrderKey {
    (
        owner_key.to_string(),
        source_guid.unwrap_or("").to_string(),
        output_key.unwrap_or("").to_string(),
    )
}

fn entry_output_key(morpheme: &crate::model::MorphemeInfo, bucket: &str) -> String {
    format!("lex_entry:{}@{bucket}", morpheme.xml_key)
}

fn rule_output_key(morpheme: &crate::model::MorphemeInfo, bucket: &str) -> String {
    format!("morph_rule:{}@{bucket}", morpheme.xml_key)
}

fn allomorph_output_key(owner_key: &str, index: u16) -> String {
    format!("{owner_key}#allo{index}")
}

fn bucket_name(grammar: &Grammar, stratum: StratumId) -> String {
    grammar.strata[stratum.0 as usize]
        .name
        .clone()
        .unwrap_or_else(|| format!("stratum#{}", stratum.0))
}

type MappingKey = (String, String, String, String);
type OrderKey = (String, String, String);
