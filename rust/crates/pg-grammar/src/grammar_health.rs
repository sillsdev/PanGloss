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

impl FieldWorksLink {
    pub fn guid(&self) -> Option<&str> {
        match self {
            Self::Available { guid, .. } => Some(guid),
            Self::Unavailable { guid, .. } => guid.as_deref(),
        }
    }
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

/// Only the `prefix#N` forms this crate mints as internal ids; authored names are never rejected.
fn is_internal_subject_label(title: &str) -> bool {
    let lower = title.trim().to_ascii_lowercase();
    let Some((prefix, suffix)) = lower.split_once('#') else {
        return false;
    };
    ["char_def", "lex_entry", "mrule", "morph_rule", "table"].contains(&prefix)
        && suffix.starts_with(|character: char| character.is_ascii_digit())
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
            match self.fieldworks.guid() {
                Some(guid) => {
                    text.push_str(" [guid ");
                    text.push_str(guid);
                    text.push(']');
                }
                None => text.push_str(" [guid unavailable]"),
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
            let mut owner = LazySubject::new(SubjectOwner::LexEntry(entry_id));
            for allomorph in &entry.allomorphs {
                let undeclared = undeclared_segment_count(table, &allomorph.shape.shape);
                if undeclared == 0 {
                    continue;
                }
                let subject = owner.get(grammar, fieldworks_project)?;
                let where_desc = format!(
                    "Lexical entry '{}' allomorph '{}'",
                    subject.title, allomorph.shape.text
                );
                push_undeclared_segments(
                    grammar,
                    fieldworks_project,
                    table,
                    stratum.table,
                    &where_desc,
                    subject,
                    undeclared,
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
            let mut owner = LazySubject::new(SubjectOwner::MorphRule(rule_id));
            match rule {
                MorphRuleDef::AffixProcess(def) => {
                    for allomorph in &def.allomorphs {
                        for action in &allomorph.rhs {
                            check_insert_segments(
                                grammar,
                                fieldworks_project,
                                action,
                                "Morphological rule",
                                &mut owner,
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
                                &mut owner,
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
    owner: &mut LazySubject,
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
    let undeclared = undeclared_segment_count(table, &shape.shape);
    if undeclared == 0 {
        return Ok(());
    }
    let subject = owner.get(grammar, fieldworks_project)?;
    let where_desc = format!(
        "{kind_label} '{}' inserted segments '{}'",
        subject.title, shape.text
    );
    push_undeclared_segments(
        grammar,
        fieldworks_project,
        table,
        *table_id,
        &where_desc,
        subject,
        undeclared,
        findings,
    );
    Ok(())
}

enum SubjectOwner {
    LexEntry(LexEntryId),
    MorphRule(MRuleId),
}

/// Builds the owner subject only when a finding needs it, so clean items cost no naming or link work.
struct LazySubject {
    owner: SubjectOwner,
    subject: Option<GrammarHealthSubject>,
}

impl LazySubject {
    fn new(owner: SubjectOwner) -> Self {
        Self {
            owner,
            subject: None,
        }
    }

    fn get(
        &mut self,
        grammar: &Grammar,
        fieldworks_project: Option<&str>,
    ) -> Result<&GrammarHealthSubject, crate::GrammarError> {
        if self.subject.is_none() {
            self.subject = Some(match self.owner {
                SubjectOwner::LexEntry(id) => lex_entry_subject(grammar, fieldworks_project, id)?,
                SubjectOwner::MorphRule(id) => morph_rule_subject(grammar, fieldworks_project, id)?,
            });
        }
        Ok(self.subject.as_ref().expect("subject was just built"))
    }
}

/// Skips structural boundary/anchor nodes and already-declared natural classes.
fn undeclared_segment_count(table: &CharDefTable, shape: &Shape) -> usize {
    shape
        .interior()
        .filter(|(_, kind, cd, _)| {
            *kind == NodeKind::Segment
                && *cd != NO_CHAR_DEF
                && !((*cd as usize) < table.len()
                    && table.get(CharDefId(*cd)).kind() == CharDefKind::Segment)
        })
        .count()
}

#[allow(clippy::too_many_arguments)]
fn push_undeclared_segments(
    grammar: &Grammar,
    fieldworks_project: Option<&str>,
    table: &CharDefTable,
    table_id: TableId,
    where_desc: &str,
    owner_subject: &GrammarHealthSubject,
    count: usize,
    findings: &mut Vec<GrammarHealthCheckFinding>,
) {
    for _ in 0..count {
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
mod tests;
