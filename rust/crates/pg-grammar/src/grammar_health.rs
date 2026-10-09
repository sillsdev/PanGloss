//! Grammar-authoring health checks and versioned reports with source subjects and owned advice.
//!
//! Reports inspect the compiled grammar without changing compilation or parsing. Canonical-fact
//! inspection errors propagate to the caller. FST publication policy remains in `pg-health`.
//! Segment natural-class membership cannot identify undeclared tables after grammar loading;
//! the model records only per-table character IDs, so that C# inspection is not represented here.

use crate::chardef::{CharDef, CharDefId, CharDefKind, CharDefTable};
use crate::grammar_health_presentation::{fieldworks_identity, fieldworks_link, FieldWorksSource};
use crate::model::{
    Grammar, LexEntryId, MRuleId, MorphRuleDef, OutputAction, PartialMorphemeFacts,
    PartialMorphemeIdentity, PartialMorphemeReason, TableId,
};
use pg_shape::{NodeKind, Shape, NO_CHAR_DEF};
use pg_snapshot::warning_metadata::{DiagnosticAdvice, FieldWorksPlace};
use pg_snapshot::{
    DiagnosticLevel, FwClass, FwObjectRef, FwOpenTarget, FwSubjectStatus, ImportWarningCode,
    Warning,
};

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
    ProvisionalPhonemeFeatures,
    StoredAnalysisNoLongerParses,
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
        Self::ProvisionalPhonemeFeatures,
        Self::StoredAnalysisNoLongerParses,
    ];

    /// The stable wire string for this diagnostic code.
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
            Self::ProvisionalPhonemeFeatures => "provisional.phoneme-features",
            Self::StoredAnalysisNoLongerParses => "grammar.stored-analysis.no-longer-parses",
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

    /// The level every diagnostic with this code carries.
    pub fn level(&self) -> DiagnosticLevel {
        match (check_diagnostic_metadata(self), self) {
            (Some(metadata), _) => metadata.level,
            (None, Self::ImportWarning(code)) => {
                let code = ImportWarningCode::from_wire_or_unregistered(code);
                pg_snapshot::import_warning_metadata(code).level
            }
            (None, _) => unreachable!("every check code has check-diagnostic metadata"),
        }
    }
}

struct CheckDiagnosticMetadata {
    group_name: &'static str,
    level: DiagnosticLevel,
    advice: DiagnosticAdvice,
}

/// Each level's rationale is in docs/grammar-diagnostics.md.
fn check_diagnostic_metadata(code: &GrammarHealthCode) -> Option<CheckDiagnosticMetadata> {
    use pg_snapshot::warning_metadata::{ALLOMORPHS_HELP, MODELLING_HELP, STEMS_HELP};
    use DiagnosticLevel::{Error, Info, Warning};
    let (title, level, explanation, guidance, places, help) = match code {
        GrammarHealthCode::UndeclaredSegment => (
            "Missing segment definition", Warning,
            "A loaded allomorph or morphological rule uses a segment absent from the grammar's declared phoneme inventory. Check the description for the form and segment; this check does not inspect every natural-class reference.",
            "In Lexicon > Lexicon Edit, check the named form's spelling. If the spelling is intended, add its phoneme and grapheme representation in Grammar > Phonemes; for an inserted compound-rule segment, check the named rule in Grammar > Compound Rules instead.",
            vec![place("lexiconEdit", "Form"), place("phonemeEdit", "In Orthography as"), place("compoundRuleAdvancedEdit", "Output")], ALLOMORPHS_HELP,
        ),
        GrammarHealthCode::DuplicateFeatureBundle => (
            "Duplicate segment features", Error,
            "Two declared phonemes have the same phonological feature values, so feature-based lookup cannot distinguish them. This check runs only when a phonological feature system exists.",
            "In Grammar > Phonemes, select each named phoneme and open its Phonological Features chooser. Correct missing or unintended values if the sounds must be distinguished. If the feature match is intentional, preserve it and report a parser limitation if that distinction is needed.",
            vec![place("phonemeEdit", "Phonological Features")], ALLOMORPHS_HELP,
        ),
        GrammarHealthCode::StemWithoutCategory => (
            "Stem has no category", Error,
            "This stem is marked partial because its grammatical category is missing. Its category restrictions cannot be enforced as authored.",
            "In Lexicon > Lexicon Edit, open the named entry and set Grammatical Info. > Category for its stem analysis.",
            vec![place("lexiconEdit", "Grammatical Info. > Category")], STEMS_HELP,
        ),
        GrammarHealthCode::InflectionalAffixWithoutTemplateSlot => (
            "Inflectional affix has no slot", Error,
            "This inflectional affix is marked partial because it has no template slot. PanGloss can retain a partial rule with incomplete template restrictions. Check the finding against the intended template restrictions.",
            "In Grammar > Category Edit > the category's Affix Templates, right-click the intended slot and choose Add inflectional affix(es) to that slot. Select the existing affix. In Lexicon > Lexicon Edit, check Grammatical Info. > Category and Slots for its inflectional analysis.",
            vec![place("posEdit", "Affix Templates"), place("lexiconEdit", "Grammatical Info. > Slots")], MODELLING_HELP,
        ),
        GrammarHealthCode::UnclassifiedAffix => (
            "Affix is unclassified", Error,
            "The affix has neither an inflectional nor a derivational analysis. It is partial and can attach without the intended classification restrictions.",
            "In Lexicon > Lexicon Edit, classify the affix's Grammatical Info. as inflectional or derivational if that describes it, then supply the category and restrictions that analysis requires.",
            vec![place("lexiconEdit", "Grammatical Info.")], MODELLING_HELP,
        ),
        GrammarHealthCode::PartialReasonUnspecified => (
            "Partial reason is unknown", Error,
            "The grammar marks this morpheme partial but records no reason. PanGloss cannot identify a specific missing FieldWorks field from this finding.",
            "Inspect the named morpheme and its grammatical analysis in Lexicon > Lexicon Edit. If the analysis is complete, report the finding with its description and PanGloss version; no specific correction has been verified.",
            vec![], MODELLING_HELP,
        ),
        GrammarHealthCode::ProvisionalPhonemeFeatures => (
            "Phoneme has no features", Info,
            "This grammar uses a natural class requiring a phonological feature value. An authored phoneme has no features, so PanGloss excludes it from such classes while keeping its explicit segment-list memberships. Unconstrained classes still match it.",
            "In Grammar > Phonemes, select the named phoneme and assign its Phonological Features to replace this provisional definition.",
            vec![place("phonemeEdit", "Phonological Features")], ALLOMORPHS_HELP,
        ),
        GrammarHealthCode::StoredAnalysisNoLongerParses => (
            "Stored analysis changed", Info,
            "A FieldWorks stored analysis is absent from PanGloss's confirmed analyses for its wordform. The reported forward-synthesis trace shows authored phonological rules that changed the surface, or explains why the loss could not be attributed to a rule.",
            "Check whether the named rule is meant to apply to these morphs. If so, update or remove the stored analysis in Lexicon > Lexicon Edit. If not, restrict the rule's environment in Grammar > Phonological Rules.",
            vec![place("PhonologicalRuleEdit", "Environment"), place("lexiconEdit", "Analysis")], ALLOMORPHS_HELP,
        ),
        GrammarHealthCode::ImportWarning(_) => return None,
    };
    Some(CheckDiagnosticMetadata {
        group_name: title,
        level,
        advice: DiagnosticAdvice {
            title,
            explanation,
            guidance,
            fieldworks_places: places,
            help_body: Some(help),
        },
    })
}

fn place(tool: &str, field: &str) -> FieldWorksPlace {
    FieldWorksPlace {
        tool: tool.to_string(),
        field: field.to_string(),
    }
}

fn diagnostic_advice(code: &GrammarHealthCode) -> Option<DiagnosticAdvice> {
    match code {
        GrammarHealthCode::ImportWarning(wire) => {
            pg_snapshot::warning_metadata::import_diagnostic_advice(
                &ImportWarningCode::from_wire_or_unregistered(wire),
            )
        }
        _ => check_diagnostic_metadata(code).map(|metadata| metadata.advice),
    }
}

fn check_guidance(code: &GrammarHealthCode, _subjects: &[GrammarHealthSubject]) -> Option<String> {
    check_diagnostic_metadata(code).map(|metadata| metadata.advice.guidance.to_string())
}

fn diagnostic_help_path(code: &GrammarHealthCode) -> Option<String> {
    diagnostic_advice(code).map(|_| format!("docs/diagnostics/{}.md", code.wire()))
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
            "provisional.phoneme-features" => Self::ProvisionalPhonemeFeatures,
            "grammar.stored-analysis.no-longer-parses" => Self::StoredAnalysisNoLongerParses,
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
    UnresolvedReference,
    ProjectSettings,
}

impl FieldWorksUnavailableReason {
    pub const fn message(self) -> &'static str {
        match self {
            Self::MissingProject => "no FieldWorks project name supplied",
            Self::GuidNotRecorded => "source item has no FieldWorks GUID",
            Self::InvalidGuid => "source item has an invalid FieldWorks GUID",
            Self::UnsupportedKind => "source item has no verified FieldWorks tool",
            Self::UnresolvedReference => {
                "referenced item is absent or not usable as the expected class"
            }
            Self::ProjectSettings => "project-wide setting has no individual FieldWorks object",
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
    pub status: FwSubjectStatus,
    pub field: Option<String>,
    pub source_class: Option<String>,
    pub title: String,
    pub subtitle: Option<String>,
    pub guid: Option<String>,
    pub internal_id: Option<String>,
    /// Where FieldWorks opens this subject when its kind alone cannot say.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opens_in: Option<FwOpenTarget>,
    pub fieldworks: FieldWorksLink,
}

/// Only the `prefix#N` forms this crate mints as internal ids; authored names are never rejected.
fn is_internal_subject_label(title: &str) -> bool {
    let lower = title.trim().to_ascii_lowercase();
    let Some((prefix, suffix)) = lower.split_once('#') else {
        return false;
    };
    let digit_count = suffix.bytes().take_while(u8::is_ascii_digit).count();
    ["char_def", "lex_entry", "mrule", "morph_rule", "table"].contains(&prefix)
        && digit_count > 0
        && (digit_count == suffix.len() || suffix[digit_count..].starts_with(':'))
}

impl GrammarHealthSubject {
    fn from_source(mut source: FwObjectRef) -> Self {
        if source.status == FwSubjectStatus::Object && source.opens_in.is_none() {
            source.opens_in = crate::grammar_health_presentation::open_target_for_raw_class(
                source.source_class.as_deref(),
                source.guid.as_deref(),
            );
        }
        let title = source
            .name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map_or_else(|| unnamed_subject_title(source.class), str::to_string);
        let guid = source.guid;
        let fieldworks = subject_link(
            source.status,
            source.class,
            guid.as_deref(),
            source.opens_in.as_ref(),
            None,
        );
        Self {
            kind: source.class,
            status: source.status,
            field: source.field,
            source_class: source.source_class,
            title,
            subtitle: None,
            fieldworks,
            guid,
            internal_id: None,
            opens_in: source.opens_in,
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

fn subject_link(
    status: FwSubjectStatus,
    class: FwClass,
    guid: Option<&str>,
    target: Option<&FwOpenTarget>,
    project: Option<&str>,
) -> FieldWorksLink {
    match status {
        FwSubjectStatus::Object => fieldworks_link(class, guid, target, project),
        FwSubjectStatus::UnresolvedReference => FieldWorksLink::Unavailable {
            reason: FieldWorksUnavailableReason::UnresolvedReference,
            guid: guid.map(str::to_string),
        },
        FwSubjectStatus::ProjectSettings => FieldWorksLink::Unavailable {
            reason: FieldWorksUnavailableReason::ProjectSettings,
            guid: None,
        },
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
    pub explanation: Option<String>,
    pub help_path: Option<String>,
    pub help_body: Option<String>,
    pub fieldworks_places: Vec<FieldWorksPlace>,
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
            title: &'a str,
            explanation: &'a Option<String>,
            help_path: &'a Option<String>,
            help_body: &'a Option<String>,
            fieldworks_places: &'a [FieldWorksPlace],
            scope: &'static str,
            #[serde(borrow)]
            subjects: &'a [GrammarHealthSubject],
        }

        Wire {
            title: &self.group_name,
            explanation: &self.explanation,
            help_path: &self.help_path,
            help_body: &self.help_body,
            fieldworks_places: &self.fieldworks_places,
            scope: self.scope(),
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
    fn scope(&self) -> &'static str {
        if self
            .subjects
            .iter()
            .any(|subject| subject.status == FwSubjectStatus::Object)
        {
            "object"
        } else if self
            .subjects
            .iter()
            .any(|subject| subject.status == FwSubjectStatus::UnresolvedReference)
        {
            "unresolved_reference"
        } else {
            "project_settings"
        }
    }

    pub fn from_import_warning(warning: &Warning) -> Self {
        let import_code = ImportWarningCode::from_wire_or_unregistered(&warning.code);
        let metadata = pg_snapshot::import_warning_metadata(import_code);
        let code = GrammarHealthCode::ImportWarning(warning.code.to_owned());
        let group_name = metadata.group_name.to_string();
        let advice = diagnostic_advice(&code);
        Self {
            explanation: advice.as_ref().map(|advice| advice.explanation.to_string()),
            help_path: diagnostic_help_path(&code),
            help_body: advice
                .as_ref()
                .and_then(|advice| advice.help_body)
                .map(str::to_string),
            fieldworks_places: advice.map_or_else(Vec::new, |advice| advice.fieldworks_places),
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

    /// Builds a check finding from FieldWorks source subjects using the registered diagnostic metadata.
    pub fn from_check(
        code: GrammarHealthCode,
        message: impl Into<String>,
        sources: Vec<FwObjectRef>,
    ) -> Self {
        let subjects = sources
            .into_iter()
            .map(GrammarHealthSubject::from_source)
            .collect();
        Self::checked(code, message.into(), subjects)
    }

    fn checked(
        code: GrammarHealthCode,
        message: String,
        subjects: Vec<GrammarHealthSubject>,
    ) -> Self {
        let advice = diagnostic_advice(&code);
        Self {
            explanation: advice.as_ref().map(|advice| advice.explanation.to_string()),
            help_path: diagnostic_help_path(&code),
            help_body: advice
                .as_ref()
                .and_then(|advice| advice.help_body)
                .map(str::to_string),
            fieldworks_places: advice.map_or_else(Vec::new, |advice| advice.fieldworks_places),
            level: code.level(),
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

pub const GRAMMAR_HEALTH_SCHEMA_VERSION: u32 = 4;

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
    locale: String,
}

impl GrammarHealthReport {
    pub fn new(
        diagnostics: Vec<GrammarHealthDiagnostic>,
    ) -> Result<Self, GrammarHealthReportError> {
        let report = Self {
            diagnostics,
            fieldworks_project: FieldWorksProject::default(),
            locale: "en".to_string(),
        }
        .with_fieldworks_project(FieldWorksProject::default());
        Self::validated(report.diagnostics, report.fieldworks_project)
    }

    fn validated(
        diagnostics: Vec<GrammarHealthDiagnostic>,
        fieldworks_project: FieldWorksProject,
    ) -> Result<Self, GrammarHealthReportError> {
        let mut metadata = std::collections::HashMap::new();
        for (diagnostic_index, diagnostic) in diagnostics.iter().enumerate() {
            validate_diagnostic(diagnostic, diagnostic_index)?;
            let details = (
                &diagnostic.group_name,
                diagnostic.level,
                diagnostic.origin,
                &diagnostic.explanation,
                &diagnostic.help_path,
                &diagnostic.help_body,
                &diagnostic.fieldworks_places,
            );
            if metadata
                .insert(diagnostic.code.wire(), details)
                .is_some_and(|previous| previous != details)
            {
                return Err(report_error(
                    GrammarHealthReportErrorCode::InvalidDiagnostic,
                    Some(diagnostic_index),
                    Some("code"),
                    "conflicting metadata for one code".to_string(),
                ));
            }
        }
        Ok(Self {
            diagnostics,
            fieldworks_project,
            locale: "en".to_string(),
        })
    }

    pub fn with_fieldworks_project(mut self, fieldworks_project: FieldWorksProject) -> Self {
        let fieldworks_project = fieldworks_project.normalized();
        for diagnostic in &mut self.diagnostics {
            for subject in &mut diagnostic.subjects {
                subject.fieldworks = subject_link(
                    subject.status,
                    subject.kind,
                    subject.guid.as_deref(),
                    subject.opens_in.as_ref(),
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
        let locale = required_field::<String>(object, "locale", None)?;
        if locale.trim().is_empty() {
            return Err(report_error(
                GrammarHealthReportErrorCode::InvalidField,
                None,
                Some("locale"),
                "missing advice locale".to_string(),
            ));
        }
        let summary: Vec<GrammarHealthSummaryRow> = required_field(object, "summary", None)?;
        let mut report = Self::validated(diagnostics, fieldworks_project)?;
        report.locale = locale;
        if report.fieldworks_project.clone().normalized() != report.fieldworks_project {
            return Err(report_error(
                GrammarHealthReportErrorCode::InvalidField,
                None,
                Some("fieldworks_project"),
                "inconsistent project identity".to_string(),
            ));
        }
        if summary != report.summary() {
            return Err(report_error(
                GrammarHealthReportErrorCode::InvalidField,
                None,
                Some("summary"),
                "summary does not agree with diagnostics".to_string(),
            ));
        }
        for (index, diagnostic) in report.diagnostics.iter().enumerate() {
            for subject in &diagnostic.subjects {
                if subject.fieldworks
                    != subject_link(
                        subject.status,
                        subject.kind,
                        subject.guid.as_deref(),
                        subject.opens_in.as_ref(),
                        report.fieldworks_project.name.as_deref(),
                    )
                {
                    return Err(report_error(
                        GrammarHealthReportErrorCode::InvalidDiagnostic,
                        Some(index),
                        Some("fieldworks"),
                        "navigation does not agree with subject and project".to_string(),
                    ));
                }
            }
        }
        Ok(report)
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
            locale: &'a str,
            fieldworks_project: &'a FieldWorksProject,
            summary: Vec<GrammarHealthSummaryRow>,
            diagnostics: &'a [GrammarHealthDiagnostic],
        }

        Wire {
            schema_version: GRAMMAR_HEALTH_SCHEMA_VERSION,
            locale: &self.locale,
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
    let subject_values: Vec<serde_json::Value> =
        required_field(object, "subjects", Some(diagnostic_index))?;
    for subject in &subject_values {
        let kind = subject
            .get("kind")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        if kind
            != serde_json::to_value(FwClass::from_wire(kind))
                .ok()
                .and_then(|value| value.as_str().map(str::to_owned))
                .unwrap_or_default()
        {
            return Err(report_error(
                GrammarHealthReportErrorCode::InvalidField,
                Some(diagnostic_index),
                Some("subjects.kind"),
                "unsupported subject kind".to_string(),
            ));
        }
    }
    let subjects =
        serde_json::from_value(serde_json::Value::Array(subject_values)).map_err(|error| {
            report_error(
                GrammarHealthReportErrorCode::InvalidField,
                Some(diagnostic_index),
                Some("subjects"),
                error.to_string(),
            )
        })?;
    let explanation = required_field(object, "explanation", Some(diagnostic_index))?;
    let help_path = required_field(object, "help_path", Some(diagnostic_index))?;
    let help_body = required_field(object, "help_body", Some(diagnostic_index))?;
    let fieldworks_places = required_field(object, "fieldworks_places", Some(diagnostic_index))?;
    let title: String = required_field(object, "title", Some(diagnostic_index))?;
    let diagnostic = GrammarHealthDiagnostic {
        level,
        code,
        group_name,
        origin,
        message,
        guidance,
        explanation,
        help_path,
        help_body,
        fieldworks_places,
        subjects,
    };
    let scope: String = required_field(object, "scope", Some(diagnostic_index))?;
    if title != diagnostic.group_name || scope != diagnostic.scope() {
        return Err(report_error(
            GrammarHealthReportErrorCode::InvalidDiagnostic,
            Some(diagnostic_index),
            Some("scope/title"),
            "scope or title does not agree with diagnostic".to_string(),
        ));
    }
    Ok(diagnostic)
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
    let code = diagnostic.code.wire();
    if code.is_empty()
        || code.contains("..")
        || !code
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
    {
        return missing("code", format!("{prefix}: invalid diagnostic code"));
    }
    if diagnostic.message.trim().is_empty() {
        return missing("description", format!("{prefix}: missing description"));
    }
    if diagnostic.group_name.trim().is_empty() {
        return missing("group_name", format!("{prefix}: missing group_name"));
    }
    if diagnostic.subjects.is_empty() {
        return missing("subjects", format!("{prefix}: no structured subject"));
    }
    let advice_presence = [
        diagnostic.explanation.is_some(),
        diagnostic.guidance.is_some(),
        diagnostic.help_path.is_some(),
    ];
    if (advice_presence.iter().any(|present| *present)
        || diagnostic.help_body.is_some()
        || !diagnostic.fieldworks_places.is_empty())
        && !advice_presence.iter().all(|present| *present)
    {
        return missing(
            "advice",
            format!("{prefix}: incomplete owned diagnostic advice"),
        );
    }
    if diagnostic_advice(&diagnostic.code).is_some()
        && (diagnostic.explanation.is_none()
            || diagnostic.guidance.is_none()
            || diagnostic.help_path.is_none())
    {
        return missing(
            "advice",
            format!("{prefix}: missing owned diagnostic advice"),
        );
    }
    for (field, text) in [
        ("explanation", &diagnostic.explanation),
        ("guidance", &diagnostic.guidance),
        ("help_body", &diagnostic.help_body),
    ] {
        if text.as_deref().is_some_and(|text| text.trim().is_empty()) {
            return missing(field, format!("{prefix}: blank {field}"));
        }
    }
    if let Some(path) = &diagnostic.help_path {
        if path != &format!("docs/diagnostics/{code}.md") {
            return missing(
                "help_path",
                format!("{prefix}: help path does not identify its code page"),
            );
        }
    }
    if diagnostic
        .fieldworks_places
        .iter()
        .any(|place| place.tool.trim().is_empty())
    {
        return missing(
            "fieldworks_places",
            format!("{prefix}: missing FieldWorks tool"),
        );
    }
    for (subject_index, subject) in diagnostic.subjects.iter().enumerate() {
        if (subject.kind == FwClass::Project)
            != (subject.status == FwSubjectStatus::ProjectSettings)
            || (subject.status == FwSubjectStatus::ProjectSettings
                && (subject.guid.is_some() || subject.opens_in.is_some()))
            || (subject.status == FwSubjectStatus::UnresolvedReference
                && (subject.opens_in.is_some()
                    || subject
                        .guid
                        .as_deref()
                        .is_none_or(|id| id.trim().is_empty())))
        {
            return missing("status", format!("{prefix}: inconsistent subject status"));
        }
        let subject_prefix = format!("{prefix} subject {subject_index}");
        if subject
            .opens_in
            .as_ref()
            .is_some_and(|target| target.tool.trim().is_empty())
        {
            return missing("opens_in.tool", format!("{subject_prefix}: missing tool"));
        }
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

/// Checked-in path, relative to the repository root, of [`render_diagnostics_reference`]'s output.
pub const DIAGNOSTICS_REFERENCE_PATH: &str = "docs/grammar-diagnostics-reference.md";

fn registered_diagnostic_codes() -> impl Iterator<Item = GrammarHealthCode> {
    GrammarHealthCode::ALL.iter().cloned().chain(
        ImportWarningCode::ALL
            .iter()
            .map(|code| GrammarHealthCode::ImportWarning(code.wire().to_string())),
    )
}

/// Generate the version-pinnable CommonMark page for each registered code.
pub fn render_diagnostic_pages() -> std::collections::BTreeMap<String, String> {
    registered_diagnostic_codes()
        .map(|code| {
            let advice = diagnostic_advice(&code).expect("registered code has owned advice");
            (
                format!("docs/diagnostics/{}.md", code.wire()),
                render_diagnostic_page(&code, advice),
            )
        })
        .collect()
}

fn render_diagnostic_page(code: &GrammarHealthCode, advice: DiagnosticAdvice) -> String {
    let mut page = format!(
        "# {}\n\n{}\n\nLevel: **{}**\n\n## Explanation\n\n{}\n\n## What to do\n\n{}\n",
        code.wire(),
        advice.title,
        code.level().wire(),
        advice.explanation,
        advice.guidance.replace("{subject}", "the named item")
    );
    if !advice.fieldworks_places.is_empty() {
        page.push_str("\n## FieldWorks places\n\n| Tool | Field |\n|---|---|\n");
        for place in advice.fieldworks_places {
            page.push_str(&format!(
                "| `{}` | {} |\n",
                place.tool,
                place.field.replace('|', "\\|")
            ));
        }
    }
    if let Some(help) = advice.help_body {
        page.push_str("\n## Background\n\n");
        page.push_str(help.replace("\r\n", "\n").trim());
        page.push('\n');
    }
    page.push_str("\n[All diagnostic codes](../grammar-diagnostics-reference.md) · [Report format](../grammar-diagnostics.md)\n");
    page
}

/// Render the index from the same catalog that supplies runtime explanations and advice.
pub fn render_diagnostics_reference() -> String {
    let mut out = "# Grammar diagnostics reference\n\nEvery registered `pangloss grammar-health` finding has a page generated from PanGloss's runtime advice catalog. The [report format](grammar-diagnostics.md) describes levels and navigation.\n\nPin a page to a release tag: `https://github.com/sillsdev/PanGloss/blob/<tag>/docs/diagnostics/<code>.md`. Each page's first heading is its code; GitHub also renders an anchor from that heading. The file path is stable even for dotted codes.\n".to_string();
    for (level, title) in [
        (DiagnosticLevel::Error, "Errors"),
        (DiagnosticLevel::Warning, "Warnings"),
        (DiagnosticLevel::Info, "Information"),
    ] {
        let codes: Vec<_> = registered_diagnostic_codes()
            .filter(|code| code.level() == level)
            .collect();
        out.push_str(&format!(
            "\n## {title} ({})\n\n| Code | Title | What to do |\n|---|---|---|\n",
            codes.len()
        ));
        for code in codes {
            let advice = diagnostic_advice(&code).expect("registered code has advice");
            out.push_str(&format!(
                "| [{}](diagnostics/{}.md) | {} | {} |\n",
                code.wire(),
                code.wire(),
                advice.title.replace('|', "\\|"),
                advice
                    .guidance
                    .replace("{subject}", "the named item")
                    .replace('|', "\\|")
            ));
        }
    }
    out
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
    let feature_class_affects_membership = grammar.natural_classes.iter().any(|class| {
        pg_grammar_model::membership::requires_feature_value(
            class,
            grammar.phon_features.type_flat(),
        )
    });
    for (table_index, table) in grammar.char_tables.iter().enumerate() {
        for (id, definition) in table.iter() {
            if feature_class_affects_membership
                && definition.kind() == CharDefKind::Segment
                && !definition.is_provisional()
                && !definition.has_authored_features()
            {
                diagnostics.push(GrammarHealthDiagnostic::checked(
                    GrammarHealthCode::ProvisionalPhonemeFeatures,
                    format!("The phoneme '{}' has no phonological features. PanGloss keeps it in segment-list natural classes that name it and excludes it from natural classes requiring a feature value. Unconstrained classes still match it. Assign its phonological features in FieldWorks to replace this provisional definition.", first_representation(definition)),
                    vec![char_def_subject(grammar, None, TableId(table_index as u16), id, definition, table)],
                ));
            }
        }
    }
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
    let fieldworks = subject_link(
        source.status,
        source.class,
        source.guid.as_deref(),
        source.opens_in.as_ref(),
        fieldworks_project,
    );
    GrammarHealthSubject {
        kind: source.class,
        status: source.status,
        field: source.field,
        source_class: source.source_class,
        title,
        subtitle,
        guid: source.guid,
        internal_id: Some(internal_id),
        opens_in: source.opens_in,
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
        fieldworks_identity(grammar, &FieldWorksSource::Table(id)),
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

/// Consume the canonical typed inventory; this error path does not inspect rule definitions.
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
    GrammarHealthDiagnostic::checked(code, message, vec![subject])
}

#[cfg(test)]
mod tests;
