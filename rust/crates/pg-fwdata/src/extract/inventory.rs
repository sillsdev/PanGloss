//! Class→inventory-kind classification ([`class_role`]) for the shared `pg_snapshot::SelectionRecorder` every extractor owner writes to as it decides, never re-deriving the decision itself.

use pg_snapshot::{InventoryKey, InventoryKind, SelectionRecorder};

use crate::xml::RawGraph;

/// Whether a tracked `.fwdata` class becomes an inventory atom of its own, or is only ever a carrier read on the way to one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClassRole {
    Tracked(InventoryKind),
    Carrier,
}

/// Classifies one `.fwdata` class; `None` means `class` is not a member of `ALLOWED_CLASSES` at all.
pub(crate) fn class_role(class: &str) -> Option<ClassRole> {
    use InventoryKind::*;
    Some(match class {
        "LexEntry" => ClassRole::Tracked(Entry),
        "LexSense" => ClassRole::Tracked(Sense),
        "LexEntryRef" => ClassRole::Tracked(EntryReference),
        "MoStemMsa" | "MoInflAffMsa" | "MoDerivAffMsa" | "MoUnclassifiedAffixMsa" => {
            ClassRole::Tracked(Msa)
        }
        "MoStemAllomorph" | "MoAffixAllomorph" => ClassRole::Tracked(Allomorph),
        "MoAffixProcess" => ClassRole::Tracked(AffixProcess),
        "PhEnvironment" => ClassRole::Tracked(Environment),
        "PhPhoneme" => ClassRole::Tracked(Phoneme),
        "PhBdryMarker" => ClassRole::Tracked(BoundaryMarker),
        "PhNCSegments" | "PhNCFeatures" => ClassRole::Tracked(NaturalClass),
        "FsClosedFeature" | "FsComplexFeature" => ClassRole::Tracked(FeatureDefinition),
        "FsSymFeatVal" => ClassRole::Tracked(FeatureValue),
        "FsFeatStruc" => ClassRole::Tracked(FeatureStructure),
        "PhSequenceContext"
        | "PhIterationContext"
        | "PhSimpleContextSeg"
        | "PhSimpleContextNC"
        | "PhSimpleContextBdry" => ClassRole::Tracked(PhonologicalContext),
        "PhFeatureConstraint" => ClassRole::Tracked(FeatureConstraint),
        "MoMorphAdhocProhib" => ClassRole::Tracked(MorphemeCoOccurrence),
        "MoAlloAdhocProhib" => ClassRole::Tracked(AllomorphCoOccurrence),
        "PhRegularRule" | "PhMetathesisRule" => ClassRole::Tracked(PhonologicalRule),
        "MoEndoCompound" | "MoExoCompound" => ClassRole::Tracked(CompoundRule),
        "PartOfSpeech" => ClassRole::Tracked(PartOfSpeech),
        "MoInflClass" => ClassRole::Tracked(InflectionClass),
        "MoStemName" => ClassRole::Tracked(StemName),
        "PhPhonRuleFeat" | "LexEntryInflType" => ClassRole::Tracked(RuleFeature),
        "MoInflAffixTemplate" => ClassRole::Tracked(Template),
        "MoInflAffixSlot" => ClassRole::Tracked(TemplateSlot),
        "LangProject" | "LexDb" | "MoMorphData" | "PhPhonData" | "FsFeatureSystem"
        | "PhPhonemeSet" | "PhCode" | "PhSegRuleRHS" | "PhVariable" | "FsClosedValue"
        | "FsComplexValue" | "CmPossibilityList" | "CmPossibility" | "MoMorphType"
        | "LexEntryType" | "MoInsertNC" | "MoCopyFromInput" | "MoInsertPhones"
        | "MoModifyFromInput" => ClassRole::Carrier,
        _ => return None,
    })
}

/// As [`class_role`], but collapsed to the tracked [`InventoryKind`] alone; `None` for a carrier or unrecognized class.
pub(crate) fn tracked_kind(class: &str) -> Option<InventoryKind> {
    match class_role(class) {
        Some(ClassRole::Tracked(kind)) => Some(kind),
        _ => None,
    }
}

/// Seeds `recorder`'s `authored` from every graph header whose class tracks and whose guid is the kept (first recognized) occurrence, never a header shadowed by an earlier duplicate.
pub(crate) fn seed_authored_from_graph(recorder: &mut SelectionRecorder, graph: &RawGraph) {
    for header in &graph.headers {
        if header.guid.is_empty() {
            continue;
        }
        let Some(kind) = tracked_kind(&header.class) else {
            continue;
        };
        let Some(record) = graph.records.get(&header.guid) else {
            continue;
        };
        if record.class != header.class {
            continue;
        }
        recorder.authored(InventoryKey::object(kind, header.guid.clone()));
    }
}

#[cfg(test)]
mod tests;
