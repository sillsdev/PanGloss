//! The exhaustive per-code owner for import warning audience, group, and FieldWorks guidance.

use crate::fieldworks_paths;
use crate::{Audience, FwClass, ImportWarningCode};

pub struct ImportWarningMetadata {
    pub group_name: &'static str,
    pub audience: Audience,
    /// `None` for developer-only notices that have no linguist action.
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

fn linguist_warning(
    group_name: &'static str,
    path: &str,
    action: &'static str,
) -> ImportWarningMetadata {
    ImportWarningMetadata {
        group_name,
        audience: Audience::Linguist,
        guidance: Some(format!("In {path}, {action}")),
    }
}

fn internal_warning(group_name: &'static str) -> ImportWarningMetadata {
    ImportWarningMetadata {
        group_name,
        audience: Audience::Developer,
        guidance: None,
    }
}

pub fn import_warning_metadata(code: ImportWarningCode) -> ImportWarningMetadata {
    use ImportWarningCode::*;

    match code {
        FwdataDanglingReference => linguist_warning(
            "Missing FieldWorks reference",
            fieldworks_paths::LEXICON_EDIT,
            "restore or remove the named item's missing reference.",
        ),
        FwdataUnexpectedClass => linguist_warning(
            "Unexpected FieldWorks item",
            fieldworks_paths::LEXICON_EDIT,
            "check the named item's class and references.",
        ),
        FwdataMissingRequiredField => linguist_warning(
            "Missing required FieldWorks field",
            fieldworks_paths::LEXICON_EDIT,
            "complete the named item's required field.",
        ),
        FwdataMissingLangProject => linguist_warning(
            "Missing language project",
            fieldworks_paths::FILE_RESTORE_PROJECT,
            "restore a valid FieldWorks project backup.",
        ),
        FwdataOnlyFirstUsed => linguist_warning(
            "Only first list item is used",
            fieldworks_paths::GRAMMAR_PHONEMES,
            "check the project's phoneme sets.",
        ),
        FwdataEmptyRepresentation => linguist_warning(
            "Empty item representation",
            fieldworks_paths::GRAMMAR_PHONEMES,
            "add a representation to the named phoneme or boundary marker.",
        ),
        FwdataUnrecognizedEnumValue => linguist_warning(
            "Unrecognized FieldWorks value",
            fieldworks_paths::LEXICON_EDIT,
            "replace the named item's value with one offered by FieldWorks.",
        ),
        FwdataMetathesisApproximation => linguist_warning(
            "Metathesis rule approximation",
            fieldworks_paths::GRAMMAR_PHONOLOGICAL_RULES,
            "review the named metathesis rule's structural description.",
        ),
        FwdataStaleAdhocProhibition => linguist_warning(
            "Stale ad hoc prohibition",
            fieldworks_paths::GRAMMAR_AD_HOC_RULES,
            "update the named prohibition's affix reference or remove it.",
        ),
        Unregistered(_) => internal_warning("Unregistered warning code"),
        FwdataNoUsableAllomorphs => linguist_warning(
            "No usable allomorphs",
            fieldworks_paths::LEXICON_EDIT,
            "add or correct an allomorph for the named lexical entry.",
        ),
        FwdataUnsupportedMorphType => linguist_warning(
            "Unsupported morph type",
            fieldworks_paths::LEXICON_EDIT,
            "choose a supported morph type for the named allomorph.",
        ),
        FwdataUnknownMorphTypeGuid => linguist_warning(
            "Unknown morph type reference",
            fieldworks_paths::LEXICON_EDIT,
            "assign a morph type that exists in the project to the named allomorph.",
        ),
        FwdataReferenceNotInScope => linguist_warning(
            "Reference outside item scope",
            fieldworks_paths::LEXICON_EDIT,
            "move the named reference into the correct FieldWorks list.",
        ),
        FwdataInvalidParserParameter => linguist_warning(
            "Invalid parser setting",
            fieldworks_paths::WORDS_EDIT_PARSER_PARAMETERS,
            "correct the named parser setting.",
        ),
        InvalidSourceActiveParser => linguist_warning(
            "Unsupported active parser",
            fieldworks_paths::WORDS_EDIT_PARSER_PARAMETERS,
            "select a supported parser.",
        ),
        InvalidSourceDuplicateGuid => linguist_warning(
            "Duplicate FieldWorks GUID",
            fieldworks_paths::LEXICON_EDIT,
            "resolve the duplicate identity for the named item.",
        ),
        InvalidSourceMissingGuid => linguist_warning(
            "Missing FieldWorks GUID",
            fieldworks_paths::LEXICON_EDIT,
            "save the named item in FieldWorks so it has a stable identity.",
        ),
        FwdataWritingSystemStoreUnreadable => ImportWarningMetadata {
            group_name: "Writing system data unavailable",
            audience: Audience::Linguist,
            guidance: Some(format!(
                "In {} and {}, check the project's writing system data.",
                fieldworks_paths::TOOLS_CONFIGURE_VERNACULAR_WRITING_SYSTEMS,
                fieldworks_paths::TOOLS_CONFIGURE_ANALYSIS_WRITING_SYSTEMS
            )),
        },
        SnapshotDanglingReference => linguist_warning(
            "Missing snapshot reference",
            fieldworks_paths::LEXICON_EDIT,
            "restore or remove the named item's missing reference.",
        ),
        SnapshotFeatureStructureUnresolved => ImportWarningMetadata {
            group_name: "Unresolved feature structure",
            audience: Audience::Linguist,
            guidance: Some(format!(
                "In {} for lexical features or {} for phonological features, correct the named item's feature values.",
                fieldworks_paths::LEXICON_EDIT,
                fieldworks_paths::GRAMMAR_PHONOLOGICAL_FEATURES
            )),
        },
        SnapshotRuleFeatureUnresolved => linguist_warning(
            "Unresolved rule feature structure",
            fieldworks_paths::GRAMMAR_PHONOLOGICAL_RULES,
            "correct the named rule's feature structure.",
        ),
        SnapshotReferenceOutOfScope => linguist_warning(
            "Snapshot reference out of scope",
            fieldworks_paths::LEXICON_EDIT,
            "move the named reference into the FieldWorks list that owns it.",
        ),
        PhonemeNoRepresentation => linguist_warning(
            "Phoneme has no representation",
            fieldworks_paths::GRAMMAR_PHONEMES,
            "add a grapheme representation to the named phoneme.",
        ),
        PhonemeNfdCollision => linguist_warning(
            "Phoneme representation collision",
            fieldworks_paths::GRAMMAR_PHONEMES,
            "change the named phoneme's representation so it is unique.",
        ),
        PhonemeFeatureUnresolved => linguist_warning(
            "Unresolved phoneme feature value",
            fieldworks_paths::GRAMMAR_PHONEMES,
            "correct the named phoneme's phonological feature values.",
        ),
        PhonemeComplexFeatureUnsupported => linguist_warning(
            "Unsupported phoneme feature value",
            fieldworks_paths::GRAMMAR_PHONEMES,
            "replace the named phoneme's complex value with a supported feature value.",
        ),
        BoundaryNfdCollision => linguist_warning(
            "Boundary representation collision",
            fieldworks_paths::GRAMMAR_PHONEMES,
            "change the named boundary marker's representation so it is unique.",
        ),
        BoundaryNoRepresentation => linguist_warning(
            "Boundary marker has no representation",
            fieldworks_paths::GRAMMAR_PHONEMES,
            "add a representation to the named boundary marker.",
        ),
        BoundaryMorphMarkerUnresolved => linguist_warning(
            "Unresolved morpheme boundary",
            fieldworks_paths::WORDS_EDIT_PARSER_PARAMETERS,
            "assign a declared boundary marker as the morpheme boundary.",
        ),
        NatclassSegmentsMemberUnresolved => linguist_warning(
            "Unresolved natural class member",
            fieldworks_paths::GRAMMAR_NATURAL_CLASSES,
            "add a valid phoneme to the named natural class or remove the missing member.",
        ),
        NatclassFeatureConstraintUnresolved => linguist_warning(
            "Unresolved natural class feature",
            fieldworks_paths::GRAMMAR_NATURAL_CLASSES,
            "correct the named natural class's phonological feature constraints.",
        ),
        NatclassComplexFeatureUnsupported => linguist_warning(
            "Unsupported natural class feature",
            fieldworks_paths::GRAMMAR_NATURAL_CLASSES,
            "replace the named natural class's complex feature with supported constraints.",
        ),
        PhonComplexFeatureUnsupported => linguist_warning(
            "Unsupported complex feature",
            fieldworks_paths::GRAMMAR_PHONOLOGICAL_FEATURES,
            "replace the named complex feature with a supported closed feature.",
        ),
        StemNameBuildFailed => linguist_warning(
            "Stem name could not be loaded",
            fieldworks_paths::GRAMMAR_CATEGORY_EDIT,
            "check the named stem name's part of speech and features.",
        ),
        StemNameEmptyRegions => linguist_warning(
            "Stem name has no regions",
            fieldworks_paths::GRAMMAR_CATEGORY_EDIT,
            "add a phonological region to the named stem name.",
        ),
        CompoundRuleBuildFailed => linguist_warning(
            "Compound rule could not be loaded",
            fieldworks_paths::GRAMMAR_COMPOUND_RULES,
            "check the named compound rule's constituent categories.",
        ),
        CompoundSidePosUnresolved => linguist_warning(
            "Unresolved compound category",
            fieldworks_paths::GRAMMAR_COMPOUND_RULES,
            "assign a valid part of speech to the named compound rule side.",
        ),
        CompoundSideExceptionFeatureUnresolved => linguist_warning(
            "Unresolved compound exception feature",
            fieldworks_paths::GRAMMAR_COMPOUND_RULES,
            "correct the named compound rule's exception feature.",
        ),
        MsaBuildFailed => linguist_warning(
            "Grammatical analysis could not be loaded",
            fieldworks_paths::LEXICON_EDIT,
            "check the named analysis's part of speech and features.",
        ),
        MsaNoAllomorphs => linguist_warning(
            "No usable entry allomorphs",
            fieldworks_paths::LEXICON_EDIT,
            "add or correct an allomorph for the named lexical entry.",
        ),
        MsaNoRuleFormAllomorphs => linguist_warning(
            "Analysis has no usable affix form",
            fieldworks_paths::LEXICON_EDIT,
            "add a usable affix allomorph to the named analysis.",
        ),
        MsaExceptionFeatureUnresolved => linguist_warning(
            "Unresolved analysis exception feature",
            fieldworks_paths::LEXICON_EDIT,
            "correct the named analysis's exception features.",
        ),
        MsaInflectionClassUnresolved => linguist_warning(
            "Unresolved analysis inflection class",
            fieldworks_paths::LEXICON_EDIT,
            "assign a valid inflection class to the named analysis.",
        ),
        MsaStemNameUnresolved => linguist_warning(
            "Unresolved analysis stem name",
            fieldworks_paths::LEXICON_EDIT,
            "assign a valid stem name to the named analysis.",
        ),
        MsaLexEntryInflTypeUnresolved => ImportWarningMetadata {
            group_name: "Unresolved entry inflection type",
            audience: Audience::Linguist,
            guidance: Some(format!(
                "In {}, assign a valid entry inflection type to the named analysis; in {}, add a missing type.",
                fieldworks_paths::LEXICON_EDIT,
                fieldworks_paths::LISTS_VARIANT_TYPES
            )),
        },
        VariantComponentUnresolved => linguist_warning(
            "Unresolved variant component",
            fieldworks_paths::LEXICON_EDIT,
            "correct the named variant entry's component reference.",
        ),
        AllomorphUnsegmentable => linguist_warning(
            "Allomorph could not be segmented",
            fieldworks_paths::LEXICON_EDIT,
            "check the spelling and phonological environments for allomorph '{subject}'.",
        ),
        AllomorphMorphTypeUnsupported => linguist_warning(
            "Unsupported allomorph morph type",
            fieldworks_paths::LEXICON_EDIT,
            "choose a supported morph type for the named allomorph.",
        ),
        AllomorphMorphTypeUnsupportedAsRuleForm => linguist_warning(
            "Allomorph type is not usable for this affix",
            fieldworks_paths::LEXICON_EDIT,
            "check the named allomorph's morph type and how it is used by the analysis.",
        ),
        AllomorphNotRuleForm => linguist_warning(
            "Affix allomorph has no form",
            fieldworks_paths::LEXICON_EDIT,
            "add a non-empty form to the named affix allomorph.",
        ),
        AllomorphReduplicationUnsupported => linguist_warning(
            "Unsupported reduplication form",
            fieldworks_paths::LEXICON_EDIT,
            "replace the named allomorph's bracket pattern with a supported form.",
        ),
        AllomorphProcessBuildFailed => linguist_warning(
            "Affix process could not be loaded",
            fieldworks_paths::LEXICON_EDIT,
            "check the named allomorph's input and output mappings.",
        ),
        AllomorphInflectionClassUnresolved => linguist_warning(
            "Unresolved allomorph inflection class",
            fieldworks_paths::LEXICON_EDIT,
            "assign a valid inflection class to the named allomorph.",
        ),
        AllomorphFeatureBuildFailed => linguist_warning(
            "Allomorph features could not be loaded",
            fieldworks_paths::LEXICON_EDIT,
            "check the named allomorph's morphosyntactic features.",
        ),
        AllomorphEnvironmentBuildFailed => linguist_warning(
            "Allomorph environment could not be loaded",
            fieldworks_paths::LEXICON_EDIT,
            "correct the named allomorph's phonological environment.",
        ),
        CircumfixMissingHalf => linguist_warning(
            "Circumfix is missing a half",
            fieldworks_paths::LEXICON_EDIT,
            "add a prefix or suffix half to the named circumfix entry.",
        ),
        CircumfixEnvironmentCombinationSkipped => linguist_warning(
            "Circumfix environment was skipped",
            fieldworks_paths::LEXICON_EDIT,
            "correct the environment on the named circumfix half.",
        ),
        EnvironmentUnresolved => ImportWarningMetadata {
            group_name: "Unresolved phonological environment",
            audience: Audience::Linguist,
            guidance: Some(format!(
                "In {}, add the missing environment; in {}, correct its reference on the named allomorph.",
                fieldworks_paths::GRAMMAR_ENVIRONMENTS,
                fieldworks_paths::LEXICON_EDIT
            )),
        },
        EnvironmentInvalid => linguist_warning(
            "Invalid phonological environment",
            fieldworks_paths::GRAMMAR_ENVIRONMENTS,
            "correct the expression for phonological environment '{subject}'.",
        ),
        TemplateSlotUnresolved => linguist_warning(
            "Affix template slot is unresolved",
            fieldworks_paths::GRAMMAR_CATEGORY_AFFIX_TEMPLATES,
            "correct the named template's reference to its slot.",
        ),
        TemplateSlotNoRules => linguist_warning(
            "Affix template slot has no rules",
            fieldworks_paths::GRAMMAR_CATEGORY_AFFIX_TEMPLATES,
            "add a loaded inflectional affix to the named slot or remove the empty slot.",
        ),
        TemplateNoSlots => linguist_warning(
            "Empty affix template",
            fieldworks_paths::GRAMMAR_CATEGORY_AFFIX_TEMPLATES,
            "add a slot containing an inflectional affix or remove the named empty template.",
        ),
        TemplateBuildFailed => linguist_warning(
            "Affix template could not be loaded",
            fieldworks_paths::GRAMMAR_CATEGORY_AFFIX_TEMPLATES,
            "check the named template's slots and inflectional affixes.",
        ),
        NullAffixMprUnresolved => linguist_warning(
            "Unresolved null-affix restriction",
            fieldworks_paths::LISTS_VARIANT_TYPES,
            "correct the named entry inflection type's morphophonological restriction.",
        ),
        NullAffixSynFsFailed => linguist_warning(
            "Null-affix features could not be loaded",
            fieldworks_paths::LISTS_VARIANT_TYPES,
            "check the named entry inflection type's feature values.",
        ),
        NullAffixSegmentFailed => linguist_warning(
            "Null-affix form could not be segmented",
            fieldworks_paths::LISTS_VARIANT_TYPES,
            "check the named entry inflection type's form and phoneme inventory.",
        ),
        RuleMetathesisUnsupported => linguist_warning(
            "Unsupported metathesis rule",
            fieldworks_paths::GRAMMAR_PHONOLOGICAL_RULES,
            "review the named metathesis rule's feature structure.",
        ),
        RuleBuildFailed => linguist_warning(
            "Phonological rule could not be loaded",
            fieldworks_paths::GRAMMAR_PHONOLOGICAL_RULES,
            "check the named rule's structural description and change.",
        ),
        FeatureConstraintUnresolved => linguist_warning(
            "Unresolved rule feature constraint",
            fieldworks_paths::GRAMMAR_PHONOLOGICAL_RULES,
            "correct the named rule's feature constraint.",
        ),
        FeatureConstraintPhonFeatureUnresolved => linguist_warning(
            "Unresolved phonological feature constraint",
            fieldworks_paths::GRAMMAR_PHONOLOGICAL_RULES,
            "correct the named constraint's phonological feature reference.",
        ),
        RuleFeatureUnresolved => linguist_warning(
            "Unresolved phonological rule feature",
            fieldworks_paths::GRAMMAR_PHONOLOGICAL_RULES,
            "correct the named rule's feature reference.",
        ),
        StrataCustomUnsupported => linguist_warning(
            "Unsupported custom strata",
            fieldworks_paths::WORDS_EDIT_PARSER_PARAMETERS,
            "review the project's parser strata settings.",
        ),
        AdhocProhibitionUnresolved => linguist_warning(
            "Unresolved ad hoc prohibition",
            fieldworks_paths::GRAMMAR_AD_HOC_RULES,
            "correct the named prohibition's references to its forms.",
        ),
        MruleUnreachableCompacted => internal_warning("Unreachable morphology rule"),
        CooccurrenceTargetUnreachable => internal_warning("Unreachable co-occurrence rule"),
        NaturalClassUnreferencedCompacted => internal_warning("Unused natural class"),
        SourceProvenanceUnknown => linguist_warning(
            "Source item could not be identified",
            fieldworks_paths::LEXICON_EDIT,
            "check the named item's FieldWorks identity and references.",
        ),
        SubstrateUnsegmentableForm => linguist_warning(
            "Allomorph form cannot be segmented",
            fieldworks_paths::LEXICON_EDIT,
            "check the named allomorph's spelling and the project's phoneme inventory.",
        ),
        SubstrateClassificationAmbiguous => linguist_warning(
            "Allomorph character is ambiguous",
            fieldworks_paths::GRAMMAR_PHONEMES,
            "decide whether the named allomorph's character is a phoneme or boundary marker.",
        ),
        MigrationInferredSegmentWithFeatureRule => linguist_warning(
            "Character is not listed as a phoneme",
            fieldworks_paths::GRAMMAR_PHONEMES,
            "add the named character as a phoneme before assigning its feature values.",
        ),
        UnsupportedConstruct => linguist_warning(
            "Unsupported FieldWorks construct",
            fieldworks_paths::LEXICON_EDIT,
            "replace the named construct with a supported FieldWorks form.",
        ),
        SubstratePositionUnmapped => linguist_warning(
            "Allomorph character position is unmapped",
            fieldworks_paths::LEXICON_EDIT,
            "check the named allomorph's character sequence against the project's phonemes and boundary markers.",
        ),
    }
}
