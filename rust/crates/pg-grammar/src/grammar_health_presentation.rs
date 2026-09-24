use crate::chardef::CharDefId;
use crate::grammar_health::{FieldWorksLink, FieldWorksUnavailableReason};
use crate::model::{Grammar, LexEntryId, MRuleId, MorphRuleDef, TableId};
use pg_snapshot::{FwClass, FwObjectRef};

pub(crate) enum FieldWorksSource {
    Table,
    CharDef(TableId, CharDefId),
    LexEntry(LexEntryId),
    MorphRule(MRuleId),
}

pub(crate) fn fieldworks_link_from_identity(
    class: FwClass,
    source_guid: Option<&str>,
    project: Option<&str>,
) -> FieldWorksLink {
    let Some(tool) = verified_tool_for_class(class) else {
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
    let Some(project_name) = project.map(str::trim).filter(|value| !value.is_empty()) else {
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

fn verified_tool_for_class(class: FwClass) -> Option<&'static str> {
    match class {
        FwClass::LexEntry
        | FwClass::MoForm
        | FwClass::MoStemMsa
        | FwClass::MoInflAffMsa
        | FwClass::MoDerivAffMsa
        | FwClass::MoUnclassifiedAffixMsa => Some("lexiconEdit"),
        FwClass::PhPhoneme => Some("phonemeEdit"),
        _ => None,
    }
}

pub(crate) fn fieldworks_identity(grammar: &Grammar, source: &FieldWorksSource) -> FwObjectRef {
    let (class, guid) = match source {
        FieldWorksSource::Table => (FwClass::PhPhonemeSet, None),
        FieldWorksSource::CharDef(table_id, char_def_id) => (
            FwClass::PhPhoneme,
            grammar
                .char_tables
                .get(table_id.0 as usize)
                .and_then(|table| {
                    table
                        .iter()
                        .find(|(candidate, _)| candidate == char_def_id)
                        .and_then(|(_, char_def)| char_def.source_guid())
                }),
        ),
        FieldWorksSource::LexEntry(id) => (
            FwClass::MoForm,
            grammar
                .entries
                .get(id.0 as usize)
                .into_iter()
                .flat_map(|entry| entry.allomorphs.iter())
                .flat_map(|allomorph| {
                    grammar
                        .allomorph_sources
                        .get(allomorph.id.0 as usize)
                        .into_iter()
                        .flat_map(|source| source.form_guids.iter())
                })
                .flatten()
                .next()
                .map(String::as_str),
        ),
        FieldWorksSource::MorphRule(id) => {
            if matches!(
                grammar.mrules.get(id.0 as usize),
                Some(MorphRuleDef::Compounding(_))
            ) {
                (FwClass::MoCompoundRule, None)
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
