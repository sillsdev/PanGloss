//! Light structural validation: cross-reference resolution warnings.
//!
//! Real FieldWorks data contains stale references (`docs/fwdata-import-plan.md` §1's motivating
//! example: a `MoMorphAdhocProhib` referencing a deleted morpheme, which crashes the legacy C#
//! exporter). This importer's contract is to tolerate that — dangling GUID references are
//! reported here as **warnings** (`Warning`s: a stable code alongside human-readable prose),
//! never as errors; `crate::Snapshot::from_json`/parsing always succeeds if the JSON itself is
//! well-formed and correctly versioned.
//!
//! This is intentionally a *light* check, not an exhaustive schema validator: it resolves the
//! cross-reference families that are (a) structurally represented as GUIDs in this format and
//! (b) checkable against an enumerated registry within the snapshot itself. A few reference
//! families are **not** checked, and are called out at their call site below, because this
//! format has no canonical registry to check them against (e.g. `EntryRef.variantEntryTypes`,
//! which may reference either a `LexEntryInflType` — checkable — or a plain `LexEntryType`
//! possibility list item never enumerated as its own top-level snapshot section).
//!
//! # Warning codes
//!
//! Every warning below carries a stable short code alongside its prose. The overwhelming
//! majority of this module's checks are the *same* situation applied to a different reference
//! kind — "a GUID cross-reference does not resolve to any definition of the expected kind in this
//! snapshot" — so they intentionally share one code, `DANGLING_REFERENCE`. Three call sites are
//! genuinely different situations and get their own code: `FEATURE_STRUCTURE_UNRESOLVED`
//! (`check_feature_structure`'s recursive closed/complex-feature-or-value resolution, which is
//! more involved than a single flat registry lookup), `RULE_FEATURE_UNRESOLVED`
//! (`check_rule_feature_ref`'s reference is documented as legitimately resolving against *either*
//! of two different registries), and `REFERENCE_OUT_OF_SCOPE` (a sense's MSA reference that
//! resolves fine as *some* MSA in the snapshot, just not one owned by its own entry — not a
//! dangling reference at all).

use std::collections::HashSet;

use crate::common::Guid;
use crate::feature::{FeatureStructure, FeatureSystem, FeatureValueKind};
use crate::lexicon::Msa;
use crate::morphology::{InflectionClass, PartOfSpeech};
use crate::phonology::PhonContext;
use crate::{FwClass, FwObjectRef};
use crate::{ImportWarningCode, Snapshot, Warning};

/// Shared by every plain "does this reference resolve to a definition of the expected kind" check.
const DANGLING_REFERENCE: ImportWarningCode = ImportWarningCode::SnapshotDanglingReference;
/// `check_feature_structure`'s recursive closed/complex feature-or-value resolution.
const FEATURE_STRUCTURE_UNRESOLVED: ImportWarningCode =
    ImportWarningCode::SnapshotFeatureStructureUnresolved;
/// `check_rule_feature_ref` legitimately resolves against either registry; see that function's doc.
const RULE_FEATURE_UNRESOLVED: ImportWarningCode = ImportWarningCode::SnapshotRuleFeatureUnresolved;
/// Resolves to a real definition elsewhere in the snapshot, but outside the required local scope.
const REFERENCE_OUT_OF_SCOPE: ImportWarningCode = ImportWarningCode::SnapshotReferenceOutOfScope;

/// Registries of every GUID this snapshot *defines*, checked against every GUID it *references*.
struct Registries {
    phon_closed: Vec<(Guid, HashSet<Guid>)>,
    phon_complex: HashSet<Guid>,
    syn_closed: Vec<(Guid, HashSet<Guid>)>,
    syn_complex: HashSet<Guid>,
    phonemes: HashSet<Guid>,
    boundary_markers: HashSet<Guid>,
    natural_classes: HashSet<Guid>,
    environments: HashSet<Guid>,
    feature_constraints: HashSet<Guid>,
    parts_of_speech: HashSet<Guid>,
    inflection_classes: HashSet<Guid>,
    exception_features: HashSet<Guid>,
    stem_names: HashSet<Guid>,
    affix_slots: HashSet<Guid>,
    entries: HashSet<Guid>,
    senses: HashSet<Guid>,
    /// Every allomorph guid across every entry, for ad-hoc allomorph-prohibition checks.
    allomorphs: HashSet<Guid>,
    /// Every MSA guid across every entry, for ad-hoc morpheme-prohibition and sense-MSA checks.
    msas: HashSet<Guid>,
}

fn feature_system_registry(fs: &FeatureSystem) -> (Vec<(Guid, HashSet<Guid>)>, HashSet<Guid>) {
    let closed = fs
        .closed_features
        .iter()
        .map(|f| {
            (
                f.guid.clone(),
                f.values.iter().map(|v| v.guid.clone()).collect(),
            )
        })
        .collect();
    let complex = fs.complex_features.iter().map(|f| f.guid.clone()).collect();
    (closed, complex)
}

fn collect_pos(
    items: &[PartOfSpeech],
    pos: &mut HashSet<Guid>,
    infl_classes: &mut HashSet<Guid>,
    stem_names: &mut HashSet<Guid>,
    affix_slots: &mut HashSet<Guid>,
) {
    for p in items {
        pos.insert(p.guid.clone());
        collect_infl_classes(&p.inflection_classes, infl_classes);
        for sn in &p.stem_names {
            stem_names.insert(sn.guid.clone());
        }
        for slot in &p.affix_slots {
            affix_slots.insert(slot.guid.clone());
        }
        collect_pos(&p.children, pos, infl_classes, stem_names, affix_slots);
    }
}

fn collect_infl_classes(items: &[InflectionClass], out: &mut HashSet<Guid>) {
    for c in items {
        out.insert(c.guid.clone());
        collect_infl_classes(&c.children, out);
    }
}

fn build_registries(snap: &Snapshot) -> Registries {
    let (phon_closed, phon_complex) = feature_system_registry(&snap.feature_systems.phonological);
    let (syn_closed, syn_complex) = feature_system_registry(&snap.feature_systems.morphosyntactic);

    let phonemes = snap
        .phonology
        .phonemes
        .iter()
        .map(|p| p.guid.clone())
        .collect();
    let boundary_markers = snap
        .phonology
        .boundary_markers
        .iter()
        .map(|b| b.guid.clone())
        .collect();
    let natural_classes = snap
        .phonology
        .natural_classes
        .iter()
        .map(|nc| match nc {
            crate::phonology::NaturalClass::Segments { guid, .. } => guid.clone(),
            crate::phonology::NaturalClass::Features { guid, .. } => guid.clone(),
        })
        .collect();
    let environments = snap
        .phonology
        .environments
        .iter()
        .map(|e| e.guid.clone())
        .collect();
    let feature_constraints = snap
        .phonology
        .feature_constraints
        .iter()
        .map(|c| c.guid.clone())
        .collect();

    let mut parts_of_speech = HashSet::new();
    let mut inflection_classes = HashSet::new();
    let mut stem_names = HashSet::new();
    let mut affix_slots = HashSet::new();
    collect_pos(
        &snap.morphology.parts_of_speech,
        &mut parts_of_speech,
        &mut inflection_classes,
        &mut stem_names,
        &mut affix_slots,
    );

    let entries = snap
        .lexicon
        .entries
        .iter()
        .map(|e| e.guid.clone())
        .collect();
    let mut senses = HashSet::new();
    let mut allomorphs = HashSet::new();
    let mut msas = HashSet::new();
    for entry in &snap.lexicon.entries {
        for sense in &entry.senses {
            senses.insert(sense.guid.clone());
        }
        for allo in &entry.allomorphs {
            allomorphs.insert(allo.guid.clone());
        }
        for msa in &entry.msas {
            msas.insert(msa.guid().to_string());
        }
    }

    let exception_features = snap
        .morphology
        .exception_features
        .iter()
        .map(|f| f.guid.clone())
        .collect();

    Registries {
        phon_closed,
        phon_complex,
        syn_closed,
        syn_complex,
        phonemes,
        boundary_markers,
        natural_classes,
        environments,
        feature_constraints,
        parts_of_speech,
        inflection_classes,
        exception_features,
        stem_names,
        affix_slots,
        entries,
        senses,
        allomorphs,
        msas,
    }
}

/// `guid` may legitimately be either an `InflectionClass` or an `ExceptionFeature` guid.
fn check_rule_feature_ref(
    guid: &Guid,
    reg: &Registries,
    context: &str,
    owner: &FwObjectRef,
    field: &str,
    warnings: &mut Vec<Warning>,
) {
    if !reg.inflection_classes.contains(guid) && !reg.exception_features.contains(guid) {
        warnings.push(reference_warning(RULE_FEATURE_UNRESOLVED, format!(
            "{context}: rule/exception feature {guid:?} does not resolve to a known inflection class or exception feature"
        ), owner, FwClass::Unknown, guid, field));
    }
}

fn reference_warning(
    code: ImportWarningCode,
    message: String,
    owner: &FwObjectRef,
    target_class: FwClass,
    target_guid: &str,
    field: &str,
) -> Warning {
    let target = FwObjectRef::new(target_class)
        .guid(target_guid)
        .unresolved_reference();
    let target = match target_class {
        FwClass::LexEntry => target.source_class("LexEntry"),
        FwClass::MoForm => target.source_class("MoForm"),
        FwClass::MoStemMsa => target.source_class("MoStemMsa"),
        FwClass::MoInflAffMsa => target.source_class("MoInflAffMsa"),
        FwClass::MoDerivAffMsa => target.source_class("MoDerivAffMsa"),
        FwClass::MoUnclassifiedAffixMsa => target.source_class("MoUnclassifiedAffixMsa"),
        FwClass::LexEntryInflType => target.source_class("LexEntryInflType"),
        FwClass::MoInflAffixTemplate => target.source_class("MoInflAffixTemplate"),
        FwClass::MoInflAffixSlot => target.source_class("MoInflAffixSlot"),
        FwClass::MoCompoundRule => target.source_class("MoCompoundRule"),
        FwClass::MoAdhocProhib => target.source_class("MoAdhocProhib"),
        FwClass::PhPhonemeSet => target.source_class("PhPhonemeSet"),
        FwClass::PhPhoneme => target.source_class("PhPhoneme"),
        FwClass::PhNaturalClass => target.source_class("PhNaturalClass"),
        FwClass::PhEnvironment => target.source_class("PhEnvironment"),
        FwClass::PhRegularRule => target.source_class("PhRegularRule"),
        FwClass::PhMetathesisRule => target.source_class("PhMetathesisRule"),
        FwClass::FsFeatureSystem => target.source_class("FsFeatureSystem"),
        FwClass::LexSense => target.source_class("LexSense"),
        FwClass::PhBdryMarker => target.source_class("PhBdryMarker"),
        FwClass::FsComplexFeature => target.source_class("FsComplexFeature"),
        FwClass::MoStemName => target.source_class("MoStemName"),
        FwClass::MoInflClass => target.source_class("MoInflClass"),
        FwClass::FsClosedFeature => target.source_class("FsClosedFeature"),
        FwClass::FsSymFeatVal => target.source_class("FsSymFeatVal"),
        FwClass::Project => target,
        FwClass::Unknown => match field {
            "PartOfSpeech"
            | "FromPartOfSpeech"
            | "ToPartOfSpeech"
            | "LeftMsaOA.PartOfSpeech"
            | "RightMsaOA.PartOfSpeech"
            | "OverridingMsaOA.PartOfSpeech"
            | "ToMsaOA.PartOfSpeech" => target.source_class("PartOfSpeech"),
            "InflectableFeats" => target.source_class("FsFeatDefn"),
            "FeatConstraints" => target.source_class("PhFeatureConstraint"),
            _ => target,
        },
    };
    Warning::new(code, message)
        .with_subject(owner.clone().field(field))
        .with_subject(target.field(field))
}

fn owner_subject(class: FwClass, guid: &str, name: Option<&str>) -> FwObjectRef {
    let subject = FwObjectRef::new(class).guid(guid);
    name.map_or(subject.clone(), |name| subject.name(name))
}

fn entry_title(entry: &crate::lexicon::LexEntry) -> Option<&str> {
    entry
        .citation_form
        .first()
        .map(|form| form.form.as_str())
        .filter(|name| !name.is_empty())
        .or_else(|| {
            entry
                .allomorphs
                .iter()
                .flat_map(|allomorph| &allomorph.forms)
                .map(|form| form.form.as_str())
                .find(|name| !name.is_empty())
        })
}

fn msa_source_class(msa: &Msa) -> &'static str {
    match msa {
        Msa::Stem { .. } => "MoStemMsa",
        Msa::Inflectional { .. } => "MoInflAffMsa",
        Msa::Derivational { .. } => "MoDerivAffMsa",
        Msa::Unclassified { .. } => "MoUnclassifiedAffixMsa",
    }
}

fn check_feature_structure(
    fs: &FeatureStructure,
    closed: &[(Guid, HashSet<Guid>)],
    complex: &HashSet<Guid>,
    context: &str,
    owner: &FwObjectRef,
    field: &str,
    warnings: &mut Vec<Warning>,
) {
    for value in &fs.values {
        let closed_hit = closed.iter().find(|(g, _)| *g == value.feature);
        match (&value.value, closed_hit) {
            (FeatureValueKind::Closed { value: v }, Some((_, values))) => {
                if !values.contains(v) {
                    warnings.push(reference_warning(
                        FEATURE_STRUCTURE_UNRESOLVED,
                        format!(
                            "{context}: feature value {v:?} does not resolve within feature {:?}",
                            value.feature
                        ),
                        owner,
                        FwClass::FsSymFeatVal,
                        v,
                        field,
                    ));
                }
            }
            (FeatureValueKind::Closed { .. }, None) => {
                warnings.push(reference_warning(
                    FEATURE_STRUCTURE_UNRESOLVED,
                    format!(
                        "{context}: closed feature {:?} does not resolve to any closed feature",
                        value.feature
                    ),
                    owner,
                    FwClass::FsClosedFeature,
                    &value.feature,
                    field,
                ));
            }
            (FeatureValueKind::Complex { value: nested }, _) => {
                if !complex.contains(&value.feature) {
                    warnings.push(reference_warning(
                        FEATURE_STRUCTURE_UNRESOLVED,
                        format!(
                        "{context}: complex feature {:?} does not resolve to any complex feature",
                        value.feature
                    ),
                        owner,
                        FwClass::FsComplexFeature,
                        &value.feature,
                        field,
                    ));
                }
                check_feature_structure(nested, closed, complex, context, owner, field, warnings);
            }
        }
    }
}

fn check_phon_context(
    ctx: &PhonContext,
    reg: &Registries,
    context: &str,
    owner: &FwObjectRef,
    field: &str,
    warnings: &mut Vec<Warning>,
) {
    match ctx {
        PhonContext::Sequence { members } => {
            for m in members {
                check_phon_context(m, reg, context, owner, field, warnings);
            }
        }
        PhonContext::Iteration { member, .. } => {
            check_phon_context(member, reg, context, owner, field, warnings)
        }
        PhonContext::Segment { phoneme } => {
            if !reg.phonemes.contains(phoneme) {
                warnings.push(reference_warning(
                    DANGLING_REFERENCE,
                    format!("{context}: phoneme {phoneme:?} does not resolve"),
                    owner,
                    FwClass::PhPhoneme,
                    phoneme,
                    field,
                ));
            }
        }
        PhonContext::NaturalClass {
            natural_class,
            plus_variables,
            minus_variables,
        } => {
            if !reg.natural_classes.contains(natural_class) {
                warnings.push(reference_warning(
                    DANGLING_REFERENCE,
                    format!("{context}: natural class {natural_class:?} does not resolve"),
                    owner,
                    FwClass::PhNaturalClass,
                    natural_class,
                    field,
                ));
            }
            for v in plus_variables.iter().chain(minus_variables) {
                if !reg.feature_constraints.contains(v) {
                    warnings.push(reference_warning(
                        DANGLING_REFERENCE,
                        format!("{context}: feature constraint {v:?} does not resolve"),
                        owner,
                        FwClass::Unknown,
                        v,
                        field,
                    ));
                }
            }
        }
        PhonContext::Boundary { marker } => {
            if !reg.boundary_markers.contains(marker) {
                warnings.push(reference_warning(
                    DANGLING_REFERENCE,
                    format!("{context}: boundary marker {marker:?} does not resolve"),
                    owner,
                    FwClass::PhBdryMarker,
                    marker,
                    field,
                ));
            }
        }
        PhonContext::WordBoundary | PhonContext::Variable => {}
    }
}

fn check_pos_ref(
    guid: &Guid,
    reg: &Registries,
    context: &str,
    owner: &FwObjectRef,
    field: &str,
    warnings: &mut Vec<Warning>,
) {
    if !reg.parts_of_speech.contains(guid) {
        warnings.push(reference_warning(
            DANGLING_REFERENCE,
            format!("{context}: part of speech {guid:?} does not resolve"),
            owner,
            FwClass::Unknown,
            guid,
            field,
        ));
    }
}

fn check_infl_class_ref(
    guid: &Guid,
    reg: &Registries,
    context: &str,
    owner: &FwObjectRef,
    field: &str,
    warnings: &mut Vec<Warning>,
) {
    if !reg.inflection_classes.contains(guid) {
        warnings.push(reference_warning(
            DANGLING_REFERENCE,
            format!("{context}: inflection class {guid:?} does not resolve"),
            owner,
            FwClass::MoInflClass,
            guid,
            field,
        ));
    }
}

fn check_compound_side(
    side: &crate::morphology::CompoundConstituentRequirement,
    reg: &Registries,
    context: &str,
    owner: &FwObjectRef,
    field: &str,
    warnings: &mut Vec<Warning>,
) {
    if let Some(pos) = &side.part_of_speech {
        check_pos_ref(
            pos,
            reg,
            context,
            owner,
            &format!("{field}.PartOfSpeech"),
            warnings,
        );
    }
    for feature in &side.exception_features {
        if !reg.exception_features.contains(feature) {
            warnings.push(reference_warning(
                DANGLING_REFERENCE,
                format!("{context}: exception feature {feature:?} does not resolve"),
                owner,
                FwClass::Unknown,
                feature,
                &format!("{field}.ProdRestrict"),
            ));
        }
    }
}

fn check_compound_outcome(
    outcome: &crate::morphology::CompoundOutcome,
    reg: &Registries,
    context: &str,
    owner: &FwObjectRef,
    field: &str,
    warnings: &mut Vec<Warning>,
) {
    if let Some(pos) = &outcome.part_of_speech {
        check_pos_ref(
            pos,
            reg,
            context,
            owner,
            &format!("{field}.PartOfSpeech"),
            warnings,
        );
    }
    if let Some(class) = &outcome.inflection_class {
        check_infl_class_ref(
            class,
            reg,
            context,
            owner,
            &format!("{field}.InflectionClass"),
            warnings,
        );
    }
}

/// Produce warnings for every GUID cross-reference in `snap` that does not resolve to a real
/// definition elsewhere in the same snapshot. See the module doc for what is and is not
/// checked.
pub fn validate(snap: &Snapshot) -> Vec<Warning> {
    let reg = build_registries(snap);
    let mut warnings = Vec::new();

    // --- phonology -------------------------------------------------------------------------
    for ph in &snap.phonology.phonemes {
        if let Some(fs) = &ph.features {
            let owner = owner_subject(FwClass::PhPhoneme, &ph.guid, Some(&ph.name));
            check_feature_structure(
                fs,
                &reg.phon_closed,
                &reg.phon_complex,
                &format!("phoneme {:?} features", ph.guid),
                &owner,
                "Features",
                &mut warnings,
            );
        }
    }
    for nc in &snap.phonology.natural_classes {
        match nc {
            crate::phonology::NaturalClass::Segments {
                guid,
                name,
                display_name,
                phonemes,
            } => {
                let owner = owner_subject(
                    FwClass::PhNaturalClass,
                    guid,
                    display_name.as_deref().or(Some(name.as_str())),
                );
                for p in phonemes {
                    if !reg.phonemes.contains(p) {
                        warnings.push(reference_warning(
                            DANGLING_REFERENCE,
                            format!(
                                "natural class {guid:?}: member phoneme {p:?} does not resolve"
                            ),
                            &owner,
                            FwClass::PhPhoneme,
                            p,
                            "Segments",
                        ));
                    }
                }
            }
            crate::phonology::NaturalClass::Features {
                guid,
                features,
                name,
                display_name,
            } => {
                let owner = owner_subject(
                    FwClass::PhNaturalClass,
                    guid,
                    display_name.as_deref().or(Some(name.as_str())),
                );
                check_feature_structure(
                    features,
                    &reg.phon_closed,
                    &reg.phon_complex,
                    &format!("natural class {guid:?} features"),
                    &owner,
                    "Features",
                    &mut warnings,
                );
            }
        }
    }
    for rule in &snap.phonology.rules {
        match rule {
            crate::phonology::PhonologicalRule::Rewrite(r) => {
                let ctx = format!("rewrite rule {:?}", r.guid);
                let owner = owner_subject(FwClass::PhRegularRule, &r.guid, Some(&r.name));
                for c in &r.structural_description {
                    check_phon_context(c, &reg, &ctx, &owner, "StrucDesc", &mut warnings);
                }
                for v in &r.feature_constraint_variables {
                    if !reg.feature_constraints.contains(v) {
                        warnings.push(reference_warning(
                            DANGLING_REFERENCE,
                            format!("{ctx}: feature constraint variable {v:?} does not resolve"),
                            &owner,
                            FwClass::Unknown,
                            v,
                            "FeatConstraints",
                        ));
                    }
                }
                for rhs in &r.right_hand_sides {
                    for c in &rhs.structural_change {
                        check_phon_context(c, &reg, &ctx, &owner, "StrucChange", &mut warnings);
                    }
                    if let Some(c) = &rhs.left_context {
                        check_phon_context(c, &reg, &ctx, &owner, "LeftContext", &mut warnings);
                    }
                    if let Some(c) = &rhs.right_context {
                        check_phon_context(c, &reg, &ctx, &owner, "RightContext", &mut warnings);
                    }
                    for p in &rhs.required_parts_of_speech {
                        check_pos_ref(p, &reg, &ctx, &owner, "InputPOSes", &mut warnings);
                    }
                    for f in &rhs.required_rule_features {
                        check_rule_feature_ref(
                            f,
                            &reg,
                            &ctx,
                            &owner,
                            "ReqRuleFeats",
                            &mut warnings,
                        );
                    }
                    for f in &rhs.excluded_rule_features {
                        check_rule_feature_ref(
                            f,
                            &reg,
                            &ctx,
                            &owner,
                            "ExclRuleFeats",
                            &mut warnings,
                        );
                    }
                }
            }
            crate::phonology::PhonologicalRule::Metathesis(m) => {
                let ctx = format!("metathesis rule {:?}", m.guid);
                let owner = owner_subject(FwClass::PhMetathesisRule, &m.guid, Some(&m.name));
                for c in &m.structural_description {
                    check_phon_context(c, &reg, &ctx, &owner, "StrucDesc", &mut warnings);
                }
            }
        }
    }

    // --- morphology --------------------------------------------------------------------------
    fn walk_pos(items: &[PartOfSpeech], reg: &Registries, warnings: &mut Vec<Warning>) {
        for p in items {
            let ctx = format!("part of speech {:?}", p.guid);
            let owner = owner_subject(FwClass::Unknown, &p.guid, Some(&p.name))
                .source_class("PartOfSpeech");
            if let Some(dic) = &p.default_inflection_class {
                check_infl_class_ref(dic, reg, &ctx, &owner, "DefaultInflectionClass", warnings);
            }
            for f in &p.inflectable_features {
                let known =
                    reg.syn_closed.iter().any(|(g, _)| g == f) || reg.syn_complex.contains(f);
                if !known {
                    warnings.push(reference_warning(
                        DANGLING_REFERENCE,
                        format!("{ctx}: inflectable feature {f:?} does not resolve"),
                        &owner,
                        FwClass::Unknown,
                        f,
                        "InflectableFeats",
                    ));
                }
            }
            for tmpl in &p.affix_templates {
                let tctx = format!("affix template {:?}", tmpl.guid);
                let template_owner =
                    owner_subject(FwClass::MoInflAffixTemplate, &tmpl.guid, Some(&tmpl.name));
                for (field, slots) in [
                    ("PrefixSlots", &tmpl.prefix_slots),
                    ("SuffixSlots", &tmpl.suffix_slots),
                ] {
                    for slot in slots {
                        if !reg.affix_slots.contains(slot) {
                            warnings.push(reference_warning(
                                DANGLING_REFERENCE,
                                format!("{tctx}: slot {slot:?} does not resolve"),
                                &template_owner,
                                FwClass::MoInflAffixSlot,
                                slot,
                                field,
                            ));
                        }
                    }
                }
            }
            walk_pos(&p.children, reg, warnings);
        }
    }
    walk_pos(&snap.morphology.parts_of_speech, &reg, &mut warnings);

    for rule in &snap.morphology.compound_rules {
        let ctx = format!("compound rule {:?}", rule.guid());
        let rule_owner = match rule {
            crate::morphology::CompoundRule::Endocentric { name, .. }
            | crate::morphology::CompoundRule::Exocentric { name, .. } => {
                owner_subject(FwClass::MoCompoundRule, rule.guid(), Some(name))
            }
        };
        match rule {
            crate::morphology::CompoundRule::Endocentric {
                left,
                right,
                overriding,
                ..
            } => {
                check_compound_side(left, &reg, &ctx, &rule_owner, "LeftMsaOA", &mut warnings);
                check_compound_side(right, &reg, &ctx, &rule_owner, "RightMsaOA", &mut warnings);
                check_compound_outcome(
                    overriding,
                    &reg,
                    &ctx,
                    &rule_owner,
                    "OverridingMsaOA",
                    &mut warnings,
                );
            }
            crate::morphology::CompoundRule::Exocentric {
                left, right, to, ..
            } => {
                check_compound_side(left, &reg, &ctx, &rule_owner, "LeftMsaOA", &mut warnings);
                check_compound_side(right, &reg, &ctx, &rule_owner, "RightMsaOA", &mut warnings);
                check_compound_outcome(to, &reg, &ctx, &rule_owner, "ToMsaOA", &mut warnings);
            }
        }
    }

    for adhoc in &snap.morphology.adhoc_prohibitions {
        match adhoc {
            crate::morphology::AdhocProhibition::Allomorph {
                guid,
                primary,
                others,
                ..
            } => {
                let ctx = format!("ad-hoc allomorph prohibition {guid:?}");
                let owner = owner_subject(FwClass::MoAdhocProhib, guid, None);
                if !reg.allomorphs.contains(primary) {
                    warnings.push(reference_warning(
                        DANGLING_REFERENCE,
                        format!("{ctx}: primary allomorph {primary:?} does not resolve"),
                        &owner,
                        FwClass::MoForm,
                        primary,
                        "FirstAllomorph",
                    ));
                }
                for o in others {
                    if !reg.allomorphs.contains(o) {
                        warnings.push(reference_warning(
                            DANGLING_REFERENCE,
                            format!("{ctx}: allomorph {o:?} does not resolve"),
                            &owner,
                            FwClass::MoForm,
                            o,
                            "RestOfAllos",
                        ));
                    }
                }
            }
            crate::morphology::AdhocProhibition::Morpheme {
                guid,
                primary,
                others,
                ..
            } => {
                let ctx = format!("ad-hoc morpheme prohibition {guid:?}");
                let owner = owner_subject(FwClass::MoAdhocProhib, guid, None);
                if !reg.msas.contains(primary) {
                    warnings.push(reference_warning(
                        DANGLING_REFERENCE,
                        format!("{ctx}: primary morpheme {primary:?} does not resolve"),
                        &owner,
                        FwClass::Unknown,
                        primary,
                        "FirstMorpheme",
                    ));
                }
                for o in others {
                    if !reg.msas.contains(o) {
                        warnings.push(reference_warning(
                            DANGLING_REFERENCE,
                            format!("{ctx}: morpheme {o:?} does not resolve"),
                            &owner,
                            FwClass::Unknown,
                            o,
                            "RestOfMorphs",
                        ));
                    }
                }
            }
        }
    }

    for t in &snap.morphology.lex_entry_infl_types {
        let owner = owner_subject(FwClass::LexEntryInflType, &t.guid, Some(&t.name));
        for slot in &t.slots {
            if !reg.affix_slots.contains(slot) {
                warnings.push(reference_warning(
                    DANGLING_REFERENCE,
                    format!(
                        "lexEntryInflType {:?}: slot {slot:?} does not resolve",
                        t.guid
                    ),
                    &owner,
                    FwClass::MoInflAffixSlot,
                    slot,
                    "Slots",
                ));
            }
        }
    }
    for m in &snap
        .morphology
        .parser_parameters
        .compound_rule_max_applications
    {
        let known = snap
            .morphology
            .compound_rules
            .iter()
            .any(|r| r.guid() == m.compound_rule);
        if !known {
            let owner = FwObjectRef::new(FwClass::Project)
                .name(snap.project.name.clone())
                .project_settings();
            warnings.push(reference_warning(
                DANGLING_REFERENCE,
                format!(
                    "parser parameters: maxApps compound rule {:?} does not resolve",
                    m.compound_rule
                ),
                &owner,
                FwClass::MoCompoundRule,
                &m.compound_rule,
                "CompoundRules.MaxApps",
            ));
        }
    }

    // --- lexicon -------------------------------------------------------------------------
    for entry in &snap.lexicon.entries {
        let entry_ctx = format!("lex entry {:?}", entry.guid);
        for allo in &entry.allomorphs {
            let ctx = format!("{entry_ctx} allomorph {:?}", allo.guid);
            let form_name = allo.forms.first().map(|form| form.form.as_str());
            let owner = owner_subject(FwClass::MoForm, &allo.guid, form_name);
            for e in &allo.environments {
                if !reg.environments.contains(e) {
                    warnings.push(reference_warning(
                        DANGLING_REFERENCE,
                        format!("{ctx}: environment {e:?} does not resolve"),
                        &owner,
                        FwClass::PhEnvironment,
                        e,
                        "PhoneEnv",
                    ));
                }
            }
            for e in &allo.positions {
                if !reg.environments.contains(e) {
                    warnings.push(reference_warning(
                        DANGLING_REFERENCE,
                        format!("{ctx}: environment {e:?} does not resolve"),
                        &owner,
                        FwClass::PhEnvironment,
                        e,
                        "Position",
                    ));
                }
            }
            if let Some(sn) = &allo.stem_name {
                if !reg.stem_names.contains(sn) {
                    warnings.push(reference_warning(
                        DANGLING_REFERENCE,
                        format!("{ctx}: stem name {sn:?} does not resolve"),
                        &owner,
                        FwClass::MoStemName,
                        sn,
                        "StemName",
                    ));
                }
            }
            for ic in &allo.inflection_classes {
                check_infl_class_ref(ic, &reg, &ctx, &owner, "InflectionClasses", &mut warnings);
            }
            if let Some(pos) = &allo.ms_env_part_of_speech {
                check_pos_ref(pos, &reg, &ctx, &owner, "MsEnvPartOfSpeech", &mut warnings);
            }
            if let Some(fs) = &allo.ms_env_features {
                check_feature_structure(
                    fs,
                    &reg.syn_closed,
                    &reg.syn_complex,
                    &ctx,
                    &owner,
                    "MsEnvFeatures",
                    &mut warnings,
                );
            }
            if let Some(proc) = &allo.process {
                for c in &proc.input {
                    check_phon_context(c, &reg, &ctx, &owner, "Input", &mut warnings);
                }
                for step in &proc.output {
                    match step {
                        crate::lexicon::RuleMapping::InsertNaturalClass { natural_class }
                            if !reg.natural_classes.contains(natural_class) =>
                        {
                            warnings.push(reference_warning(
                                DANGLING_REFERENCE,
                                format!("{ctx}: natural class {natural_class:?} does not resolve"),
                                &owner,
                                FwClass::PhNaturalClass,
                                natural_class,
                                "Output.Content",
                            ));
                        }
                        crate::lexicon::RuleMapping::ModifyFromInput { natural_class, .. }
                            if !reg.natural_classes.contains(natural_class) =>
                        {
                            warnings.push(reference_warning(
                                DANGLING_REFERENCE,
                                format!("{ctx}: natural class {natural_class:?} does not resolve"),
                                &owner,
                                FwClass::PhNaturalClass,
                                natural_class,
                                "Output.Modification",
                            ));
                        }
                        _ => {}
                    }
                }
            }
        }
        for msa in &entry.msas {
            let ctx = format!("{entry_ctx} msa {:?}", msa.guid());
            let owner = owner_subject(msa.fw_class(), msa.guid(), entry_title(entry));
            match msa {
                Msa::Stem {
                    part_of_speech,
                    inflection_class,
                    features,
                    exception_features,
                    slots,
                    ..
                } => {
                    if let Some(p) = part_of_speech {
                        check_pos_ref(p, &reg, &ctx, &owner, "PartOfSpeech", &mut warnings);
                    }
                    if let Some(ic) = inflection_class {
                        check_infl_class_ref(
                            ic,
                            &reg,
                            &ctx,
                            &owner,
                            "InflectionClass",
                            &mut warnings,
                        );
                    }
                    if let Some(fs) = features {
                        check_feature_structure(
                            fs,
                            &reg.syn_closed,
                            &reg.syn_complex,
                            &ctx,
                            &owner,
                            "MsFeatures",
                            &mut warnings,
                        );
                    }
                    for f in exception_features {
                        if !reg.exception_features.contains(f) {
                            warnings.push(reference_warning(
                                DANGLING_REFERENCE,
                                format!("{ctx}: exception feature {f:?} does not resolve"),
                                &owner,
                                FwClass::Unknown,
                                f,
                                "ProdRestrict",
                            ));
                        }
                    }
                    for s in slots {
                        if !reg.affix_slots.contains(s) {
                            warnings.push(reference_warning(
                                DANGLING_REFERENCE,
                                format!("{ctx}: slot {s:?} does not resolve"),
                                &owner,
                                FwClass::MoInflAffixSlot,
                                s,
                                "Slots",
                            ));
                        }
                    }
                }
                Msa::Inflectional {
                    part_of_speech,
                    slots,
                    features,
                    exception_features,
                    ..
                } => {
                    if let Some(p) = part_of_speech {
                        check_pos_ref(p, &reg, &ctx, &owner, "PartOfSpeech", &mut warnings);
                    }
                    for s in slots {
                        if !reg.affix_slots.contains(s) {
                            warnings.push(reference_warning(
                                DANGLING_REFERENCE,
                                format!("{ctx}: slot {s:?} does not resolve"),
                                &owner,
                                FwClass::MoInflAffixSlot,
                                s,
                                "Slots",
                            ));
                        }
                    }
                    if let Some(fs) = features {
                        check_feature_structure(
                            fs,
                            &reg.syn_closed,
                            &reg.syn_complex,
                            &ctx,
                            &owner,
                            "InflFeats",
                            &mut warnings,
                        );
                    }
                    for f in exception_features {
                        if !reg.exception_features.contains(f) {
                            warnings.push(reference_warning(
                                DANGLING_REFERENCE,
                                format!("{ctx}: exception feature {f:?} does not resolve"),
                                &owner,
                                FwClass::Unknown,
                                f,
                                "FromProdRestrict",
                            ));
                        }
                    }
                }
                Msa::Derivational {
                    from_part_of_speech,
                    to_part_of_speech,
                    from_features,
                    to_features,
                    from_inflection_class,
                    to_inflection_class,
                    from_exception_features,
                    to_exception_features,
                    from_stem_name,
                    ..
                } => {
                    for (field, pos) in [
                        ("FromPartOfSpeech", from_part_of_speech),
                        ("ToPartOfSpeech", to_part_of_speech),
                    ] {
                        if let Some(pos) = pos {
                            check_pos_ref(pos, &reg, &ctx, &owner, field, &mut warnings);
                        }
                    }
                    for (field, fs) in [
                        ("FromMsFeatures", from_features),
                        ("ToMsFeatures", to_features),
                    ] {
                        let Some(fs) = fs else { continue };
                        check_feature_structure(
                            fs,
                            &reg.syn_closed,
                            &reg.syn_complex,
                            &ctx,
                            &owner,
                            field,
                            &mut warnings,
                        );
                    }
                    for (field, class) in [
                        ("FromInflectionClass", from_inflection_class),
                        ("ToInflectionClass", to_inflection_class),
                    ] {
                        if let Some(class) = class {
                            check_infl_class_ref(class, &reg, &ctx, &owner, field, &mut warnings);
                        }
                    }
                    for (field, features) in [
                        ("FromProdRestrict", from_exception_features),
                        ("ToProdRestrict", to_exception_features),
                    ] {
                        for f in features {
                            if !reg.exception_features.contains(f) {
                                warnings.push(reference_warning(
                                    DANGLING_REFERENCE,
                                    format!("{ctx}: exception feature {f:?} does not resolve"),
                                    &owner,
                                    FwClass::Unknown,
                                    f,
                                    field,
                                ));
                            }
                        }
                    }
                    if let Some(sn) = from_stem_name {
                        if !reg.stem_names.contains(sn) {
                            warnings.push(reference_warning(
                                DANGLING_REFERENCE,
                                format!("{ctx}: stem name {sn:?} does not resolve"),
                                &owner,
                                FwClass::MoStemName,
                                sn,
                                "FromStemName",
                            ));
                        }
                    }
                }
                Msa::Unclassified { part_of_speech, .. } => {
                    if let Some(p) = part_of_speech {
                        check_pos_ref(p, &reg, &ctx, &owner, "PartOfSpeech", &mut warnings);
                    }
                }
            }
        }
        for sense in &entry.senses {
            let sense_name = sense
                .gloss
                .first()
                .map(|form| form.form.as_str())
                .filter(|name| !name.is_empty());
            let owner = owner_subject(FwClass::LexSense, &sense.guid, sense_name);
            if let Some(m) = &sense.msa {
                let found = entry.msas.iter().any(|msa| msa.guid() == m);
                if !found {
                    let global_msa = snap.lexicon.entries.iter().find_map(|entry| {
                        entry
                            .msas
                            .iter()
                            .find(|msa| msa.guid() == m)
                            .map(|msa| (entry, msa))
                    });
                    let mut warning = Warning::new(
                        REFERENCE_OUT_OF_SCOPE,
                        format!(
                            "{entry_ctx} sense {:?}: msa {m:?} does not resolve within this entry",
                            sense.guid
                        ),
                    )
                    .with_subject(owner.clone().field("MorphoSyntaxAnalysis"));
                    let target = match global_msa {
                        Some((target_entry, msa)) => {
                            owner_subject(msa.fw_class(), msa.guid(), entry_title(target_entry))
                                .source_class(msa_source_class(msa))
                        }
                        None => FwObjectRef::new(FwClass::Unknown)
                            .guid(m)
                            .unresolved_reference(),
                    };
                    warning.subjects.push(target.field("MorphoSyntaxAnalysis"));
                    warnings.push(warning);
                }
            }
        }
        for entry_ref in &entry.entry_refs {
            let (ctx_kind, guid, components) = match entry_ref {
                crate::lexicon::EntryRef::Variant {
                    guid,
                    component_lexemes,
                    ..
                } => ("variant", guid, component_lexemes),
                crate::lexicon::EntryRef::ComplexForm {
                    guid,
                    component_lexemes,
                    ..
                } => ("complex form", guid, component_lexemes),
            };
            let ctx = format!("{entry_ctx} {ctx_kind} ref {guid:?}");
            let owner = owner_subject(FwClass::Unknown, guid, None).source_class("LexEntryRef");
            for c in components {
                if !reg.entries.contains(c) && !reg.senses.contains(c) {
                    let warning = reference_warning(
                        DANGLING_REFERENCE,
                        format!("{ctx}: component {c:?} does not resolve to an entry or sense"),
                        &owner,
                        FwClass::Unknown,
                        c,
                        "ComponentLexemes",
                    )
                    .with_subject(
                        owner_subject(FwClass::LexEntry, &entry.guid, entry_title(entry))
                            .field("EntryRefs"),
                    );
                    warnings.push(warning);
                }
            }
            // May reference an unenumerated possibility list item, so unresolved here isn't dangling.
        }
    }

    warnings
}

#[cfg(test)]
mod subject_tests {
    use super::*;
    use crate::common::WsForm;
    use crate::lexicon::{Allomorph, LexEntry, Lexicon};
    use crate::morphology::{MorphType, Morphology};
    use crate::phonology::Phonology;
    use crate::project::Project;

    fn snapshot_with_msa(msa: Msa) -> Snapshot {
        Snapshot::new(
            Project::default(),
            crate::feature::FeatureSystems::default(),
            Phonology::default(),
            Morphology::default(),
            Lexicon {
                entries: vec![LexEntry {
                    guid: "entry-guid".to_string(),
                    citation_form: vec![WsForm {
                        ws: "en".to_string(),
                        form: "plantain".to_string(),
                    }],
                    lexeme_morph_type: MorphType::Stem,
                    allomorphs: Vec::new(),
                    msas: vec![msa],
                    senses: Vec::new(),
                    entry_refs: Vec::new(),
                }],
            },
        )
    }

    #[test]
    fn msa_owner_uses_entry_title_and_reference_field() {
        let snapshot = snapshot_with_msa(Msa::Stem {
            guid: "msa-guid".to_string(),
            part_of_speech: Some("missing-pos".to_string()),
            inflection_class: None,
            features: None,
            exception_features: Vec::new(),
            from_parts_of_speech: Vec::new(),
            slots: Vec::new(),
        });
        let warning = validate(&snapshot)
            .into_iter()
            .find(|warning| warning.message.contains("part of speech"))
            .expect("dangling part of speech is reported");
        let owner = warning
            .subjects
            .iter()
            .find(|subject| subject.guid.as_deref() == Some("msa-guid"))
            .expect("the MSA is the warning owner");
        assert_eq!(owner.name.as_deref(), Some("plantain"));
        assert_eq!(owner.field.as_deref(), Some("PartOfSpeech"));
        let target = warning
            .subjects
            .iter()
            .find(|subject| subject.guid.as_deref() == Some("missing-pos"))
            .expect("the missing POS is identified");
        assert_eq!(target.status, crate::FwSubjectStatus::UnresolvedReference);
        assert_eq!(target.source_class.as_deref(), Some("PartOfSpeech"));
        assert_eq!(target.field.as_deref(), Some("PartOfSpeech"));
    }

    #[test]
    fn derivational_references_keep_their_from_and_to_fields_distinct() {
        let snapshot = snapshot_with_msa(Msa::Derivational {
            guid: "msa-guid".to_string(),
            from_part_of_speech: Some("missing-from-pos".to_string()),
            to_part_of_speech: Some("missing-to-pos".to_string()),
            from_features: None,
            to_features: None,
            from_inflection_class: Some("missing-from-class".to_string()),
            to_inflection_class: Some("missing-to-class".to_string()),
            from_exception_features: vec!["missing-from-restriction".to_string()],
            to_exception_features: vec!["missing-to-restriction".to_string()],
            from_stem_name: None,
        });
        let warnings = validate(&snapshot);
        for (guid, expected_field) in [
            ("missing-from-pos", "FromPartOfSpeech"),
            ("missing-to-pos", "ToPartOfSpeech"),
            ("missing-from-class", "FromInflectionClass"),
            ("missing-to-class", "ToInflectionClass"),
            ("missing-from-restriction", "FromProdRestrict"),
            ("missing-to-restriction", "ToProdRestrict"),
        ] {
            let warning = warnings
                .iter()
                .find(|warning| {
                    warning.subjects.iter().any(|subject| {
                        subject.guid.as_deref() == Some(guid)
                            && subject.status == crate::FwSubjectStatus::UnresolvedReference
                    })
                })
                .unwrap_or_else(|| panic!("missing warning for {guid}"));
            let owner = warning
                .subjects
                .iter()
                .find(|subject| subject.guid.as_deref() == Some("msa-guid"))
                .expect("the MSA is the warning owner");
            let target = warning
                .subjects
                .iter()
                .find(|subject| subject.guid.as_deref() == Some(guid))
                .expect("the missing reference is identified");
            assert_eq!(owner.field.as_deref(), Some(expected_field));
            assert_eq!(target.field.as_deref(), Some(expected_field));
        }
    }

    #[test]
    fn allomorph_phone_environments_and_positions_keep_distinct_fields() {
        let mut snapshot = snapshot_with_msa(Msa::Unclassified {
            guid: "msa-guid".to_string(),
            part_of_speech: None,
        });
        snapshot.lexicon.entries[0].allomorphs.push(Allomorph {
            guid: "allomorph-guid".to_string(),
            morph_type: MorphType::Stem,
            is_abstract: false,
            forms: Vec::new(),
            environments: vec!["missing-phone-environment".to_string()],
            positions: vec!["missing-position-environment".to_string()],
            stem_name: None,
            inflection_classes: Vec::new(),
            ms_env_features: None,
            ms_env_part_of_speech: None,
            process: None,
        });

        let warnings = validate(&snapshot);
        for (guid, field) in [
            ("missing-phone-environment", "PhoneEnv"),
            ("missing-position-environment", "Position"),
        ] {
            let warning = warnings
                .iter()
                .find(|warning| warning.message.contains(guid))
                .expect("each missing environment is reported");
            let owner = warning
                .subjects
                .iter()
                .find(|subject| subject.guid.as_deref() == Some("allomorph-guid"))
                .expect("the allomorph is the warning owner");
            let target = warning
                .subjects
                .iter()
                .find(|subject| subject.guid.as_deref() == Some(guid))
                .expect("the missing environment is identified");
            assert_eq!(owner.field.as_deref(), Some(field));
            assert_eq!(target.field.as_deref(), Some(field));
        }
    }
}
