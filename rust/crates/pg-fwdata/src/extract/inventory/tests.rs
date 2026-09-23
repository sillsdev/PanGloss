use super::*;
use crate::xml::ALLOWED_CLASSES;

#[test]
fn every_allowed_class_is_classified() {
    for class in ALLOWED_CLASSES {
        assert!(class_role(class).is_some(), "unclassified class {class}");
    }
}

#[test]
fn an_unrecognized_class_is_not_classified() {
    assert!(class_role("ZzUnknown").is_none());
}

#[test]
fn tracked_mapping_table() {
    use InventoryKind::*;
    let cases: &[(&str, InventoryKind)] = &[
        ("LexEntry", Entry),
        ("LexSense", Sense),
        ("LexEntryRef", EntryReference),
        ("MoStemMsa", Msa),
        ("MoInflAffMsa", Msa),
        ("MoDerivAffMsa", Msa),
        ("MoUnclassifiedAffixMsa", Msa),
        ("MoStemAllomorph", Allomorph),
        ("MoAffixAllomorph", Allomorph),
        ("MoAffixProcess", AffixProcess),
        ("PhEnvironment", Environment),
        ("PhPhoneme", Phoneme),
        ("PhBdryMarker", BoundaryMarker),
        ("PhNCSegments", NaturalClass),
        ("PhNCFeatures", NaturalClass),
        ("FsClosedFeature", FeatureDefinition),
        ("FsComplexFeature", FeatureDefinition),
        ("FsSymFeatVal", FeatureValue),
        ("FsFeatStruc", FeatureStructure),
        ("PhSequenceContext", PhonologicalContext),
        ("PhIterationContext", PhonologicalContext),
        ("PhSimpleContextSeg", PhonologicalContext),
        ("PhSimpleContextNC", PhonologicalContext),
        ("PhSimpleContextBdry", PhonologicalContext),
        ("PhFeatureConstraint", FeatureConstraint),
        ("MoMorphAdhocProhib", MorphemeCoOccurrence),
        ("MoAlloAdhocProhib", AllomorphCoOccurrence),
        ("PhRegularRule", PhonologicalRule),
        ("PhMetathesisRule", PhonologicalRule),
        ("MoEndoCompound", CompoundRule),
        ("MoExoCompound", CompoundRule),
        ("PartOfSpeech", PartOfSpeech),
        ("MoInflClass", InflectionClass),
        ("MoStemName", StemName),
        ("PhPhonRuleFeat", RuleFeature),
        ("LexEntryInflType", RuleFeature),
        ("MoInflAffixTemplate", Template),
        ("MoInflAffixSlot", TemplateSlot),
    ];
    for (class, kind) in cases {
        assert_eq!(
            class_role(class),
            Some(ClassRole::Tracked(*kind)),
            "{class} must map to {kind:?}"
        );
    }
}

#[test]
fn carrier_classes_are_never_tracked() {
    let carriers = [
        "LangProject",
        "LexDb",
        "MoMorphData",
        "PhPhonData",
        "FsFeatureSystem",
        "PhPhonemeSet",
        "PhCode",
        "PhSegRuleRHS",
        "PhVariable",
        "FsClosedValue",
        "FsComplexValue",
        "CmPossibilityList",
        "CmPossibility",
        "MoMorphType",
        "LexEntryType",
        "MoInsertNC",
        "MoCopyFromInput",
        "MoInsertPhones",
        "MoModifyFromInput",
    ];
    for class in carriers {
        assert_eq!(
            class_role(class),
            Some(ClassRole::Carrier),
            "{class} must be a carrier"
        );
    }
}
