//! Stable rejection codes for `pg_grammar::compile`'s snapshot-to-grammar selection recording; each names one owner's drop/fallback decision, independent of the (freely reworded) warning prose.

use pg_snapshot::ImportWarningCode;

pub(crate) const PHONEME_NO_REPRESENTATION: &str =
    ImportWarningCode::PhonemeNoRepresentation.wire();
pub(crate) const PHONEME_NFD_COLLISION: &str = ImportWarningCode::PhonemeNfdCollision.wire();
pub(crate) const PHONEME_FEATURE_UNRESOLVED: &str =
    ImportWarningCode::PhonemeFeatureUnresolved.wire();
pub(crate) const PHONEME_COMPLEX_FEATURE_UNSUPPORTED: &str =
    ImportWarningCode::PhonemeComplexFeatureUnsupported.wire();
pub(crate) const BOUNDARY_NFD_COLLISION: &str = ImportWarningCode::BoundaryNfdCollision.wire();
pub(crate) const BOUNDARY_NO_REPRESENTATION: &str =
    ImportWarningCode::BoundaryNoRepresentation.wire();
pub(crate) const BOUNDARY_MORPH_MARKER_UNRESOLVED: &str =
    ImportWarningCode::BoundaryMorphMarkerUnresolved.wire();

pub(crate) const NATCLASS_SEGMENTS_MEMBER_UNRESOLVED: &str =
    ImportWarningCode::NatclassSegmentsMemberUnresolved.wire();
pub(crate) const NATCLASS_FEATURE_CONSTRAINT_UNRESOLVED: &str =
    ImportWarningCode::NatclassFeatureConstraintUnresolved.wire();
pub(crate) const NATCLASS_COMPLEX_FEATURE_UNSUPPORTED: &str =
    ImportWarningCode::NatclassComplexFeatureUnsupported.wire();

pub(crate) const PHON_COMPLEX_FEATURE_UNSUPPORTED: &str =
    ImportWarningCode::PhonComplexFeatureUnsupported.wire();
pub(crate) const STEM_NAME_BUILD_FAILED: &str = ImportWarningCode::StemNameBuildFailed.wire();
pub(crate) const STEM_NAME_EMPTY_REGIONS: &str = ImportWarningCode::StemNameEmptyRegions.wire();

pub(crate) const COMPOUND_RULE_BUILD_FAILED: &str =
    ImportWarningCode::CompoundRuleBuildFailed.wire();
pub(crate) const COMPOUND_SIDE_POS_UNRESOLVED: &str =
    ImportWarningCode::CompoundSidePosUnresolved.wire();
pub(crate) const COMPOUND_SIDE_EXCEPTION_FEATURE_UNRESOLVED: &str =
    ImportWarningCode::CompoundSideExceptionFeatureUnresolved.wire();

pub(crate) const MSA_BUILD_FAILED: &str = ImportWarningCode::MsaBuildFailed.wire();
pub(crate) const MSA_NO_ALLOMORPHS: &str = ImportWarningCode::MsaNoAllomorphs.wire();
pub(crate) const MSA_NO_RULE_FORM_ALLOMORPHS: &str =
    ImportWarningCode::MsaNoRuleFormAllomorphs.wire();
pub(crate) const MSA_EXCEPTION_FEATURE_UNRESOLVED: &str =
    ImportWarningCode::MsaExceptionFeatureUnresolved.wire();
pub(crate) const MSA_INFLECTION_CLASS_UNRESOLVED: &str =
    ImportWarningCode::MsaInflectionClassUnresolved.wire();
pub(crate) const MSA_STEM_NAME_UNRESOLVED: &str = ImportWarningCode::MsaStemNameUnresolved.wire();
pub(crate) const MSA_LEX_ENTRY_INFL_TYPE_UNRESOLVED: &str =
    ImportWarningCode::MsaLexEntryInflTypeUnresolved.wire();
pub(crate) const VARIANT_COMPONENT_UNRESOLVED: &str =
    ImportWarningCode::VariantComponentUnresolved.wire();
pub(crate) const ALLOMORPH_UNSEGMENTABLE: &str = ImportWarningCode::AllomorphUnsegmentable.wire();
pub(crate) const ALLOMORPH_MORPH_TYPE_UNSUPPORTED: &str =
    ImportWarningCode::AllomorphMorphTypeUnsupported.wire();
pub(crate) const ALLOMORPH_MORPH_TYPE_UNSUPPORTED_AS_RULE_FORM: &str =
    ImportWarningCode::AllomorphMorphTypeUnsupportedAsRuleForm.wire();
pub(crate) const ALLOMORPH_NOT_RULE_FORM: &str = ImportWarningCode::AllomorphNotRuleForm.wire();
pub(crate) const ALLOMORPH_REDUPLICATION_UNSUPPORTED: &str =
    ImportWarningCode::AllomorphReduplicationUnsupported.wire();
pub(crate) const ALLOMORPH_PROCESS_BUILD_FAILED: &str =
    ImportWarningCode::AllomorphProcessBuildFailed.wire();
pub(crate) const ALLOMORPH_INFLECTION_CLASS_UNRESOLVED: &str =
    ImportWarningCode::AllomorphInflectionClassUnresolved.wire();
pub(crate) const ALLOMORPH_FEATURE_BUILD_FAILED: &str =
    ImportWarningCode::AllomorphFeatureBuildFailed.wire();
pub(crate) const ALLOMORPH_ENVIRONMENT_BUILD_FAILED: &str =
    ImportWarningCode::AllomorphEnvironmentBuildFailed.wire();
pub(crate) const SUBSTRATE_UNSEGMENTABLE_FORM: &str =
    ImportWarningCode::SubstrateUnsegmentableForm.wire();
pub(crate) const SUBSTRATE_CLASSIFICATION_AMBIGUOUS: &str =
    ImportWarningCode::SubstrateClassificationAmbiguous.wire();
pub(crate) const SUBSTRATE_POSITION_UNMAPPED: &str =
    ImportWarningCode::SubstratePositionUnmapped.wire();
pub(crate) const UNSUPPORTED_CONSTRUCT: &str = ImportWarningCode::UnsupportedConstruct.wire();
pub(crate) const CIRCUMFIX_ENVIRONMENT_COMBINATION_SKIPPED: &str =
    ImportWarningCode::CircumfixEnvironmentCombinationSkipped.wire();
pub(crate) const CIRCUMFIX_MISSING_HALF: &str = ImportWarningCode::CircumfixMissingHalf.wire();

pub(crate) const ENVIRONMENT_UNRESOLVED: &str = ImportWarningCode::EnvironmentUnresolved.wire();
pub(crate) const ENVIRONMENT_INVALID: &str = ImportWarningCode::EnvironmentInvalid.wire();

pub(crate) const TEMPLATE_SLOT_UNRESOLVED: &str = ImportWarningCode::TemplateSlotUnresolved.wire();
pub(crate) const TEMPLATE_SLOT_NO_RULES: &str = ImportWarningCode::TemplateSlotNoRules.wire();
pub(crate) const TEMPLATE_NO_SLOTS: &str = ImportWarningCode::TemplateNoSlots.wire();
pub(crate) const TEMPLATE_BUILD_FAILED: &str = ImportWarningCode::TemplateBuildFailed.wire();
pub(crate) const NULL_AFFIX_MPR_UNRESOLVED: &str = ImportWarningCode::NullAffixMprUnresolved.wire();
pub(crate) const NULL_AFFIX_SYN_FS_FAILED: &str = ImportWarningCode::NullAffixSynFsFailed.wire();
pub(crate) const NULL_AFFIX_SEGMENT_FAILED: &str = ImportWarningCode::NullAffixSegmentFailed.wire();

pub(crate) const RULE_METATHESIS_UNSUPPORTED: &str =
    ImportWarningCode::RuleMetathesisUnsupported.wire();
pub(crate) const RULE_BUILD_FAILED: &str = ImportWarningCode::RuleBuildFailed.wire();
pub(crate) const FEATURE_CONSTRAINT_UNRESOLVED: &str =
    ImportWarningCode::FeatureConstraintUnresolved.wire();
pub(crate) const FEATURE_CONSTRAINT_PHON_FEATURE_UNRESOLVED: &str =
    ImportWarningCode::FeatureConstraintPhonFeatureUnresolved.wire();
pub(crate) const RULE_FEATURE_UNRESOLVED: &str = ImportWarningCode::RuleFeatureUnresolved.wire();

pub(crate) const STRATA_CUSTOM_UNSUPPORTED: &str =
    ImportWarningCode::StrataCustomUnsupported.wire();

pub(crate) const ADHOC_PROHIBITION_UNRESOLVED: &str =
    ImportWarningCode::AdhocProhibitionUnresolved.wire();

pub(crate) const MRULE_UNREACHABLE_COMPACTED: &str =
    ImportWarningCode::MruleUnreachableCompacted.wire();
pub(crate) const COOCCURRENCE_TARGET_UNREACHABLE: &str =
    ImportWarningCode::CooccurrenceTargetUnreachable.wire();
pub(crate) const NATURAL_CLASS_UNREFERENCED_COMPACTED: &str =
    ImportWarningCode::NaturalClassUnreferencedCompacted.wire();
