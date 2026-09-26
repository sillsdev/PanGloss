//! Import warnings pair stable codes with messages and source subjects.
use std::fmt;
use std::ops::Deref;

/// One importer or snapshot-validation warning. See the module doc for the `code`/`message` contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Warning {
    /// Short, stable, dotted identifier naming the situation (e.g. `"fwdata.dangling-reference"`); must stay the same across a `message` reword, and two structurally different situations must never share one.
    pub code: String,
    /// User-facing prose may change; it names the affected FieldWorks items.
    pub message: String,
    /// The FieldWorks objects the warning is about, most specific first.
    pub subjects: Vec<FwObjectRef>,
}

impl Warning {
    pub fn new(code: ImportWarningCode, message: impl Into<String>) -> Self {
        let wire = code.wire().to_string();
        let message = message.into();
        let message = if matches!(code, ImportWarningCode::Unregistered(_)) {
            format!("Unregistered warning code '{wire}': {message}")
        } else {
            message
        };
        Warning {
            code: wire,
            message,
            subjects: Vec::new(),
        }
    }

    pub fn with_subject(mut self, subject: FwObjectRef) -> Self {
        self.subjects.push(subject);
        self
    }

    /// Adds the resolved name for the primary source.
    pub fn set_primary_subject_name(&mut self, name: impl Into<String>) {
        let name = name.into();
        if let Some(subject) = self.subjects.first_mut() {
            subject.name = Some(name.clone());
        }
    }

    /// Deduplication uses stable code and source identity, not prose that may change.
    pub fn same_fact_as(&self, other: &Self) -> bool {
        if self.code != other.code {
            return false;
        }
        if self.subjects.is_empty() || other.subjects.is_empty() {
            return false;
        }
        self.subjects.iter().all(|left| {
            other
                .subjects
                .iter()
                .any(|right| same_subject_identity(left, right))
        }) && other.subjects.iter().all(|right| {
            self.subjects
                .iter()
                .any(|left| same_subject_identity(left, right))
        })
    }

    pub fn from_conversion_issue(issue: &crate::ConversionIssue) -> Self {
        let mut warning = Warning::new(issue.code.clone(), issue.message.clone());
        if let Some(source) = &issue.source {
            let subject = FwObjectRef::new(source.kind).guid(source.id.clone());
            warning = warning.with_subject(subject);
        }
        warning
    }
}

pub fn canonical_guid(value: &str) -> Option<String> {
    let well_formed = value.len() == 36
        && value.bytes().enumerate().all(|(index, byte)| {
            if matches!(index, 8 | 13 | 18 | 23) {
                byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        });
    well_formed.then(|| value.to_ascii_lowercase())
}

fn same_subject_identity(left: &FwObjectRef, right: &FwObjectRef) -> bool {
    left.class == right.class
        && match (&left.guid, &right.guid) {
            (Some(left), Some(right)) => left == right,
            _ => matches!(
                (&left.name, &right.name),
                (Some(left), Some(right)) if left == right
            ),
        }
}

macro_rules! import_warning_codes {
    ($($variant:ident => $wire:literal,)+) => {
        /// Stable codes used by the FieldWorks importer and grammar compiler.
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        #[allow(missing_docs)]
        pub enum ImportWarningCode {
            $($variant,)+
            /// A code supplied by a producer this build does not know.
            Unregistered(String),
        }

        impl ImportWarningCode {
            /// Every warning code emitted by the importer and grammar compiler.
            pub const ALL: &'static [Self] = &[$(Self::$variant,)+];

            /// The stable wire identifier for this code.
            pub fn wire(&self) -> &str {
                match self {
                    $(Self::$variant => $wire,)+
                    Self::Unregistered(raw) => raw,
                }
            }

            /// Resolves a wire identifier to its registered warning code.
            pub fn from_wire(wire: &str) -> Option<Self> {
                match wire {
                    $($wire => Some(Self::$variant),)+
                    _ => None,
                }
            }

            /// Resolves known identifiers and retains unknown ones explicitly.
            pub fn from_wire_or_unregistered(wire: &str) -> Self {
                Self::from_wire(wire).unwrap_or_else(|| Self::Unregistered(wire.to_string()))
            }
        }
    };
}

import_warning_codes! {
    FwdataDanglingReference => "fwdata.dangling-reference",
    FwdataUnexpectedClass => "fwdata.unexpected-class",
    FwdataMissingRequiredField => "fwdata.missing-required-field",
    FwdataMissingLangProject => "fwdata.missing-lang-project",
    FwdataOnlyFirstUsed => "fwdata.only-first-used",
    FwdataEmptyRepresentation => "fwdata.empty-representation",
    FwdataUnrecognizedEnumValue => "fwdata.unrecognized-enum-value",
    FwdataMetathesisApproximation => "fwdata.metathesis-approximation",
    FwdataStaleAdhocProhibition => "fwdata.stale-adhoc-prohibition",
    FwdataNoUsableAllomorphs => "fwdata.no-usable-allomorphs",
    FwdataUnsupportedMorphType => "fwdata.unsupported-morph-type",
    FwdataUnknownMorphTypeGuid => "fwdata.unknown-morph-type-guid",
    FwdataReferenceNotInScope => "fwdata.reference-not-in-scope",
    FwdataInvalidParserParameter => "fwdata.invalid-parser-parameter",
    InvalidSourceActiveParser => "invalid-source.active-parser",
    InvalidSourceDuplicateGuid => "invalid-source.duplicate-guid",
    InvalidSourceMissingGuid => "invalid-source.missing-guid",
    FwdataWritingSystemStoreUnreadable => "fwdata.writing-system-store-unreadable",
    SnapshotDanglingReference => "snapshot.dangling-reference",
    SnapshotFeatureStructureUnresolved => "snapshot.feature-structure-unresolved",
    SnapshotRuleFeatureUnresolved => "snapshot.rule-feature-unresolved",
    SnapshotReferenceOutOfScope => "snapshot.reference-out-of-scope",
    PhonemeNoRepresentation => "grammar.phoneme.no-representation",
    PhonemeNfdCollision => "grammar.phoneme.nfd-collision",
    PhonemeFeatureUnresolved => "grammar.phoneme.feature-unresolved",
    PhonemeComplexFeatureUnsupported => "grammar.phoneme.complex-feature-unsupported",
    BoundaryNfdCollision => "grammar.boundary.nfd-collision",
    BoundaryNoRepresentation => "grammar.boundary.no-representation",
    BoundaryMorphMarkerUnresolved => "grammar.boundary.morph-marker-unresolved",
    NatclassSegmentsMemberUnresolved => "grammar.natclass.segments-member-unresolved",
    NatclassFeatureConstraintUnresolved => "grammar.natclass.feature-constraint-unresolved",
    NatclassComplexFeatureUnsupported => "grammar.natclass.complex-feature-unsupported",
    PhonComplexFeatureUnsupported => "grammar.feature.phon-complex-unsupported",
    StemNameBuildFailed => "grammar.stem-name.build-failed",
    StemNameEmptyRegions => "grammar.stem-name.empty-regions",
    CompoundRuleBuildFailed => "grammar.compound-rule.build-failed",
    CompoundSidePosUnresolved => "grammar.compound-rule.side-pos-unresolved",
    CompoundSideExceptionFeatureUnresolved => "grammar.compound-rule.side-exception-feature-unresolved",
    MsaBuildFailed => "grammar.msa.build-failed",
    MsaNoAllomorphs => "grammar.msa.no-allomorphs",
    MsaNoRuleFormAllomorphs => "grammar.msa.no-rule-form-allomorphs",
    MsaExceptionFeatureUnresolved => "grammar.msa.exception-feature-unresolved",
    MsaInflectionClassUnresolved => "grammar.msa.inflection-class-unresolved",
    MsaStemNameUnresolved => "grammar.msa.stem-name-unresolved",
    MsaLexEntryInflTypeUnresolved => "grammar.msa.lex-entry-infl-type-unresolved",
    VariantComponentUnresolved => "grammar.variant.component-unresolved",
    AllomorphUnsegmentable => "grammar.allomorph.unsegmentable",
    AllomorphMorphTypeUnsupported => "grammar.allomorph.morph-type-unsupported",
    AllomorphMorphTypeUnsupportedAsRuleForm => "grammar.allomorph.morph-type-unsupported-as-rule-form",
    AllomorphNotRuleForm => "grammar.allomorph.not-a-rule-form",
    AllomorphReduplicationUnsupported => "grammar.allomorph.reduplication-unsupported",
    AllomorphProcessBuildFailed => "grammar.allomorph.process-build-failed",
    AllomorphInflectionClassUnresolved => "grammar.allomorph.inflection-class-unresolved",
    AllomorphFeatureBuildFailed => "grammar.allomorph.feature-build-failed",
    AllomorphEnvironmentBuildFailed => "grammar.allomorph.environment-build-failed",
    CircumfixMissingHalf => "grammar.circumfix.missing-half",
    CircumfixEnvironmentCombinationSkipped => "grammar.circumfix.environment-combination-skipped",
    EnvironmentUnresolved => "grammar.environment.unresolved",
    EnvironmentInvalid => "grammar.environment.invalid",
    TemplateSlotUnresolved => "grammar.template.slot-unresolved",
    TemplateSlotNoRules => "grammar.template.slot-no-rules",
    TemplateNoSlots => "grammar.template.no-slots",
    TemplateBuildFailed => "grammar.template.build-failed",
    NullAffixMprUnresolved => "grammar.null-affix.mpr-unresolved",
    NullAffixSynFsFailed => "grammar.null-affix.syn-fs-failed",
    NullAffixSegmentFailed => "grammar.null-affix.segment-failed",
    RuleMetathesisUnsupported => "grammar.rule.metathesis-unsupported",
    RuleBuildFailed => "grammar.rule.build-failed",
    FeatureConstraintUnresolved => "grammar.rule.feature-constraint-unresolved",
    FeatureConstraintPhonFeatureUnresolved => "grammar.rule.feature-constraint-phon-feature-unresolved",
    RuleFeatureUnresolved => "grammar.rule.rule-feature-unresolved",
    StrataCustomUnsupported => "grammar.strata.custom-unsupported",
    AdhocProhibitionUnresolved => "grammar.adhoc-prohibition.unresolved",
    MruleUnreachableCompacted => "grammar.mrule.unreachable-compacted",
    CooccurrenceTargetUnreachable => "grammar.cooccurrence.target-unreachable",
    NaturalClassUnreferencedCompacted => "grammar.natclass.unreferenced-compacted",
    SourceProvenanceUnknown => "conversion.source-provenance-unknown",
    SubstrateUnsegmentableForm => "conversion.unsegmentable-form",
    SubstrateClassificationAmbiguous => "substrate.classification-ambiguous",
    MigrationInferredSegmentWithFeatureRule => "migration.inferred-segment-with-feature-rule",
    UnsupportedConstruct => "conversion.unsupported-construct",
    SubstratePositionUnmapped => "substrate.position-unmapped",
}

impl serde::Serialize for ImportWarningCode {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.wire())
    }
}

impl fmt::Display for ImportWarningCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.wire())
    }
}

impl<'de> serde::Deserialize<'de> for ImportWarningCode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let wire = <String as serde::Deserialize>::deserialize(deserializer)?;
        let code = Self::from_wire_or_unregistered(&wire);
        if matches!(code, Self::Unregistered(_)) {
            eprintln!("unregistered import warning code '{wire}'");
        }
        Ok(code)
    }
}

/// `Error` must be fixed; `Warning` asks for a FieldWorks change; `Info` needs nothing fixed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticLevel {
    Error,
    Warning,
    Info,
}

impl DiagnosticLevel {
    pub const fn wire(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Info => "info",
        }
    }
}

/// This source identity lets warnings name items dropped before grammar compilation.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FwObjectRef {
    pub class: FwClass,
    /// Lower-case canonical GUID, only when the value is the FieldWorks object's own GUID.
    pub guid: Option<String>,
    /// The name a linguist sees in FieldWorks (form, gloss, rule name, phoneme representation).
    pub name: Option<String>,
    /// Where FieldWorks opens this object when its class alone cannot say.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opens_in: Option<FwOpenTarget>,
}

/// A FieldWorks tool and the GUID it selects, e.g. a boundary marker opens its phoneme set in `phonemeEdit`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct FwOpenTarget {
    pub tool: String,
    pub guid: String,
}

impl FwObjectRef {
    pub fn new(class: FwClass) -> Self {
        FwObjectRef {
            class,
            guid: None,
            name: None,
            opens_in: None,
        }
    }

    pub fn opens_in(mut self, tool: impl Into<String>, guid: impl Into<String>) -> Self {
        let guid = guid.into();
        self.opens_in = Some(FwOpenTarget {
            tool: tool.into(),
            guid: canonical_guid(&guid).unwrap_or(guid),
        });
        self
    }

    pub fn guid(mut self, guid: impl Into<String>) -> Self {
        let guid = guid.into();
        self.guid = Some(canonical_guid(&guid).unwrap_or(guid));
        self
    }

    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }
}

/// FieldWorks (LCM) class of a warning subject; the wire value is the LCM class name. Append-only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
pub enum FwClass {
    LexEntry,
    MoForm,
    MoStemMsa,
    MoInflAffMsa,
    MoDerivAffMsa,
    MoUnclassifiedAffixMsa,
    LexEntryInflType,
    MoInflAffixTemplate,
    MoInflAffixSlot,
    MoCompoundRule,
    MoAdhocProhib,
    PhPhonemeSet,
    PhPhoneme,
    PhNaturalClass,
    PhEnvironment,
    PhRegularRule,
    PhMetathesisRule,
    FsFeatureSystem,
    /// No single FieldWorks object (e.g. project-wide settings).
    Project,
    LexSense,
    PhBdryMarker,
    FsComplexFeature,
    MoStemName,
    MoInflClass,
    FsClosedFeature,
    FsSymFeatVal,
    Unknown,
}

impl<'de> serde::Deserialize<'de> for FwClass {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let wire = <String as serde::Deserialize>::deserialize(deserializer)?;
        Ok(Self::from_wire(&wire))
    }
}

impl FwClass {
    /// Converts a FieldWorks class name at an external-data boundary.
    pub fn from_wire(wire: &str) -> Self {
        match wire {
            "LexEntry" => Self::LexEntry,
            "MoForm" | "MoStemAllomorph" | "MoAffixAllomorph" | "MoAffixProcess" | "allomorph" => {
                Self::MoForm
            }
            "MoStemMsa" => Self::MoStemMsa,
            "MoInflAffMsa" => Self::MoInflAffMsa,
            "MoDerivAffMsa" => Self::MoDerivAffMsa,
            "MoUnclassifiedAffixMsa" => Self::MoUnclassifiedAffixMsa,
            "LexEntryInflType" => Self::LexEntryInflType,
            "MoInflAffixTemplate" => Self::MoInflAffixTemplate,
            "MoInflAffixSlot" => Self::MoInflAffixSlot,
            "MoCompoundRule" => Self::MoCompoundRule,
            "MoAdhocProhib" | "MoAlloAdhocProhib" | "MoMorphAdhocProhib" => Self::MoAdhocProhib,
            "PhPhonemeSet" => Self::PhPhonemeSet,
            "PhPhoneme" => Self::PhPhoneme,
            "PhNaturalClass" => Self::PhNaturalClass,
            "PhEnvironment" => Self::PhEnvironment,
            "PhRegularRule" => Self::PhRegularRule,
            "PhMetathesisRule" => Self::PhMetathesisRule,
            "FsFeatureSystem" => Self::FsFeatureSystem,
            "Project" => Self::Project,
            "LexSense" => Self::LexSense,
            "PhBdryMarker" => Self::PhBdryMarker,
            "FsComplexFeature" => Self::FsComplexFeature,
            "MoStemName" => Self::MoStemName,
            "MoInflClass" => Self::MoInflClass,
            "FsClosedFeature" => Self::FsClosedFeature,
            "FsSymFeatVal" => Self::FsSymFeatVal,
            _ => Self::Unknown,
        }
    }
}

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

/// Lets existing `&str`-shaped callers keep working against `message`.
impl Deref for Warning {
    type Target = str;

    fn deref(&self) -> &str {
        &self.message
    }
}

impl From<Warning> for String {
    fn from(w: Warning) -> String {
        w.message
    }
}

#[cfg(test)]
mod tests;
