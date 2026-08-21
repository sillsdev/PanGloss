//! Stable, structured advice for compiler-observed backend compatibility shapes.
//!
//! The catalog is deliberately generic: it describes compiler evidence and conditional
//! transformations, never a language-specific recommendation.  The embedded TOML is parsed at
//! runtime using the small, strict subset needed by this versioned resource.  Keeping the loader
//! here avoids adding a parser dependency to the compiler crate while still making malformed
//! catalog changes fail closed.

use std::collections::BTreeSet;
use std::fmt;

use serde::{Deserialize, Serialize};

/// The current wire/schema version of the embedded advice catalog.
pub const ADVICE_CATALOG_SCHEMA_VERSION: u32 = 1;

/// Safety warning appended to every rendered remedy group.
pub const GRAMMAR_SAFETY_WARNING: &str =
    "Don't make any change that would make your language invalid!";

/// A complete validated catalog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdviceCatalog {
    pub schema_version: u32,
    pub entries: Vec<AdviceEntry>,
}

/// Compatibility alias for callers that prefer the shorter name.
pub type Catalog = AdviceCatalog;

/// One compiler-observed grammar shape and the advice applicable to it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdviceEntry {
    pub shape_key: String,
    pub backend_id: String,
    pub route: String,
    pub failed_predicate: String,
    pub evidence_refs: Vec<EvidenceReference>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub equivalence_caveat: Option<String>,
    pub remedies: Vec<Remedy>,
}

/// Compatibility alias for callers that use the catalog vocabulary.
pub type CatalogEntry = AdviceEntry;

/// One typed reference to compiler evidence supporting a finding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceReference {
    pub kind: EvidenceKind,
    pub value: String,
}

/// The compiler evidence categories accepted by schema v1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    Backend,
    Route,
    Predicate,
    Rule,
    Template,
    Slot,
    Stratum,
    Cycle,
    Witness,
    Metric,
}

impl EvidenceKind {
    fn parse(value: &str) -> Result<Self, CatalogError> {
        match value {
            "backend" => Ok(Self::Backend),
            "route" => Ok(Self::Route),
            "predicate" => Ok(Self::Predicate),
            "rule" | "rule_id" | "rule_ids" => Ok(Self::Rule),
            "template" | "template_id" | "template_ids" => Ok(Self::Template),
            "slot" | "slot_id" | "slot_ids" => Ok(Self::Slot),
            "stratum" | "stratum_id" | "stratum_ids" => Ok(Self::Stratum),
            "cycle" | "cycle_kind" => Ok(Self::Cycle),
            "witness" | "witness_id" | "witness_ids" => Ok(Self::Witness),
            "metric" | "factor" => Ok(Self::Metric),
            _ => Err(CatalogError::InvalidValue {
                field: "evidence_refs".to_string(),
                value: value.to_string(),
                detail: "unknown evidence kind".to_string(),
            }),
        }
    }
}

/// Estimated effort for one remedy applied to one shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum RemedyEffort {
    Easy,
    Medium,
    Hard,
}

impl RemedyEffort {
    fn parse(value: &str) -> Result<Self, CatalogError> {
        match value.to_ascii_lowercase().as_str() {
            "easy" => Ok(Self::Easy),
            "medium" => Ok(Self::Medium),
            "hard" => Ok(Self::Hard),
            _ => Err(CatalogError::InvalidValue {
                field: "effort".to_string(),
                value: value.to_string(),
                detail: "expected easy, medium, or hard".to_string(),
            }),
        }
    }
}

/// One conditional remedy.  Effort belongs here, rather than on the shared remedy key, because
/// the same remedy can have a different cost for different observed shapes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Remedy {
    pub rank: u32,
    pub remedy_key: String,
    pub description: String,
    pub effort: RemedyEffort,
    #[serde(default)]
    pub prerequisites: Vec<String>,
    #[serde(default)]
    pub contraindications: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub equivalence_caveat: Option<String>,
}

/// Compatibility alias for callers that call these entries advice items.
pub type RemedyAdvice = Remedy;

/// Errors returned when a catalog cannot be parsed or validated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogError {
    Parse { line: usize, detail: String },
    MissingField { context: String, field: String },
    DuplicateShapeKey(String),
    DuplicateRemedyShapePair { shape_key: String, remedy_key: String },
    UnsupportedSchemaVersion(u32),
    InvalidValue {
        field: String,
        value: String,
        detail: String,
    },
    NondeterministicOrder { context: String },
}

impl fmt::Display for CatalogError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse { line, detail } => write!(formatter, "line {line}: {detail}"),
            Self::MissingField { context, field } => {
                write!(formatter, "{context} is missing required field {field}")
            }
            Self::DuplicateShapeKey(key) => write!(formatter, "duplicate shape key {key:?}"),
            Self::DuplicateRemedyShapePair {
                shape_key,
                remedy_key,
            } => write!(
                formatter,
                "duplicate remedy {remedy_key:?} for shape {shape_key:?}"
            ),
            Self::UnsupportedSchemaVersion(version) => {
                write!(formatter, "unsupported advice catalog schema version {version}")
            }
            Self::InvalidValue {
                field,
                value,
                detail,
            } => write!(formatter, "invalid {field} value {value:?}: {detail}"),
            Self::NondeterministicOrder { context } => {
                write!(formatter, "{context} are not in deterministic order")
            }
        }
    }
}

impl std::error::Error for CatalogError {}

/// Parse and validate a schema-v1 TOML catalog.
pub fn parse_catalog(source: &str) -> Result<AdviceCatalog, CatalogError> {
    let mut schema_version = None;
    let mut entries = Vec::new();
    let mut entry: Option<AdviceEntry> = None;
    let mut remedy: Option<Remedy> = None;
    let mut section = Section::Root;

    for (line_number, raw_line) in source.lines().enumerate() {
        let line_number = line_number + 1;
        let line = strip_comment(raw_line).trim();
        if line.is_empty() {
            continue;
        }
        if line == "[[entry]]" || line == "[[entries]]" {
            finish_remedy(&mut entry, &mut remedy, line_number)?;
            finish_entry(&mut entries, &mut entry, line_number)?;
            entry = Some(AdviceEntry {
                shape_key: String::new(),
                backend_id: String::new(),
                route: String::new(),
                failed_predicate: String::new(),
                evidence_refs: Vec::new(),
                equivalence_caveat: None,
                remedies: Vec::new(),
            });
            section = Section::Entry;
            continue;
        }
        if line == "[[entry.remedy]]"
            || line == "[[entry.remedies]]"
            || line == "[[entries.remedy]]"
            || line == "[[entries.remedies]]"
        {
            if entry.is_none() {
                return Err(CatalogError::Parse {
                    line: line_number,
                    detail: "a remedy must follow an entry table".to_string(),
                });
            }
            finish_remedy(&mut entry, &mut remedy, line_number)?;
            remedy = Some(Remedy {
                rank: 0,
                remedy_key: String::new(),
                description: String::new(),
                effort: RemedyEffort::Easy,
                prerequisites: Vec::new(),
                contraindications: Vec::new(),
                equivalence_caveat: None,
            });
            section = Section::Remedy;
            continue;
        }

        let (key, value) = line.split_once('=').ok_or_else(|| CatalogError::Parse {
            line: line_number,
            detail: "expected key = value or an entry/remedy table".to_string(),
        })?;
        let key = key.trim();
        if key.is_empty() {
            return Err(CatalogError::Parse {
                line: line_number,
                detail: "empty key".to_string(),
            });
        }
        let value = value.trim();
        match section {
            Section::Root => {
                if key != "schema_version" {
                    return Err(unknown_key(line_number, key));
                }
                if schema_version.is_some() {
                    return Err(CatalogError::Parse {
                        line: line_number,
                        detail: "schema_version specified more than once".to_string(),
                    });
                }
                schema_version = Some(parse_u32(value, line_number, key)?);
            }
            Section::Entry => {
                let current = entry.as_mut().expect("entry section has an entry");
                parse_entry_field(current, key, value, line_number)?;
            }
            Section::Remedy => {
                let current = remedy.as_mut().expect("remedy section has a remedy");
                parse_remedy_field(current, key, value, line_number)?;
            }
        }
    }

    finish_remedy(&mut entry, &mut remedy, source.lines().count() + 1)?;
    finish_entry(&mut entries, &mut entry, source.lines().count() + 1)?;
    let schema_version = schema_version.ok_or_else(|| CatalogError::MissingField {
        context: "catalog".to_string(),
        field: "schema_version".to_string(),
    })?;
    let catalog = AdviceCatalog {
        schema_version,
        entries,
    };
    validate_catalog(&catalog)?;
    Ok(catalog)
}

impl AdviceCatalog {
    /// Parse and validate a TOML catalog.
    pub fn from_toml(source: &str) -> Result<Self, CatalogError> {
        parse_catalog(source)
    }

    /// Validate a catalog assembled by a caller rather than parsed from TOML.
    pub fn validate(&self) -> Result<(), CatalogError> {
        validate_catalog(self)
    }
}

/// Validate schema version, required fields, uniqueness, and canonical ordering.
pub fn validate_catalog(catalog: &AdviceCatalog) -> Result<(), CatalogError> {
    if catalog.schema_version != ADVICE_CATALOG_SCHEMA_VERSION {
        return Err(CatalogError::UnsupportedSchemaVersion(catalog.schema_version));
    }
    let mut shape_keys = BTreeSet::new();
    for (index, entry) in catalog.entries.iter().enumerate() {
        let context = format!("entry {index}");
        required(&context, "shape_key", &entry.shape_key)?;
        required(&context, "backend_id", &entry.backend_id)?;
        required(&context, "route", &entry.route)?;
        required(&context, "failed_predicate", &entry.failed_predicate)?;
        if entry.evidence_refs.is_empty() {
            return Err(CatalogError::MissingField {
                context,
                field: "evidence_refs".to_string(),
            });
        }
        if entry.remedies.is_empty() {
            return Err(CatalogError::MissingField {
                context: format!("entry {}", entry.shape_key),
                field: "remedies".to_string(),
            });
        }
        if !shape_keys.insert(entry.shape_key.as_str()) {
            return Err(CatalogError::DuplicateShapeKey(entry.shape_key.clone()));
        }
        for evidence in &entry.evidence_refs {
            if evidence.value.trim().is_empty() {
                return Err(CatalogError::MissingField {
                    context: format!("entry {} evidence", entry.shape_key),
                    field: "value".to_string(),
                });
            }
        }
        let mut remedy_keys = BTreeSet::new();
        for remedy in &entry.remedies {
            let context = format!("remedy in entry {}", entry.shape_key);
            required(&context, "remedy_key", &remedy.remedy_key)?;
            required(&context, "description", &remedy.description)?;
            if remedy.rank == 0 {
                return Err(CatalogError::MissingField {
                    context,
                    field: "rank".to_string(),
                });
            }
            if !remedy_keys.insert(remedy.remedy_key.as_str()) {
                return Err(CatalogError::DuplicateRemedyShapePair {
                    shape_key: entry.shape_key.clone(),
                    remedy_key: remedy.remedy_key.clone(),
                });
            }
        }
    }
    if catalog
        .entries
        .windows(2)
        .any(|pair| pair[0].shape_key >= pair[1].shape_key)
    {
        return Err(CatalogError::NondeterministicOrder {
            context: "catalog entries".to_string(),
        });
    }
    for entry in &catalog.entries {
        if entry.remedies.windows(2).any(|pair| {
            (pair[0].rank, pair[0].remedy_key.as_str())
                >= (pair[1].rank, pair[1].remedy_key.as_str())
        }) {
            return Err(CatalogError::NondeterministicOrder {
                context: format!("remedies for {}", entry.shape_key),
            });
        }
    }
    Ok(())
}

/// Load the deterministic built-in schema-v1 catalog.
pub fn builtin_catalog() -> Result<AdviceCatalog, CatalogError> {
    parse_catalog(include_str!("../assets/backend-advice-v1.toml"))
}

/// Render one entry's remedies as a conditional backend-specific advice group.
pub fn render_remedy_group(entry: &AdviceEntry) -> String {
    let mut rendered = format!(
        "{} ({}) — {} evidence: {}.\n",
        entry.shape_key,
        entry.backend_id,
        entry.failed_predicate,
        entry
            .evidence_refs
            .iter()
            .map(|reference| format!("{}={}", reference.kind.as_str(), reference.value))
            .collect::<Vec<_>>()
            .join(", ")
    );
    for remedy in &entry.remedies {
        rendered.push_str(&format!(
            "- {} [{}]: {} If its prerequisites hold, this change would make this backend work for your language.\n",
            remedy.remedy_key,
            remedy.effort.as_str(),
            remedy.description
        ));
        if !remedy.prerequisites.is_empty() {
            rendered.push_str(&format!(
                "  Prerequisites: {}.\n",
                remedy.prerequisites.join(", ")
            ));
        }
        if !remedy.contraindications.is_empty() {
            rendered.push_str(&format!(
                "  Contraindications: {}.\n",
                remedy.contraindications.join(", ")
            ));
        }
        if let Some(caveat) = remedy.equivalence_caveat.as_deref() {
            rendered.push_str(&format!("  Equivalence caveat: {caveat}\n"));
        }
    }
    if let Some(caveat) = entry.equivalence_caveat.as_deref() {
        rendered.push_str(&format!("Equivalence caveat: {caveat}\n"));
    }
    rendered.push_str(GRAMMAR_SAFETY_WARNING);
    rendered
}

impl EvidenceKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Backend => "backend",
            Self::Route => "route",
            Self::Predicate => "predicate",
            Self::Rule => "rule",
            Self::Template => "template",
            Self::Slot => "slot",
            Self::Stratum => "stratum",
            Self::Cycle => "cycle",
            Self::Witness => "witness",
            Self::Metric => "metric",
        }
    }
}

impl RemedyEffort {
    fn as_str(self) -> &'static str {
        match self {
            Self::Easy => "easy",
            Self::Medium => "medium",
            Self::Hard => "hard",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    Root,
    Entry,
    Remedy,
}

fn finish_remedy(
    entry: &mut Option<AdviceEntry>,
    remedy: &mut Option<Remedy>,
    line: usize,
) -> Result<(), CatalogError> {
    let Some(remedy) = remedy.take() else {
        return Ok(());
    };
    let Some(entry) = entry.as_mut() else {
        return Err(CatalogError::Parse {
            line,
            detail: "remedy has no containing entry".to_string(),
        });
    };
    entry.remedies.push(remedy);
    Ok(())
}

fn finish_entry(
    entries: &mut Vec<AdviceEntry>,
    entry: &mut Option<AdviceEntry>,
    _line: usize,
) -> Result<(), CatalogError> {
    if let Some(entry) = entry.take() {
        entries.push(entry);
    }
    Ok(())
}

fn parse_entry_field(
    entry: &mut AdviceEntry,
    key: &str,
    value: &str,
    line: usize,
) -> Result<(), CatalogError> {
    match key {
        "shape_key" | "key" => entry.shape_key = parse_string(value, line, key)?,
        "backend_id" | "backend" => entry.backend_id = parse_string(value, line, key)?,
        "route" => entry.route = parse_string(value, line, key)?,
        "failed_predicate" | "predicate" => {
            entry.failed_predicate = parse_string(value, line, key)?
        }
        "evidence_refs" | "required_evidence" => {
            entry.evidence_refs = parse_evidence_refs(value, line)?
        }
        "equivalence_caveat" | "caveat" => {
            entry.equivalence_caveat = Some(parse_string(value, line, key)?)
        }
        _ => return Err(unknown_key(line, key)),
    }
    Ok(())
}

fn parse_remedy_field(
    remedy: &mut Remedy,
    key: &str,
    value: &str,
    line: usize,
) -> Result<(), CatalogError> {
    match key {
        "rank" => remedy.rank = parse_u32(value, line, key)?,
        "remedy_key" | "key" => remedy.remedy_key = parse_string(value, line, key)?,
        "description" | "text" => remedy.description = parse_string(value, line, key)?,
        "effort" => remedy.effort = RemedyEffort::parse(&parse_string(value, line, key)?)?,
        "prerequisites" | "requires" => remedy.prerequisites = parse_string_array(value, line)?,
        "contraindications" | "contraindicated_when" => {
            remedy.contraindications = parse_string_array(value, line)?
        }
        "equivalence_caveat" | "caveat" => {
            remedy.equivalence_caveat = Some(parse_string(value, line, key)?)
        }
        _ => return Err(unknown_key(line, key)),
    }
    Ok(())
}

fn parse_evidence_refs(value: &str, line: usize) -> Result<Vec<EvidenceReference>, CatalogError> {
    parse_string_array(value, line)?
        .into_iter()
        .map(|item| {
            let (kind, evidence) = item.split_once(':').ok_or_else(|| CatalogError::Parse {
                line,
                detail: "evidence references must use kind:value syntax".to_string(),
            })?;
            let evidence = evidence.trim();
            if evidence.is_empty() {
                return Err(CatalogError::Parse {
                    line,
                    detail: "evidence reference value must not be empty".to_string(),
                });
            }
            Ok(EvidenceReference {
                kind: EvidenceKind::parse(kind.trim())?,
                value: evidence.to_string(),
            })
        })
        .collect()
}

fn required(context: &str, field: &str, value: &str) -> Result<(), CatalogError> {
    if value.trim().is_empty() {
        Err(CatalogError::MissingField {
            context: context.to_string(),
            field: field.to_string(),
        })
    } else {
        Ok(())
    }
}

fn unknown_key(line: usize, key: &str) -> CatalogError {
    CatalogError::Parse {
        line,
        detail: format!("unknown key {key:?}"),
    }
}

fn parse_u32(value: &str, line: usize, field: &str) -> Result<u32, CatalogError> {
    let _ = line;
    value
        .trim()
        .parse()
        .map_err(|_| CatalogError::InvalidValue {
            field: field.to_string(),
            value: value.to_string(),
            detail: "expected an unsigned integer".to_string(),
        })
}

fn parse_string(value: &str, line: usize, field: &str) -> Result<String, CatalogError> {
    let value = value.trim();
    if value.len() < 2 || !value.starts_with('"') || !value.ends_with('"') {
        return Err(CatalogError::Parse {
            line,
            detail: format!("{field} must be a basic TOML string"),
        });
    }
    let mut result = String::new();
    let mut chars = value[1..value.len() - 1].chars();
    while let Some(character) = chars.next() {
        if character != '\\' {
            result.push(character);
            continue;
        }
        let escaped = chars.next().ok_or_else(|| CatalogError::Parse {
            line,
            detail: format!("unterminated escape in {field}"),
        })?;
        result.push(match escaped {
            '"' => '"',
            '\\' => '\\',
            'n' => '\n',
            'r' => '\r',
            't' => '\t',
            _ => {
                return Err(CatalogError::Parse {
                    line,
                    detail: format!("unsupported escape in {field}"),
                })
            }
        });
    }
    Ok(result)
}

fn parse_string_array(value: &str, line: usize) -> Result<Vec<String>, CatalogError> {
    let value = value.trim();
    if value.len() < 2 || !value.starts_with('[') || !value.ends_with(']') {
        return Err(CatalogError::Parse {
            line,
            detail: "expected an array of basic TOML strings".to_string(),
        });
    }
    let body = &value[1..value.len() - 1];
    split_array_items(body, line)?
        .into_iter()
        .map(|item| parse_string(item, line, "array item"))
        .collect()
}

fn split_array_items(body: &str, line: usize) -> Result<Vec<&str>, CatalogError> {
    let mut items = Vec::new();
    let mut start = 0;
    let mut in_string = false;
    let mut escaped = false;
    for (index, character) in body.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if in_string && character == '\\' {
            escaped = true;
            continue;
        }
        if character == '"' {
            in_string = !in_string;
        } else if character == ',' && !in_string {
            let item = body[start..index].trim();
            if item.is_empty() {
                return Err(CatalogError::Parse {
                    line,
                    detail: "empty array item".to_string(),
                });
            }
            items.push(item);
            start = index + character.len_utf8();
        }
    }
    if in_string || escaped {
        return Err(CatalogError::Parse {
            line,
            detail: "unterminated string in array".to_string(),
        });
    }
    let item = body[start..].trim();
    if !item.is_empty() {
        items.push(item);
    }
    Ok(items)
}

fn strip_comment(line: &str) -> &str {
    let mut in_string = false;
    let mut escaped = false;
    for (index, character) in line.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if in_string && character == '\\' {
            escaped = true;
            continue;
        }
        if character == '"' {
            in_string = !in_string;
        } else if character == '#' && !in_string {
            return &line[..index];
        }
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> AdviceCatalog {
        AdviceCatalog {
            schema_version: 1,
            entries: vec![AdviceEntry {
                shape_key: "a-shape".to_string(),
                backend_id: "foma".to_string(),
                route: "route".to_string(),
                failed_predicate: "predicate".to_string(),
                evidence_refs: vec![EvidenceReference {
                    kind: EvidenceKind::Witness,
                    value: "witness-1".to_string(),
                }],
                equivalence_caveat: None,
                remedies: vec![Remedy {
                    rank: 1,
                    remedy_key: "shared".to_string(),
                    description: "test remedy".to_string(),
                    effort: RemedyEffort::Easy,
                    prerequisites: vec!["proof".to_string()],
                    contraindications: vec!["not-proven".to_string()],
                    equivalence_caveat: Some("review required".to_string()),
                }],
            }],
        }
    }

    #[test]
    fn validates_duplicate_shape_and_remedy_pairs() {
        let mut catalog = sample();
        catalog.entries.push(catalog.entries[0].clone());
        assert!(matches!(
            validate_catalog(&catalog),
            Err(CatalogError::DuplicateShapeKey(_))
        ));
        catalog.entries.pop();
        let duplicate = catalog.entries[0].remedies[0].clone();
        catalog.entries[0].remedies.push(duplicate);
        assert!(matches!(
            validate_catalog(&catalog),
            Err(CatalogError::DuplicateRemedyShapePair { .. })
        ));
    }

    #[test]
    fn rejects_unsupported_versions_and_unordered_entries() {
        let mut catalog = sample();
        catalog.schema_version = 2;
        assert!(matches!(
            validate_catalog(&catalog),
            Err(CatalogError::UnsupportedSchemaVersion(2))
        ));
        catalog.schema_version = 1;
        catalog.entries.push(AdviceEntry {
            shape_key: "0-shape".to_string(),
            ..catalog.entries[0].clone()
        });
        assert!(matches!(
            validate_catalog(&catalog),
            Err(CatalogError::NondeterministicOrder { .. })
        ));
    }

    #[test]
    fn parser_requires_typed_evidence_and_renders_warning() {
        let source = r#"
schema_version = 1
[[entry]]
shape_key = "a-shape"
backend_id = "foma"
route = "route"
failed_predicate = "predicate"
evidence_refs = ["witness:w1"]
[[entry.remedy]]
rank = 1
remedy_key = "shared"
description = "test remedy"
effort = "easy"
prerequisites = ["proof"]
contraindications = ["not-proven"]
equivalence_caveat = "review"
"#;
        let catalog = parse_catalog(source).expect("sample catalog parses");
        let rendered = render_remedy_group(&catalog.entries[0]);
        assert!(rendered.contains("would make this backend work for your language"));
        assert!(rendered.contains(GRAMMAR_SAFETY_WARNING));
    }
}
