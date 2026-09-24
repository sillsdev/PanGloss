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
//! pair to fit it. `hc-undeclared-segment` and `hc-duplicate-feature-bundle` are the stable C#
//! strings; C#'s single `hc-partial-morpheme` is split here into `hc-stem-no-grammatical-category`
//! and `hc-inflectional-affix-missing-template-slot`, so compare partial findings by subject.
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
use crate::grammar_health_presentation::{
    fieldworks_identity, fieldworks_link_from_identity, FieldWorksSource,
};
use crate::model::{
    Grammar, LexEntryId, MRuleId, MorphRuleDef, OutputAction, PartialMorphemeFacts,
    PartialMorphemeIdentity, PartialMorphemeReason, TableId,
};
use pg_shape::{NodeKind, Shape, NO_CHAR_DEF};
use pg_snapshot::{DiagnosticLevel, FwClass, FwObjectRef, ImportWarningCode, Warning};

/// The stable diagnostic codes this module reports (C# `GrammarHealthCodes`). Treat
/// [`GrammarHealthCode::wire`], not [`GrammarHealthDiagnostic::message`], as the identifier a host
/// filters/suppresses/tests on -- the message text is free to change. Serializes as the bare wire
/// string.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum GrammarHealthCode {
    UndeclaredSegment,
    DuplicateFeatureBundle,
    StemWithoutCategory,
    InflectionalAffixWithoutTemplateSlot,
    UnclassifiedAffix,
    PartialReasonUnspecified,
    ImportWarning(String),
}

impl GrammarHealthCode {
    /// Every grammar-health code in the stable contract.
    pub const ALL: &'static [Self] = &[
        Self::UndeclaredSegment,
        Self::DuplicateFeatureBundle,
        Self::StemWithoutCategory,
        Self::InflectionalAffixWithoutTemplateSlot,
        Self::UnclassifiedAffix,
        Self::PartialReasonUnspecified,
    ];

    /// The stable C# wire string this code shares with `GrammarHealthCodes`.
    pub fn wire(&self) -> &str {
        match self {
            Self::UndeclaredSegment => "hc-undeclared-segment",
            Self::DuplicateFeatureBundle => "hc-duplicate-feature-bundle",
            Self::StemWithoutCategory => "hc-stem-no-grammatical-category",
            Self::InflectionalAffixWithoutTemplateSlot => {
                "hc-inflectional-affix-missing-template-slot"
            }
            Self::UnclassifiedAffix => "hc-unclassified-affix",
            Self::PartialReasonUnspecified => "hc-partial-reason-unspecified",
            Self::ImportWarning(code) => code,
        }
    }

    /// Stable group label for a diagnostic code.
    pub fn group_name(&self) -> String {
        check_diagnostic_metadata(self).map_or_else(
            || match self {
                Self::ImportWarning(code) => {
                    let code = ImportWarningCode::from_wire_or_unregistered(code);
                    pg_snapshot::import_warning_metadata(code)
                        .group_name
                        .to_string()
                }
                _ => unreachable!("every check code has check-diagnostic metadata"),
            },
            |metadata| metadata.group_name.to_string(),
        )
    }
}

#[derive(Clone, Copy)]
enum CheckGuidance {
    UndeclaredSegment,
    DuplicateFeatureBundle,
    StemWithoutCategory,
    InflectionalAffixWithoutTemplateSlot,
    UnclassifiedAffix,
    PartialReasonUnspecified,
}

struct CheckDiagnosticMetadata {
    group_name: &'static str,
    guidance: CheckGuidance,
}

fn check_diagnostic_metadata(code: &GrammarHealthCode) -> Option<CheckDiagnosticMetadata> {
    let (group_name, guidance) = match code {
        GrammarHealthCode::UndeclaredSegment => (
            "Missing segment definition",
            CheckGuidance::UndeclaredSegment,
        ),
        GrammarHealthCode::DuplicateFeatureBundle => (
            "Duplicate segment features",
            CheckGuidance::DuplicateFeatureBundle,
        ),
        GrammarHealthCode::StemWithoutCategory => {
            ("Stem has no category", CheckGuidance::StemWithoutCategory)
        }
        GrammarHealthCode::InflectionalAffixWithoutTemplateSlot => (
            "Inflectional affix has no slot",
            CheckGuidance::InflectionalAffixWithoutTemplateSlot,
        ),
        GrammarHealthCode::UnclassifiedAffix => {
            ("Affix is unclassified", CheckGuidance::UnclassifiedAffix)
        }
        GrammarHealthCode::PartialReasonUnspecified => (
            "Partial reason is unknown",
            CheckGuidance::PartialReasonUnspecified,
        ),
        GrammarHealthCode::ImportWarning(_) => return None,
    };
    Some(CheckDiagnosticMetadata {
        group_name,
        guidance,
    })
}

fn check_guidance(code: &GrammarHealthCode, subjects: &[GrammarHealthSubject]) -> Option<String> {
    let metadata = check_diagnostic_metadata(code)?;
    use pg_snapshot::fieldworks_paths as path;
    let guidance = match metadata.guidance {
        CheckGuidance::UndeclaredSegment => {
            let kind = subjects.get(1).map(|subject| subject.kind)?;
            let (edit_path, action) = match kind {
                FwClass::MoForm | FwClass::MoStemMsa => (
                    path::LEXICON_EDIT,
                    "correct the form or add the missing phoneme",
                ),
                FwClass::MoInflAffMsa
                | FwClass::MoDerivAffMsa
                | FwClass::MoUnclassifiedAffixMsa => (
                    path::LEXICON_EDIT,
                    "correct the affix form or add the missing phoneme",
                ),
                FwClass::MoCompoundRule => (
                    path::GRAMMAR_COMPOUND_RULES,
                    "correct the rule or add the missing phoneme",
                ),
                _ => (path::GRAMMAR_PHONEMES, "add the missing phoneme"),
            };
            format!("In {edit_path}, {action} in {}.", path::GRAMMAR_PHONEMES)
        }
        CheckGuidance::DuplicateFeatureBundle => format!(
            "In {}, assign distinct feature values to these phonemes.",
            path::GRAMMAR_PHONEMES
        ),
        CheckGuidance::StemWithoutCategory => format!(
            "In {}, open the entry and set Grammatical Info. > Category.",
            path::LEXICON_EDIT
        ),
        CheckGuidance::InflectionalAffixWithoutTemplateSlot => format!(
            "In {}, assign the affix to a template slot.",
            path::GRAMMAR_CATEGORY_AFFIX_TEMPLATES
        ),
        CheckGuidance::UnclassifiedAffix => format!(
            "In {}, set the affix's Grammatical Info. to an inflectional or derivational affix.",
            path::LEXICON_EDIT
        ),
        CheckGuidance::PartialReasonUnspecified => format!(
            "In {}, check the affix's category and template slot assignments.",
            path::GRAMMAR_CATEGORY_AFFIX_TEMPLATES
        ),
    };
    Some(guidance)
}

impl serde::Serialize for GrammarHealthCode {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.wire())
    }
}

impl<'de> serde::Deserialize<'de> for GrammarHealthCode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let wire = String::deserialize(deserializer)?;
        Ok(match wire.as_str() {
            "hc-undeclared-segment" => Self::UndeclaredSegment,
            "hc-duplicate-feature-bundle" => Self::DuplicateFeatureBundle,
            "hc-stem-no-grammatical-category" => Self::StemWithoutCategory,
            "hc-inflectional-affix-missing-template-slot" => {
                Self::InflectionalAffixWithoutTemplateSlot
            }
            "hc-unclassified-affix" => Self::UnclassifiedAffix,
            "hc-partial-reason-unspecified" => Self::PartialReasonUnspecified,
            _ => Self::ImportWarning(wire),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticOrigin {
    Check,
    Import,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldWorksProjectSource {
    Argument,
    FwdataPath,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FieldWorksProject {
    pub name: Option<String>,
    pub source: Option<FieldWorksProjectSource>,
}

impl FieldWorksProject {
    fn normalized(self) -> Self {
        let name = self
            .name
            .map(|name| name.trim().to_string())
            .filter(|name| !name.is_empty());
        let source = name
            .as_ref()
            .map(|_| self.source.unwrap_or(FieldWorksProjectSource::Argument));
        Self { name, source }
    }
}

/// Why a subject cannot be opened in FieldWorks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldWorksUnavailableReason {
    MissingProject,
    GuidNotRecorded,
    InvalidGuid,
    UnsupportedKind,
}

impl FieldWorksUnavailableReason {
    pub const fn message(self) -> &'static str {
        match self {
            Self::MissingProject => "no FieldWorks project name supplied",
            Self::GuidNotRecorded => "source item has no FieldWorks GUID",
            Self::InvalidGuid => "source item has an invalid FieldWorks GUID",
            Self::UnsupportedKind => "source item has no verified FieldWorks tool",
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

/// One structured item a grammar-health diagnostic references. `title` is the only identity a human
/// report should display; `internal_id` is retained solely for tooling and navigation joins.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GrammarHealthSubject {
    pub kind: FwClass,
    pub title: String,
    pub subtitle: Option<String>,
    pub guid: Option<String>,
    pub internal_id: Option<String>,
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
    fn from_source(source: FwObjectRef) -> Self {
        let title = source
            .name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map_or_else(|| unnamed_subject_title(source.class), str::to_string);
        let guid = source.guid;
        let fieldworks = fieldworks_link_from_identity(source.class, guid.as_deref(), None);
        Self {
            kind: source.class,
            title,
            subtitle: None,
            fieldworks,
            guid,
            internal_id: None,
        }
    }

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

/// A FieldWorks item left unnamed there is still reported, as "Unnamed affix template" etc.
fn unnamed_subject_title(kind: FwClass) -> String {
    format!(
        "Unnamed {}",
        pg_snapshot::warning_metadata::fieldworks_subject_kind_label(kind)
    )
}

/// One grammar-health diagnostic. JSON uses `description` for its human-readable message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrammarHealthDiagnostic {
    pub level: DiagnosticLevel,
    pub code: GrammarHealthCode,
    pub group_name: String,
    pub origin: DiagnosticOrigin,
    pub message: String,
    pub guidance: Option<String>,
    pub subjects: Vec<GrammarHealthSubject>,
}

impl serde::Serialize for GrammarHealthDiagnostic {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        #[derive(serde::Serialize)]
        struct Wire<'a> {
            level: DiagnosticLevel,
            code: GrammarHealthCode,
            group_name: &'a str,
            origin: DiagnosticOrigin,
            description: &'a str,
            guidance: &'a Option<String>,
            #[serde(borrow)]
            subjects: &'a [GrammarHealthSubject],
        }

        Wire {
            level: self.level,
            code: self.code.clone(),
            group_name: &self.group_name,
            origin: self.origin,
            description: &self.message,
            guidance: &self.guidance,
            subjects: &self.subjects,
        }
        .serialize(serializer)
    }
}

impl GrammarHealthDiagnostic {
    pub fn from_import_warning(warning: &Warning) -> Self {
        let import_code = ImportWarningCode::from_wire_or_unregistered(&warning.code);
        let metadata = pg_snapshot::import_warning_metadata(import_code);
        let code = GrammarHealthCode::ImportWarning(warning.code.to_owned());
        let group_name = metadata.group_name.to_string();
        Self {
            level: metadata.level,
            code,
            group_name,
            origin: DiagnosticOrigin::Import,
            message: warning.message.clone(),
            guidance: metadata.guidance_for_subject(
                warning
                    .subjects
                    .first()
                    .and_then(|subject| subject.name.as_deref()),
                warning.subjects.first().map(|subject| subject.class),
            ),
            subjects: warning
                .subjects
                .iter()
                .cloned()
                .map(GrammarHealthSubject::from_source)
                .collect(),
        }
    }

    fn checked(
        level: DiagnosticLevel,
        code: GrammarHealthCode,
        message: String,
        subjects: Vec<GrammarHealthSubject>,
    ) -> Self {
        Self {
            level,
            group_name: code.group_name(),
            guidance: check_guidance(&code, &subjects),
            code,
            origin: DiagnosticOrigin::Check,
            message,
            subjects,
        }
    }

    fn log_line(&self, include_guids: bool) -> String {
        let kinds = self
            .subjects
            .iter()
            .map(|subject| {
                pg_snapshot::warning_metadata::fieldworks_subject_kind_label(subject.kind)
            })
            .collect::<Vec<_>>();
        let titles = self
            .subjects
            .iter()
            .map(|subject| subject.render_location(include_guids))
            .collect::<Vec<_>>();
        format!(
            "{} [{}] {}: {} - {}",
            self.level.wire(),
            self.group_name,
            kinds.join(", "),
            titles.join(", "),
            self.message.trim()
        )
    }
}

pub const GRAMMAR_HEALTH_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrammarHealthReportErrorCode {
    MalformedJson,
    InvalidShape,
    MissingField,
    InvalidField,
    UnsupportedSchemaVersion,
    MissingSubjects,
    InvalidDiagnostic,
    Serialization,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrammarHealthReportError {
    pub code: GrammarHealthReportErrorCode,
    pub diagnostic_index: Option<usize>,
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
            Self::InvalidDiagnostic => "invalid_diagnostic",
            Self::Serialization => "serialization",
        };
        formatter.write_str(name)
    }
}

/// The sole validated in-memory grammar-health report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrammarHealthReport {
    diagnostics: Vec<GrammarHealthDiagnostic>,
    fieldworks_project: FieldWorksProject,
}

impl GrammarHealthReport {
    pub fn new(
        diagnostics: Vec<GrammarHealthDiagnostic>,
    ) -> Result<Self, GrammarHealthReportError> {
        for (diagnostic_index, diagnostic) in diagnostics.iter().enumerate() {
            validate_diagnostic(diagnostic, diagnostic_index)?;
        }
        Ok(Self {
            diagnostics,
            fieldworks_project: FieldWorksProject::default(),
        })
    }

    pub fn with_fieldworks_project(mut self, fieldworks_project: FieldWorksProject) -> Self {
        let fieldworks_project = fieldworks_project.normalized();
        for diagnostic in &mut self.diagnostics {
            for subject in &mut diagnostic.subjects {
                subject.fieldworks = fieldworks_link_from_identity(
                    subject.kind,
                    subject.guid.as_deref(),
                    fieldworks_project.name.as_deref(),
                );
            }
        }
        self.fieldworks_project = fieldworks_project;
        self
    }

    pub fn diagnostics(&self) -> &[GrammarHealthDiagnostic] {
        &self.diagnostics
    }

    pub fn len(&self) -> usize {
        self.diagnostics.len()
    }

    pub fn is_empty(&self) -> bool {
        self.diagnostics.is_empty()
    }

    pub fn to_json(&self) -> Result<String, GrammarHealthReportError> {
        serde_json::to_string_pretty(self).map_err(|error| GrammarHealthReportError {
            code: GrammarHealthReportErrorCode::Serialization,
            diagnostic_index: None,
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
        let fieldworks_project = required_field(object, "fieldworks_project", None)?;
        let diagnostic_values =
            required_field::<Vec<serde_json::Value>>(object, "diagnostics", None)?;
        let mut diagnostics = Vec::with_capacity(diagnostic_values.len());
        for (diagnostic_index, value) in diagnostic_values.into_iter().enumerate() {
            diagnostics.push(decode_diagnostic(value, diagnostic_index)?);
        }
        Self::new(diagnostics).map(|report| report.with_fieldworks_project(fieldworks_project))
    }

    fn summary(&self) -> Vec<GrammarHealthSummaryRow> {
        let mut rows = std::collections::BTreeMap::<String, GrammarHealthSummaryRow>::new();
        for diagnostic in &self.diagnostics {
            rows.entry(diagnostic.code.wire().to_string())
                .and_modify(|row| row.count += 1)
                .or_insert_with(|| GrammarHealthSummaryRow {
                    code: diagnostic.code.wire().to_string(),
                    group_name: diagnostic.group_name.clone(),
                    level: diagnostic.level,
                    count: 1,
                });
        }
        rows.into_values().collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GrammarHealthSummaryRow {
    pub code: String,
    pub group_name: String,
    pub level: DiagnosticLevel,
    pub count: usize,
}

impl std::ops::Deref for GrammarHealthReport {
    type Target = [GrammarHealthDiagnostic];

    fn deref(&self) -> &Self::Target {
        self.diagnostics()
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
            fieldworks_project: &'a FieldWorksProject,
            summary: Vec<GrammarHealthSummaryRow>,
            diagnostics: &'a [GrammarHealthDiagnostic],
        }

        Wire {
            schema_version: GRAMMAR_HEALTH_SCHEMA_VERSION,
            fieldworks_project: &self.fieldworks_project,
            summary: self.summary(),
            diagnostics: &self.diagnostics,
        }
        .serialize(serializer)
    }
}

fn report_error(
    code: GrammarHealthReportErrorCode,
    diagnostic_index: Option<usize>,
    field: Option<&str>,
    message: String,
) -> GrammarHealthReportError {
    GrammarHealthReportError {
        code,
        diagnostic_index,
        field: field.map(str::to_string),
        message,
    }
}

fn required_field<T: serde::de::DeserializeOwned>(
    object: &serde_json::Map<String, serde_json::Value>,
    field: &str,
    diagnostic_index: Option<usize>,
) -> Result<T, GrammarHealthReportError> {
    let value = object.get(field).ok_or_else(|| {
        report_error(
            GrammarHealthReportErrorCode::MissingField,
            diagnostic_index,
            Some(field),
            format!("missing field {field}"),
        )
    })?;
    serde_json::from_value(value.clone()).map_err(|error| {
        report_error(
            GrammarHealthReportErrorCode::InvalidField,
            diagnostic_index,
            Some(field),
            error.to_string(),
        )
    })
}

fn decode_diagnostic(
    value: serde_json::Value,
    diagnostic_index: usize,
) -> Result<GrammarHealthDiagnostic, GrammarHealthReportError> {
    let object = value.as_object().ok_or_else(|| {
        report_error(
            GrammarHealthReportErrorCode::InvalidShape,
            Some(diagnostic_index),
            None,
            "diagnostic must be an object".to_string(),
        )
    })?;
    let level: DiagnosticLevel = required_field(object, "level", Some(diagnostic_index))?;
    let code: GrammarHealthCode = required_field(object, "code", Some(diagnostic_index))?;
    let group_name = required_field(object, "group_name", Some(diagnostic_index))?;
    let origin = required_field(object, "origin", Some(diagnostic_index))?;
    let message = required_field(object, "description", Some(diagnostic_index))?;
    let guidance = required_field(object, "guidance", Some(diagnostic_index))?;
    let subjects = required_field(object, "subjects", Some(diagnostic_index))?;
    Ok(GrammarHealthDiagnostic {
        level,
        code,
        group_name,
        origin,
        message,
        guidance,
        subjects,
    })
}

fn validate_diagnostic(
    diagnostic: &GrammarHealthDiagnostic,
    diagnostic_index: usize,
) -> Result<(), GrammarHealthReportError> {
    let prefix = format!(
        "grammar-health diagnostic {diagnostic_index} ({})",
        diagnostic.code.wire()
    );
    let missing = |field: &'static str, message: String| {
        Err(report_error(
            GrammarHealthReportErrorCode::InvalidDiagnostic,
            Some(diagnostic_index),
            Some(field),
            message,
        ))
    };
    if diagnostic.message.trim().is_empty() {
        return missing("description", format!("{prefix}: missing description"));
    }
    if diagnostic.group_name.trim().is_empty() {
        return missing("group_name", format!("{prefix}: missing group_name"));
    }
    for (subject_index, subject) in diagnostic.subjects.iter().enumerate() {
        let subject_prefix = format!("{prefix} subject {subject_index}");
        if subject.title.trim().is_empty() {
            return missing("title", format!("{subject_prefix}: missing title"));
        }
        if subject
            .internal_id
            .as_deref()
            .is_some_and(|internal_id| internal_id.trim().is_empty())
        {
            return missing(
                "internal_id",
                format!("{subject_prefix}: missing internal_id"),
            );
        }
        match &subject.fieldworks {
            FieldWorksLink::Available { guid, tool, url } => {
                if pg_snapshot::canonical_guid(guid).is_none() {
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

/// Render complete diagnostics as one nonblank plain-text line per diagnostic.
///
/// include_guids is opt-in because the title remains the human identity in the default log.
pub fn render_log(report: &GrammarHealthReport, include_guids: bool) -> String {
    report
        .diagnostics()
        .iter()
        .map(|diagnostic| diagnostic.log_line(include_guids))
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
    let name = fieldworks_project.map(str::to_string);
    check_grammar_health_with_project(
        grammar,
        FieldWorksProject {
            source: name.as_ref().map(|_| FieldWorksProjectSource::Argument),
            name,
        },
    )
}

/// Runs every registered check with the caller's project name and its source.
pub fn check_grammar_health_with_project(
    grammar: &Grammar,
    fieldworks_project: FieldWorksProject,
) -> Result<GrammarHealthReport, crate::GrammarError> {
    let diagnostics = check_grammar_health_diagnostics(grammar)?;
    let report = GrammarHealthReport::new(diagnostics)
        .map_err(|error| crate::GrammarError::Semantic(error.to_string()))?
        .with_fieldworks_project(fieldworks_project);
    Ok(report)
}

/// Runs every registered check and returns the diagnostics without assembling a report.
/// Callers that combine check diagnostics with import diagnostics can build one report after merging.
pub fn check_grammar_health_diagnostics(
    grammar: &Grammar,
) -> Result<Vec<GrammarHealthDiagnostic>, crate::GrammarError> {
    let partial_facts = grammar.partial_morpheme_facts()?;
    let mut diagnostics = Vec::new();
    check_duplicate_feature_bundles(grammar, None, &mut diagnostics)?;
    check_undeclared_segments(grammar, None, &mut diagnostics)?;
    check_partial_morphemes(grammar, None, &partial_facts, &mut diagnostics)?;
    Ok(diagnostics)
}

// --- hc-duplicate-feature-bundle -------------------------------------------------------------

/// Distinct-bundle segments only; skipped for a zero-feature grammar, where every bundle is the same empty struct by construction.
fn check_duplicate_feature_bundles(
    grammar: &Grammar,
    fieldworks_project: Option<&str>,
    diagnostics: &mut Vec<GrammarHealthDiagnostic>,
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
            let diagnostic = GrammarHealthDiagnostic::checked(
                DiagnosticLevel::Warning,
                GrammarHealthCode::DuplicateFeatureBundle,
                format!(
                    "Phonemes {} share the same feature values.",
                    names.join(", ")
                ),
                subjects,
            );
            diagnostics.push(diagnostic);
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
        .unwrap_or("Phonemes")
}

fn make_subject(
    source: FwObjectRef,
    title: String,
    subtitle: Option<String>,
    internal_id: String,
    fieldworks_project: Option<&str>,
) -> GrammarHealthSubject {
    let source = source.name(title.clone());
    let fieldworks =
        fieldworks_link_from_identity(source.class, source.guid.as_deref(), fieldworks_project);
    GrammarHealthSubject {
        kind: source.class,
        title,
        subtitle,
        guid: source.guid,
        internal_id: Some(internal_id),
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
        fieldworks_identity(grammar, &FieldWorksSource::Table),
        table_display_name(table).to_string(),
        None,
        format!("table#{}:{}", id.0, table.xml_id()),
        fieldworks_project,
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
        fieldworks_identity(grammar, &FieldWorksSource::CharDef(table_id, id)),
        first_representation(cd).to_string(),
        Some(format!("in {}", table_display_name(table))),
        format!("table#{}:char_def#{}:{}", table_id.0, id.0, cd.xml_id()),
        fieldworks_project,
    )
}

fn partial_stem_subject(
    grammar: &Grammar,
    fieldworks_project: Option<&str>,
    id: LexEntryId,
) -> Result<GrammarHealthSubject, crate::GrammarError> {
    let (title, subtitle) = grammar.lex_entry_display_parts(id)?;
    let internal_id = grammar.lex_entry_internal_id(id)?;
    Ok(make_subject(
        fieldworks_identity(grammar, &FieldWorksSource::LexEntry(id)),
        title,
        subtitle,
        internal_id,
        fieldworks_project,
    ))
}

fn lex_entry_subject(
    grammar: &Grammar,
    fieldworks_project: Option<&str>,
    id: LexEntryId,
) -> Result<GrammarHealthSubject, crate::GrammarError> {
    let (title, subtitle) = grammar.lex_entry_display_parts(id)?;
    let internal_id = grammar.lex_entry_internal_id(id)?;
    Ok(make_subject(
        fieldworks_identity(grammar, &FieldWorksSource::LexEntry(id)),
        title,
        subtitle,
        internal_id,
        fieldworks_project,
    ))
}

fn morph_rule_subject(
    grammar: &Grammar,
    fieldworks_project: Option<&str>,
    id: MRuleId,
) -> Result<GrammarHealthSubject, crate::GrammarError> {
    let (title, subtitle) = grammar.morph_rule_display_parts(id)?;
    let internal_id = grammar.morph_rule_internal_id(id)?;
    Ok(make_subject(
        fieldworks_identity(grammar, &FieldWorksSource::MorphRule(id)),
        title,
        subtitle,
        internal_id,
        fieldworks_project,
    ))
}

// --- hc-undeclared-segment ---------------------------------------------------------------------

/// Checks lex-entry allomorphs and InsertSegments in a stratum's ordinary mrules.
fn check_undeclared_segments(
    grammar: &Grammar,
    fieldworks_project: Option<&str>,
    diagnostics: &mut Vec<GrammarHealthDiagnostic>,
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
                push_undeclared_segments(
                    grammar,
                    fieldworks_project,
                    table,
                    stratum.table,
                    subject,
                    undeclared,
                    diagnostics,
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
                                &mut owner,
                                diagnostics,
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
                                &mut owner,
                                diagnostics,
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
    owner: &mut LazySubject,
    diagnostics: &mut Vec<GrammarHealthDiagnostic>,
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
    push_undeclared_segments(
        grammar,
        fieldworks_project,
        table,
        *table_id,
        subject,
        undeclared,
        diagnostics,
    );
    Ok(())
}

enum SubjectOwner {
    LexEntry(LexEntryId),
    MorphRule(MRuleId),
}

/// Builds the owner subject only when a diagnostic needs it, so clean items cost no naming or link work.
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
    owner_subject: &GrammarHealthSubject,
    count: usize,
    diagnostics: &mut Vec<GrammarHealthDiagnostic>,
) {
    let kind = undeclared_segment_subject_label(owner_subject.kind);
    let description = format!(
        "{kind} '{}' uses a phoneme that is missing from the phoneme inventory.",
        owner_subject.title
    );
    for _ in 0..count {
        let diagnostic = GrammarHealthDiagnostic::checked(
            DiagnosticLevel::Warning,
            GrammarHealthCode::UndeclaredSegment,
            description.clone(),
            vec![
                table_subject(grammar, fieldworks_project, table_id, table),
                owner_subject.clone(),
            ],
        );
        diagnostics.push(diagnostic);
    }
}

fn undeclared_segment_subject_label(kind: FwClass) -> &'static str {
    match kind {
        FwClass::LexEntry | FwClass::MoForm => "Lexical entry",
        FwClass::MoStemMsa => "Stem",
        FwClass::MoInflAffMsa | FwClass::MoDerivAffMsa | FwClass::MoUnclassifiedAffixMsa => {
            affix_kind_label(kind)
        }
        FwClass::MoCompoundRule => "Compound rule",
        _ => "Grammar item",
    }
}

fn affix_kind_label(kind: FwClass) -> &'static str {
    match kind {
        FwClass::MoInflAffMsa => "Inflectional affix",
        FwClass::MoDerivAffMsa => "Derivational affix",
        FwClass::MoUnclassifiedAffixMsa => "Unclassified affix",
        _ => "Affix",
    }
}

// --- hc-partial-morpheme -------------------------------------------------------------------

/// Consume the canonical typed inventory; this warning path does not inspect rule definitions.
fn check_partial_morphemes(
    grammar: &Grammar,
    fieldworks_project: Option<&str>,
    facts: &PartialMorphemeFacts,
    diagnostics: &mut Vec<GrammarHealthDiagnostic>,
) -> Result<(), crate::GrammarError> {
    for identity in facts.identities() {
        match identity {
            PartialMorphemeIdentity::LexicalEntry { id, reason, .. } => {
                let subject = partial_stem_subject(grammar, fieldworks_project, *id)?;
                diagnostics.push(partial_diagnostic(*reason, subject));
            }
            PartialMorphemeIdentity::MorphologicalRule { id, reason, .. } => {
                let subject = morph_rule_subject(grammar, fieldworks_project, *id)?;
                diagnostics.push(partial_diagnostic(*reason, subject));
            }
        }
    }
    Ok(())
}

fn partial_diagnostic(
    reason: PartialMorphemeReason,
    subject: GrammarHealthSubject,
) -> GrammarHealthDiagnostic {
    let name = subject
        .subtitle
        .as_deref()
        .filter(|subtitle| !subtitle.trim().is_empty())
        .map(|subtitle| format!("'{}' ({subtitle})", subject.title))
        .unwrap_or_else(|| format!("'{}'", subject.title));
    let (code, message) = match reason {
        PartialMorphemeReason::StemWithoutCategory => (
            GrammarHealthCode::StemWithoutCategory,
            format!("Lexical entry {name} has no grammatical category."),
        ),
        PartialMorphemeReason::InflectionalAffixWithoutTemplateSlot => (
            GrammarHealthCode::InflectionalAffixWithoutTemplateSlot,
            format!(
                "{} {name} has no template slot.",
                affix_kind_label(subject.kind)
            ),
        ),
        PartialMorphemeReason::UnclassifiedAffix => (
            GrammarHealthCode::UnclassifiedAffix,
            format!("Affix {name} is unclassified, so it can attach anywhere."),
        ),
        PartialMorphemeReason::Unspecified => (
            GrammarHealthCode::PartialReasonUnspecified,
            format!(
                "Affix {name} is marked partial in the grammar file; the reason is not recorded."
            ),
        ),
    };
    GrammarHealthDiagnostic::checked(DiagnosticLevel::Warning, code, message, vec![subject])
}

#[cfg(test)]
mod tests;
