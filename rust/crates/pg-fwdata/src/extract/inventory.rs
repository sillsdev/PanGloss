//! Class→inventory-kind classification ([`class_role`]) and the [`SelectionRecorder`] every extractor owner writes to as it decides, never re-deriving the decision itself.

use pg_snapshot::{ConversionInventory, ConversionIssue, InventoryKey, InventoryKind};

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

/// Accumulates the graph→snapshot selection inventory (one set per pipeline stage, plus rejection issues); every mutation names its stage and nothing here re-decides what the caller already decided.
#[derive(Debug, Default)]
pub(crate) struct SelectionRecorder {
    inventory: ConversionInventory,
    issues: Vec<ConversionIssue>,
}

impl SelectionRecorder {
    pub(crate) fn authored(&mut self, key: InventoryKey) {
        self.inventory.authored.insert(key);
    }

    pub(crate) fn considered(&mut self, key: InventoryKey) {
        self.inventory.considered.insert(key);
    }

    pub(crate) fn selected(&mut self, key: InventoryKey) {
        self.inventory.selected.insert(key);
    }

    pub(crate) fn represented(&mut self, key: InventoryKey) {
        self.inventory.represented.insert(key);
    }

    pub(crate) fn synthesized(&mut self, key: InventoryKey) {
        self.inventory.synthesized.insert(key);
    }

    pub(crate) fn rejected(&mut self, key: InventoryKey, issue: ConversionIssue) {
        self.inventory.rejected.insert(key);
        self.issues.push(issue);
    }

    pub(crate) fn is_represented(&self, key: &InventoryKey) -> bool {
        self.inventory.represented.contains(key)
    }

    /// Seeds `authored` from every graph header whose class tracks and whose guid is the kept (first recognized) occurrence, never a header shadowed by an earlier duplicate.
    pub(crate) fn seed_authored_from_graph(&mut self, graph: &RawGraph) {
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
            self.authored(InventoryKey::object(kind, header.guid.clone()));
        }
    }

    /// `represented ⊆ selected ⊆ considered ⊆ authored ∪ synthesized`; `rejected ⊆ selected`; `represented ∩ rejected = ∅`.
    pub(crate) fn check_invariants(&self) -> Result<(), String> {
        let authored_or_synthesized: std::collections::BTreeSet<_> = self
            .inventory
            .authored
            .union(&self.inventory.synthesized)
            .cloned()
            .collect();
        for key in &self.inventory.considered {
            if !authored_or_synthesized.contains(key) {
                return Err(format!(
                    "considered but neither authored nor synthesized: {key:?}"
                ));
            }
        }
        for key in &self.inventory.selected {
            if !self.inventory.considered.contains(key) {
                return Err(format!("selected but not considered: {key:?}"));
            }
        }
        for key in &self.inventory.represented {
            if !self.inventory.selected.contains(key) {
                return Err(format!("represented but not selected: {key:?}"));
            }
        }
        for key in &self.inventory.rejected {
            if !self.inventory.selected.contains(key) {
                return Err(format!("rejected but not selected: {key:?}"));
            }
            if self.inventory.represented.contains(key) {
                return Err(format!("both represented and rejected: {key:?}"));
            }
        }
        Ok(())
    }

    pub(crate) fn finish(self) -> (ConversionInventory, Vec<ConversionIssue>) {
        let result = self.check_invariants();
        debug_assert!(result.is_ok(), "selection recorder invariant violated: {result:?}");
        (self.inventory, self.issues)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xml::ALLOWED_CLASSES;
    use pg_snapshot::IssueClass;

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

    #[test]
    fn invariants_hold_for_a_well_formed_sequence() {
        let mut r = SelectionRecorder::default();
        let key = InventoryKey::object(InventoryKind::Entry, "g1");
        r.authored(key.clone());
        r.considered(key.clone());
        r.selected(key.clone());
        r.represented(key);
        assert!(r.check_invariants().is_ok());
    }

    #[test]
    fn invariants_reject_selected_without_considered() {
        let mut r = SelectionRecorder::default();
        let key = InventoryKey::object(InventoryKind::Entry, "g1");
        r.authored(key.clone());
        r.selected(key);
        assert!(r.check_invariants().is_err());
    }

    #[test]
    fn invariants_reject_considered_without_authored_or_synthesized() {
        let mut r = SelectionRecorder::default();
        let key = InventoryKey::object(InventoryKind::Entry, "g1");
        r.considered(key);
        assert!(r.check_invariants().is_err());
    }

    #[test]
    fn invariants_reject_represented_and_rejected_together() {
        let mut r = SelectionRecorder::default();
        let key = InventoryKey::object(InventoryKind::Entry, "g1");
        r.authored(key.clone());
        r.considered(key.clone());
        r.selected(key.clone());
        r.represented(key.clone());
        r.rejected(
            key,
            ConversionIssue {
                code: "test.code".to_string(),
                class: IssueClass::UnrepresentableForHc,
                source: None,
                fatal: false,
                message: "test".to_string(),
            },
        );
        assert!(r.check_invariants().is_err());
    }

    #[test]
    fn synthesized_satisfies_the_authored_or_synthesized_requirement() {
        let mut r = SelectionRecorder::default();
        let key = InventoryKey::object(InventoryKind::Msa, "synth-1");
        r.synthesized(key.clone());
        r.considered(key.clone());
        r.selected(key.clone());
        r.represented(key);
        assert!(r.check_invariants().is_ok());
    }
}
