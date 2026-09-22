use crate::grammar_health::{
    FieldWorksLink, GrammarHealthCheckFinding, GrammarHealthSubjectKind,
};
use crate::model::{Grammar, MRuleId, MorphRuleDef};

pub(crate) fn canonical_guid(source_id: &str) -> Option<String> {
    let bytes = source_id.as_bytes();
    if bytes.len() != 36
        || ![8, 13, 18, 23].iter().all(|&index| bytes[index] == b'-')
        || bytes.iter().enumerate().any(|(index, byte)| {
            if [8, 13, 18, 23].contains(&index) {
                *byte != b'-'
            } else {
                !byte.is_ascii_hexdigit()
            }
        })
    {
        return None;
    }
    Some(source_id.to_ascii_lowercase())
}

fn tool_for_kind(kind: GrammarHealthSubjectKind) -> &'static str {
    match kind {
        GrammarHealthSubjectKind::Table => "phonologicalFeaturesAdvancedEdit",
        GrammarHealthSubjectKind::CharDef => "phonemeEdit",
        GrammarHealthSubjectKind::LexEntry => "lexiconEdit",
        GrammarHealthSubjectKind::MorphRule => "lexiconEdit",
    }
}

pub(crate) fn subject_link(kind: GrammarHealthSubjectKind, source_id: &str) -> FieldWorksLink {
    FieldWorksLink {
        guid: canonical_guid(source_id),
        tool: tool_for_kind(kind).to_string(),
        url: None,
        url_unavailable: None,
    }
}

pub(crate) fn morph_rule_link(
    grammar: &Grammar,
    id: MRuleId,
    source_id: &str,
) -> FieldWorksLink {
    let tool = match &grammar.mrules[id.0 as usize] {
        MorphRuleDef::Compounding(_) => "compoundRuleAdvancedEdit",
        MorphRuleDef::AffixProcess(_) | MorphRuleDef::Realizational(_) => "lexiconEdit",
    };
    FieldWorksLink {
        guid: canonical_guid(source_id),
        tool: tool.to_string(),
        url: None,
        url_unavailable: None,
    }
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

fn prepared_link(link: &FieldWorksLink, fieldworks_project: Option<&str>) -> FieldWorksLink {
    let project = fieldworks_project
        .map(str::trim)
        .filter(|project| !project.is_empty());
    let (url, url_unavailable) = match (project, link.guid.as_deref()) {
        (None, _) => (None, Some("no FieldWorks project name supplied".to_string())),
        (Some(_), None) => (None, Some("source item has no FieldWorks GUID".to_string())),
        (Some(project), Some(guid)) => (
            Some({
                let query = format!(
                    "database={project}&tool={}&guid={guid}&tag=",
                    link.tool
                );
                format!("silfw://localhost/link?{}", encode_query(&query))
            }),
            None,
        ),
    };
    FieldWorksLink {
        guid: link.guid.clone(),
        tool: link.tool.clone(),
        url,
        url_unavailable,
    }
}

pub(crate) fn prepare_report(
    findings: &mut [GrammarHealthCheckFinding],
    fieldworks_project: Option<&str>,
) {
    for finding in findings {
        for subject in &mut finding.subjects {
            subject.fieldworks = prepared_link(&subject.fieldworks, fieldworks_project);
        }
    }
}
