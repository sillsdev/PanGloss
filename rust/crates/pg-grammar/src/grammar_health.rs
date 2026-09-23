//! The `hc-*` grammar-authoring health checks, ported from C#
//! `SIL.Machine.Morphology.HermitCrab.GrammarHealthChecker`/`GrammarHealthCheckFinding`
//! (`sillsdev/machine` PR 475, branch `feature/grammar-health-checker`). Diagnostic only: running
//! these checks never refuses or changes how a [`Grammar`] compiles or parses. If the model cannot
//! provide a canonical diagnostic fact, the inspection error is returned to the caller.
//!
//! This answers a *different* question from `pg_health::health`'s FST-compilation vocabulary
//! (`Severity`/`FindingCode`/`FindingClass`): that module asks "can this grammar be compiled to an
//! FST and published" (gated on `FindingClass`: Representability/Readiness/Containment/Process).
//! These checks ask "is this grammar well-formed for its author" -- an authoring-correctness
//! question with no publication-blocking tier and no representability/containment axis, so a
//! separate, smaller vocabulary lives here rather than contorting the FST schema's severity/class
//! pair to fit it. Wire codes are the stable C# strings (`hc-undeclared-segment`,
//! `hc-duplicate-feature-bundle`, `hc-partial-morpheme`) so the two implementations' output can be
//! compared directly.
//!
//! Lives in `pg-grammar`, not `pg-health`, because the checks read [`Grammar`] directly.
//! `pg-health` is a leaf crate whose only dependencies are `serde`/`serde_json`, kept that way so
//! it builds for `wasm32-unknown-unknown` with no compiler/model machinery in its graph
//! (`pg-wasm/tests/wasm_excludes_compiler.rs`); adding a `pg-grammar` dependency there would be a
//! new, unnecessary edge. `pg-grammar` is already an unconditional dependency of `pg-wasm` (see
//! that crate's `Cargo.toml`), so this module adds no new wasm32 exposure of its own.
//!
//! # What is NOT ported
//! C#'s `CheckUndeclaredSegments` also flags a `SegmentNaturalClass` member whose
//! `CharacterDefinition.CharacterDefinitionTable` is not one of the language's declared tables.
//! [`crate::model::NaturalClassKind::Segments`] stores only a per-table
//! [`crate::chardef::CharDefId`] with no owning-table reference, and `load.rs`'s natural-class pass
//! resolves every `<Segment segment="...">` reference through an index built ONLY from
//! already-declared tables -- so a member naming an undeclared table cannot exist once a grammar
//! has loaded, and the model carries no field this check could read even by hand construction.
//! Porting it would require adding a table reference to every `Segments` member, a model-shape
//! change out of scope for this port.

use crate::chardef::{CharDef, CharDefId, CharDefKind, CharDefTable};
use crate::grammar_health_presentation::{fieldworks_link, FieldWorksSource};
use crate::model::{
    Grammar, LexEntryId, MRuleId, MorphRuleDef, OutputAction, PartialMorphemeFacts,
    PartialMorphemeIdentity, TableId,
};
use pg_shape::{NodeKind, Shape, NO_CHAR_DEF};

/// How serious a [`GrammarHealthCheckFinding`] is. Mirrors C# `GrammarHealthSeverity`: `Error` means
/// the engine behaves incorrectly (or refuses the word outright) whenever the offending construct
/// is exercised; `Warning` means the construct is a genuine reliability risk whose actual impact
/// depends on how the grammar's rules use it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrammarHealthSeverity {
    Warning,
    Error,
}

impl GrammarHealthSeverity {
    pub const fn wire(self) -> &'static str {
        match self {
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }
}

/// The stable finding codes this module reports (C# `GrammarHealthCodes`). Treat
/// [`GrammarHealthCode::wire`], not [`GrammarHealthCheckFinding::message`], as the identifier a host
/// filters/suppresses/tests on -- the message text is free to change. Serializes as the bare wire
/// string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum GrammarHealthCode {
    #[serde(rename = "hc-undeclared-segment")]
    UndeclaredSegment,
    #[serde(rename = "hc-duplicate-feature-bundle")]
    DuplicateFeatureBundle,
    #[serde(rename = "hc-partial-morpheme")]
    PartialMorpheme,
}

impl GrammarHealthCode {
    /// Every grammar-health code in the stable contract.
    pub const ALL: &'static [Self] = &[
        Self::UndeclaredSegment,
        Self::DuplicateFeatureBundle,
        Self::PartialMorpheme,
    ];

    /// The stable C# wire string this code shares with `GrammarHealthCodes`.
    pub const fn wire(self) -> &'static str {
        match self {
            Self::UndeclaredSegment => "hc-undeclared-segment",
            Self::DuplicateFeatureBundle => "hc-duplicate-feature-bundle",
            Self::PartialMorpheme => "hc-partial-morpheme",
        }
    }

    /// Stable plain-language sidebar/log grouping label. Keep these short and linguist-facing.
    pub const fn group_name(self) -> &'static str {
        match self {
            Self::UndeclaredSegment => "Missing segment definition",
            Self::DuplicateFeatureBundle => "Duplicate segment features",
            Self::PartialMorpheme => "Partial morpheme analysis",
        }
    }
}

/// The subject kind used by every grammar-health renderer. This is deliberately separate from
/// the finding code: a finding can name several kinds of model object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrammarHealthSubjectKind {
    Table,
    CharDef,
    LexEntry,
    MorphRule,
}

impl GrammarHealthSubjectKind {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Table => "character-definition table",
            Self::CharDef => "character definition",
            Self::LexEntry => "lexical entry",
            Self::MorphRule => "morphological rule",
        }
    }
}

/// Why a subject cannot be opened in FieldWorks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldWorksUnavailableReason {
    MissingProject,
    MissingGuid,
    InvalidGuid,
    UnverifiedGuidKind,
    UnverifiedTool,
}

impl FieldWorksUnavailableReason {
    pub const fn message(self) -> &'static str {
        match self {
            Self::MissingProject => "no FieldWorks project name supplied",
            Self::MissingGuid => "source item has no FieldWorks GUID",
            Self::InvalidGuid => "source item has an invalid FieldWorks GUID",
            Self::UnverifiedGuidKind => {
                "source item GUID kind is not proven to navigate to its owning record"
            }
            Self::UnverifiedTool => "source item has no verified FieldWorks tool",
        }
    }
}

/// Complete FieldWorks navigation state, resolved once while the subject is built.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum FieldWorksLink {
    Available {
        guid: String,
        tool: String,
        url: String,
    },
    Unavailable {
        reason: FieldWorksUnavailableReason,
        guid: Option<String>,
    },
}

/// One structured item a grammar-health finding references. `title` is the only identity a human
/// report should display; `internal_id` is retained solely for tooling and navigation joins.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GrammarHealthSubject {
    pub kind: GrammarHealthSubjectKind,
    pub title: String,
    pub subtitle: Option<String>,
    pub internal_id: String,
    pub fieldworks: FieldWorksLink,
}

fn is_internal_subject_label(title: &str) -> bool {
    let lower = title.trim().to_ascii_lowercase();
    if crate::grammar_health_presentation::canonical_guid(&lower).is_some() {
        return true;
    }
    if let Some((prefix, suffix)) = lower.split_once('#') {
        return [
            "char_def",
            "entry",
            "lex_entry",
            "mrule",
            "morph_rule",
            "rule",
            "slot",
            "table",
            "template",
        ]
        .contains(&prefix)
            && !suffix.trim().is_empty();
    }
    ["entry", "mrule", "rule", "slot", "template"]
        .iter()
        .any(|prefix| {
            lower.strip_prefix(prefix).is_some_and(|rest| {
                !rest.is_empty() && rest.chars().all(|character| character.is_ascii_digit())
            })
        })
}

impl GrammarHealthSubject {
    fn render_location(&self, include_guid: bool) -> String {
        let mut text = self.title.clone();
        if let Some(subtitle) = self.subtitle.as_deref().filter(|text| !text.is_empty()) {
            text.push_str(" (");
            text.push_str(subtitle);
            text.push(')');
        }
        if include_guid {
            match &self.fieldworks {
                FieldWorksLink::Available { guid, .. } => {
                    text.push_str(" [guid ");
                    text.push_str(guid);
                    text.push(']');
                }
                FieldWorksLink::Unavailable { guid, .. } => match guid {
                    Some(guid) => {
                        text.push_str(" [guid ");
                        text.push_str(guid);
                        text.push(']');
                    }
                    None => text.push_str(" [guid unavailable]"),
                },
            }
        }
        match &self.fieldworks {
            FieldWorksLink::Available { url, .. } => {
                text.push_str(" [");
                text.push_str(url);
                text.push(']');
            }
            FieldWorksLink::Unavailable { reason, .. } => {
                text.push_str(" [FieldWorks link unavailable: ");
                text.push_str(reason.message());
                text.push(']');
            }
        }
        text
    }
}

/// One problem found by check_grammar_health. JSON uses `problem` for the human message while
/// the Rust field remains `message` for the diagnostic API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrammarHealthCheckFinding {
    pub severity: GrammarHealthSeverity,
    pub code: GrammarHealthCode,
    pub message: String,
    pub subjects: Vec<GrammarHealthSubject>,
}

impl serde::Serialize for GrammarHealthCheckFinding {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        #[derive(serde::Serialize)]
        struct Wire<'a> {
            severity: GrammarHealthSeverity,
            code: GrammarHealthCode,
            group_name: &'static str,
            #[serde(rename = "problem")]
            message: &'a str,
            subjects: &'a [GrammarHealthSubject],
        }

        Wire {
            severity: self.severity,
            code: self.code,
            group_name: self.code.group_name(),
            message: &self.message,
            subjects: &self.subjects,
        }
        .serialize(serializer)
    }
}

impl GrammarHealthCheckFinding {
    fn log_line(&self, include_guids: bool) -> String {
        let kinds = self
            .subjects
            .iter()
            .map(|subject| subject.kind.label())
            .collect::<Vec<_>>();
        let titles = self
            .subjects
            .iter()
            .map(|subject| subject.render_location(include_guids))
            .collect::<Vec<_>>();
        format!(
            "{} [{}] {}: {} - {}",
            self.severity.wire(),
            self.code.group_name(),
            kinds.join(", "),
            titles.join(", "),
            self.message.trim()
        )
    }
}

pub const GRAMMAR_HEALTH_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrammarHealthReportErrorCode {
    MalformedJson,
    InvalidShape,
    MissingField,
    InvalidField,
    UnsupportedSchemaVersion,
    MissingSubjects,
    InvalidFinding,
    Serialization,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrammarHealthReportError {
    pub code: GrammarHealthReportErrorCode,
    pub finding_index: Option<usize>,
    pub field: Option<String>,
    pub message: String,
}

impl std::fmt::Display for GrammarHealthReportError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for GrammarHealthReportError {}

impl std::fmt::Display for GrammarHealthReportErrorCode {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Self::MalformedJson => "malformed_json",
            Self::InvalidShape => "invalid_shape",
            Self::MissingField => "missing_field",
            Self::InvalidField => "invalid_field",
            Self::UnsupportedSchemaVersion => "unsupported_schema_version",
            Self::MissingSubjects => "missing_subjects",
            Self::InvalidFinding => "invalid_finding",
            Self::Serialization => "serialization",
        };
        formatter.write_str(name)
    }
}

/// The sole validated in-memory grammar-health report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrammarHealthReport {
    findings: Vec<GrammarHealthCheckFinding>,
}

impl GrammarHealthReport {
    pub fn new(findings: Vec<GrammarHealthCheckFinding>) -> Result<Self, GrammarHealthReportError> {
        for (finding_index, finding) in findings.iter().enumerate() {
            validate_finding(finding, finding_index)?;
        }
        Ok(Self { findings })
    }

    pub fn findings(&self) -> &[GrammarHealthCheckFinding] {
        &self.findings
    }

    pub fn len(&self) -> usize {
        self.findings.len()
    }

    pub fn is_empty(&self) -> bool {
        self.findings.is_empty()
    }

    pub fn to_json(&self) -> Result<String, GrammarHealthReportError> {
        serde_json::to_string_pretty(self).map_err(|error| GrammarHealthReportError {
            code: GrammarHealthReportErrorCode::Serialization,
            finding_index: None,
            field: None,
            message: error.to_string(),
        })
    }

    pub fn from_json(json: &str) -> Result<Self, GrammarHealthReportError> {
        let value: serde_json::Value = serde_json::from_str(json).map_err(|error| {
            report_error(
                GrammarHealthReportErrorCode::MalformedJson,
                None,
                None,
                error.to_string(),
            )
        })?;
        let object = value.as_object().ok_or_else(|| {
            report_error(
                GrammarHealthReportErrorCode::InvalidShape,
                None,
                None,
                "grammar-health report must be an object".to_string(),
            )
        })?;
        let schema_version = required_field::<u32>(object, "schema_version", None)?;
        if schema_version != GRAMMAR_HEALTH_SCHEMA_VERSION {
            return Err(report_error(
                GrammarHealthReportErrorCode::UnsupportedSchemaVersion,
                None,
                Some("schema_version"),
                format!(
                    "unsupported grammar-health schema version {schema_version}; expected {GRAMMAR_HEALTH_SCHEMA_VERSION}"
                ),
            ));
        }
        let finding_values = required_field::<Vec<serde_json::Value>>(object, "findings", None)?;
        let mut findings = Vec::with_capacity(finding_values.len());
        for (finding_index, value) in finding_values.into_iter().enumerate() {
            findings.push(decode_finding(value, finding_index)?);
        }
        Self::new(findings)
    }
}

impl std::ops::Deref for GrammarHealthReport {
    type Target = [GrammarHealthCheckFinding];

    fn deref(&self) -> &Self::Target {
        self.findings()
    }
}

impl serde::Serialize for GrammarHealthReport {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        #[derive(serde::Serialize)]
        struct Wire<'a> {
            schema_version: u32,
            findings: &'a [GrammarHealthCheckFinding],
        }

        Wire {
            schema_version: GRAMMAR_HEALTH_SCHEMA_VERSION,
            findings: &self.findings,
        }
        .serialize(serializer)
    }
}

fn report_error(
    code: GrammarHealthReportErrorCode,
    finding_index: Option<usize>,
    field: Option<&str>,
    message: String,
) -> GrammarHealthReportError {
    GrammarHealthReportError {
        code,
        finding_index,
        field: field.map(str::to_string),
        message,
    }
}

fn required_field<T: serde::de::DeserializeOwned>(
    object: &serde_json::Map<String, serde_json::Value>,
    field: &str,
    finding_index: Option<usize>,
) -> Result<T, GrammarHealthReportError> {
    let value = object.get(field).ok_or_else(|| {
        report_error(
            GrammarHealthReportErrorCode::MissingField,
            finding_index,
            Some(field),
            format!("missing field {field}"),
        )
    })?;
    serde_json::from_value(value.clone()).map_err(|error| {
        report_error(
            GrammarHealthReportErrorCode::InvalidField,
            finding_index,
            Some(field),
            error.to_string(),
        )
    })
}

fn decode_finding(
    value: serde_json::Value,
    finding_index: usize,
) -> Result<GrammarHealthCheckFinding, GrammarHealthReportError> {
    let object = value.as_object().ok_or_else(|| {
        report_error(
            GrammarHealthReportErrorCode::InvalidShape,
            Some(finding_index),
            None,
            "finding must be an object".to_string(),
        )
    })?;
    let severity: GrammarHealthSeverity = required_field(object, "severity", Some(finding_index))?;
    let code: GrammarHealthCode = required_field(object, "code", Some(finding_index))?;
    let group_name: String = required_field(object, "group_name", Some(finding_index))?;
    if group_name != code.group_name() {
        return Err(report_error(
            GrammarHealthReportErrorCode::InvalidField,
            Some(finding_index),
            Some("group_name"),
            format!("group_name does not match code-owned label {}", code.wire()),
        ));
    }
    let message = required_field(object, "problem", Some(finding_index))?;
    let subjects = required_field(object, "subjects", Some(finding_index))?;
    Ok(GrammarHealthCheckFinding {
        severity,
        code,
        message,
        subjects,
    })
}

fn validate_finding(
    finding: &GrammarHealthCheckFinding,
    finding_index: usize,
) -> Result<(), GrammarHealthReportError> {
    let prefix = format!(
        "grammar-health finding {finding_index} ({})",
        finding.code.wire()
    );
    let missing = |field: &'static str, message: String| {
        Err(report_error(
            GrammarHealthReportErrorCode::InvalidFinding,
            Some(finding_index),
            Some(field),
            message,
        ))
    };
    if finding.message.trim().is_empty() {
        return missing("problem", format!("{prefix}: missing problem"));
    }
    if finding.subjects.is_empty() {
        return Err(report_error(
            GrammarHealthReportErrorCode::MissingSubjects,
            Some(finding_index),
            Some("subjects"),
            format!("{prefix}: missing subjects"),
        ));
    }
    for (subject_index, subject) in finding.subjects.iter().enumerate() {
        let subject_prefix = format!("{prefix} subject {subject_index}");
        if subject.title.trim().is_empty() {
            return missing("title", format!("{subject_prefix}: missing title"));
        }
        if subject.internal_id.trim().is_empty() {
            return missing(
                "internal_id",
                format!("{subject_prefix}: missing internal_id"),
            );
        }
        match &subject.fieldworks {
            FieldWorksLink::Available { guid, tool, url } => {
                if crate::grammar_health_presentation::canonical_guid(guid).is_none() {
                    return missing("fieldworks.guid", format!("{subject_prefix}: invalid GUID"));
                }
                if tool.trim().is_empty() {
                    return missing("fieldworks.tool", format!("{subject_prefix}: missing tool"));
                }
                if url.trim().is_empty() {
                    return missing("fieldworks.url", format!("{subject_prefix}: missing URL"));
                }
            }
            FieldWorksLink::Unavailable { .. } => {}
        }
        if is_internal_subject_label(&subject.title) {
            return missing(
                "title",
                format!("{subject_prefix}: title must be a human-readable subject name"),
            );
        }
    }
    Ok(())
}

/// Render complete findings as one nonblank plain-text line per finding.
///
/// include_guids is opt-in because the title remains the human identity in the default log.
pub fn render_log(report: &GrammarHealthReport, include_guids: bool) -> String {
    report
        .findings()
        .iter()
        .map(|finding| finding.log_line(include_guids))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Render the versioned grammar-health report.
pub fn render_json(report: &GrammarHealthReport) -> Result<String, GrammarHealthReportError> {
    report.to_json()
}

/// Runs every registered check against grammar. `fieldworks_project` is caller-supplied because
/// the project/database name is not part of the compiled grammar.
pub fn check_grammar_health(
    grammar: &Grammar,
    fieldworks_project: Option<&str>,
) -> Result<GrammarHealthReport, crate::GrammarError> {
    let partial_facts = grammar.partial_morpheme_facts()?;
    let mut findings = Vec::new();
    check_duplicate_feature_bundles(grammar, fieldworks_project, &mut findings)?;
    check_undeclared_segments(grammar, fieldworks_project, &mut findings)?;
    check_partial_morphemes(grammar, fieldworks_project, &partial_facts, &mut findings)?;
    let report = GrammarHealthReport::new(findings)
        .map_err(|error| crate::GrammarError::Semantic(error.to_string()))?;
    Ok(report)
}

// --- hc-duplicate-feature-bundle -------------------------------------------------------------

/// Distinct-bundle segments only; skipped for a zero-feature grammar, where every bundle is the same empty struct by construction.
fn check_duplicate_feature_bundles(
    grammar: &Grammar,
    fieldworks_project: Option<&str>,
    findings: &mut Vec<GrammarHealthCheckFinding>,
) -> Result<(), crate::GrammarError> {
    if grammar.phon_features.is_empty() {
        return Ok(());
    }
    for (table_idx, table) in grammar.char_tables.iter().enumerate() {
        let table_id = TableId(table_idx as u16);
        let mut segs: Vec<(CharDefId, &CharDef)> = table
            .iter()
            .filter(|(_, cd)| cd.kind() == CharDefKind::Segment)
            .collect();
        segs.sort_by(|(_, a), (_, b)| first_representation(a).cmp(first_representation(b)));

        let mut groups: Vec<Vec<(CharDefId, &CharDef)>> = Vec::new();
        for entry in segs {
            match groups
                .iter_mut()
                .find(|g| stripped_lanes(g[0].1) == stripped_lanes(entry.1))
            {
                Some(group) => group.push(entry),
                None => groups.push(vec![entry]),
            }
        }

        for group in groups {
            if group.len() < 2 {
                continue;
            }
            let names: Vec<&str> = group
                .iter()
                .map(|(_, cd)| first_representation(cd))
                .collect();
            let mut subjects = vec![table_subject(grammar, fieldworks_project, table_id, table)];
            subjects.extend(group.iter().map(|(id, cd)| {
                char_def_subject(grammar, fieldworks_project, table_id, *id, cd, table)
            }));
            findings.push(GrammarHealthCheckFinding {
                severity: GrammarHealthSeverity::Warning,
                code: GrammarHealthCode::DuplicateFeatureBundle,
                message: format!(
                    "Character definition table '{}' has {} segments with an identical \
                     phonological feature bundle, so a segment-changing rule cannot reliably \
                     tell them apart: {}.",
                    table_display_name(table),
                    group.len(),
                    names.join(", ")
                ),
                subjects,
            });
        }
    }
    Ok(())
}

// The `Type` lane is always appended last (`PhonFeatureSystem::from_raw`), so slicing it off is a bare truncation, not a search.
fn stripped_lanes(cd: &CharDef) -> &[u64] {
    let lanes = cd.feature_lanes();
    &lanes[..lanes.len() - 1]
}

fn first_representation(cd: &CharDef) -> &str {
    cd.representations()
        .iter()
        .map(String::as_str)
        .find(|representation| !representation.trim().is_empty())
        .unwrap_or("unnamed character definition")
}

fn table_display_name(table: &CharDefTable) -> &str {
    table
        .name()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or("unnamed character-definition table")
}

fn make_subject(
    kind: GrammarHealthSubjectKind,
    title: String,
    subtitle: Option<String>,
    internal_id: String,
    fieldworks: FieldWorksLink,
) -> GrammarHealthSubject {
    GrammarHealthSubject {
        kind,
        title,
        subtitle,
        internal_id,
        fieldworks,
    }
}

fn table_subject(
    grammar: &Grammar,
    fieldworks_project: Option<&str>,
    id: TableId,
    table: &CharDefTable,
) -> GrammarHealthSubject {
    make_subject(
        GrammarHealthSubjectKind::Table,
        table_display_name(table).to_string(),
        None,
        format!("table#{}:{}", id.0, table.xml_id()),
        fieldworks_link(grammar, FieldWorksSource::Table, fieldworks_project),
    )
}

fn char_def_subject(
    grammar: &Grammar,
    fieldworks_project: Option<&str>,
    table_id: TableId,
    id: CharDefId,
    cd: &CharDef,
    table: &CharDefTable,
) -> GrammarHealthSubject {
    make_subject(
        GrammarHealthSubjectKind::CharDef,
        first_representation(cd).to_string(),
        Some(format!("in {}", table_display_name(table))),
        format!("table#{}:char_def#{}:{}", table_id.0, id.0, cd.xml_id()),
        fieldworks_link(grammar, FieldWorksSource::CharDef, fieldworks_project),
    )
}

fn lex_entry_subject(
    grammar: &Grammar,
    fieldworks_project: Option<&str>,
    id: LexEntryId,
) -> Result<GrammarHealthSubject, crate::GrammarError> {
    Ok(lex_entry_subject_named(
        grammar,
        fieldworks_project,
        id,
        grammar.lex_entry_display_name(id)?,
        grammar.lex_entry_internal_id(id)?,
    ))
}

fn lex_entry_subject_named(
    grammar: &Grammar,
    fieldworks_project: Option<&str>,
    id: LexEntryId,
    title: String,
    internal_id: String,
) -> GrammarHealthSubject {
    make_subject(
        GrammarHealthSubjectKind::LexEntry,
        title,
        None,
        internal_id,
        fieldworks_link(grammar, FieldWorksSource::LexEntry(id), fieldworks_project),
    )
}

fn morph_rule_subject(
    grammar: &Grammar,
    fieldworks_project: Option<&str>,
    id: MRuleId,
) -> Result<GrammarHealthSubject, crate::GrammarError> {
    let rule = grammar.mrules.get(id.0 as usize).ok_or_else(|| {
        crate::GrammarError::Semantic(format!("morphological rule id {} is out of range", id.0))
    })?;
    Ok(morph_rule_subject_named(
        grammar,
        fieldworks_project,
        id,
        rule.health_kind_label(),
        grammar.morph_rule_display_name(id)?,
        grammar.morph_rule_internal_id(id)?,
    ))
}

fn morph_rule_subject_named(
    grammar: &Grammar,
    fieldworks_project: Option<&str>,
    id: MRuleId,
    kind: &str,
    title: String,
    internal_id: String,
) -> GrammarHealthSubject {
    make_subject(
        GrammarHealthSubjectKind::MorphRule,
        title,
        Some(kind.to_string()),
        internal_id,
        fieldworks_link(grammar, FieldWorksSource::MorphRule(id), fieldworks_project),
    )
}

// --- hc-undeclared-segment ---------------------------------------------------------------------

/// Checks lex-entry allomorphs and InsertSegments in a stratum's ordinary mrules.
fn check_undeclared_segments(
    grammar: &Grammar,
    fieldworks_project: Option<&str>,
    findings: &mut Vec<GrammarHealthCheckFinding>,
) -> Result<(), crate::GrammarError> {
    for stratum in &grammar.strata {
        let table = grammar
            .char_tables
            .get(stratum.table.0 as usize)
            .ok_or_else(|| {
                crate::GrammarError::Semantic(format!(
                    "stratum table id {} is out of range",
                    stratum.table.0
                ))
            })?;
        for &entry_id in &stratum.entries {
            let entry = grammar.entries.get(entry_id.0 as usize).ok_or_else(|| {
                crate::GrammarError::Semantic(format!(
                    "stratum lexical entry id {} is out of range",
                    entry_id.0
                ))
            })?;
            let subject = lex_entry_subject(grammar, fieldworks_project, entry_id)?;
            let name = subject.title.clone();
            for allomorph in &entry.allomorphs {
                check_segments_declared(
                    grammar,
                    fieldworks_project,
                    table,
                    stratum.table,
                    &allomorph.shape.shape,
                    &format!(
                        "Lexical entry '{name}' allomorph '{}'",
                        allomorph.shape.text
                    ),
                    subject.clone(),
                    findings,
                );
            }
        }

        for &rule_id in &stratum.mrules {
            let rule = grammar.mrules.get(rule_id.0 as usize).ok_or_else(|| {
                crate::GrammarError::Semantic(format!(
                    "stratum morphological rule id {} is out of range",
                    rule_id.0
                ))
            })?;
            let subject = morph_rule_subject(grammar, fieldworks_project, rule_id)?;
            let name = subject.title.clone();
            match rule {
                MorphRuleDef::AffixProcess(def) => {
                    for allomorph in &def.allomorphs {
                        for action in &allomorph.rhs {
                            check_insert_segments(
                                grammar,
                                fieldworks_project,
                                action,
                                "Morphological rule",
                                &name,
                                subject.clone(),
                                findings,
                            )?;
                        }
                    }
                }
                MorphRuleDef::Compounding(def) => {
                    for subrule in &def.subrules {
                        for action in &subrule.rhs {
                            check_insert_segments(
                                grammar,
                                fieldworks_project,
                                action,
                                "Compounding rule",
                                &name,
                                subject.clone(),
                                findings,
                            )?;
                        }
                    }
                }
                // C# casts to `AffixProcessRule`/`CompoundingRule` only; `RealizationalAffixProcessRule` is never checked there either.
                MorphRuleDef::Realizational(_) => {}
            }
        }
    }
    Ok(())
}

fn check_insert_segments(
    grammar: &Grammar,
    fieldworks_project: Option<&str>,
    action: &OutputAction,
    kind_label: &str,
    rule_name: &str,
    owner_subject: GrammarHealthSubject,
    findings: &mut Vec<GrammarHealthCheckFinding>,
) -> Result<(), crate::GrammarError> {
    let OutputAction::InsertSegments {
        table: table_id,
        shape,
    } = action
    else {
        return Ok(());
    };
    let table = grammar
        .char_tables
        .get(table_id.0 as usize)
        .ok_or_else(|| {
            crate::GrammarError::Semantic(format!(
                "insert-segments table id {} is out of range",
                table_id.0
            ))
        })?;
    check_segments_declared(
        grammar,
        fieldworks_project,
        table,
        *table_id,
        &shape.shape,
        &format!(
            "{kind_label} '{rule_name}' inserted segments '{}'",
            shape.text
        ),
        owner_subject,
        findings,
    );
    Ok(())
}

/// Skips structural boundary/anchor nodes and already-declared natural classes.
fn check_segments_declared(
    grammar: &Grammar,
    fieldworks_project: Option<&str>,
    table: &CharDefTable,
    table_id: TableId,
    shape: &Shape,
    where_desc: &str,
    owner_subject: GrammarHealthSubject,
    findings: &mut Vec<GrammarHealthCheckFinding>,
) {
    for (_, kind, cd, _) in shape.interior() {
        if kind != NodeKind::Segment || cd == NO_CHAR_DEF {
            continue;
        }
        let declared =
            (cd as usize) < table.len() && table.get(CharDefId(cd)).kind() == CharDefKind::Segment;
        if declared {
            continue;
        }
        findings.push(GrammarHealthCheckFinding {
            severity: GrammarHealthSeverity::Error,
            code: GrammarHealthCode::UndeclaredSegment,
            message: format!(
                "{where_desc} contains an undeclared segment; character definition table '{}' does not declare it.",
                table_display_name(table)
            ),
            subjects: vec![
                table_subject(grammar, fieldworks_project, table_id, table),
                owner_subject.clone(),
            ],
        });
    }
}

// --- hc-partial-morpheme -------------------------------------------------------------------

/// Consume the canonical typed inventory; this warning path does not inspect rule definitions.
fn check_partial_morphemes(
    grammar: &Grammar,
    fieldworks_project: Option<&str>,
    facts: &PartialMorphemeFacts,
    findings: &mut Vec<GrammarHealthCheckFinding>,
) -> Result<(), crate::GrammarError> {
    for identity in facts.identities() {
        match identity {
            PartialMorphemeIdentity::LexicalEntry {
                id,
                display_name,
                internal_id,
                ..
            } => findings.push(partial_finding(
                "Lexical entry",
                display_name,
                lex_entry_subject_named(
                    grammar,
                    fieldworks_project,
                    *id,
                    display_name.clone(),
                    internal_id.clone(),
                ),
            )),
            PartialMorphemeIdentity::MorphologicalRule {
                id,
                display_name,
                internal_id,
                rule_kind,
                ..
            } => findings.push(partial_finding(
                "Morphological rule",
                display_name,
                morph_rule_subject_named(
                    grammar,
                    fieldworks_project,
                    *id,
                    rule_kind,
                    display_name.clone(),
                    internal_id.clone(),
                ),
            )),
        }
    }
    Ok(())
}

fn partial_finding(
    kind_label: &str,
    name: &str,
    subject: GrammarHealthSubject,
) -> GrammarHealthCheckFinding {
    GrammarHealthCheckFinding {
        severity: GrammarHealthSeverity::Warning,
        code: GrammarHealthCode::PartialMorpheme,
        message: format!(
            "{kind_label} '{name}' is partially analyzed. Supply its missing category or \
             template/slot analysis; leaving it partial can broaden analysis and disable safe \
             final-template pruning."
        ),
        subjects: vec![subject],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pg_shape::ShapeBuilder;

    fn grammar(xml: &str) -> Grammar {
        crate::load(xml).unwrap_or_else(|e| panic!("fixture grammar failed to load: {e}"))
    }

    fn codes(findings: &[GrammarHealthCheckFinding]) -> Vec<GrammarHealthCode> {
        findings.iter().map(|f| f.code).collect()
    }

    #[test]
    fn every_code_has_a_distinct_stable_linguist_group_name() {
        let mut names = Vec::new();
        for &code in GrammarHealthCode::ALL {
            let name = code.group_name();
            assert!(!name.trim().is_empty(), "{code:?} has no group name");
            assert!(
                name.chars().count() <= 30,
                "{code:?} group name is too long: {name}"
            );
            assert!(!names.contains(&name), "duplicate group name: {name}");
            names.push(name);
        }
    }
    // --- hc-duplicate-feature-bundle ------------------------------------------------------

    const TWO_SEGMENTS_SHARE_BUNDLE_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>DuplicateBundle</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <PhonologicalFeatureSystem>
      <SymbolicFeature id="feat_voc"><Name>voc</Name>
        <Symbols><Symbol id="sym_p">+</Symbol><Symbol id="sym_m">-</Symbol></Symbols>
      </SymbolicFeature>
    </PhonologicalFeatureSystem>
    <CharacterDefinitionTable id="table1">
      <Name>table1</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="char_a"><Representations><Representation>a</Representation></Representations>
          <FeatureValue feature="feat_voc" symbolValues="sym_p" />
        </SegmentDefinition>
        <SegmentDefinition id="char_b"><Representations><Representation>b</Representation></Representations>
          <FeatureValue feature="feat_voc" symbolValues="sym_p" />
        </SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses></NaturalClasses>
  </Language>
</HermitCrabInput>
"#;

    #[test]
    fn two_segments_share_feature_bundle_reports_both_by_name() {
        let g = grammar(TWO_SEGMENTS_SHARE_BUNDLE_XML);
        let findings = check_grammar_health(&g, None).expect("grammar-health checks");
        assert_eq!(findings.len(), 1);
        let finding = &findings[0];
        assert_eq!(finding.code, GrammarHealthCode::DuplicateFeatureBundle);
        assert_eq!(finding.severity, GrammarHealthSeverity::Warning);
        assert!(finding.message.contains('a'));
        assert!(finding.message.contains('b'));
        assert!(matches!(
            &finding.subjects[0],
            GrammarHealthSubject { kind: GrammarHealthSubjectKind::Table, title, .. } if title == "table1"
        ));
    }

    const DISTINCT_BUNDLES_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>DistinctBundles</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <PhonologicalFeatureSystem>
      <SymbolicFeature id="feat_voc"><Name>voc</Name>
        <Symbols><Symbol id="sym_p">+</Symbol><Symbol id="sym_m">-</Symbol></Symbols>
      </SymbolicFeature>
    </PhonologicalFeatureSystem>
    <CharacterDefinitionTable id="table1">
      <Name>table1</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="char_a"><Representations><Representation>a</Representation></Representations>
          <FeatureValue feature="feat_voc" symbolValues="sym_p" />
        </SegmentDefinition>
        <SegmentDefinition id="char_b"><Representations><Representation>b</Representation></Representations>
          <FeatureValue feature="feat_voc" symbolValues="sym_m" />
        </SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses></NaturalClasses>
  </Language>
</HermitCrabInput>
"#;

    #[test]
    fn every_segment_has_distinct_feature_bundle_no_findings() {
        let g = grammar(DISTINCT_BUNDLES_XML);
        assert!(check_grammar_health(&g, None)
            .expect("grammar-health checks")
            .is_empty());
    }

    const ZERO_FEATURE_SYSTEM_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>ZeroFeatureSystem</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="table1">
      <Name>table1</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="char_a"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="char_b"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="char_c"><Representations><Representation>c</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses></NaturalClasses>
  </Language>
</HermitCrabInput>
"#;

    #[test]
    fn no_phonological_feature_system_does_not_flag_trivially_identical_bundles() {
        // No `<PhonologicalFeatureSystem>` at all (the real Sena shape) -- every bundle is the same empty struct, so this must not report a duplicate.
        let g = grammar(ZERO_FEATURE_SYSTEM_XML);
        assert!(check_grammar_health(&g, None)
            .expect("grammar-health checks")
            .is_empty());
    }

    // --- hc-undeclared-segment -------------------------------------------------------------

    const CLEAN_LEXICON_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>CleanLexicon</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="table1">
      <Name>table1</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="char_a"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="char_b"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses></NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="table1">
        <Name>Surface</Name>
        <LexicalEntries>
          <LexicalEntry id="e1" partOfSpeech="posV">
            <Allomorphs><Allomorph id="a1"><PhoneticShape>ab</PhoneticShape></Allomorph></Allomorphs>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

    #[test]
    fn clean_grammar_no_findings_at_all() {
        let g = grammar(CLEAN_LEXICON_XML);
        assert!(check_grammar_health(&g, None)
            .expect("grammar-health checks")
            .is_empty());
    }

    /// A hand-built `Shape` bypassing the table's own validated segmentation -- mirrors C#'s own test note that direct object-model construction need not go through it.
    fn undeclared_shape() -> Shape {
        let mut b = ShapeBuilder::new();
        b.push_segment(9_999);
        b.finish()
    }

    #[test]
    fn lexical_entry_uses_segment_no_table_declares_reports_finding() {
        let mut g = grammar(CLEAN_LEXICON_XML);
        g.entries[0].allomorphs[0].shape.shape = undeclared_shape();

        let findings = check_grammar_health(&g, None).expect("grammar-health checks");
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, GrammarHealthCode::UndeclaredSegment);
        assert_eq!(findings[0].severity, GrammarHealthSeverity::Error);
        assert!(findings[0].message.contains("e1"));
    }

    const AFFIX_INSERT_SEGMENTS_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>AffixInsertSegments</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="table1">
      <Name>table1</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="char_a"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="char_a" /></SegmentNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="table1" morphologicalRules="mr1">
        <Name>Surface</Name>
        <MorphologicalRuleDefinitions>
          <MorphologicalRule id="mr1" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
            <Name>plural</Name>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="sub1">
                <MorphologicalInput>
                  <PhoneticSequence id="stem">
                    <OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAll" /></OptionalSegmentSequence>
                  </PhoneticSequence>
                </MorphologicalInput>
                <MorphologicalOutput>
                  <CopyFromInput index="stem" />
                  <InsertSegments><PhoneticShape>a</PhoneticShape></InsertSegments>
                </MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
          </MorphologicalRule>
        </MorphologicalRuleDefinitions>
        <LexicalEntries>
          <LexicalEntry id="e1" partOfSpeech="posV">
            <Allomorphs><Allomorph id="a1"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

    /// Finds the sole `InsertSegments` action inside `grammar.mrules[0]`'s single allomorph.
    fn insert_segments_shape_mut(g: &mut Grammar) -> &mut Shape {
        let MorphRuleDef::AffixProcess(def) = &mut g.mrules[0] else {
            panic!("expected an AffixProcess rule");
        };
        for action in &mut def.allomorphs[0].rhs {
            if let OutputAction::InsertSegments { shape, .. } = action {
                return &mut shape.shape;
            }
        }
        panic!("expected an InsertSegments action");
    }

    #[test]
    fn affix_process_rule_insert_segments_undeclared_reports_finding() {
        let mut g = grammar(AFFIX_INSERT_SEGMENTS_XML);
        *insert_segments_shape_mut(&mut g) = undeclared_shape();

        let findings = check_grammar_health(&g, None).expect("grammar-health checks");
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, GrammarHealthCode::UndeclaredSegment);
        assert!(findings[0].message.contains("plural"));
    }

    const COMPOUNDING_INSERT_SEGMENTS_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>CompoundingInsertSegments</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="table1">
      <Name>table1</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="char_a"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
      <BoundaryDefinitions>
        <BoundaryDefinition id="char_bnd"><Representations><Representation>+</Representation></Representations></BoundaryDefinition>
      </BoundaryDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="char_a" /></SegmentNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="table1" morphologicalRules="mrC">
        <Name>Surface</Name>
        <MorphologicalRuleDefinitions>
          <CompoundingRule id="mrC">
            <Name>compound1</Name>
            <CompoundingSubrules><CompoundingSubrule>
              <HeadMorphologicalInput><PhoneticSequence id="head"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAll" /></OptionalSegmentSequence></PhoneticSequence></HeadMorphologicalInput>
              <NonHeadMorphologicalInput><PhoneticSequence id="nonHead"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAll" /></OptionalSegmentSequence></PhoneticSequence></NonHeadMorphologicalInput>
              <MorphologicalOutput><CopyFromInput index="head" /><InsertSegments><PhoneticShape>+</PhoneticShape></InsertSegments><CopyFromInput index="nonHead" /></MorphologicalOutput>
            </CompoundingSubrule></CompoundingSubrules>
          </CompoundingRule>
        </MorphologicalRuleDefinitions>
        <LexicalEntries>
          <LexicalEntry id="e1" partOfSpeech="posV">
            <Allomorphs><Allomorph id="a1"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

    #[test]
    fn compounding_rule_insert_segments_undeclared_reports_finding() {
        let mut g = grammar(COMPOUNDING_INSERT_SEGMENTS_XML);
        let MorphRuleDef::Compounding(def) = &mut g.mrules[0] else {
            panic!("expected a Compounding rule");
        };
        let mut replaced = false;
        for action in &mut def.subrules[0].rhs {
            if let OutputAction::InsertSegments { shape, .. } = action {
                shape.shape = undeclared_shape();
                replaced = true;
            }
        }
        assert!(replaced, "fixture must contain an InsertSegments action");

        let findings = check_grammar_health(&g, None).expect("grammar-health checks");
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, GrammarHealthCode::UndeclaredSegment);
        assert!(findings[0].message.contains("compound1"));
    }

    // --- hc-partial-morpheme -----------------------------------------------------------------

    const PARTIAL_LEX_ENTRY_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>PartialLexEntry</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="table1">
      <Name>table1</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="char_a"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses></NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="table1">
        <Name>Surface</Name>
        <LexicalEntries>
          <LexicalEntry id="entry1" partial="true">
            <Allomorphs><Allomorph id="a1"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

    #[test]
    fn partial_lexical_entry_reports_actionable_warning() {
        let g = grammar(PARTIAL_LEX_ENTRY_XML);
        let findings = check_grammar_health(&g, None).expect("grammar-health checks");
        assert_eq!(findings.len(), 1);
        let finding = &findings[0];
        assert_eq!(finding.code, GrammarHealthCode::PartialMorpheme);
        assert_eq!(finding.severity, GrammarHealthSeverity::Warning);
        assert!(finding.message.contains("Lexical entry 'a'"));
        assert!(finding.message.contains("partially analyzed"));
        assert!(finding.message.contains("final-template pruning"));
        assert!(matches!(
            &finding.subjects[..],
            [GrammarHealthSubject { kind: GrammarHealthSubjectKind::LexEntry, title, .. }] if title == "a"
        ));
    }

    const PARTIAL_TEMPLATE_RULE_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>PartialTemplateRule</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="table1">
      <Name>table1</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="char_a"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="char_a" /></SegmentNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="table1">
        <Name>Surface</Name>
        <MorphologicalRuleDefinitions>
          <MorphologicalRule id="mr1" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV" partial="true">
            <Name>subject</Name>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="sub1">
                <MorphologicalInput>
                  <PhoneticSequence id="stem">
                    <OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAll" /></OptionalSegmentSequence>
                  </PhoneticSequence>
                </MorphologicalInput>
                <MorphologicalOutput><CopyFromInput index="stem" /></MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
          </MorphologicalRule>
        </MorphologicalRuleDefinitions>
        <AffixTemplates>
          <AffixTemplate>
            <Name>verb1</Name>
            <Slot morphologicalRules="mr1"><Name>Sl1</Name></Slot>
          </AffixTemplate>
          <AffixTemplate>
            <Name>verb2</Name>
            <Slot morphologicalRules="mr1"><Name>Sl2</Name></Slot>
          </AffixTemplate>
        </AffixTemplates>
        <LexicalEntries>
          <LexicalEntry id="e1" partOfSpeech="posV">
            <Allomorphs><Allomorph id="a1"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

    #[test]
    fn partial_ordinary_rule_reports_rule() {
        // Distinct from the template-only fixture below: this one lists `mr1` in the stratum's own `morphologicalRules`, exercising the ordinary-rule path.
        const XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>PartialOrdinaryRule</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="table1">
      <Name>table1</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="char_a"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="char_a" /></SegmentNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="table1" morphologicalRules="mr1">
        <Name>Surface</Name>
        <MorphologicalRuleDefinitions>
          <MorphologicalRule id="mr1" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV" partial="true">
            <Name>plural</Name>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="sub1">
                <MorphologicalInput>
                  <PhoneticSequence id="stem">
                    <OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAll" /></OptionalSegmentSequence>
                  </PhoneticSequence>
                </MorphologicalInput>
                <MorphologicalOutput><CopyFromInput index="stem" /></MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
          </MorphologicalRule>
        </MorphologicalRuleDefinitions>
        <LexicalEntries>
          <LexicalEntry id="e1" partOfSpeech="posV">
            <Allomorphs><Allomorph id="a1"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;
        let g = grammar(XML);
        let findings = check_grammar_health(&g, None).expect("grammar-health checks");
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, GrammarHealthCode::PartialMorpheme);
        assert!(findings[0].message.contains("plural"));
        assert!(matches!(
            &findings[0].subjects[..],
            [GrammarHealthSubject { kind: GrammarHealthSubjectKind::MorphRule, title, .. }] if title == "plural"
        ));
    }

    #[test]
    fn partial_template_rule_referenced_twice_reports_once() {
        let g = grammar(PARTIAL_TEMPLATE_RULE_XML);
        let findings = check_grammar_health(&g, None).expect("grammar-health checks");
        assert_eq!(findings.len(), 1, "referenced by two slots, reported once");
        assert_eq!(findings[0].code, GrammarHealthCode::PartialMorpheme);
        assert!(findings[0].message.contains("subject"));
    }

    const PARTIAL_MORPHEME_AND_EXISTING_PROBLEM_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>PartialAndDuplicate</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <PhonologicalFeatureSystem>
      <SymbolicFeature id="feat_voc"><Name>voc</Name>
        <Symbols><Symbol id="sym_p">+</Symbol><Symbol id="sym_m">-</Symbol></Symbols>
      </SymbolicFeature>
    </PhonologicalFeatureSystem>
    <CharacterDefinitionTable id="table1">
      <Name>table1</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="char_a"><Representations><Representation>a</Representation></Representations>
          <FeatureValue feature="feat_voc" symbolValues="sym_p" />
        </SegmentDefinition>
        <SegmentDefinition id="char_b"><Representations><Representation>b</Representation></Representations>
          <FeatureValue feature="feat_voc" symbolValues="sym_p" />
        </SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses></NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="table1">
        <Name>Surface</Name>
        <LexicalEntries>
          <LexicalEntry id="entry1" partial="true">
            <Allomorphs><Allomorph id="a1"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

    #[test]
    fn partial_morpheme_and_existing_problem_reports_both() {
        let g = grammar(PARTIAL_MORPHEME_AND_EXISTING_PROBLEM_XML);
        let mut found = codes(&check_grammar_health(&g, None).expect("grammar-health checks"));
        found.sort_by_key(|c| c.wire());
        let mut expected = vec![
            GrammarHealthCode::DuplicateFeatureBundle,
            GrammarHealthCode::PartialMorpheme,
        ];
        expected.sort_by_key(|c| c.wire());
        assert_eq!(found, expected);
    }

    // --- serialization ------------------------------------------------------------------------

    #[test]
    fn report_round_trips_through_the_single_decode_path() {
        let g = grammar(PARTIAL_LEX_ENTRY_XML);
        let report = check_grammar_health(&g, None).expect("grammar-health checks");
        let json = report.to_json().expect("report must serialize");
        assert!(json.contains("hc-partial-morpheme"));
        let round_tripped = GrammarHealthReport::from_json(&json).expect("report must decode");
        assert_eq!(round_tripped, report);
    }

    #[test]
    fn every_code_serializes_to_its_stable_wire_string() {
        for (code, wire) in [
            (
                GrammarHealthCode::UndeclaredSegment,
                "hc-undeclared-segment",
            ),
            (
                GrammarHealthCode::DuplicateFeatureBundle,
                "hc-duplicate-feature-bundle",
            ),
            (GrammarHealthCode::PartialMorpheme, "hc-partial-morpheme"),
        ] {
            assert_eq!(code.wire(), wire);
            assert_eq!(serde_json::to_string(&code).unwrap(), format!("{wire:?}"));
        }
    }

    #[test]
    fn group_names_describe_their_codes() {
        assert_eq!(
            GrammarHealthCode::UndeclaredSegment.group_name(),
            "Missing segment definition"
        );
        assert_eq!(
            GrammarHealthCode::DuplicateFeatureBundle.group_name(),
            "Duplicate segment features"
        );
        assert_eq!(
            GrammarHealthCode::PartialMorpheme.group_name(),
            "Partial morpheme analysis"
        );
    }

    const FIELDWORKS_GUID_PARTIAL_XML: &str = r#"<HermitCrabInput><Language>
<Name>FieldWorks Demo</Name>
<PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
<CharacterDefinitionTable id="table1"><Name>Orthography</Name>
<SegmentDefinitions><SegmentDefinition id="segA"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions>
</CharacterDefinitionTable>
<Strata><Stratum characterDefinitionTable="table1"><Name>main</Name><LexicalEntries>
<LexicalEntry id="f4e4b416-5a15-41e3-9039-c3cca7093153" partial="true">
<MorphemeId>walk</MorphemeId><Gloss>walk</Gloss>
<Allomorphs><Allomorph id="allo1"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs>
</LexicalEntry>
</LexicalEntries></Stratum></Strata>
</Language></HermitCrabInput>"#;

    #[test]
    fn fieldworks_guid_partial_entry_uses_its_readable_name() {
        let g = grammar(FIELDWORKS_GUID_PARTIAL_XML);
        let findings = check_grammar_health(&g, None).expect("grammar-health checks");
        assert_eq!(findings.len(), 1);
        assert!(matches!(
            &findings[0].subjects[..],
            [GrammarHealthSubject { kind: GrammarHealthSubjectKind::LexEntry, title, .. }] if title == "a - walk"
        ));
        assert!(!findings[0]
            .message
            .contains("f4e4b416-5a15-41e3-9039-c3cca7093153"));
    }

    fn assert_no_blank_or_internal_subjects(report: &GrammarHealthReport) {
        let findings = report.findings();
        for finding in findings {
            assert!(!finding.message.trim().is_empty());
            assert!(!finding.code.wire().is_empty());
            for subject in &finding.subjects {
                assert!(!subject.kind.label().is_empty());
                assert!(!subject.title.trim().is_empty());
                assert!(!subject.render_location(false).trim().is_empty());
                assert!(!is_internal_subject_label(&subject.title));
            }
            let json = serde_json::to_value(finding).expect("finding serializes");
            assert!(json["severity"]
                .as_str()
                .is_some_and(|value| !value.is_empty()));
            assert_eq!(json["group_name"], finding.code.group_name());
            assert!(json["problem"]
                .as_str()
                .is_some_and(|value| !value.is_empty()));
            let subjects = json["subjects"]
                .as_array()
                .expect("subjects serialize as an array");
            assert!(!subjects.is_empty());
            for subject in subjects {
                assert!(subject["kind"]
                    .as_str()
                    .is_some_and(|value| !value.is_empty()));
                assert!(subject["title"]
                    .as_str()
                    .is_some_and(|value| !value.trim().is_empty()));
            }
        }
    }

    #[test]
    fn report_rejects_an_incomplete_finding_instead_of_dropping_it() {
        let incomplete = GrammarHealthCheckFinding {
            severity: GrammarHealthSeverity::Warning,
            code: GrammarHealthCode::PartialMorpheme,
            message: "incomplete".to_string(),
            subjects: Vec::new(),
        };
        let error = GrammarHealthReport::new(vec![incomplete])
            .expect_err("incomplete findings must fail report construction");
        assert_eq!(error.code, GrammarHealthReportErrorCode::MissingSubjects);
        assert_eq!(error.finding_index, Some(0));
        assert_eq!(error.field.as_deref(), Some("subjects"));
    }

    #[test]
    fn validated_empty_report_renders_lossless_log() {
        let report = GrammarHealthReport::new(Vec::new()).expect("empty report is valid");
        assert_eq!(
            render_log(&report, false),
            ""
        );
    }

    #[test]
    fn an_empty_report_remains_valid() {
        let report = GrammarHealthReport::new(Vec::new()).expect("empty report is valid");
        assert_eq!(render_log(&report, false), "");
        let json = render_json(&report).expect("empty report serializes");
        let report: serde_json::Value = serde_json::from_str(&json).expect("versioned report");
        assert_eq!(report["schema_version"], GRAMMAR_HEALTH_SCHEMA_VERSION);
        assert_eq!(report["findings"], serde_json::json!([]));
    }

    #[test]
    fn every_code_variant_occurs_once_in_all() {
        assert_eq!(GrammarHealthCode::ALL.len(), 3);
        for code in [
            GrammarHealthCode::UndeclaredSegment,
            GrammarHealthCode::DuplicateFeatureBundle,
            GrammarHealthCode::PartialMorpheme,
        ] {
            assert_eq!(
                GrammarHealthCode::ALL
                    .iter()
                    .filter(|candidate| **candidate == code)
                    .count(),
                1
            );
        }
    }

    #[test]
    fn internal_hash_identifiers_are_not_accepted_as_human_titles() {
        assert!(is_internal_subject_label("mrule#18"));
        assert!(is_internal_subject_label("lex_entry#4:entry"));
    }

    #[test]
    fn direct_report_decode_rejects_an_unsupported_schema_version() {
        let json = serde_json::json!({
            "schema_version": 99,
            "findings": [],
        });
        let error = GrammarHealthReport::from_json(&json.to_string())
            .expect_err("unsupported schema versions must be rejected");
        assert_eq!(
            error.code,
            GrammarHealthReportErrorCode::UnsupportedSchemaVersion
        );
        assert_eq!(error.field.as_deref(), Some("schema_version"));
    }

    #[test]
    fn direct_report_decode_rejects_both_fieldworks_link_states() {
        let grammar = grammar(FIELDWORKS_GUID_PARTIAL_XML);
        let source_report = check_grammar_health(&grammar, None).expect("grammar-health checks");
        let finding = source_report
            .findings()
            .iter()
            .next()
            .expect("fixture has a finding");
        let report = GrammarHealthReport::new(vec![finding.clone()]).expect("report validates");
        let mut json = serde_json::to_value(&report).expect("report serializes");
        json["findings"][0]["subjects"][0]["fieldworks"] = serde_json::json!({
            "status": "available",
            "guid": "invalid",
            "tool": "lexiconEdit",
            "url": "silfw://invalid"
        });
        let error = GrammarHealthReport::from_json(&json.to_string())
            .expect_err("invalid link state must be rejected");
        assert_eq!(error.field.as_deref(), Some("fieldworks.guid"));
    }

    #[test]
    fn canonical_report_round_trips_without_changing_consumer_values() {
        let findings = check_grammar_health(&grammar(FIELDWORKS_GUID_PARTIAL_XML), None)
            .expect("grammar-health checks");
        let original = findings;
        let json = original.to_json().expect("canonical report serializes");
        let decoded = GrammarHealthReport::from_json(&json).expect("canonical report decodes");
        assert_eq!(decoded, original);
    }

    #[test]
    fn every_existing_grammar_fixture_has_complete_human_findings() {
        let mut compounding_grammar = grammar(COMPOUNDING_INSERT_SEGMENTS_XML);
        let MorphRuleDef::Compounding(def) = &mut compounding_grammar.mrules[0] else {
            panic!("expected a Compounding rule");
        };
        for action in &mut def.subrules[0].rhs {
            if let OutputAction::InsertSegments { shape, .. } = action {
                shape.shape = undeclared_shape();
            }
        }
        let compounding_findings =
            check_grammar_health(&compounding_grammar, Some("FieldWorks Demo"))
                .expect("grammar-health checks");
        let compounding_subject = compounding_findings
            .iter()
            .flat_map(|finding| &finding.subjects)
            .find(|subject| subject.kind == GrammarHealthSubjectKind::MorphRule)
            .expect("compounding finding names its rule");
        assert!(matches!(
            compounding_subject.fieldworks,
            FieldWorksLink::Unavailable {
                reason: FieldWorksUnavailableReason::MissingGuid,
                ..
            }
        ));
        for xml in [
            TWO_SEGMENTS_SHARE_BUNDLE_XML,
            DISTINCT_BUNDLES_XML,
            ZERO_FEATURE_SYSTEM_XML,
            CLEAN_LEXICON_XML,
            AFFIX_INSERT_SEGMENTS_XML,
            COMPOUNDING_INSERT_SEGMENTS_XML,
            PARTIAL_LEX_ENTRY_XML,
            PARTIAL_TEMPLATE_RULE_XML,
            PARTIAL_MORPHEME_AND_EXISTING_PROBLEM_XML,
            FIELDWORKS_GUID_PARTIAL_XML,
        ] {
            let findings =
                check_grammar_health(&grammar(xml), None).expect("grammar-health checks");
            assert_no_blank_or_internal_subjects(&findings);
        }
    }

    #[test]
    fn log_and_json_render_the_same_guid_fixture_with_explicit_link_state() {
        let mut g = grammar(FIELDWORKS_GUID_PARTIAL_XML);
        let allomorph_id = g.entries[0].allomorphs[0].id.0 as usize;
        g.allomorph_sources[allomorph_id].form_guids =
            vec![Some("f4e4b416-5a15-41e3-9039-c3cca7093153".to_string())];
        let with_project =
            check_grammar_health(&g, Some("FieldWorks Demo")).expect("grammar-health checks");
        assert_no_blank_or_internal_subjects(&with_project);
        let subject = &with_project[0].subjects[0];
        assert_eq!(subject.title, "a - walk");
        let FieldWorksLink::Available { guid, tool, url } = &subject.fieldworks else {
            panic!("provenance-backed lexical subject should have a link");
        };
        assert_eq!(guid, "f4e4b416-5a15-41e3-9039-c3cca7093153");
        assert_eq!(tool, "lexiconEdit");
        assert!(url.contains("database%3DFieldWorks+Demo%26tool%3DlexiconEdit%26guid%3Df4e4b416-5a15-41e3-9039-c3cca7093153%26tag%3D"));
        assert!(!url.contains("&tool="));
        assert!(!url.contains("&guid="));

        let log = render_log(&with_project, false);
        assert_eq!(log.lines().count(), with_project.len());
        assert!(log.lines().all(|line| !line.trim().is_empty()));
        assert!(log.contains("a - walk"));
        assert!(log.contains("Partial morpheme analysis"));
        assert!(log.contains(url));
        assert!(!log.contains("entry0"));
        let log_with_guids = render_log(&with_project, true);
        assert!(log_with_guids.contains("a - walk [guid f4e4b416-5a15-41e3-9039-c3cca7093153]"));
        assert!(!log_with_guids.contains("entry0"));

        let json = render_json(&with_project).expect("structured findings serialize");
        assert!(json.contains("\"schema_version\": 1"));
        assert!(json.contains("\"group_name\": \"Partial morpheme analysis\""));
        assert!(
            json.contains("silfw://localhost/link?database%3DFieldWorks+Demo%26tool%3DlexiconEdit")
        );
        assert!(json.contains("\"internal_id\""));

        let without_project = check_grammar_health(&g, None).expect("grammar-health checks");
        let no_link = &without_project[0].subjects[0].fieldworks;
        assert!(matches!(
            no_link,
            FieldWorksLink::Unavailable {
                reason: FieldWorksUnavailableReason::MissingProject,
                guid: Some(_),
            }
        ));
        let no_project_json = render_json(&without_project).expect("structured findings serialize");
        assert!(no_project_json.contains("missing_project"));
        assert!(no_project_json.contains("f4e4b416-5a15-41e3-9039-c3cca7093153"));
        assert!(!no_project_json.contains("silfw://localhost/link?database="));

        let no_guid_findings = check_grammar_health(
            &grammar(TWO_SEGMENTS_SHARE_BUNDLE_XML),
            Some("FieldWorks Demo"),
        )
        .expect("grammar-health checks");
        let no_guid_log = render_log(&no_guid_findings, true);
        assert!(no_guid_log.contains("source item has no FieldWorks GUID"));
        assert!(no_guid_log.contains("[guid unavailable]"));
    }

    #[test]
    fn log_rendering_keeps_subject_context_and_navigation_state() {
        let duplicate = check_grammar_health(
            &grammar(TWO_SEGMENTS_SHARE_BUNDLE_XML),
            Some("FieldWorks Demo"),
        )
        .expect("duplicate fixture checks");
        let log = render_log(&duplicate, false);
        assert!(log.contains("in table1"), "{log}");
        assert!(log.contains("FieldWorks link unavailable:"), "{log}");
        assert!(log.contains("source item has no FieldWorks GUID"), "{log}");
    }

    #[test]
    fn partial_warning_subjects_match_canonical_facts_in_both_directions() {
        for xml in [PARTIAL_MORPHEME_AND_EXISTING_PROBLEM_XML, PARTIAL_TEMPLATE_RULE_XML] {
            assert_partial_subjects_match_facts(&grammar(xml));
        }
    }

    fn assert_partial_subjects_match_facts(g: &Grammar) {
        let facts = g.partial_morpheme_facts().expect("valid partial facts");
        assert!(facts.has_partials(), "fixture must declare a partial morpheme");
        let findings = check_grammar_health(g, None).expect("grammar-health checks");
        let mut warning_subjects = findings
            .iter()
            .filter(|finding| finding.code == GrammarHealthCode::PartialMorpheme)
            .flat_map(|finding| finding.subjects.iter())
            .map(|subject| {
                (
                    subject.kind,
                    subject.title.clone(),
                    subject.internal_id.clone(),
                )
            })
            .collect::<Vec<_>>();
        let mut fact_subjects = facts
            .identities()
            .map(|identity| match identity {
                PartialMorphemeIdentity::LexicalEntry {
                    display_name,
                    internal_id,
                    ..
                } => (
                    GrammarHealthSubjectKind::LexEntry,
                    display_name.clone(),
                    internal_id.clone(),
                ),
                PartialMorphemeIdentity::MorphologicalRule {
                    display_name,
                    internal_id,
                    ..
                } => (
                    GrammarHealthSubjectKind::MorphRule,
                    display_name.clone(),
                    internal_id.clone(),
                ),
            })
            .collect::<Vec<_>>();
        let sort_subjects = |subjects: &mut Vec<(GrammarHealthSubjectKind, String, String)>| {
            subjects.sort_by(|left, right| {
                left.1
                    .cmp(&right.1)
                    .then_with(|| left.2.cmp(&right.2))
                    .then_with(|| left.0.label().cmp(right.0.label()))
            });
        };
        sort_subjects(&mut warning_subjects);
        sort_subjects(&mut fact_subjects);
        assert_eq!(warning_subjects, fact_subjects);
        for (_, _, internal_id) in &warning_subjects {
            assert_eq!(internal_id.matches('#').count(), 1, "{internal_id}");
        }
    }

    #[test]
    fn rule_subject_internal_id_is_the_canonical_model_id() {
        let g = grammar(AFFIX_INSERT_SEGMENTS_XML);
        let subject = morph_rule_subject(&g, None, MRuleId(0)).expect("rule subject");
        assert_eq!(
            subject.internal_id,
            g.morph_rule_internal_id(MRuleId(0)).expect("model id")
        );
        assert!(subject.internal_id.starts_with("morph_rule#0"), "{}", subject.internal_id);
        assert_eq!(subject.internal_id.matches('#').count(), 1, "{}", subject.internal_id);
    }

    #[test]
    fn partial_fact_failure_is_propagated_instead_of_admitted() {
        let mut g = grammar(PARTIAL_TEMPLATE_RULE_XML);
        let MorphRuleDef::AffixProcess(def) = &mut g.mrules[0] else {
            panic!("fixture must contain an affix-process rule");
        };
        def.morpheme = crate::model::MorphemeId(u32::MAX);
        let error = check_grammar_health(&g, None).expect_err("invalid partial facts must fail");
        assert!(error.to_string().contains("unknown morpheme"), "{error}");
    }

    #[test]
    fn invalid_subject_references_return_named_errors_instead_of_panicking() {
        let mut lexical = grammar(FIELDWORKS_GUID_PARTIAL_XML);
        lexical.entries[0].morpheme = crate::model::MorphemeId(u32::MAX);
        let error = lex_entry_subject(&lexical, None, LexEntryId(0))
            .expect_err("invalid lexical subject reference must fail");
        assert!(
            error.to_string().contains("morpheme id 4294967295"),
            "{error}"
        );

        let mut rule = grammar(AFFIX_INSERT_SEGMENTS_XML);
        let MorphRuleDef::AffixProcess(def) = &mut rule.mrules[0] else {
            panic!("fixture must contain an affix-process rule");
        };
        def.morpheme = crate::model::MorphemeId(u32::MAX);
        let error = morph_rule_subject(&rule, None, MRuleId(0))
            .expect_err("invalid morphological subject reference must fail");
        assert!(
            error.to_string().contains("morpheme id 4294967295"),
            "{error}"
        );
    }

    #[test]
    fn fieldworks_navigation_has_exact_supported_and_unavailable_states() {
        // Proof citations: FieldWorks/Src/xWorks/FwLinkArgs.cs; RecordClerk.cs:1036, 998-1016; RecordList.cs:3435.
        let mut lexical_grammar = grammar(FIELDWORKS_GUID_PARTIAL_XML);
        let allomorph_id = lexical_grammar.entries[0].allomorphs[0].id.0 as usize;
        lexical_grammar.allomorph_sources[allomorph_id].form_guids =
            vec![Some("f4e4b416-5a15-41e3-9039-c3cca7093153".to_string())];
        let lexical =
            check_grammar_health(&lexical_grammar, Some("Demo")).expect("lexical fixture checks");
        let lexical = &lexical[0].subjects[0].fieldworks;
        assert!(matches!(
            lexical,
            FieldWorksLink::Available { guid, tool, url }
                if guid == "f4e4b416-5a15-41e3-9039-c3cca7093153"
                    && tool == "lexiconEdit"
                    && url == "silfw://localhost/link?database%3DDemo%26tool%3DlexiconEdit%26guid%3Df4e4b416-5a15-41e3-9039-c3cca7093153%26tag%3D"
        ));

        let mut affix = grammar(AFFIX_INSERT_SEGMENTS_XML);
        *insert_segments_shape_mut(&mut affix) = undeclared_shape();
        let MorphRuleDef::AffixProcess(def) = &affix.mrules[0] else {
            panic!("affix fixture checks");
        };
        affix.morphemes[def.morpheme.0 as usize].source_msa_guid =
            Some("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa".to_string());
        let affix = check_grammar_health(&affix, Some("Demo")).expect("affix fixture checks");
        let affix = affix[0]
            .subjects
            .iter()
            .find(|subject| subject.kind == GrammarHealthSubjectKind::MorphRule)
            .expect("affix subject")
            .fieldworks
            .clone();
        assert!(matches!(
            affix,
            FieldWorksLink::Available { guid, tool, url }
                if guid == "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa"
                    && tool == "lexiconEdit"
                    && url.contains("tool%3DlexiconEdit")
        ));

        let mut infl_type = grammar(AFFIX_INSERT_SEGMENTS_XML);
        *insert_segments_shape_mut(&mut infl_type) = undeclared_shape();
        let MorphRuleDef::AffixProcess(def) = &infl_type.mrules[0] else {
            panic!("infl-type fixture checks");
        };
        let info = &mut infl_type.morphemes[def.morpheme.0 as usize];
        info.source_msa_guid = None;
        info.source_infl_type_guid = Some("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb".to_string());
        let infl_type =
            check_grammar_health(&infl_type, Some("Demo")).expect("infl-type fixture checks");
        let infl_type = infl_type[0]
            .subjects
            .iter()
            .find(|subject| subject.kind == GrammarHealthSubjectKind::MorphRule)
            .expect("infl-type subject")
            .fieldworks
            .clone();
        assert!(matches!(
            infl_type,
            FieldWorksLink::Unavailable {
                reason: FieldWorksUnavailableReason::UnverifiedGuidKind,
                guid: Some(guid),
            } if guid == "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb"
        ));

        let mut compound = grammar(COMPOUNDING_INSERT_SEGMENTS_XML);
        let MorphRuleDef::Compounding(def) = &mut compound.mrules[0] else {
            panic!("fixture must contain a compounding rule");
        };
        for action in &mut def.subrules[0].rhs {
            if let OutputAction::InsertSegments { shape, .. } = action {
                shape.shape = undeclared_shape();
            }
        }
        let compound =
            check_grammar_health(&compound, Some("Demo")).expect("compound fixture checks");
        let compound = compound[0]
            .subjects
            .iter()
            .find(|subject| subject.kind == GrammarHealthSubjectKind::MorphRule)
            .expect("compound subject")
            .fieldworks
            .clone();
        assert!(matches!(
            compound,
            FieldWorksLink::Unavailable {
                reason: FieldWorksUnavailableReason::MissingGuid,
                ..
            }
        ));

        let duplicate = check_grammar_health(&grammar(TWO_SEGMENTS_SHARE_BUNDLE_XML), Some("Demo"))
            .expect("table fixture checks");
        for kind in [
            GrammarHealthSubjectKind::Table,
            GrammarHealthSubjectKind::CharDef,
        ] {
            let link = duplicate[0]
                .subjects
                .iter()
                .find(|subject| subject.kind == kind)
                .expect("character-definition subject")
                .fieldworks
                .clone();
            assert!(matches!(
                link,
                FieldWorksLink::Unavailable {
                    reason: FieldWorksUnavailableReason::MissingGuid,
                    ..
                }
            ));
        }
    }

    #[test]
    fn report_constructor_owns_validation_and_renderers_accept_only_reports() {
        let finding = GrammarHealthCheckFinding {
            severity: GrammarHealthSeverity::Warning,
            code: GrammarHealthCode::PartialMorpheme,
            message: "partial".to_string(),
            subjects: vec![],
        };
        let error =
            GrammarHealthReport::new(vec![finding]).expect_err("empty subjects are invalid");
        assert_eq!(error.code, GrammarHealthReportErrorCode::MissingSubjects);
        assert_eq!(error.finding_index, Some(0));
        assert_eq!(error.field.as_deref(), Some("subjects"));
    }

    #[test]
    fn authored_xml_guid_shape_does_not_create_a_fieldworks_link() {
        let report = check_grammar_health(&grammar(FIELDWORKS_GUID_PARTIAL_XML), Some("Demo"))
            .expect("grammar-health checks");
        assert!(matches!(
            &report.findings()[0].subjects[0].fieldworks,
            FieldWorksLink::Unavailable {
                reason: FieldWorksUnavailableReason::MissingGuid,
                ..
            }
        ));
    }
}
