//! Stable rejection codes for `pg_grammar::compile`'s snapshot-to-grammar selection recording; each names one owner's drop/fallback decision, independent of the (freely reworded) warning prose.

use pg_snapshot::ImportWarningCode;

pub(crate) const PHONEME_NO_REPRESENTATION: ImportWarningCode =
    ImportWarningCode::PhonemeNoRepresentation;
pub(crate) const PHONEME_NFD_COLLISION: ImportWarningCode = ImportWarningCode::PhonemeNfdCollision;
pub(crate) const PHONEME_FEATURE_UNRESOLVED: ImportWarningCode =
    ImportWarningCode::PhonemeFeatureUnresolved;
pub(crate) const PHONEME_COMPLEX_FEATURE_UNSUPPORTED: ImportWarningCode =
    ImportWarningCode::PhonemeComplexFeatureUnsupported;
pub(crate) const BOUNDARY_NFD_COLLISION: ImportWarningCode =
    ImportWarningCode::BoundaryNfdCollision;
pub(crate) const BOUNDARY_NO_REPRESENTATION: ImportWarningCode =
    ImportWarningCode::BoundaryNoRepresentation;
pub(crate) const BOUNDARY_MORPH_MARKER_UNRESOLVED: ImportWarningCode =
    ImportWarningCode::BoundaryMorphMarkerUnresolved;

pub(crate) const NATCLASS_SEGMENTS_MEMBER_UNRESOLVED: ImportWarningCode =
    ImportWarningCode::NatclassSegmentsMemberUnresolved;
pub(crate) const NATCLASS_FEATURE_CONSTRAINT_UNRESOLVED: ImportWarningCode =
    ImportWarningCode::NatclassFeatureConstraintUnresolved;
pub(crate) const NATCLASS_COMPLEX_FEATURE_UNSUPPORTED: ImportWarningCode =
    ImportWarningCode::NatclassComplexFeatureUnsupported;

pub(crate) const PHON_COMPLEX_FEATURE_UNSUPPORTED: ImportWarningCode =
    ImportWarningCode::PhonComplexFeatureUnsupported;
pub(crate) const STEM_NAME_BUILD_FAILED: ImportWarningCode = ImportWarningCode::StemNameBuildFailed;
pub(crate) const STEM_NAME_EMPTY_REGIONS: ImportWarningCode =
    ImportWarningCode::StemNameEmptyRegions;

pub(crate) const COMPOUND_RULE_BUILD_FAILED: ImportWarningCode =
    ImportWarningCode::CompoundRuleBuildFailed;
pub(crate) const COMPOUND_SIDE_POS_UNRESOLVED: ImportWarningCode =
    ImportWarningCode::CompoundSidePosUnresolved;
pub(crate) const COMPOUND_SIDE_EXCEPTION_FEATURE_UNRESOLVED: ImportWarningCode =
    ImportWarningCode::CompoundSideExceptionFeatureUnresolved;

pub(crate) const MSA_BUILD_FAILED: ImportWarningCode = ImportWarningCode::MsaBuildFailed;
pub(crate) const MSA_NO_ALLOMORPHS: ImportWarningCode = ImportWarningCode::MsaNoAllomorphs;
pub(crate) const MSA_NO_RULE_FORM_ALLOMORPHS: ImportWarningCode =
    ImportWarningCode::MsaNoRuleFormAllomorphs;
pub(crate) const MSA_EXCEPTION_FEATURE_UNRESOLVED: ImportWarningCode =
    ImportWarningCode::MsaExceptionFeatureUnresolved;
pub(crate) const MSA_INFLECTION_CLASS_UNRESOLVED: ImportWarningCode =
    ImportWarningCode::MsaInflectionClassUnresolved;
pub(crate) const MSA_STEM_NAME_UNRESOLVED: ImportWarningCode =
    ImportWarningCode::MsaStemNameUnresolved;
pub(crate) const MSA_LEX_ENTRY_INFL_TYPE_UNRESOLVED: ImportWarningCode =
    ImportWarningCode::MsaLexEntryInflTypeUnresolved;
pub(crate) const VARIANT_COMPONENT_UNRESOLVED: ImportWarningCode =
    ImportWarningCode::VariantComponentUnresolved;
pub(crate) const ALLOMORPH_UNSEGMENTABLE: ImportWarningCode =
    ImportWarningCode::AllomorphUnsegmentable;
pub(crate) const ALLOMORPH_MORPH_TYPE_UNSUPPORTED: ImportWarningCode =
    ImportWarningCode::AllomorphMorphTypeUnsupported;
pub(crate) const ALLOMORPH_MORPH_TYPE_UNSUPPORTED_AS_RULE_FORM: ImportWarningCode =
    ImportWarningCode::AllomorphMorphTypeUnsupportedAsRuleForm;
pub(crate) const ALLOMORPH_NOT_RULE_FORM: ImportWarningCode =
    ImportWarningCode::AllomorphNotRuleForm;
pub(crate) const ALLOMORPH_REDUPLICATION_UNSUPPORTED: ImportWarningCode =
    ImportWarningCode::AllomorphReduplicationUnsupported;
pub(crate) const ALLOMORPH_PROCESS_BUILD_FAILED: ImportWarningCode =
    ImportWarningCode::AllomorphProcessBuildFailed;
pub(crate) const ALLOMORPH_INFLECTION_CLASS_UNRESOLVED: ImportWarningCode =
    ImportWarningCode::AllomorphInflectionClassUnresolved;
pub(crate) const ALLOMORPH_FEATURE_BUILD_FAILED: ImportWarningCode =
    ImportWarningCode::AllomorphFeatureBuildFailed;
pub(crate) const ALLOMORPH_ENVIRONMENT_BUILD_FAILED: ImportWarningCode =
    ImportWarningCode::AllomorphEnvironmentBuildFailed;
pub(crate) const CIRCUMFIX_ENVIRONMENT_COMBINATION_SKIPPED: ImportWarningCode =
    ImportWarningCode::CircumfixEnvironmentCombinationSkipped;
pub(crate) const CIRCUMFIX_MISSING_HALF: ImportWarningCode =
    ImportWarningCode::CircumfixMissingHalf;

pub(crate) const ENVIRONMENT_UNRESOLVED: ImportWarningCode =
    ImportWarningCode::EnvironmentUnresolved;
pub(crate) const ENVIRONMENT_INVALID: ImportWarningCode = ImportWarningCode::EnvironmentInvalid;

pub(crate) const TEMPLATE_SLOT_UNRESOLVED: ImportWarningCode =
    ImportWarningCode::TemplateSlotUnresolved;
pub(crate) const TEMPLATE_SLOT_NO_RULES: ImportWarningCode = ImportWarningCode::TemplateSlotNoRules;
pub(crate) const TEMPLATE_NO_SLOTS: ImportWarningCode = ImportWarningCode::TemplateNoSlots;
pub(crate) const TEMPLATE_BUILD_FAILED: ImportWarningCode = ImportWarningCode::TemplateBuildFailed;
pub(crate) const NULL_AFFIX_MPR_UNRESOLVED: ImportWarningCode =
    ImportWarningCode::NullAffixMprUnresolved;
pub(crate) const NULL_AFFIX_SYN_FS_FAILED: ImportWarningCode =
    ImportWarningCode::NullAffixSynFsFailed;
pub(crate) const NULL_AFFIX_SEGMENT_FAILED: ImportWarningCode =
    ImportWarningCode::NullAffixSegmentFailed;

pub(crate) const RULE_METATHESIS_UNSUPPORTED: ImportWarningCode =
    ImportWarningCode::RuleMetathesisUnsupported;
pub(crate) const RULE_BUILD_FAILED: ImportWarningCode = ImportWarningCode::RuleBuildFailed;
pub(crate) const FEATURE_CONSTRAINT_UNRESOLVED: ImportWarningCode =
    ImportWarningCode::FeatureConstraintUnresolved;
pub(crate) const FEATURE_CONSTRAINT_PHON_FEATURE_UNRESOLVED: ImportWarningCode =
    ImportWarningCode::FeatureConstraintPhonFeatureUnresolved;
pub(crate) const RULE_FEATURE_UNRESOLVED: ImportWarningCode =
    ImportWarningCode::RuleFeatureUnresolved;

pub(crate) const STRATA_CUSTOM_UNSUPPORTED: ImportWarningCode =
    ImportWarningCode::StrataCustomUnsupported;

pub(crate) const ADHOC_PROHIBITION_UNRESOLVED: ImportWarningCode =
    ImportWarningCode::AdhocProhibitionUnresolved;

pub(crate) const MRULE_UNREACHABLE_COMPACTED: ImportWarningCode =
    ImportWarningCode::MruleUnreachableCompacted;
pub(crate) const COOCCURRENCE_TARGET_UNREACHABLE: ImportWarningCode =
    ImportWarningCode::CooccurrenceTargetUnreachable;
pub(crate) const NATURAL_CLASS_UNREFERENCED_COMPACTED: ImportWarningCode =
    ImportWarningCode::NaturalClassUnreferencedCompacted;
