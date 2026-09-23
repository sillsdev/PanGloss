use crate::grammar_health::{FieldWorksLink, FieldWorksUnavailableReason};
use crate::model::{Grammar, LexEntryId, MRuleId};

pub(crate) enum FieldWorksSource {
    Table,
    CharDef,
    LexEntry(LexEntryId),
    MorphRule(MRuleId),
}

#[derive(Clone, Copy)]
enum FieldWorksGuidKind {
    MoForm,
    Msa,
    InflType,
}

pub(crate) fn canonical_guid(source_id: &str) -> Option<String> {
    let bytes = source_id.as_bytes();
    let well_formed = bytes.len() == 36
        && bytes.iter().enumerate().all(|(index, byte)| match index {
            8 | 13 | 18 | 23 => *byte == b'-',
            _ => byte.is_ascii_hexdigit(),
        });
    well_formed.then(|| source_id.to_ascii_lowercase())
}

fn source_guid<'a>(
    grammar: &'a Grammar,
    source: &FieldWorksSource,
) -> Option<(&'a str, FieldWorksGuidKind)> {
    match source {
        FieldWorksSource::Table | FieldWorksSource::CharDef => None,
        FieldWorksSource::LexEntry(id) => grammar
            .entries
            .get(id.0 as usize)?
            .allomorphs
            .iter()
            .flat_map(|allomorph| {
                grammar
                    .allomorph_sources
                    .get(allomorph.id.0 as usize)
                    .into_iter()
                    .flat_map(|source| source.form_guids.iter())
            })
            .flatten()
            .next()
            .map(|guid| (guid.as_str(), FieldWorksGuidKind::MoForm)),
        FieldWorksSource::MorphRule(id) => {
            let morpheme = grammar.mrules.get(id.0 as usize)?.morpheme()?;
            let info = grammar.morphemes.get(morpheme.0 as usize)?;
            info.source_msa_guid
                .as_deref()
                .map(|guid| (guid, FieldWorksGuidKind::Msa))
                .or_else(|| {
                    info.source_infl_type_guid
                        .as_deref()
                        .map(|guid| (guid, FieldWorksGuidKind::InflType))
                })
        }
    }
}

fn verified_tool(source: &FieldWorksSource) -> Option<&'static str> {
    match source {
        // FieldWorks DistFiles/Language Explorer/Configuration/Lexicon/Edit/toolConfiguration.xml:6.
        FieldWorksSource::LexEntry(_) | FieldWorksSource::MorphRule(_) => Some("lexiconEdit"),
        FieldWorksSource::Table | FieldWorksSource::CharDef => None,
    }
}

// FieldWorks LinkListener.cs:578 -> RecordClerk.cs:998-1016 -> RecordList.cs:3435 walks owners to the entry.
fn verified_guid_kind(kind: FieldWorksGuidKind) -> bool {
    matches!(kind, FieldWorksGuidKind::MoForm | FieldWorksGuidKind::Msa)
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

/// Build complete navigation state before a subject enters a report.
pub(crate) fn fieldworks_link(
    grammar: &Grammar,
    source: FieldWorksSource,
    fieldworks_project: Option<&str>,
) -> FieldWorksLink {
    let Some((raw_guid, guid_kind)) = source_guid(grammar, &source) else {
        return FieldWorksLink::Unavailable {
            reason: FieldWorksUnavailableReason::MissingGuid,
            guid: None,
        };
    };
    let Some(guid) = canonical_guid(raw_guid) else {
        return FieldWorksLink::Unavailable {
            reason: FieldWorksUnavailableReason::InvalidGuid,
            guid: None,
        };
    };
    let Some(tool) = verified_tool(&source) else {
        return FieldWorksLink::Unavailable {
            reason: FieldWorksUnavailableReason::UnverifiedTool,
            guid: Some(guid),
        };
    };
    if !verified_guid_kind(guid_kind) {
        return FieldWorksLink::Unavailable {
            reason: FieldWorksUnavailableReason::UnverifiedGuidKind,
            guid: Some(guid),
        };
    }
    let Some(project) = fieldworks_project
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return FieldWorksLink::Unavailable {
            reason: FieldWorksUnavailableReason::MissingProject,
            guid: Some(guid),
        };
    };
    let query = format!("database={project}&tool={tool}&guid={guid}&tag=");
    let url = format!("silfw://localhost/link?{}", encode_query(&query));
    FieldWorksLink::Available {
        guid,
        tool: tool.to_string(),
        url,
    }
}
