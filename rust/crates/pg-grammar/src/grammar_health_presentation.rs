use crate::chardef::{CharDefId, CharDefKind};
use crate::grammar_health::{FieldWorksLink, FieldWorksUnavailableReason};
use crate::model::{Grammar, LexEntryId, MRuleId, MorphRuleDef, TableId};
use pg_snapshot::{FwClass, FwObjectRef, FwOpenTarget};

pub(crate) enum FieldWorksSource {
    Table(TableId),
    CharDef(TableId, CharDefId),
    LexEntry(LexEntryId),
    MorphRule(MRuleId),
}

#[cfg(test)]
pub(crate) fn fieldworks_link_from_identity(
    class: FwClass,
    source_guid: Option<&str>,
    project: Option<&str>,
) -> FieldWorksLink {
    fieldworks_link(class, source_guid, None, project)
}

/// `opens_in`, when present, replaces both the class's tool and the object's own GUID.
pub(crate) fn fieldworks_link(
    class: FwClass,
    source_guid: Option<&str>,
    opens_in: Option<&FwOpenTarget>,
    project: Option<&str>,
) -> FieldWorksLink {
    let (tool, source_guid) = match opens_in {
        Some(target) => (Some(target.tool.as_str()), Some(target.guid.as_str())),
        None => (verified_tool_for_class(class), source_guid),
    };
    let Some(tool) = tool else {
        return FieldWorksLink::Unavailable {
            reason: FieldWorksUnavailableReason::UnsupportedKind,
            guid: source_guid.map(str::to_string),
        };
    };
    let Some(raw_guid) = source_guid else {
        return FieldWorksLink::Unavailable {
            reason: FieldWorksUnavailableReason::GuidNotRecorded,
            guid: None,
        };
    };
    let Some(guid) = pg_snapshot::canonical_guid(raw_guid) else {
        return FieldWorksLink::Unavailable {
            reason: FieldWorksUnavailableReason::InvalidGuid,
            guid: Some(raw_guid.to_string()),
        };
    };
    let Some(project_name) = project.filter(|value| !value.is_empty()) else {
        return FieldWorksLink::Unavailable {
            reason: FieldWorksUnavailableReason::MissingProject,
            guid: Some(guid),
        };
    };
    let query = format!("database={project_name}&tool={tool}&guid={guid}&tag=");
    let url = format!("silfw://localhost/link?{}", encode_query(&query));
    FieldWorksLink::Available {
        guid,
        tool: tool.to_string(),
        url,
    }
}

/// FieldWorks tool ids (`DistFiles/Language Explorer/Configuration/*/Edit/toolConfiguration.xml`).
pub(crate) mod tool {
    pub const LEXICON_EDIT: &str = "lexiconEdit";
    pub const PHONEMES: &str = "phonemeEdit";
    pub const NATURAL_CLASSES: &str = "naturalClassedit";
    pub const ENVIRONMENTS: &str = "EnvironmentEdit";
    pub const PHONOLOGICAL_RULES: &str = "PhonologicalRuleEdit";
    pub const AD_HOC_RULES: &str = "AdhocCoprohibEdit";
    pub const COMPOUND_RULES: &str = "compoundRuleAdvancedEdit";
    pub const CATEGORY_EDIT: &str = "posEdit";
    pub const PHONOLOGICAL_FEATURES: &str = "phonologicalFeaturesAdvancedEdit";
    pub const INFLECTION_FEATURES: &str = "featuresAdvancedEdit";
    pub const VARIANT_TYPES: &str = "variantEntryTypeEdit";
}

/// Owned objects open through their owner (`RecordClerk.IndexOfObjOrChildOrParent`).
fn verified_tool_for_class(class: FwClass) -> Option<&'static str> {
    match class {
        FwClass::LexEntry
        | FwClass::LexSense
        | FwClass::MoForm
        | FwClass::MoStemMsa
        | FwClass::MoInflAffMsa
        | FwClass::MoDerivAffMsa
        | FwClass::MoUnclassifiedAffixMsa => Some(tool::LEXICON_EDIT),
        FwClass::PhPhoneme | FwClass::PhPhonemeSet => Some(tool::PHONEMES),
        FwClass::PhNaturalClass => Some(tool::NATURAL_CLASSES),
        FwClass::PhEnvironment => Some(tool::ENVIRONMENTS),
        FwClass::PhRegularRule | FwClass::PhMetathesisRule => Some(tool::PHONOLOGICAL_RULES),
        FwClass::MoAdhocProhib => Some(tool::AD_HOC_RULES),
        FwClass::MoCompoundRule => Some(tool::COMPOUND_RULES),
        FwClass::MoInflAffixTemplate
        | FwClass::MoInflAffixSlot
        | FwClass::MoStemName
        | FwClass::MoInflClass => Some(tool::CATEGORY_EDIT),
        FwClass::LexEntryInflType => Some(tool::VARIANT_TYPES),
        // No tool lists boundary markers; a feature's tool depends on the system that owns it.
        FwClass::PhBdryMarker
        | FwClass::FsFeatureSystem
        | FwClass::FsComplexFeature
        | FwClass::FsClosedFeature
        | FwClass::FsSymFeatVal
        | FwClass::Unknown
        | FwClass::Project => None,
    }
}

pub(crate) fn fieldworks_identity(grammar: &Grammar, source: &FieldWorksSource) -> FwObjectRef {
    let table_guid = |table_id: &TableId| {
        grammar
            .char_tables
            .get(table_id.0 as usize)
            .and_then(|table| table.source_guid())
    };
    let (class, guid) = match source {
        FieldWorksSource::Table(table_id) => (FwClass::PhPhonemeSet, table_guid(table_id)),
        FieldWorksSource::CharDef(table_id, char_def_id) => {
            let char_def = grammar
                .char_tables
                .get(table_id.0 as usize)
                .and_then(|table| {
                    table
                        .iter()
                        .find(|(candidate, _)| candidate == char_def_id)
                        .map(|(_, char_def)| char_def)
                });
            let class = match char_def.map(|char_def| char_def.kind()) {
                Some(CharDefKind::Boundary) => FwClass::PhBdryMarker,
                _ => FwClass::PhPhoneme,
            };
            let guid = char_def.and_then(|char_def| char_def.source_guid());
            let mut source = FwObjectRef::new(class);
            if let Some(guid) = guid {
                source = source.guid(guid);
            }
            // A boundary marker, or a character not yet listed as a phoneme, opens its phoneme set.
            if class == FwClass::PhBdryMarker || guid.is_none() {
                if let Some(set_guid) = table_guid(table_id) {
                    source = source.opens_in(tool::PHONEMES, set_guid);
                }
            }
            return source;
        }
        FieldWorksSource::LexEntry(id) => (
            FwClass::LexEntry,
            grammar
                .entries
                .get(id.0 as usize)
                .and_then(|entry| entry.source_guid.as_deref()),
        ),
        FieldWorksSource::MorphRule(id) => {
            if let Some(MorphRuleDef::Compounding(def)) = grammar.mrules.get(id.0 as usize) {
                (FwClass::MoCompoundRule, def.source_guid.as_deref())
            } else {
                let info = grammar
                    .mrules
                    .get(id.0 as usize)
                    .and_then(|rule| rule.morpheme())
                    .and_then(|morpheme| grammar.morphemes.get(morpheme.0 as usize));
                match info {
                    Some(info) if info.source_msa_guid.is_some() => (
                        info.source_msa_class.unwrap_or(FwClass::Project),
                        info.source_msa_guid.as_deref(),
                    ),
                    Some(info) if info.source_infl_type_guid.is_some() => (
                        FwClass::LexEntryInflType,
                        info.source_infl_type_guid.as_deref(),
                    ),
                    Some(info) => (info.source_msa_class.unwrap_or(FwClass::MoInflAffMsa), None),
                    None => (FwClass::MoInflAffMsa, None),
                }
            }
        }
    };
    let mut source = FwObjectRef::new(class);
    if let Some(guid) = guid {
        source = source.guid(guid);
    }
    source
}

fn encode_query(value: &str) -> String {
    value
        .as_bytes()
        .iter()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' => {
                (*byte as char).to_string()
            }
            b' ' => "+".to_string(),
            byte => format!("%{byte:02X}"),
        })
        .collect()
}
