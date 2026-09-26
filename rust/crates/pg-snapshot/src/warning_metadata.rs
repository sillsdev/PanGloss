//! The exhaustive per-code owner for import warning level, group, and FieldWorks guidance.

use crate::fieldworks_paths;
use crate::{DiagnosticLevel, FwClass, ImportWarningCode};

pub struct ImportWarningMetadata {
    pub group_name: &'static str,
    pub level: DiagnosticLevel,
    /// `None` for an info notice with nothing to fix.
    pub guidance: Option<String>,
}

impl ImportWarningMetadata {
    pub fn guidance_for_subject(
        &self,
        subject_name: Option<&str>,
        subject_class: Option<FwClass>,
    ) -> Option<String> {
        let subject_name = subject_name
            .filter(|name| !name.trim().is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| {
                format!(
                    "the {}",
                    subject_class.map_or("item", fieldworks_subject_kind_label)
                )
            });
        self.guidance
            .as_ref()
            .map(|template| template.replace("{subject}", &subject_name))
    }
}

/// The linguist-facing fallback label for a FieldWorks source kind.
pub fn fieldworks_subject_kind_label(kind: FwClass) -> &'static str {
    match kind {
        FwClass::LexEntry => "lexical entry",
        FwClass::LexSense => "sense",
        FwClass::MoForm => "form",
        FwClass::MoStemMsa
        | FwClass::MoInflAffMsa
        | FwClass::MoDerivAffMsa
        | FwClass::MoUnclassifiedAffixMsa => "grammatical analysis",
        FwClass::LexEntryInflType => "entry inflection type",
        FwClass::MoStemName => "stem name",
        FwClass::MoInflClass => "inflection class",
        FwClass::MoInflAffixTemplate => "affix template",
        FwClass::MoInflAffixSlot => "affix template slot",
        FwClass::MoCompoundRule => "compound rule",
        FwClass::MoAdhocProhib => "ad-hoc prohibition",
        FwClass::PhPhonemeSet => "phoneme set",
        FwClass::PhPhoneme => "phoneme",
        FwClass::PhBdryMarker => "boundary marker",
        FwClass::PhNaturalClass => "natural class",
        FwClass::PhEnvironment => "phonological environment",
        FwClass::PhRegularRule => "phonological rule",
        FwClass::PhMetathesisRule => "metathesis rule",
        FwClass::FsFeatureSystem => "feature system",
        FwClass::FsComplexFeature => "complex phonological feature",
        FwClass::FsClosedFeature => "phonological feature",
        FwClass::FsSymFeatVal => "feature value",
        FwClass::Unknown => "item",
        FwClass::Project => "project",
    }
}

/// Display fallback used when a source object has no authored name or representation.
pub fn fieldworks_missing_name_fallback(kind: FwClass) -> &'static str {
    match kind {
        FwClass::LexEntry => "Unnamed lexical entry",
        FwClass::LexSense => "Unnamed sense",
        FwClass::MoForm => "Unnamed affix allomorph",
        FwClass::MoStemMsa
        | FwClass::MoInflAffMsa
        | FwClass::MoDerivAffMsa
        | FwClass::MoUnclassifiedAffixMsa => "Unnamed grammatical analysis",
        FwClass::LexEntryInflType => "Unnamed entry inflection type",
        FwClass::MoStemName => "Unnamed stem name",
        FwClass::MoInflClass => "Unnamed inflection class",
        FwClass::MoInflAffixTemplate => "Unnamed affix template",
        FwClass::MoInflAffixSlot => "Unnamed affix template slot",
        FwClass::MoCompoundRule => "Unnamed compound rule",
        FwClass::MoAdhocProhib => "Unnamed ad-hoc prohibition",
        FwClass::PhPhonemeSet => "Unnamed phoneme set",
        FwClass::PhPhoneme => "Unnamed phoneme",
        FwClass::PhBdryMarker => "Unnamed boundary marker",
        FwClass::PhNaturalClass => "Unnamed natural class",
        FwClass::PhEnvironment => "Unnamed phonological environment",
        FwClass::PhRegularRule => "Unnamed phonological rule",
        FwClass::PhMetathesisRule => "Unnamed metathesis rule",
        FwClass::FsFeatureSystem => "Unnamed feature system",
        FwClass::FsComplexFeature => "Unnamed complex phonological feature",
        FwClass::FsClosedFeature => "Unnamed phonological feature",
        FwClass::FsSymFeatVal => "Unnamed feature value",
        FwClass::Unknown => "Unnamed FieldWorks item",
        FwClass::Project => "Unnamed project",
    }
}

/// A known-bad item that loosens or confuses the grammar: a dropped restriction, a collision, or a lost morpheme.
fn error(group_name: &'static str, path: &str, action: &'static str) -> ImportWarningMetadata {
    ImportWarningMetadata {
        level: DiagnosticLevel::Error,
        ..warning(group_name, path, action)
    }
}

fn warning(group_name: &'static str, path: &str, action: &'static str) -> ImportWarningMetadata {
    ImportWarningMetadata {
        group_name,
        level: DiagnosticLevel::Warning,
        guidance: Some(format!("In {path}, {action}")),
    }
}

fn info(group_name: &'static str) -> ImportWarningMetadata {
    ImportWarningMetadata {
        group_name,
        level: DiagnosticLevel::Info,
        guidance: None,
    }
}

pub fn import_warning_metadata(code: ImportWarningCode) -> ImportWarningMetadata {
    use ImportWarningCode::*;

    match code {
        FwdataDanglingReference => warning(
            "Missing FieldWorks reference",
            fieldworks_paths::LEXICON_EDIT,
            "restore or remove the named item's missing reference.",
        ),
        FwdataUnexpectedClass => warning(
            "Unexpected FieldWorks item",
            fieldworks_paths::LEXICON_EDIT,
            "check the named item's class and references.",
        ),
        FwdataMissingRequiredField => warning(
            "Missing required FieldWorks field",
            fieldworks_paths::LEXICON_EDIT,
            "complete the named item's required field.",
        ),
        FwdataMissingLangProject => warning(
            "Missing language project",
            fieldworks_paths::FILE_RESTORE_PROJECT,
            "restore a valid FieldWorks project backup.",
        ),
        // FieldWorks' own parser reads only the first phoneme set too (HCLoader.cs:204).
        FwdataOnlyFirstUsed => ImportWarningMetadata {
            level: DiagnosticLevel::Info,
            ..warning(
                "Only first list item is used",
                fieldworks_paths::GRAMMAR_PHONEMES,
                "check the project's phoneme sets.",
            )
        },
        FwdataEmptyRepresentation => warning(
            "Empty item representation",
            fieldworks_paths::GRAMMAR_PHONEMES,
            "add a representation to the named phoneme or boundary marker.",
        ),
        FwdataUnrecognizedEnumValue => warning(
            "Unrecognized FieldWorks value",
            fieldworks_paths::LEXICON_EDIT,
            "replace the named item's value with one offered by FieldWorks.",
        ),
        FwdataMetathesisApproximation => warning(
            "Metathesis rule approximation",
            fieldworks_paths::GRAMMAR_PHONOLOGICAL_RULES,
            "review the named metathesis rule's structural description.",
        ),
        FwdataStaleAdhocProhibition => warning(
            "Stale ad hoc prohibition",
            fieldworks_paths::GRAMMAR_AD_HOC_RULES,
            "update the named prohibition's affix reference or remove it.",
        ),
        Unregistered(_) => ImportWarningMetadata {
            group_name: "Unregistered warning code",
            level: DiagnosticLevel::Warning,
            guidance: None,
        },
        FwdataNoUsableAllomorphs => error(
            "No usable allomorphs",
            fieldworks_paths::LEXICON_EDIT,
            "add or correct an allomorph for the named lexical entry.",
        ),
        FwdataUnsupportedMorphType => warning(
            "Unsupported morph type",
            fieldworks_paths::LEXICON_EDIT,
            "choose a supported morph type for the named allomorph.",
        ),
        FwdataUnknownMorphTypeGuid => warning(
            "Unknown morph type reference",
            fieldworks_paths::LEXICON_EDIT,
            "assign a morph type that exists in the project to the named allomorph.",
        ),
        FwdataReferenceNotInScope => warning(
            "Reference outside item scope",
            fieldworks_paths::LEXICON_EDIT,
            "move the named reference into the correct FieldWorks list.",
        ),
        FwdataInvalidParserParameter => warning(
            "Invalid parser setting",
            fieldworks_paths::WORDS_EDIT_PARSER_PARAMETERS,
            "correct the named parser setting.",
        ),
        InvalidSourceActiveParser => warning(
            "Unsupported active parser",
            fieldworks_paths::WORDS_EDIT_PARSER_PARAMETERS,
            "select a supported parser.",
        ),
        InvalidSourceDuplicateGuid => warning(
            "Duplicate FieldWorks GUID",
            fieldworks_paths::LEXICON_EDIT,
            "resolve the duplicate identity for the named item.",
        ),
        InvalidSourceMissingGuid => warning(
            "Missing FieldWorks GUID",
            fieldworks_paths::LEXICON_EDIT,
            "save the named item in FieldWorks so it has a stable identity.",
        ),
        FwdataWritingSystemStoreUnreadable => ImportWarningMetadata {
            group_name: "Writing system data unavailable",
            level: DiagnosticLevel::Warning,
            guidance: Some(format!(
                "In {} and {}, check the project's writing system data.",
                fieldworks_paths::TOOLS_CONFIGURE_VERNACULAR_WRITING_SYSTEMS,
                fieldworks_paths::TOOLS_CONFIGURE_ANALYSIS_WRITING_SYSTEMS
            )),
        },
        SnapshotDanglingReference => warning(
            "Missing snapshot reference",
            fieldworks_paths::LEXICON_EDIT,
            "restore or remove the named item's missing reference.",
        ),
        SnapshotFeatureStructureUnresolved => ImportWarningMetadata {
            group_name: "Unresolved feature structure",
            level: DiagnosticLevel::Warning,
            guidance: Some(format!(
                "In {} for lexical features or {} for phonological features, correct the named item's feature values.",
                fieldworks_paths::LEXICON_EDIT,
                fieldworks_paths::GRAMMAR_PHONOLOGICAL_FEATURES
            )),
        },
        SnapshotRuleFeatureUnresolved => warning(
            "Unresolved rule feature structure",
            fieldworks_paths::GRAMMAR_PHONOLOGICAL_RULES,
            "correct the named rule's feature structure.",
        ),
        SnapshotReferenceOutOfScope => warning(
            "Snapshot reference out of scope",
            fieldworks_paths::LEXICON_EDIT,
            "move the named reference into the FieldWorks list that owns it.",
        ),
        PhonemeNoRepresentation => warning(
            "Phoneme has no representation",
            fieldworks_paths::GRAMMAR_PHONEMES,
            "add a grapheme representation to the named phoneme.",
        ),
        PhonemeNfdCollision => error(
            "Phoneme representation collision",
            fieldworks_paths::GRAMMAR_PHONEMES,
            "change the named phoneme's representation so it is unique.",
        ),
        PhonemeFeatureUnresolved => error(
            "Unresolved phoneme feature value",
            fieldworks_paths::GRAMMAR_PHONEMES,
            "correct the named phoneme's phonological feature values.",
        ),
        PhonemeComplexFeatureUnsupported => error(
            "Unsupported phoneme feature value",
            fieldworks_paths::GRAMMAR_PHONEMES,
            "replace the named phoneme's complex value with a supported feature value.",
        ),
        BoundaryNfdCollision => error(
            "Boundary representation collision",
            fieldworks_paths::GRAMMAR_PHONEMES,
            "change the named boundary marker's representation so it is unique.",
        ),
        BoundaryNoRepresentation => warning(
            "Boundary marker has no representation",
            fieldworks_paths::GRAMMAR_PHONEMES,
            "add a representation to the named boundary marker.",
        ),
        BoundaryMorphMarkerUnresolved => warning(
            "Unresolved morpheme boundary",
            fieldworks_paths::WORDS_EDIT_PARSER_PARAMETERS,
            "assign a declared boundary marker as the morpheme boundary.",
        ),
        NatclassSegmentsMemberUnresolved => warning(
            "Unresolved natural class member",
            fieldworks_paths::GRAMMAR_NATURAL_CLASSES,
            "add a valid phoneme to the named natural class or remove the missing member.",
        ),
        NatclassFeatureConstraintUnresolved => error(
            "Unresolved natural class feature",
            fieldworks_paths::GRAMMAR_NATURAL_CLASSES,
            "correct the named natural class's phonological feature constraints.",
        ),
        NatclassComplexFeatureUnsupported => error(
            "Unsupported natural class feature",
            fieldworks_paths::GRAMMAR_NATURAL_CLASSES,
            "replace the named natural class's complex feature with supported constraints.",
        ),
        PhonComplexFeatureUnsupported => warning(
            "Unsupported complex feature",
            fieldworks_paths::GRAMMAR_PHONOLOGICAL_FEATURES,
            "replace the named complex feature with a supported closed feature.",
        ),
        StemNameBuildFailed => warning(
            "Stem name could not be loaded",
            fieldworks_paths::GRAMMAR_CATEGORY_EDIT,
            "check the named stem name's part of speech and features.",
        ),
        StemNameEmptyRegions => warning(
            "Stem name has no regions",
            fieldworks_paths::GRAMMAR_CATEGORY_EDIT,
            "add a phonological region to the named stem name.",
        ),
        CompoundRuleBuildFailed => warning(
            "Compound rule could not be loaded",
            fieldworks_paths::GRAMMAR_COMPOUND_RULES,
            "check the named compound rule's constituent categories.",
        ),
        CompoundSidePosUnresolved => error(
            "Unresolved compound category",
            fieldworks_paths::GRAMMAR_COMPOUND_RULES,
            "assign a valid part of speech to the named compound rule side.",
        ),
        CompoundSideExceptionFeatureUnresolved => error(
            "Unresolved compound exception feature",
            fieldworks_paths::GRAMMAR_COMPOUND_RULES,
            "correct the named compound rule's exception feature.",
        ),
        MsaBuildFailed => warning(
            "Grammatical analysis could not be loaded",
            fieldworks_paths::LEXICON_EDIT,
            "check the named analysis's part of speech and features.",
        ),
        MsaNoAllomorphs => error(
            "No usable entry allomorphs",
            fieldworks_paths::LEXICON_EDIT,
            "add or correct an allomorph for the named lexical entry.",
        ),
        MsaNoRuleFormAllomorphs => error(
            "Analysis has no usable affix form",
            fieldworks_paths::LEXICON_EDIT,
            "add a usable affix allomorph to the named analysis.",
        ),
        MsaExceptionFeatureUnresolved => error(
            "Unresolved analysis exception feature",
            fieldworks_paths::LEXICON_EDIT,
            "correct the named analysis's exception features.",
        ),
        MsaInflectionClassUnresolved => error(
            "Unresolved analysis inflection class",
            fieldworks_paths::LEXICON_EDIT,
            "assign a valid inflection class to the named analysis.",
        ),
        MsaStemNameUnresolved => error(
            "Unresolved analysis stem name",
            fieldworks_paths::LEXICON_EDIT,
            "assign a valid stem name to the named analysis.",
        ),
        MsaLexEntryInflTypeUnresolved => ImportWarningMetadata {
            group_name: "Unresolved entry inflection type",
            level: DiagnosticLevel::Error,
            guidance: Some(format!(
                "In {}, assign a valid entry inflection type to the named analysis; in {}, add a missing type.",
                fieldworks_paths::LEXICON_EDIT,
                fieldworks_paths::LISTS_VARIANT_TYPES
            )),
        },
        VariantComponentUnresolved => warning(
            "Unresolved variant component",
            fieldworks_paths::LEXICON_EDIT,
            "correct the named variant entry's component reference.",
        ),
        AllomorphUnsegmentable => warning(
            "Allomorph could not be segmented",
            fieldworks_paths::LEXICON_EDIT,
            "check the spelling and phonological environments for allomorph '{subject}'.",
        ),
        AllomorphMorphTypeUnsupported => warning(
            "Unsupported allomorph morph type",
            fieldworks_paths::LEXICON_EDIT,
            "choose a supported morph type for the named allomorph.",
        ),
        // Only a circumfix or discontiguous phrase's whole form, which loads through its separate parts.
        AllomorphMorphTypeUnsupportedAsRuleForm => {
            info("Loaded through its separate parts")
        }
        AllomorphNotRuleForm => warning(
            "Affix allomorph has no form",
            fieldworks_paths::LEXICON_EDIT,
            "add a non-empty form to the named affix allomorph.",
        ),
        AllomorphReduplicationUnsupported => warning(
            "Unsupported reduplication form",
            fieldworks_paths::LEXICON_EDIT,
            "replace the named allomorph's bracket pattern with a supported form.",
        ),
        AllomorphProcessBuildFailed => warning(
            "Affix process could not be loaded",
            fieldworks_paths::LEXICON_EDIT,
            "check the named allomorph's input and output mappings.",
        ),
        AllomorphInflectionClassUnresolved => error(
            "Unresolved allomorph inflection class",
            fieldworks_paths::LEXICON_EDIT,
            "assign a valid inflection class to the named allomorph.",
        ),
        AllomorphFeatureBuildFailed => warning(
            "Allomorph features could not be loaded",
            fieldworks_paths::LEXICON_EDIT,
            "check the named allomorph's morphosyntactic features.",
        ),
        AllomorphEnvironmentBuildFailed => warning(
            "Allomorph environment could not be loaded",
            fieldworks_paths::LEXICON_EDIT,
            "correct the named allomorph's phonological environment.",
        ),
        CircumfixMissingHalf => error(
            "Circumfix is missing a half",
            fieldworks_paths::LEXICON_EDIT,
            "add a prefix or suffix half to the named circumfix entry.",
        ),
        CircumfixEnvironmentCombinationSkipped => warning(
            "Circumfix environment was skipped",
            fieldworks_paths::LEXICON_EDIT,
            "correct the environment on the named circumfix half.",
        ),
        EnvironmentUnresolved => ImportWarningMetadata {
            group_name: "Unresolved phonological environment",
            level: DiagnosticLevel::Error,
            guidance: Some(format!(
                "In {}, add the missing environment; in {}, correct its reference on the named allomorph.",
                fieldworks_paths::GRAMMAR_ENVIRONMENTS,
                fieldworks_paths::LEXICON_EDIT
            )),
        },
        EnvironmentInvalid => error(
            "Invalid phonological environment",
            fieldworks_paths::GRAMMAR_ENVIRONMENTS,
            "correct the expression for phonological environment '{subject}'.",
        ),
        TemplateSlotUnresolved => warning(
            "Affix template slot is unresolved",
            fieldworks_paths::GRAMMAR_CATEGORY_AFFIX_TEMPLATES,
            "correct the named template's reference to its slot.",
        ),
        TemplateSlotNoRules => warning(
            "Affix template slot has no rules",
            fieldworks_paths::GRAMMAR_CATEGORY_AFFIX_TEMPLATES,
            "add a loaded inflectional affix to the named slot or remove the empty slot.",
        ),
        TemplateNoSlots => warning(
            "Empty affix template",
            fieldworks_paths::GRAMMAR_CATEGORY_AFFIX_TEMPLATES,
            "add a slot containing an inflectional affix or remove the named empty template.",
        ),
        TemplateBuildFailed => warning(
            "Affix template could not be loaded",
            fieldworks_paths::GRAMMAR_CATEGORY_AFFIX_TEMPLATES,
            "check the named template's slots and inflectional affixes.",
        ),
        NullAffixMprUnresolved => warning(
            "Unresolved null-affix restriction",
            fieldworks_paths::LISTS_VARIANT_TYPES,
            "correct the named entry inflection type's morphophonological restriction.",
        ),
        NullAffixSynFsFailed => warning(
            "Null-affix features could not be loaded",
            fieldworks_paths::LISTS_VARIANT_TYPES,
            "check the named entry inflection type's feature values.",
        ),
        NullAffixSegmentFailed => warning(
            "Null-affix form could not be segmented",
            fieldworks_paths::LISTS_VARIANT_TYPES,
            "check the named entry inflection type's form and phoneme inventory.",
        ),
        RuleMetathesisUnsupported => warning(
            "Unsupported metathesis rule",
            fieldworks_paths::GRAMMAR_PHONOLOGICAL_RULES,
            "review the named metathesis rule's feature structure.",
        ),
        RuleBuildFailed => warning(
            "Phonological rule could not be loaded",
            fieldworks_paths::GRAMMAR_PHONOLOGICAL_RULES,
            "check the named rule's structural description and change.",
        ),
        FeatureConstraintUnresolved => error(
            "Unresolved rule feature constraint",
            fieldworks_paths::GRAMMAR_PHONOLOGICAL_RULES,
            "correct the named rule's feature constraint.",
        ),
        FeatureConstraintPhonFeatureUnresolved => error(
            "Unresolved phonological feature constraint",
            fieldworks_paths::GRAMMAR_PHONOLOGICAL_RULES,
            "correct the named constraint's phonological feature reference.",
        ),
        RuleFeatureUnresolved => error(
            "Unresolved phonological rule feature",
            fieldworks_paths::GRAMMAR_PHONOLOGICAL_RULES,
            "correct the named rule's feature reference.",
        ),
        StrataCustomUnsupported => warning(
            "Unsupported custom strata",
            fieldworks_paths::WORDS_EDIT_PARSER_PARAMETERS,
            "review the project's parser strata settings.",
        ),
        AdhocProhibitionUnresolved => warning(
            "Unresolved ad hoc prohibition",
            fieldworks_paths::GRAMMAR_AD_HOC_RULES,
            "correct the named prohibition's references to its forms.",
        ),
        MruleUnreachableCompacted => info("Affix is never used"),
        CooccurrenceTargetUnreachable => info("Ad hoc rule is never used"),
        NaturalClassUnreferencedCompacted => info("Unused natural class"),
        SourceProvenanceUnknown => warning(
            "Source item could not be identified",
            fieldworks_paths::LEXICON_EDIT,
            "check the named item's FieldWorks identity and references.",
        ),
        SubstrateUnsegmentableForm => warning(
            "Allomorph form cannot be segmented",
            fieldworks_paths::LEXICON_EDIT,
            "check the named allomorph's spelling and the project's phoneme inventory.",
        ),
        SubstrateClassificationAmbiguous => warning(
            "Allomorph character is ambiguous",
            fieldworks_paths::GRAMMAR_PHONEMES,
            "decide whether the named allomorph's character is a phoneme or boundary marker.",
        ),
        MigrationInferredSegmentWithFeatureRule => error(
            "Character is not listed as a phoneme",
            fieldworks_paths::GRAMMAR_PHONEMES,
            "add the named character as a phoneme before assigning its feature values.",
        ),
        UnsupportedConstruct => warning(
            "Unsupported FieldWorks construct",
            fieldworks_paths::LEXICON_EDIT,
            "replace the named construct with a supported FieldWorks form.",
        ),
        SubstratePositionUnmapped => warning(
            "Allomorph character position is unmapped",
            fieldworks_paths::LEXICON_EDIT,
            "check the named allomorph's character sequence against the project's phonemes and boundary markers.",
        ),
    }
}
