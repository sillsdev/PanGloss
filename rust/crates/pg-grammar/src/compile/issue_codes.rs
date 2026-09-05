//! Stable rejection codes for `pg_grammar::compile`'s snapshot-to-grammar selection recording; each names one owner's drop/fallback decision, independent of the (freely reworded) warning prose.

pub(crate) const PHONEME_NO_REPRESENTATION: &str = "grammar.phoneme.no-representation";
pub(crate) const PHONEME_NFD_COLLISION: &str = "grammar.phoneme.nfd-collision";
pub(crate) const BOUNDARY_NFD_COLLISION: &str = "grammar.boundary.nfd-collision";
pub(crate) const BOUNDARY_NO_REPRESENTATION: &str = "grammar.boundary.no-representation";

pub(crate) const NATCLASS_SEGMENTS_MEMBER_UNRESOLVED: &str =
    "grammar.natclass.segments-member-unresolved";

pub(crate) const PHON_COMPLEX_FEATURE_UNSUPPORTED: &str = "grammar.feature.phon-complex-unsupported";
pub(crate) const STEM_NAME_BUILD_FAILED: &str = "grammar.stem-name.build-failed";
pub(crate) const STEM_NAME_EMPTY_REGIONS: &str = "grammar.stem-name.empty-regions";

pub(crate) const COMPOUND_RULE_BUILD_FAILED: &str = "grammar.compound-rule.build-failed";
pub(crate) const COMPOUND_SIDE_POS_UNRESOLVED: &str = "grammar.compound-rule.side-pos-unresolved";
pub(crate) const COMPOUND_SIDE_EXCEPTION_FEATURE_UNRESOLVED: &str =
    "grammar.compound-rule.side-exception-feature-unresolved";

pub(crate) const MSA_BUILD_FAILED: &str = "grammar.msa.build-failed";
pub(crate) const MSA_NO_ALLOMORPHS: &str = "grammar.msa.no-allomorphs";
pub(crate) const MSA_NO_RULE_FORM_ALLOMORPHS: &str = "grammar.msa.no-rule-form-allomorphs";
pub(crate) const MSA_EXCEPTION_FEATURE_UNRESOLVED: &str = "grammar.msa.exception-feature-unresolved";
pub(crate) const MSA_INFLECTION_CLASS_UNRESOLVED: &str = "grammar.msa.inflection-class-unresolved";
pub(crate) const MSA_STEM_NAME_UNRESOLVED: &str = "grammar.msa.stem-name-unresolved";
pub(crate) const MSA_LEX_ENTRY_INFL_TYPE_UNRESOLVED: &str =
    "grammar.msa.lex-entry-infl-type-unresolved";
pub(crate) const VARIANT_COMPONENT_UNRESOLVED: &str = "grammar.variant.component-unresolved";
pub(crate) const ALLOMORPH_UNSEGMENTABLE: &str = "grammar.allomorph.unsegmentable";
pub(crate) const ALLOMORPH_MORPH_TYPE_UNSUPPORTED: &str = "grammar.allomorph.morph-type-unsupported";
pub(crate) const ALLOMORPH_NOT_RULE_FORM: &str = "grammar.allomorph.not-a-rule-form";
pub(crate) const ALLOMORPH_REDUPLICATION_UNSUPPORTED: &str =
    "grammar.allomorph.reduplication-unsupported";
pub(crate) const ALLOMORPH_PROCESS_BUILD_FAILED: &str = "grammar.allomorph.process-build-failed";
pub(crate) const CIRCUMFIX_MISSING_HALF: &str = "grammar.circumfix.missing-half";

pub(crate) const ENVIRONMENT_UNRESOLVED: &str = "grammar.environment.unresolved";
pub(crate) const ENVIRONMENT_INVALID: &str = "grammar.environment.invalid";

pub(crate) const TEMPLATE_SLOT_UNRESOLVED: &str = "grammar.template.slot-unresolved";
pub(crate) const TEMPLATE_SLOT_NO_RULES: &str = "grammar.template.slot-no-rules";
pub(crate) const TEMPLATE_NO_SLOTS: &str = "grammar.template.no-slots";
pub(crate) const TEMPLATE_BUILD_FAILED: &str = "grammar.template.build-failed";
pub(crate) const NULL_AFFIX_MPR_UNRESOLVED: &str = "grammar.null-affix.mpr-unresolved";
pub(crate) const NULL_AFFIX_SYN_FS_FAILED: &str = "grammar.null-affix.syn-fs-failed";
pub(crate) const NULL_AFFIX_SEGMENT_FAILED: &str = "grammar.null-affix.segment-failed";

pub(crate) const RULE_METATHESIS_UNSUPPORTED: &str = "grammar.rule.metathesis-unsupported";
pub(crate) const RULE_BUILD_FAILED: &str = "grammar.rule.build-failed";
pub(crate) const FEATURE_CONSTRAINT_UNRESOLVED: &str = "grammar.rule.feature-constraint-unresolved";
pub(crate) const FEATURE_CONSTRAINT_PHON_FEATURE_UNRESOLVED: &str =
    "grammar.rule.feature-constraint-phon-feature-unresolved";
pub(crate) const RULE_FEATURE_UNRESOLVED: &str = "grammar.rule.rule-feature-unresolved";

pub(crate) const STRATA_CUSTOM_UNSUPPORTED: &str = "grammar.strata.custom-unsupported";

pub(crate) const ADHOC_PROHIBITION_UNRESOLVED: &str = "grammar.adhoc-prohibition.unresolved";
