//! Seeds the snapshot-to-grammar `SelectionRecorder`'s `authored` set from the snapshot's own collections — a plain enumeration that makes no decision, mirroring `pg_fwdata`'s own `seed_authored_from_graph`.

use pg_snapshot::morphology::{AdhocProhibition, PartOfSpeech};
use pg_snapshot::phonology::{NaturalClass, PhonologicalRule};
use pg_snapshot::{ConversionIssue, InventoryKey, InventoryKind, IssueClass, SelectionRecorder, Snapshot};

/// Records `key` rejected and emits the SAME warning a caller would otherwise have pushed alone, so converting a warn-only site to also reject never changes warning prose or counts. For the phases that run before `Ctx` exists (`Ctx::reject` covers everything after).
pub(crate) fn reject(
    recorder: &mut SelectionRecorder,
    warnings: &mut Vec<String>,
    key: InventoryKey,
    code: &'static str,
    class: IssueClass,
    msg: impl Into<String>,
) {
    let msg = msg.into();
    warnings.push(msg.clone());
    recorder.rejected(
        key,
        ConversionIssue {
            code: code.to_string(),
            class,
            source: None,
            fatal: false,
            message: msg,
        },
    );
}

/// As [`reject`], but pushes no warning — for a site that was already silent about dropping it.
pub(crate) fn reject_quietly(
    recorder: &mut SelectionRecorder,
    key: InventoryKey,
    code: &'static str,
    class: IssueClass,
    msg: impl Into<String>,
) {
    recorder.rejected(
        key,
        ConversionIssue {
            code: code.to_string(),
            class,
            source: None,
            fatal: false,
            message: msg.into(),
        },
    );
}

/// Every snapshot object of a tracked kind becomes one `object(kind, guid)` authored key; parser settings this compiler actually reads and the custom-Strata setting (when present) round out the set.
pub(crate) fn seed_authored_from_snapshot(
    recorder: &mut pg_snapshot::SelectionRecorder,
    snapshot: &Snapshot,
) {
    use InventoryKind::*;

    for e in &snapshot.lexicon.entries {
        recorder.authored(InventoryKey::object(Entry, e.guid.clone()));
        for s in &e.senses {
            recorder.authored(InventoryKey::object(Sense, s.guid.clone()));
        }
        for er in &e.entry_refs {
            recorder.authored(InventoryKey::object(EntryReference, entry_ref_guid(er)));
        }
        for m in &e.msas {
            recorder.authored(InventoryKey::object(Msa, m.guid().to_string()));
        }
        for a in &e.allomorphs {
            recorder.authored(InventoryKey::object(Allomorph, a.guid.clone()));
        }
    }

    for env in &snapshot.phonology.environments {
        recorder.authored(InventoryKey::object(Environment, env.guid.clone()));
    }
    for ph in &snapshot.phonology.phonemes {
        recorder.authored(InventoryKey::object(Phoneme, ph.guid.clone()));
    }
    for bd in &snapshot.phonology.boundary_markers {
        recorder.authored(InventoryKey::object(BoundaryMarker, bd.guid.clone()));
    }
    for nc in &snapshot.phonology.natural_classes {
        recorder.authored(InventoryKey::object(NaturalClass, natclass_guid(nc)));
    }
    for fc in &snapshot.phonology.feature_constraints {
        recorder.authored(InventoryKey::object(FeatureConstraint, fc.guid.clone()));
    }
    for r in &snapshot.phonology.rules {
        recorder.authored(InventoryKey::object(PhonologicalRule, phon_rule_guid(r)));
    }

    seed_feature_system(recorder, &snapshot.feature_systems.phonological);
    seed_feature_system(recorder, &snapshot.feature_systems.morphosyntactic);

    for cr in &snapshot.morphology.compound_rules {
        recorder.authored(InventoryKey::object(CompoundRule, cr.guid().to_string()));
    }
    seed_pos_tree(recorder, &snapshot.morphology.parts_of_speech);
    for ef in &snapshot.morphology.exception_features {
        recorder.authored(InventoryKey::object(RuleFeature, ef.guid.clone()));
    }
    for it in &snapshot.morphology.lex_entry_infl_types {
        recorder.authored(InventoryKey::object(RuleFeature, it.guid.clone()));
    }
    for ap in &snapshot.morphology.adhoc_prohibitions {
        match ap {
            AdhocProhibition::Allomorph { guid, .. } => {
                recorder.authored(InventoryKey::object(AllomorphCoOccurrence, guid.clone()));
            }
            AdhocProhibition::Morpheme { guid, .. } => {
                recorder.authored(InventoryKey::object(MorphemeCoOccurrence, guid.clone()));
            }
        }
    }

    // Parser settings this compiler actually consults; both are always "present" (booleans with FieldWorks defaults), so always authored.
    recorder.authored(InventoryKey::setting(ParserSetting, "notOnClitics"));
    recorder.authored(InventoryKey::setting(ParserSetting, "noDefaultCompounding"));
    if snapshot
        .morphology
        .parser_parameters
        .strata
        .as_deref()
        .is_some_and(|s| !s.trim().is_empty())
    {
        recorder.authored(InventoryKey::setting(StrataConfiguration, "Strata"));
    }
}

fn seed_feature_system(
    recorder: &mut pg_snapshot::SelectionRecorder,
    fs: &pg_snapshot::feature::FeatureSystem,
) {
    use InventoryKind::*;
    for cf in &fs.closed_features {
        recorder.authored(InventoryKey::object(FeatureDefinition, cf.guid.clone()));
        for v in &cf.values {
            recorder.authored(InventoryKey::object(FeatureValue, v.guid.clone()));
        }
    }
    for cf in &fs.complex_features {
        recorder.authored(InventoryKey::object(FeatureDefinition, cf.guid.clone()));
    }
}

fn seed_pos_tree(recorder: &mut pg_snapshot::SelectionRecorder, items: &[PartOfSpeech]) {
    use InventoryKind::*;
    for pos in items {
        recorder.authored(InventoryKey::object(PartOfSpeech, pos.guid.clone()));
        seed_infl_classes(recorder, &pos.inflection_classes);
        for sn in &pos.stem_names {
            recorder.authored(InventoryKey::object(StemName, sn.guid.clone()));
        }
        for slot in &pos.affix_slots {
            recorder.authored(InventoryKey::object(TemplateSlot, slot.guid.clone()));
        }
        for tmpl in &pos.affix_templates {
            recorder.authored(InventoryKey::object(Template, tmpl.guid.clone()));
        }
        seed_pos_tree(recorder, &pos.children);
    }
}

fn seed_infl_classes(
    recorder: &mut pg_snapshot::SelectionRecorder,
    items: &[pg_snapshot::morphology::InflectionClass],
) {
    for ic in items {
        recorder.authored(InventoryKey::object(InventoryKind::InflectionClass, ic.guid.clone()));
        seed_infl_classes(recorder, &ic.children);
    }
}

fn entry_ref_guid(er: &pg_snapshot::lexicon::EntryRef) -> String {
    use pg_snapshot::lexicon::EntryRef::*;
    match er {
        Variant { guid, .. } | ComplexForm { guid, .. } => guid.clone(),
    }
}

fn natclass_guid(nc: &NaturalClass) -> String {
    match nc {
        NaturalClass::Segments { guid, .. } | NaturalClass::Features { guid, .. } => guid.clone(),
    }
}

fn phon_rule_guid(r: &PhonologicalRule) -> String {
    match r {
        PhonologicalRule::Rewrite(rr) => rr.guid.clone(),
        PhonologicalRule::Metathesis(mr) => mr.guid.clone(),
    }
}
