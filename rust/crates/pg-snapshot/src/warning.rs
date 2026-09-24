//! Import warnings pair stable codes with messages, source subjects, FieldWorks guidance, and audience.
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
    /// What to do about it in FieldWorks (area, tool, field); `None` when there is no user action.
    pub guidance: Option<String>,
    pub audience: Audience,
}

impl Warning {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        let code = code.into();
        let (audience, guidance) = ImportWarningCode::from_wire(&code)
            .map(|code| {
                let metadata = crate::import_warning_metadata(code);
                (metadata.audience, metadata.guidance)
            })
            .unwrap_or((Audience::Linguist, None));
        Warning {
            code,
            message: message.into(),
            subjects: Vec::new(),
            guidance,
            audience,
        }
    }

    pub fn with_subject(mut self, subject: FwObjectRef) -> Self {
        if let Some(name) = subject
            .name
            .as_deref()
            .filter(|name| !name.trim().is_empty())
        {
            self.render_guidance_subject(name);
        }
        self.subjects.push(subject);
        self
    }

    /// Adds the resolved name for the primary source and fills any table guidance template.
    pub fn set_primary_subject_name(&mut self, name: impl Into<String>) {
        let name = name.into();
        if let Some(subject) = self.subjects.first_mut() {
            subject.name = Some(name.clone());
        }
        self.render_guidance_subject(&name);
    }

    fn render_guidance_subject(&mut self, name: &str) {
        if let Some(guidance) = &mut self.guidance {
            *guidance = guidance.replace("{subject}", name);
        }
    }

    pub fn with_guidance(mut self, guidance: impl Into<String>) -> Self {
        self.guidance = Some(guidance.into());
        self
    }

    pub fn with_audience(mut self, audience: Audience) -> Self {
        self.audience = audience;
        self
    }

    /// Marks an engine-internal notice (e.g. an index dropped by compaction) that no linguist can act on.
    pub fn for_developers(mut self) -> Self {
        self.audience = Audience::Developer;
        self
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
        if ImportWarningCode::from_wire(&issue.code).is_none() {
            warning.audience = issue.audience;
        }
        if let Some(source) = &issue.source {
            if let Some(class) = fw_class_from_source_kind(&source.kind) {
                let mut subject = FwObjectRef::new(class);
                if is_canonical_guid(&source.id) {
                    subject = subject.guid(source.id.clone());
                }
                warning = warning.with_subject(subject);
            }
        }
        warning
    }
}

fn is_canonical_guid(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(index, byte)| {
            if matches!(index, 8 | 13 | 18 | 23) {
                byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        })
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

fn fw_class_from_source_kind(kind: &str) -> Option<FwClass> {
    Some(match kind {
        "LexEntry" => FwClass::LexEntry,
        "LexSense" => FwClass::LexSense,
        "MoForm" | "MoStemAllomorph" | "MoAffixAllomorph" | "MoAffixProcess" | "allomorph" => {
            FwClass::MoForm
        }
        "MoStemMsa" => FwClass::MoStemMsa,
        "MoInflAffMsa" => FwClass::MoInflAffMsa,
        "MoDerivAffMsa" => FwClass::MoDerivAffMsa,
        "MoUnclassifiedAffixMsa" => FwClass::MoUnclassifiedAffixMsa,
        "LexEntryInflType" => FwClass::LexEntryInflType,
        "MoStemName" => FwClass::MoStemName,
        "MoInflAffixTemplate" => FwClass::MoInflAffixTemplate,
        "MoInflAffixSlot" => FwClass::MoInflAffixSlot,
        "MoCompoundRule" => FwClass::MoCompoundRule,
        "MoAdhocProhib" | "MoAlloAdhocProhib" | "MoMorphAdhocProhib" => FwClass::MoAdhocProhib,
        "PhPhonemeSet" => FwClass::PhPhonemeSet,
        "PhPhoneme" => FwClass::PhPhoneme,
        "PhBdryMarker" => FwClass::PhBdryMarker,
        "PhNaturalClass" => FwClass::PhNaturalClass,
        "PhEnvironment" => FwClass::PhEnvironment,
        "PhRegularRule" => FwClass::PhRegularRule,
        "PhMetathesisRule" => FwClass::PhMetathesisRule,
        "FsFeatureSystem" => FwClass::FsFeatureSystem,
        "FsComplexFeature" => FwClass::FsComplexFeature,
        "Project" => FwClass::Project,
        _ => return None,
    })
}

macro_rules! import_warning_codes {
    ($($variant:ident => $wire:literal,)+) => {
        /// Stable codes used by the FieldWorks importer and grammar compiler.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        #[allow(missing_docs)]
        pub enum ImportWarningCode {
            $($variant,)+
        }

        impl ImportWarningCode {
            /// Every warning code emitted by the importer and grammar compiler.
            pub const ALL: &'static [Self] = &[$(Self::$variant,)+];

            /// The stable wire identifier for this code.
            pub const fn wire(self) -> &'static str {
                match self {
                    $(Self::$variant => $wire,)+
                }
            }

            /// Resolves a wire identifier to its registered warning code.
            pub fn from_wire(wire: &str) -> Option<Self> {
                match wire {
                    $($wire => Some(Self::$variant),)+
                    _ => None,
                }
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

/// Who can act on a warning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Audience {
    Linguist,
    Developer,
}

impl Default for Audience {
    fn default() -> Self {
        Self::Linguist
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
}

impl FwObjectRef {
    pub fn new(class: FwClass) -> Self {
        FwObjectRef {
            class,
            guid: None,
            name: None,
        }
    }

    pub fn guid(mut self, guid: impl Into<String>) -> Self {
        let guid = guid.into();
        if is_canonical_guid(&guid) {
            self.guid = Some(guid.to_ascii_lowercase());
        }
        self
    }

    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }
}

/// FieldWorks (LCM) class of a warning subject; the wire value is the LCM class name. Append-only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
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
