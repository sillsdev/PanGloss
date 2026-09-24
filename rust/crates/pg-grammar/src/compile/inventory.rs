//! Seeds the snapshot-to-grammar `SelectionRecorder`'s `authored` set from the snapshot's own collections — a plain enumeration that makes no decision, mirroring `pg_fwdata`'s own `seed_authored_from_graph`.

use hashbrown::HashMap;

use pg_snapshot::morphology::{AdhocProhibition, PartOfSpeech};
use pg_snapshot::phonology::{NaturalClass, PhonologicalRule};
use pg_snapshot::{
    ConversionIssue, ImportWarningCode, InventoryKey, InventoryKind, IssueClass, SelectionRecorder,
    Snapshot, SourceRef,
};

/// Which owner an [`InventoryKey`] was published as representing, at the moment that owner pushed
/// it -- the identity a later reachability/reference compaction pass names when it drops that
/// owner, so [`finalize`] can revoke exactly the keys that owner represented without rediscovering
/// them by scanning the compiled `Grammar`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum LineageTarget {
    MRule(u32),
    NaturalClass(u32),
    MorphemeCoOccurrence(u32, usize),
    AllomorphCoOccurrence(u32, usize),
}

/// Every [`InventoryKey`] published as representing an mrule, natural class, or co-occurrence
/// rule, keyed by that owner's identity as of the push (see [`LineageTarget`]); read only by
/// [`finalize`], which maps a finalizer's removed-id report through these maps rather than
/// re-deriving which keys an owner represented.
#[derive(Debug, Default)]
pub(crate) struct Lineage {
    pub mrules: HashMap<u32, Vec<InventoryKey>>,
    pub natural_classes: HashMap<u32, Vec<InventoryKey>>,
    pub morpheme_cooccurrence: HashMap<(u32, usize), Vec<InventoryKey>>,
    pub allomorph_cooccurrence: HashMap<(u32, usize), Vec<InventoryKey>>,
}

impl Lineage {
    fn insert(&mut self, target: LineageTarget, key: InventoryKey) {
        match target {
            LineageTarget::MRule(id) => self.mrules.entry(id).or_default().push(key),
            LineageTarget::NaturalClass(id) => {
                self.natural_classes.entry(id).or_default().push(key)
            }
            LineageTarget::MorphemeCoOccurrence(id, idx) => self
                .morpheme_cooccurrence
                .entry((id, idx))
                .or_default()
                .push(key),
            LineageTarget::AllomorphCoOccurrence(id, idx) => self
                .allomorph_cooccurrence
                .entry((id, idx))
                .or_default()
                .push(key),
        }
    }
}

/// Marks `key` represented (exactly as [`SelectionRecorder::represented`]) and publishes it into
/// `lineage` under `target`, so a later finalizer that removes `target` can revoke `key` by name
/// instead of rediscovering it from the compiled `Grammar`.
pub(crate) fn represent_via(
    recorder: &mut SelectionRecorder,
    lineage: &mut Lineage,
    target: LineageTarget,
    key: InventoryKey,
) {
    recorder.represented(key.clone());
    lineage.insert(target, key);
}

/// Revokes every key a reachability/reference compaction pass reports removed, mapping each
/// removed id through `lineage` to the keys its owner published at push time. A removed id absent
/// from `lineage` is a bug in the owner that pushed it (represented a key without publishing it),
/// so this panics rather than silently leaving the stale key represented.
pub(crate) fn finalize(
    snapshot: &Snapshot,
    recorder: &mut SelectionRecorder,
    lineage: &Lineage,
    removed_mrules: Vec<u32>,
    removed_allomorph_cooccurrence: Vec<(u32, usize)>,
    removed_morpheme_cooccurrence: Vec<(u32, usize)>,
    removed_natural_classes: Vec<u32>,
) {
    for id in removed_mrules {
        let keys = lineage.mrules.get(&id).unwrap_or_else(|| {
            panic!("mrule {id} removed by reachability compaction but published no lineage")
        });
        for key in keys.clone() {
            let source = super::warnings::source_for_key(snapshot, &key);
            recorder.revoke_represented(
                key,
                ConversionIssue {
                    code: super::issue_codes::MRULE_UNREACHABLE_COMPACTED,
                    class: IssueClass::UnreachableInGrammar,
                    source,
                    fatal: false,
                    message: format!("mrule {id} unreachable after reachability compaction"),
                },
            );
        }
    }
    for target in removed_allomorph_cooccurrence {
        let keys = lineage.allomorph_cooccurrence.get(&target).unwrap_or_else(|| {
            panic!("allomorph co-occurrence rule {target:?} removed by reachability compaction but published no lineage")
        });
        for key in keys.clone() {
            let source = super::warnings::source_for_key(snapshot, &key);
            recorder.revoke_represented(
                key,
                ConversionIssue {
                    code: super::issue_codes::COOCCURRENCE_TARGET_UNREACHABLE,
                    class: IssueClass::UnreachableInGrammar,
                    source,
                    fatal: false,
                    message: format!(
                        "allomorph co-occurrence rule (owner {}, index {}) unreachable after \
                         mrule reachability compaction",
                        target.0, target.1
                    ),
                },
            );
        }
    }
    for target in removed_morpheme_cooccurrence {
        let keys = lineage.morpheme_cooccurrence.get(&target).unwrap_or_else(|| {
            panic!("morpheme co-occurrence rule {target:?} removed by reachability compaction but published no lineage")
        });
        for key in keys.clone() {
            let source = super::warnings::source_for_key(snapshot, &key);
            recorder.revoke_represented(
                key,
                ConversionIssue {
                    code: super::issue_codes::COOCCURRENCE_TARGET_UNREACHABLE,
                    class: IssueClass::UnreachableInGrammar,
                    source,
                    fatal: false,
                    message: format!(
                        "morpheme co-occurrence rule {target:?} unreachable after reachability compaction"
                    ),
                },
            );
        }
    }
    for id in removed_natural_classes {
        let keys = lineage.natural_classes.get(&id).unwrap_or_else(|| {
            panic!("natural class {id} removed by reachability compaction but published no lineage")
        });
        for key in keys.clone() {
            let source = super::warnings::source_for_key(snapshot, &key);
            recorder.revoke_represented(
                key,
                ConversionIssue {
                    code: super::issue_codes::NATURAL_CLASS_UNREFERENCED_COMPACTED,
                    class: IssueClass::UnreachableInGrammar,
                    source,
                    fatal: false,
                    message: format!("natural class {id} unreferenced after compaction"),
                },
            );
        }
    }
}

/// Records `key` rejected; its issue becomes the report's warning. For the phases that run before `Ctx` exists (`Ctx::reject` covers everything after).
pub(crate) fn reject(
    recorder: &mut SelectionRecorder,
    snapshot: &Snapshot,
    key: InventoryKey,
    code: ImportWarningCode,
    class: IssueClass,
    msg: impl Into<String>,
) {
    let msg = msg.into();
    let source = super::warnings::source_for_key(snapshot, &key);
    let issue = ConversionIssue {
        code,
        class,
        source,
        fatal: false,
        message: msg,
    };
    recorder.rejected(key, issue);
}

/// Records a warning about an object or value that remains represented in the grammar.
pub(crate) fn note(
    recorder: &mut SelectionRecorder,
    code: ImportWarningCode,
    class: IssueClass,
    source: SourceRef,
    msg: impl Into<String>,
) {
    let issue = ConversionIssue {
        code,
        class,
        source: Some(source),
        fatal: false,
        message: msg.into(),
    };
    recorder.noted(issue);
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
        // `lexicon::build_variant` walks a `Variant` ref only when the entry has no senses; `ComplexForm` refs are never consumed by this compiler at all (see `EntryRef::ComplexForm`'s own doc).
        if super::lexicon::entry_yields_variant_refs(e) {
            for er in &e.entry_refs {
                if let pg_snapshot::lexicon::EntryRef::Variant { guid, .. } = er {
                    recorder.authored(InventoryKey::object(EntryReference, guid.clone()));
                }
            }
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
        recorder.authored(InventoryKey::object(
            InventoryKind::InflectionClass,
            ic.guid.clone(),
        ));
        seed_infl_classes(recorder, &ic.children);
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
