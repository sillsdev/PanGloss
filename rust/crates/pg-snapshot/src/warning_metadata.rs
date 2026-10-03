//! The exhaustive per-code owner for import warning level, group, and FieldWorks guidance.

use crate::fieldworks_paths;
use crate::{DiagnosticLevel, FwClass, ImportWarningCode};

/// Shared help for grammar-wide advice. These are CommonMark fragments with no page chrome.
pub const MODELLING_HELP: &str = include_str!("../help/modelling-a-grammar-the-parser-can-use.md");
pub const ALLOMORPHS_HELP: &str = include_str!("../help/allomorphs-and-environments.md");
pub const STEMS_HELP: &str = include_str!("../help/stems-and-the-lexicon.md");
pub const PARSER_HELP: &str = include_str!("../help/parser-behavior.md");
pub const WORD_FAILURE_HELP: &str = include_str!("../help/reading-why-a-word-fails.md");

/// One verified FieldWorks destination and an optional editable field name.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldWorksPlace {
    /// FieldWorks tool ID accepted by Motif's FieldWorks link resolver.
    pub tool: String,
    /// Field name verified for this correction, or empty when only the tool is known.
    pub field: String,
}

/// Stable, shareable advice for one registered diagnostic code.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticAdvice {
    /// Short human-readable label for this kind of finding.
    pub title: &'static str,
    /// One sentence describing the actual import or compiler behavior.
    pub explanation: &'static str,
    /// The next step for the grammar author; may contain `{subject}`.
    pub guidance: &'static str,
    /// Verified FieldWorks destinations; an empty list means none is established.
    pub fieldworks_places: Vec<FieldWorksPlace>,
    /// Optional reusable CommonMark help fragment.
    pub help_body: Option<&'static str>,
}

fn advice(
    title: &'static str,
    explanation: &'static str,
    guidance: &'static str,
    places: &[(&str, &str)],
    help_body: Option<&'static str>,
) -> DiagnosticAdvice {
    DiagnosticAdvice {
        title,
        explanation,
        guidance,
        fieldworks_places: places
            .iter()
            .map(|(tool, field)| {
                fieldworks_path(tool).unwrap_or_else(|| {
                    panic!("warning catalog references an unverified FieldWorks tool: {tool}")
                });
                FieldWorksPlace {
                    tool: (*tool).to_owned(),
                    field: (*field).to_owned(),
                }
            })
            .collect(),
        help_body,
    }
}

/// Tool IDs with established FieldWorks navigation paths.
fn fieldworks_path(tool: &str) -> Option<&'static str> {
    match tool {
        "lexiconEdit" => Some(fieldworks_paths::LEXICON_EDIT),
        "posEdit" => Some(fieldworks_paths::GRAMMAR_CATEGORY_EDIT),
        "phonemeEdit" => Some(fieldworks_paths::GRAMMAR_PHONEMES),
        "PhonologicalRuleEdit" => Some(fieldworks_paths::GRAMMAR_PHONOLOGICAL_RULES),
        "naturalClassedit" => Some(fieldworks_paths::GRAMMAR_NATURAL_CLASSES),
        "EnvironmentEdit" => Some(fieldworks_paths::GRAMMAR_ENVIRONMENTS),
        "compoundRuleAdvancedEdit" => Some(fieldworks_paths::GRAMMAR_COMPOUND_RULES),
        "AdhocCoprohibEdit" => Some(fieldworks_paths::GRAMMAR_AD_HOC_RULES),
        "phonologicalFeaturesAdvancedEdit" => Some(fieldworks_paths::GRAMMAR_PHONOLOGICAL_FEATURES),
        "variantEntryTypeEdit" => Some(fieldworks_paths::LISTS_VARIANT_TYPES),
        _ => None,
    }
}

/// The complete advice catalog for registered importer and compiler warning codes.
///
/// The exhaustive match is deliberate: adding an import code requires reviewing its meaning and
/// next step here. Unknown codes have no catalog entry and are handled by the producer's message.
pub fn import_diagnostic_advice(code: &ImportWarningCode) -> Option<DiagnosticAdvice> {
    use ImportWarningCode::*;
    let entry = match code {
        FwdataDanglingReference => advice(
            "Reference to a missing item",
            "An item refers to a target that is absent from the imported project data, so that reference cannot be followed.",
            "Use the warning description to identify the owning item and field. Restore the intended target or repair the reference only when the source data is malformed.",
            &[],
            Some(PARSER_HELP),
        ),
        FwdataUnexpectedClass => advice(
            "Item of an unexpected type",
            "A reference resolves to an item class that the field being imported does not accept.",
            "Use the warning description to locate the owner and field. Repair a malformed reference; report it if the source data is valid.",
            &[],
            Some(PARSER_HELP),
        ),
        FwdataMissingRequiredField => advice(
            "Required source value is missing",
            "A required source value was absent at the owner and field identified in the warning.",
            "Use the warning description to identify the owner and field. Supply a value only when the source is incomplete; otherwise report the valid source case.",
            &[],
            Some(PARSER_HELP),
        ),
        FwdataMissingLangProject => advice(
            "Missing language project",
            "The imported project data does not contain its required language project.",
            "Open the intended project in FieldWorks and save it. If FieldWorks cannot open it, use File > Restore a Project... with a known good backup; if it opens normally, report the reading failure to PanGloss.",
            &[],
            Some(PARSER_HELP),
        ),
        FwdataOnlyFirstUsed => advice(
            "Only the first phoneme set is used",
            "This import path reads the first phoneme set and ignores later sets.",
            "In Grammar > Phonemes, check that the intended inventory appears in the first set. If it does, no change is needed; otherwise report the limitation before reorganizing the project.",
            &[("phonemeEdit", "")],
            Some(MODELLING_HELP),
        ),
        FwdataEmptyRepresentation => advice(
            "Item has no usable representation",
            "A phoneme, boundary marker, or terminal mapping has no usable text representation.",
            "For a phoneme, inspect In Orthography as in Grammar > Phonemes. For an affix process terminal mapping, inspect Allomorphs > Affix Process Rule in Lexicon > Lexicon Edit and correct an unintended phoneme reference or spelling. No editable boundary field has been verified; report that case.",
            &[],
            Some(MODELLING_HELP),
        ),
        FwdataUnrecognizedEnumValue => advice(
            "Unrecognized FieldWorks value",
            "A source value is not recognized by this importer.",
            "Use the warning description to identify the value; select a supported value only when the source is invalid, otherwise report the limitation.",
            &[],
            Some(PARSER_HELP),
        ),
        FwdataMetathesisApproximation => advice(
            "Metathesis rule approximation",
            "The FieldWorks importer converts this rule to an approximation rather than preserving its exact metathesis behavior.",
            "In Grammar > Phonological Rules, review the emitted rule description and compare affected words; report a valid rule whose behavior is not preserved.",
            &[("PhonologicalRuleEdit", "")],
            Some(MODELLING_HELP),
        ),
        FwdataStaleAdhocProhibition => advice(
            "Stale ad hoc prohibition",
            "An ad hoc morpheme rule names an inflectional affix whose slot is outside every enabled template.",
            "In Grammar > Ad hoc Rules, inspect Key Morpheme and Other Morpheme(s); in Grammar > Category Edit, inspect the affix's Affix Templates. Restore the intended assignment if the affix should be usable.",
            &[("AdhocCoprohibEdit", "Key Morpheme"), ("AdhocCoprohibEdit", "Other Morpheme(s)"), ("posEdit", "Affix Templates")],
            Some(PARSER_HELP),
        ),
        FwdataNoUsableAllomorphs => advice(
            "No usable allomorphs",
            "The lexical entry has no allomorph the importer can use.",
            "In Lexicon > Lexicon Edit, inspect Lexeme Form and Allomorphs, including Morph Type. Correct an empty or incorrect form; if the form is accurate, review the specific finding explaining why PanGloss skipped it.",
            &[("lexiconEdit", "Lexeme Form"), ("lexiconEdit", "Allomorphs")],
            Some(STEMS_HELP),
        ),
        FwdataUnsupportedMorphType => advice(
            "Unsupported morph type",
            "The allomorph uses a morph type this importer cannot represent.",
            "In Lexicon > Lexicon Edit, check the warning's named morph type; select a supported type only if it matches the intended analysis.",
            &[("lexiconEdit", "Morph Type")],
            Some(PARSER_HELP),
        ),
        FwdataUnknownMorphTypeGuid => advice(
            "Unknown morph type reference",
            "The allomorph has a morph-type GUID that is not recognized as a known FieldWorks morph type.",
            "In Lexicon > Lexicon Edit, inspect the named allomorph's Morph Type. Use the structured MorphType reference to identify the target; an unresolved reference cannot open a source object. Choose a morph type only if the source classification is wrong; otherwise report the valid type.",
            &[("lexiconEdit", "Morph Type")],
            Some(PARSER_HELP),
        ),
        FwdataReferenceNotInScope => advice(
            "Reference outside item scope",
            "An affix process output mapping refers to an input part outside the process's own input list.",
            "In Lexicon > Lexicon Edit, inspect Allomorphs > Affix Process Rule. Correct a mapping to use an intended input part in that rule; if FieldWorks shows a valid rule, report the mapping and reference details.",
            &[("lexiconEdit", "Allomorphs > Affix Process Rule")],
            Some(PARSER_HELP),
        ),
        FwdataInvalidParserParameter => advice(
            "Invalid parser setting",
            "A parser parameter has a value this importer cannot use.",
            "In Words > Edit Parser Parameters..., correct the named setting when it is malformed.",
            &[],
            Some(PARSER_HELP),
        ),
        InvalidSourceActiveParser => advice(
            "Unsupported active parser",
            "PanGloss could not determine a supported active parser from the saved project.",
            "Check the project's parser selection in FieldWorks. If FieldWorks shows a supported parser, report the reading failure to PanGloss.",
            &[],
            Some(PARSER_HELP),
        ),
        InvalidSourceDuplicateGuid => advice(
            "Duplicate FieldWorks GUID",
            "Two source items share an identity that must be unique.",
            "Report the named records to FieldWorks support and PanGloss. Internal identities are not editable grammar fields. Restore a known good backup if FieldWorks cannot open the project.",
            &[],
            Some(PARSER_HELP),
        ),
        InvalidSourceMissingGuid => advice(
            "Missing FieldWorks GUID",
            "A source item has no stable identity for import.",
            "Report the record details to FieldWorks support and PanGloss. No verified editable identity field repairs this finding; restore a known good backup if the project cannot open.",
            &[],
            Some(PARSER_HELP),
        ),
        FwdataWritingSystemStoreUnreadable => advice(
            "Writing system data unavailable",
            "The project writing-system data could not be read.",
            "In Tools > Configure > Set up Vernacular Writing Systems... and Set up Analysis Writing Systems..., check the project data and retry the import.",
            &[],
            Some(PARSER_HELP),
        ),
        SnapshotDanglingReference => advice(
            "Missing snapshot reference",
            "Snapshot validation found a reference whose target is absent.",
            "Restore the target or remove the stale reference in the source project.",
            &[],
            Some(PARSER_HELP),
        ),
        SnapshotFeatureStructureUnresolved => advice(
            "Unresolved feature structure",
            "A snapshot reference to a feature or value cannot be resolved in the source data.",
            "Use the warning description to identify the owning object and feature field. Repair a stale reference only when it is malformed; otherwise report the source case.",
            &[],
            Some(PARSER_HELP),
        ),
        SnapshotRuleFeatureUnresolved => advice(
            "Unresolved rule or exception feature",
            "A rule or exception feature reference resolves to neither an inflection class nor an exception feature.",
            "In Grammar > Phonological Rules, inspect the named rule's required or excluded rule features. The unresolved target may be an inflection class or an exception feature.",
            &[("PhonologicalRuleEdit", "")],
            Some(PARSER_HELP),
        ),
        SnapshotReferenceOutOfScope => advice(
            "Snapshot reference out of scope",
            "A sense references a grammatical analysis outside its owning entry's analysis list.",
            "In Lexicon > Lexicon Edit, inspect the sense's Grammatical Info. and select the intended analysis belonging to that entry. If FieldWorks already displays a valid analysis, report the conversion problem.",
            &[("lexiconEdit", "Grammatical Info.")],
            Some(PARSER_HELP),
        ),
        PhonemeNoRepresentation => advice(
            "Phoneme has no representation",
            "The phoneme has no grapheme representation available to the parser.",
            "In Grammar > Phonemes, add a representation if this phoneme belongs in the parser inventory.",
            &[("phonemeEdit", "In Orthography as")],
            Some(MODELLING_HELP),
        ),
        PhonemeNfdCollision => advice(
            "Phoneme representation collision",
            "A phoneme spelling collides with an earlier phoneme or boundary after Unicode normalization.",
            "In Grammar > Phonemes, compare the named phoneme's In Orthography as values. Correct an unintended duplicate; retain a linguistically intentional distinction and report the limitation.",
            &[("phonemeEdit", "In Orthography as")],
            Some(MODELLING_HELP),
        ),
        PhonemeFeatureUnresolved => advice(
            "Unresolved phoneme feature value",
            "A phoneme feature structure contains an unresolved feature or value.",
            "In Grammar > Phonemes, repair the named phoneme's feature values if the source reference is stale.",
            &[("phonemeEdit", "Phonological Features"), ("phonologicalFeaturesAdvancedEdit", "")],
            Some(PARSER_HELP),
        ),
        PhonemeComplexFeatureUnsupported => advice(
            "Unsupported phoneme feature value",
            "The phoneme has a complex feature value this parser cannot represent.",
            "In Grammar > Phonemes, keep the intended analysis and report it if the value is valid; use a supported value only when linguistically equivalent.",
            &[("phonemeEdit", "Phonological Features"), ("phonologicalFeaturesAdvancedEdit", "")],
            Some(PARSER_HELP),
        ),
        BoundaryNfdCollision => advice(
            "Boundary representation collision",
            "A boundary marker representation normalizes to an earlier phoneme or boundary representation, so the later item is not distinct to the parser.",
            "Compare any named phoneme's In Orthography as values in Grammar > Phonemes. No editable FieldWorks boundary field has been verified; report an intentional collision to PanGloss or FieldWorks support.",
            &[],
            Some(MODELLING_HELP),
        ),
        BoundaryNoRepresentation => advice(
            "Boundary marker has no representation",
            "The boundary marker has no usable representation.",
            "Use the description to identify the marker. No editable FieldWorks boundary field has been verified; report the missing representation to PanGloss or FieldWorks support.",
            &[],
            Some(MODELLING_HELP),
        ),
        BoundaryMorphMarkerUnresolved => advice(
            "Unresolved morpheme boundary",
            "The compiler cannot resolve its fixed morpheme-boundary representation (+) and uses a null boundary instead.",
            "Report the project and boundary details to PanGloss. No configurable FieldWorks parser setting or verified editable field selects this compiler boundary.",
            &[],
            Some(PARSER_HELP),
        ),
        NatclassSegmentsMemberUnresolved => advice(
            "Unresolved natural class member",
            "An explicit segment-list natural class contains a phoneme that cannot be resolved, so the whole class is skipped.",
            "In Grammar > Natural Classes, check the phoneme inventory and the named class's member list; remove a stale member or restore its phoneme.",
            &[("naturalClassedit", "Phonemes")],
            Some(MODELLING_HELP),
        ),
        NatclassFeatureConstraintUnresolved => advice(
            "Unresolved natural class feature",
            "A feature-defined natural class has an unresolved feature constraint; other constraints can still load.",
            "In Grammar > Natural Classes, check the named class and feature system, and repair only a stale reference.",
            &[("naturalClassedit", "Phonological Features"), ("phonologicalFeaturesAdvancedEdit", "")],
            Some(MODELLING_HELP),
        ),
        NatclassComplexFeatureUnsupported => advice(
            "Unsupported natural class feature",
            "A natural-class constraint uses a complex feature value the parser cannot represent; remaining constraints may still load.",
            "In Grammar > Natural Classes, keep valid feature data and report the unsupported value, or use an equivalent supported constraint if one exists.",
            &[("naturalClassedit", "Phonological Features")],
            Some(MODELLING_HELP),
        ),
        PhonComplexFeatureUnsupported => advice(
            "Unsupported complex feature",
            "A phonological feature definition uses a complex value the parser cannot represent.",
            "In Grammar > Phonological Features, preserve the intended feature system and report the unsupported value unless an equivalent closed feature is appropriate.",
            &[("phonologicalFeaturesAdvancedEdit", "")],
            Some(PARSER_HELP),
        ),
        StemNameBuildFailed => advice(
            "Stem allomorph label could not be loaded",
            "The stem allomorph label could not be represented in the compiled grammar.",
            "In Grammar > Category Edit, inspect Stem Allomorph Labels and the label's Feature Sets. Correct an invalid feature reference; if FieldWorks shows a valid definition, report the loading failure.",
            &[("posEdit", "Stem Allomorph Labels > Feature Sets")],
            Some(STEMS_HELP),
        ),
        StemNameEmptyRegions => advice(
            "Stem allomorph label has no feature sets",
            "The stem allomorph label has no non-empty inflection feature set.",
            "In Grammar > Category Edit, inspect Stem Allomorph Labels > Feature Sets and supply an intended Feature Set if the label should constrain an allomorph. Otherwise remove an unintended label assignment in Lexicon > Lexicon Edit.",
            &[("posEdit", "Stem Allomorph Labels > Feature Sets")],
            Some(STEMS_HELP),
        ),
        CompoundRuleBuildFailed => advice(
            "Compound rule could not be loaded",
            "The compound rule could not be represented in the compiled grammar.",
            "In Grammar > Compound Rules, check the rule's constituent categories; report it if those categories are valid.",
            &[("compoundRuleAdvancedEdit", "")],
            Some(PARSER_HELP),
        ),
        CompoundSidePosUnresolved => advice(
            "Unresolved compound category",
            "A compound rule input or output refers to a grammatical category that cannot be resolved.",
            "In Grammar > Compound Rules, inspect Category on the named input or output and select the intended existing category.",
            &[("compoundRuleAdvancedEdit", "Category")],
            Some(PARSER_HELP),
        ),
        CompoundSideExceptionFeatureUnresolved => advice(
            "Unresolved compound exception feature",
            "A compound rule exception refers to a feature that cannot be resolved.",
            "In Grammar > Compound Rules, repair the named exception feature reference if it is stale.",
            &[("compoundRuleAdvancedEdit", "Exception \"Features\"")],
            Some(PARSER_HELP),
        ),
        MsaBuildFailed => advice(
            "Grammatical analysis could not be loaded",
            "The grammatical analysis could not be represented in the compiled grammar.",
            "In Lexicon > Lexicon Edit, check the named analysis's part of speech and features; report valid data the parser cannot use.",
            &[("lexiconEdit", "Grammatical Info.")],
            Some(STEMS_HELP),
        ),
        MsaNoAllomorphs => advice(
            "No usable entry allomorphs",
            "The lexical entry has no allomorph available to this analysis.",
            "In Lexicon > Lexicon Edit, inspect Lexeme Form, Allomorphs and Morph Type. Resolve the specific allomorph loading findings before adding a new form.",
            &[("lexiconEdit", "Lexeme Form"), ("lexiconEdit", "Allomorphs")],
            Some(STEMS_HELP),
        ),
        MsaNoRuleFormAllomorphs => advice(
            "Analysis has no usable affix form",
            "This analysis has no usable affix-form allomorph.",
            "In Lexicon > Lexicon Edit, inspect Allomorphs, their forms, Morph Type and Environments. Review the specific finding for each rejected form.",
            &[("lexiconEdit", "Allomorphs")],
            Some(STEMS_HELP),
        ),
        MsaExceptionFeatureUnresolved => advice(
            "Unresolved analysis exception feature",
            "The analysis refers to an exception feature that cannot be resolved.",
            "In Lexicon > Lexicon Edit, repair the named feature reference if it is stale.",
            &[("lexiconEdit", "Exception \"Features\""), ("lexiconEdit", "From Exception \"Features\""), ("lexiconEdit", "To Exception \"Features\"")],
            Some(STEMS_HELP),
        ),
        MsaInflectionClassUnresolved => advice(
            "Unresolved analysis inflection class",
            "The analysis's inflection-class reference cannot be resolved.",
            "In Lexicon > Lexicon Edit, assign an existing inflection class if the reference is stale.",
            &[("lexiconEdit", "Inflection Class"), ("lexiconEdit", "From Inflection Class"), ("lexiconEdit", "To Inflection Class"), ("posEdit", "Inflection Class Info.")],
            Some(STEMS_HELP),
        ),
        MsaStemNameUnresolved => advice(
            "Unresolved stem allomorph label",
            "The analysis's required stem allomorph label cannot be resolved.",
            "In Lexicon > Lexicon Edit, inspect From Stem Allomorph Label. In Grammar > Category Edit, check Stem Allomorph Labels and select the intended existing label.",
            &[("lexiconEdit", "From Stem Allomorph Label"), ("posEdit", "Stem Allomorph Labels")],
            Some(STEMS_HELP),
        ),
        MsaLexEntryInflTypeUnresolved => advice(
            "Unresolved entry inflection type",
            "An analysis refers to an entry inflection type that cannot be resolved.",
            "In Lexicon > Lexicon Edit and Lists > Variant Types, check the analysis reference and restore the type or repair a stale reference.",
            &[("lexiconEdit", "Variant Type"), ("variantEntryTypeEdit", "")],
            Some(STEMS_HELP),
        ),
        VariantComponentUnresolved => advice(
            "Unresolved variant component",
            "A variant refers to an entry or sense that cannot be resolved.",
            "In Lexicon > Lexicon Edit, inspect Variant of and select the intended entry or sense. If FieldWorks displays it correctly, report the reference.",
            &[("lexiconEdit", "Variant of")],
            Some(STEMS_HELP),
        ),
        AllomorphUnsegmentable => advice(
            "Allomorph could not be segmented",
            "The allomorph spelling cannot be divided into the parser's phoneme and boundary inventory.",
            "In Lexicon > Lexicon Edit, inspect the named form and its environments against the phoneme inventory. Use the diagnostic cause to identify the failing half, environment or character when supplied; correct an unintended spelling or report an accurate unsupported form.",
            &[("lexiconEdit", "Allomorphs > Form")],
            Some(ALLOMORPHS_HELP),
        ),
        AllomorphMorphTypeUnsupported => advice(
            "Unsupported allomorph morph type",
            "The allomorph's morph type is not supported for this compiler path.",
            "In Lexicon > Lexicon Edit, check the named morph type and report the case if the source analysis is valid.",
            &[("lexiconEdit", "Morph Type")],
            Some(PARSER_HELP),
        ),
        AllomorphMorphTypeUnsupportedAsRuleForm => advice(
            "Loaded through separate parts",
            "A circumfix or discontiguous phrase has a whole form that is not loaded as a rule form; its separate parts are loaded through their own path.",
            "In Lexicon > Lexicon Edit, no change is needed when the separate parts are the intended parser representation.",
            &[("lexiconEdit", "Morph Type")],
            Some(ALLOMORPHS_HELP),
        ),
        AllomorphNotRuleForm => advice(
            "Allomorph cannot be used as an affix",
            "An affix form is empty, or an infix has no position environment, so this allomorph cannot be loaded through that affix-rule path.",
            "In Lexicon > Lexicon Edit, inspect Allomorphs > Form or Infix Positions as identified by the warning description. Supply missing source data only if the allomorph should be an affix.",
            &[("lexiconEdit", "Allomorphs > Form"), ("lexiconEdit", "Allomorphs > Infix Positions")],
            Some(ALLOMORPHS_HELP),
        ),
        AllomorphReduplicationUnsupported => advice(
            "Unsupported reduplication form",
            "The allomorph uses a reduplication pattern that this parser does not run.",
            "In Lexicon > Lexicon Edit, keep the intended pattern and report it; do not replace it unless an equivalent supported analysis is known.",
            &[("lexiconEdit", "Allomorphs > Form")],
            Some(MODELLING_HELP),
        ),
        AllomorphProcessBuildFailed => advice(
            "Affix process could not be loaded",
            "The affix process mappings could not be represented in the compiled grammar.",
            "In Lexicon > Lexicon Edit, check the named input and output mappings; report valid mappings the parser cannot use.",
            &[("lexiconEdit", "Allomorphs > Affix Process Rule")],
            Some(ALLOMORPHS_HELP),
        ),
        AllomorphInflectionClassUnresolved => advice(
            "Unresolved allomorph inflection class",
            "The allomorph refers to an inflection class that cannot be resolved.",
            "In Lexicon > Lexicon Edit, repair the reference if the named class is missing.",
            &[("lexiconEdit", "Allomorphs > Inflection Classes"), ("posEdit", "Inflection Class Info.")],
            Some(ALLOMORPHS_HELP),
        ),
        AllomorphFeatureBuildFailed => advice(
            "Allomorph features could not be loaded",
            "The allomorph's morphosyntactic features could not be represented.",
            "In Lexicon > Lexicon Edit, inspect Allomorphs > Required Features and reselect valid intended values. If FieldWorks already shows valid feature data, report the loading failure.",
            &[("lexiconEdit", "Allomorphs > Required Features")],
            Some(ALLOMORPHS_HELP),
        ),
        AllomorphEnvironmentBuildFailed => advice(
            "Allomorph environment could not be loaded",
            "The allomorph environment could not be converted for parsing.",
            "In Lexicon > Lexicon Edit, use the warning description to distinguish invalid syntax from an unresolved named environment before editing.",
            &[("lexiconEdit", "Allomorphs > Environments"), ("EnvironmentEdit", "String Representation")],
            Some(ALLOMORPHS_HELP),
        ),
        CircumfixMissingHalf => advice(
            "Circumfix is missing a half",
            "The circumfix entry does not have both required prefix and suffix parts.",
            "In Lexicon > Lexicon Edit, add the missing half if the entry is intended to be a circumfix.",
            &[("lexiconEdit", "Allomorphs"), ("lexiconEdit", "Morph Type")],
            Some(ALLOMORPHS_HELP),
        ),
        CircumfixEnvironmentCombinationSkipped => advice(
            "Circumfix environment combination skipped",
            "An environment prevented this circumfix-half combination from being compiled.",
            "In Lexicon > Lexicon Edit, inspect Environments on both parts under Allomorphs. In Grammar > Environments, inspect their String Representation. If both express the intended restriction, report this unsupported combination.",
            &[("lexiconEdit", "Allomorphs > Environments"), ("EnvironmentEdit", "String Representation")],
            Some(ALLOMORPHS_HELP),
        ),
        EnvironmentUnresolved => advice(
            "Unresolved phonological environment",
            "A referenced named environment is missing, so this finding is an import error.",
            "In Lexicon > Lexicon Edit, inspect Allomorphs > Environments; in Grammar > Environments, inspect String Representation. Restore the intended environment or reselect it only if the reference is stale.",
            &[("lexiconEdit", "Allomorphs > Environments"), ("EnvironmentEdit", "String Representation")],
            Some(ALLOMORPHS_HELP),
        ),
        CompileFailed => advice(
            "Grammar compilation failed",
            "The grammar input could not be compiled, so parsing is unavailable.",
            "Use the error description to locate and correct malformed grammar input. If FieldWorks accepts the input, report the unsupported construct with this diagnostic.",
            &[],
            Some(PARSER_HELP),
        ),
        EnvironmentInvalid => advice(
            "Invalid phonological environment",
            "The environment expression could not be parsed and is ignored as a restriction.",
            "In Grammar > Environments, correct the expression for phonological environment '{subject}'. In Lexicon > Lexicon Edit, inspect Allomorphs > Environments. Roots ignore invalid restrictions; ordinary affixes also get an unrestricted pass. An infix still needs a valid position.",
            &[("EnvironmentEdit", "String Representation"), ("lexiconEdit", "Allomorphs > Environments")],
            Some(ALLOMORPHS_HELP),
        ),
        TemplateSlotUnresolved => advice(
            "Affix template slot is unresolved",
            "An affix template refers to a slot that cannot be resolved.",
            "In Grammar > Category Edit, repair the template's slot reference if the slot is missing.",
            &[("posEdit", "Affix Templates"), ("posEdit", "Affix Slots")],
            Some(PARSER_HELP),
        ),
        TemplateSlotNoRules => advice(
            "Affix template slot has no rules",
            "No usable inflectional affix was loaded for this template slot.",
            "In Grammar > Category Edit, inspect Affix Templates and assigned affixes. Resolve their individual loading findings when affixes are already assigned; assign an appropriate affix only if the slot is unintentionally empty.",
            &[("posEdit", "Affix Templates"), ("posEdit", "Affix Slots")],
            Some(PARSER_HELP),
        ),
        TemplateNoSlots => advice(
            "Empty affix template",
            "The affix template has no slots containing a loaded affix rule; slots can exist in FieldWorks and still be unavailable here.",
            "In Grammar > Category Edit, inspect Affix Templates and slot assignments. If slots are already present, review their affixes' loading findings. Add a missing slot or affix only if the template is unintentionally empty.",
            &[("posEdit", "Affix Templates"), ("posEdit", "Affix Slots")],
            Some(PARSER_HELP),
        ),
        TemplateBuildFailed => advice(
            "Affix template could not be loaded",
            "The template could not be represented from its slots and inflectional affixes.",
            "In Grammar > Category Edit, check the template and its referenced slots and affixes; report a valid unsupported arrangement.",
            &[("posEdit", "Affix Templates"), ("posEdit", "Affix Slots")],
            Some(PARSER_HELP),
        ),
        NullAffixMprUnresolved => advice(
            "Unresolved null-affix restriction",
            "An internal restriction for an irregular inflection type could not be resolved.",
            "In Lists > Variant Types, inspect the named type. No editable restriction field has been verified for this registry failure; report the type and failure details to PanGloss.",
            &[("variantEntryTypeEdit", "")],
            Some(PARSER_HELP),
        ),
        NullAffixSynFsFailed => advice(
            "Null-affix features could not be loaded",
            "Features for an internally generated null affix could not be represented.",
            "In Lists > Variant Types, inspect Inflection Features and choose valid intended values. If the values are valid, report them and the detected cause to PanGloss.",
            &[("variantEntryTypeEdit", "Inflection Features")],
            Some(PARSER_HELP),
        ),
        NullAffixSegmentFailed => advice(
            "Null-affix form could not be segmented",
            "PanGloss could not build its generated empty-affix marker for this irregular inflection type.",
            "Report the type and marker details to PanGloss. The failing marker is generated internally; no verified FieldWorks form correction is known.",
            &[],
            Some(PARSER_HELP),
        ),
        RuleMetathesisUnsupported => advice(
            "Unsupported metathesis rule",
            "The compiler skips this metathesis rule; it does not approximate or run it.",
            "In Grammar > Phonological Rules, keep the valid rule and report it if words rely on its behavior; inspect affected words with a parse trace.",
            &[("PhonologicalRuleEdit", "")],
            Some(MODELLING_HELP),
        ),
        RuleBuildFailed => advice(
            "Phonological rule could not be loaded",
            "The phonological rule could not be represented in the compiled grammar.",
            "In Grammar > Phonological Rules, check the structural description and change; report the rule if its source data is valid.",
            &[("PhonologicalRuleEdit", "")],
            Some(PARSER_HELP),
        ),
        FeatureConstraintUnresolved => advice(
            "Unresolved rule feature constraint",
            "The rule refers to a feature-constraint object that cannot be resolved.",
            "In Grammar > Phonological Rules, inspect the named rule part. Correct a stale constraint reference; if FieldWorks displays a valid constraint, report the rule and missing-object details.",
            &[("PhonologicalRuleEdit", "")],
            Some(PARSER_HELP),
        ),
        FeatureConstraintPhonFeatureUnresolved => advice(
            "Unresolved phonological feature constraint",
            "A rule constraint refers to a missing phonological feature.",
            "In Grammar > Phonological Rules, inspect the named constraint; in Grammar > Phonological Features, check the intended feature and reselect it in the rule.",
            &[("PhonologicalRuleEdit", ""), ("phonologicalFeaturesAdvancedEdit", "")],
            Some(PARSER_HELP),
        ),
        RuleFeatureUnresolved => advice(
            "Unresolved phonological rule feature",
            "A phonological rule feature reference cannot be resolved.",
            "In Grammar > Phonological Rules, repair the named rule's feature reference if it is stale.",
            &[("PhonologicalRuleEdit", "")],
            Some(PARSER_HELP),
        ),
        StrataCustomUnsupported => advice(
            "Unsupported custom strata",
            "Custom strata are replaced by the parser's default strata.",
            "No grammar edit is required for valid custom strata; report the configuration if it is needed for the analysis.",
            &[],
            Some(MODELLING_HELP),
        ),
        AdhocProhibitionUnresolved => advice(
            "Unresolved ad hoc prohibition",
            "An ad hoc prohibition refers to forms the compiler cannot resolve.",
            "In Grammar > Ad hoc Rules, check the prohibition's form references; report it if the source forms are valid.",
            &[("AdhocCoprohibEdit", "")],
            Some(PARSER_HELP),
        ),
        MruleUnreachableCompacted => advice(
            "Unreachable affix omitted",
            "An affix rule unreachable from the compiled grammar was compacted.",
            "No change is needed if the affix is intentionally unused. Otherwise inspect its category's Affix Templates in Grammar > Category Edit and its Grammatical Info. in Lexicon > Lexicon Edit.",
            &[("posEdit", "Affix Templates"), ("lexiconEdit", "Grammatical Info.")],
            Some(WORD_FAILURE_HELP),
        ),
        CooccurrenceTargetUnreachable => advice(
            "Unused ad hoc rule omitted",
            "An ad hoc rule whose target cannot be reached in the compiled grammar was compacted.",
            "No change is needed if the rule is intentionally unused. Otherwise check its target affix's Grammatical Info. in Lexicon > Lexicon Edit and Affix Templates in Grammar > Category Edit, then review Grammar > Ad hoc Rules.",
            &[("lexiconEdit", "Grammatical Info."), ("posEdit", "Affix Templates"), ("AdhocCoprohibEdit", "")],
            Some(WORD_FAILURE_HELP),
        ),
        NaturalClassUnreferencedCompacted => advice(
            "Unused natural class omitted",
            "An eligible natural class not referenced by the compiled grammar was compacted.",
            "No change is needed if the class is intentionally unused. If it should be used, inspect the relevant rule in Grammar > Phonological Rules or String Representation in Grammar > Environments.",
            &[("PhonologicalRuleEdit", ""), ("EnvironmentEdit", "String Representation")],
            Some(MODELLING_HELP),
        ),
        SourceProvenanceUnknown => advice(
            "Conversion provenance is unavailable",
            "Conversion provenance is missing or invalid, so PanGloss cannot certify the completeness of the conversion.",
            "Report the project and diagnostic details to PanGloss. No FieldWorks identity field can repair this finding; preserve source provenance when regenerating an intermediate grammar.",
            &[],
            Some(PARSER_HELP),
        ),
        SubstrateUnsegmentableForm => advice(
            "Allomorph form cannot be segmented",
            "The substrate allomorph cannot be divided using the imported phoneme and boundary inventory.",
            "In Lexicon > Lexicon Edit, check its spelling against the inventory and use the warning's position detail to locate the unresolved material.",
            &[("lexiconEdit", "Allomorphs > Form")],
            Some(ALLOMORPHS_HELP),
        ),
        SubstrateClassificationAmbiguous => advice(
            "Allomorph character is ambiguous",
            "PanGloss lacks evidence to classify an allomorph character and skips the form instead of guessing.",
            "In Lexicon > Lexicon Edit, inspect the form. Correct an unintended character; if it is a genuine phoneme, define it in Grammar > Phonemes > In Orthography as. If spelling and inventory are correct, inspect the vernacular writing system and report missing classification evidence.",
            &[("lexiconEdit", "Allomorphs > Form"), ("phonemeEdit", "In Orthography as")],
            Some(MODELLING_HELP),
        ),
        MigrationInferredSegmentWithFeatureRule => advice(
            "Unlisted character matches a feature class",
            "A character inferred from an allomorph has no authored phonological features yet matches a feature-defined natural class.",
            "If the character is a language phoneme, define its In Orthography as and Phonological Features in Grammar > Phonemes. Otherwise correct the unintended allomorph spelling in Lexicon > Lexicon Edit.",
            &[("phonemeEdit", "In Orthography as"), ("phonemeEdit", "Phonological Features"), ("lexiconEdit", "Allomorphs > Form")],
            Some(MODELLING_HELP),
        ),
        UnsupportedConstruct => advice(
            "Reduplication inventory check is unsupported",
            "PanGloss cannot check this reduplication pattern against the phoneme inventory.",
            "In Lexicon > Lexicon Edit, inspect the allomorph form. Keep an accurate pattern and report it; change it only if an equivalent supported representation is known.",
            &[("lexiconEdit", "Allomorphs > Form")],
            Some(PARSER_HELP),
        ),
        SubstratePositionUnmapped => advice(
            "Allomorph character position is unmapped",
            "PanGloss's character-position mapping failed for this allomorph.",
            "In Lexicon > Lexicon Edit, inspect the named position in the form. Correct an unintended character; if FieldWorks displays the intended spelling, report the mapping problem and diagnostic details.",
            &[("lexiconEdit", "Allomorphs > Form")],
            Some(ALLOMORPHS_HELP),
        ),
        Unregistered(_) => return None,
    };
    Some(entry)
}

pub struct ImportWarningMetadata {
    pub group_name: &'static str,
    pub level: DiagnosticLevel,
    /// `None` only for unknown codes; registered informational findings have explicit no-fix advice.
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

/// Classification is independent of the human wording.
fn import_warning_level(code: &ImportWarningCode) -> DiagnosticLevel {
    use ImportWarningCode::*;
    match code {
        ImportWarningCode::FwdataDanglingReference => DiagnosticLevel::Warning,
        ImportWarningCode::FwdataUnexpectedClass => DiagnosticLevel::Warning,
        ImportWarningCode::FwdataMissingRequiredField => DiagnosticLevel::Warning,
        ImportWarningCode::FwdataMissingLangProject => DiagnosticLevel::Warning,
        ImportWarningCode::FwdataOnlyFirstUsed => DiagnosticLevel::Info,
        ImportWarningCode::FwdataEmptyRepresentation => DiagnosticLevel::Warning,
        ImportWarningCode::FwdataUnrecognizedEnumValue => DiagnosticLevel::Warning,
        ImportWarningCode::FwdataMetathesisApproximation => DiagnosticLevel::Warning,
        ImportWarningCode::FwdataStaleAdhocProhibition => DiagnosticLevel::Warning,
        ImportWarningCode::FwdataNoUsableAllomorphs => DiagnosticLevel::Error,
        ImportWarningCode::FwdataUnsupportedMorphType => DiagnosticLevel::Warning,
        ImportWarningCode::FwdataUnknownMorphTypeGuid => DiagnosticLevel::Warning,
        ImportWarningCode::FwdataReferenceNotInScope => DiagnosticLevel::Warning,
        ImportWarningCode::FwdataInvalidParserParameter => DiagnosticLevel::Warning,
        ImportWarningCode::InvalidSourceActiveParser => DiagnosticLevel::Warning,
        ImportWarningCode::InvalidSourceDuplicateGuid => DiagnosticLevel::Warning,
        ImportWarningCode::InvalidSourceMissingGuid => DiagnosticLevel::Warning,
        ImportWarningCode::FwdataWritingSystemStoreUnreadable => DiagnosticLevel::Warning,
        ImportWarningCode::SnapshotDanglingReference => DiagnosticLevel::Warning,
        ImportWarningCode::SnapshotFeatureStructureUnresolved => DiagnosticLevel::Warning,
        ImportWarningCode::SnapshotRuleFeatureUnresolved => DiagnosticLevel::Warning,
        ImportWarningCode::SnapshotReferenceOutOfScope => DiagnosticLevel::Warning,
        ImportWarningCode::PhonemeNoRepresentation => DiagnosticLevel::Warning,
        ImportWarningCode::PhonemeNfdCollision => DiagnosticLevel::Error,
        ImportWarningCode::PhonemeFeatureUnresolved => DiagnosticLevel::Error,
        ImportWarningCode::PhonemeComplexFeatureUnsupported => DiagnosticLevel::Error,
        ImportWarningCode::BoundaryNfdCollision => DiagnosticLevel::Error,
        ImportWarningCode::BoundaryNoRepresentation => DiagnosticLevel::Warning,
        ImportWarningCode::BoundaryMorphMarkerUnresolved => DiagnosticLevel::Warning,
        ImportWarningCode::NatclassSegmentsMemberUnresolved => DiagnosticLevel::Warning,
        ImportWarningCode::NatclassFeatureConstraintUnresolved => DiagnosticLevel::Error,
        ImportWarningCode::NatclassComplexFeatureUnsupported => DiagnosticLevel::Error,
        ImportWarningCode::PhonComplexFeatureUnsupported => DiagnosticLevel::Warning,
        ImportWarningCode::StemNameBuildFailed => DiagnosticLevel::Warning,
        ImportWarningCode::StemNameEmptyRegions => DiagnosticLevel::Warning,
        ImportWarningCode::CompoundRuleBuildFailed => DiagnosticLevel::Warning,
        ImportWarningCode::CompoundSidePosUnresolved => DiagnosticLevel::Error,
        ImportWarningCode::CompoundSideExceptionFeatureUnresolved => DiagnosticLevel::Error,
        ImportWarningCode::MsaBuildFailed => DiagnosticLevel::Warning,
        ImportWarningCode::MsaNoAllomorphs => DiagnosticLevel::Error,
        ImportWarningCode::MsaNoRuleFormAllomorphs => DiagnosticLevel::Error,
        ImportWarningCode::MsaExceptionFeatureUnresolved => DiagnosticLevel::Error,
        ImportWarningCode::MsaInflectionClassUnresolved => DiagnosticLevel::Error,
        ImportWarningCode::MsaStemNameUnresolved => DiagnosticLevel::Error,
        ImportWarningCode::MsaLexEntryInflTypeUnresolved => DiagnosticLevel::Error,
        ImportWarningCode::VariantComponentUnresolved => DiagnosticLevel::Warning,
        ImportWarningCode::AllomorphUnsegmentable => DiagnosticLevel::Warning,
        ImportWarningCode::AllomorphMorphTypeUnsupported => DiagnosticLevel::Warning,
        ImportWarningCode::AllomorphMorphTypeUnsupportedAsRuleForm => DiagnosticLevel::Info,
        ImportWarningCode::AllomorphNotRuleForm => DiagnosticLevel::Warning,
        ImportWarningCode::AllomorphReduplicationUnsupported => DiagnosticLevel::Warning,
        ImportWarningCode::AllomorphProcessBuildFailed => DiagnosticLevel::Warning,
        ImportWarningCode::AllomorphInflectionClassUnresolved => DiagnosticLevel::Error,
        ImportWarningCode::AllomorphFeatureBuildFailed => DiagnosticLevel::Warning,
        ImportWarningCode::AllomorphEnvironmentBuildFailed => DiagnosticLevel::Warning,
        ImportWarningCode::CircumfixMissingHalf => DiagnosticLevel::Error,
        ImportWarningCode::CircumfixEnvironmentCombinationSkipped => DiagnosticLevel::Warning,
        ImportWarningCode::EnvironmentUnresolved => DiagnosticLevel::Error,
        ImportWarningCode::EnvironmentInvalid => DiagnosticLevel::Warning,
        ImportWarningCode::CompileFailed => DiagnosticLevel::Error,
        ImportWarningCode::TemplateSlotUnresolved => DiagnosticLevel::Warning,
        ImportWarningCode::TemplateSlotNoRules => DiagnosticLevel::Warning,
        ImportWarningCode::TemplateNoSlots => DiagnosticLevel::Warning,
        ImportWarningCode::TemplateBuildFailed => DiagnosticLevel::Warning,
        ImportWarningCode::NullAffixMprUnresolved => DiagnosticLevel::Warning,
        ImportWarningCode::NullAffixSynFsFailed => DiagnosticLevel::Warning,
        ImportWarningCode::NullAffixSegmentFailed => DiagnosticLevel::Warning,
        ImportWarningCode::RuleMetathesisUnsupported => DiagnosticLevel::Warning,
        ImportWarningCode::RuleBuildFailed => DiagnosticLevel::Warning,
        ImportWarningCode::FeatureConstraintUnresolved => DiagnosticLevel::Error,
        ImportWarningCode::FeatureConstraintPhonFeatureUnresolved => DiagnosticLevel::Error,
        ImportWarningCode::RuleFeatureUnresolved => DiagnosticLevel::Error,
        ImportWarningCode::StrataCustomUnsupported => DiagnosticLevel::Warning,
        ImportWarningCode::AdhocProhibitionUnresolved => DiagnosticLevel::Warning,
        ImportWarningCode::MruleUnreachableCompacted => DiagnosticLevel::Info,
        ImportWarningCode::CooccurrenceTargetUnreachable => DiagnosticLevel::Info,
        ImportWarningCode::NaturalClassUnreferencedCompacted => DiagnosticLevel::Info,
        ImportWarningCode::SourceProvenanceUnknown => DiagnosticLevel::Warning,
        ImportWarningCode::SubstrateUnsegmentableForm => DiagnosticLevel::Warning,
        ImportWarningCode::SubstrateClassificationAmbiguous => DiagnosticLevel::Warning,
        ImportWarningCode::MigrationInferredSegmentWithFeatureRule => DiagnosticLevel::Error,
        ImportWarningCode::UnsupportedConstruct => DiagnosticLevel::Warning,
        ImportWarningCode::SubstratePositionUnmapped => DiagnosticLevel::Warning,
        Unregistered(_) => DiagnosticLevel::Warning,
    }
}

pub fn import_warning_metadata(code: ImportWarningCode) -> ImportWarningMetadata {
    let level = import_warning_level(&code);
    match import_diagnostic_advice(&code) {
        Some(advice) => ImportWarningMetadata {
            group_name: advice.title,
            level,
            guidance: Some(advice.guidance.to_owned()),
        },
        None => ImportWarningMetadata {
            group_name: "Unregistered warning code",
            level,
            guidance: None,
        },
    }
}
