//! Light structural validation: cross-reference resolution warnings.
//!
//! Real FieldWorks data contains stale references (`docs/fwdata-import-plan.md` §1's motivating
//! example: a `MoMorphAdhocProhib` referencing a deleted morpheme, which crashes the legacy C#
//! exporter). This importer's contract is to tolerate that — dangling GUID references are
//! reported here as **warnings** ([`Warning`]s: a stable code alongside human-readable prose),
//! never as errors; [`crate::Snapshot::from_json`]/parsing always succeeds if the JSON itself is
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
//! # Warning codes (`add-grammar-assessment` task 3.8)
//!
//! Every warning below carries a stable short code alongside its prose. The overwhelming
//! majority of this module's checks are the *same* situation applied to a different reference
//! kind — "a GUID cross-reference does not resolve to any definition of the expected kind in this
//! snapshot" — so they intentionally share one code, [`DANGLING_REFERENCE`]. Three call sites are
//! genuinely different situations and get their own code: [`FEATURE_STRUCTURE_UNRESOLVED`]
//! (`check_feature_structure`'s recursive closed/complex-feature-or-value resolution, which is
//! more involved than a single flat registry lookup), [`RULE_FEATURE_UNRESOLVED`]
//! (`check_rule_feature_ref`'s reference is documented as legitimately resolving against *either*
//! of two different registries), and [`REFERENCE_OUT_OF_SCOPE`] (a sense's MSA reference that
//! resolves fine as *some* MSA in the snapshot, just not one owned by its own entry — not a
//! dangling reference at all).

use std::collections::HashSet;

use crate::common::Guid;
use crate::feature::{FeatureStructure, FeatureSystem, FeatureValueKind};
use crate::lexicon::Msa;
use crate::morphology::{InflectionClass, PartOfSpeech};
use crate::phonology::PhonContext;
use crate::{Snapshot, Warning};

/// A GUID cross-reference does not resolve to any definition of the expected kind within this
/// snapshot. Shared by every plain "does this reference resolve" check in this module.
const DANGLING_REFERENCE: &str = "snapshot.dangling-reference";
/// `check_feature_structure`'s recursive closed/complex feature-or-value resolution.
const FEATURE_STRUCTURE_UNRESOLVED: &str = "snapshot.feature-structure-unresolved";
/// `check_rule_feature_ref`'s reference, which may legitimately resolve against either an
/// inflection class or an exception feature registry (see that function's doc).
const RULE_FEATURE_UNRESOLVED: &str = "snapshot.rule-feature-unresolved";
/// A reference resolves to a real definition elsewhere in the snapshot, but not within the scope
/// (e.g. owning entry) it was required to be local to.
const REFERENCE_OUT_OF_SCOPE: &str = "snapshot.reference-out-of-scope";

/// Registries of every GUID this snapshot *defines*, used to check that every GUID it
/// *references* resolves to something real.
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

/// `guid` is a "rule feature"/"exception feature" reference: it may legitimately be either an
/// [`crate::morphology::InflectionClass`] guid or an
/// [`crate::morphology::ExceptionFeature`] guid (see `HCLoader.LoadMprFeatures`,
/// HCLoader.cs:2610-2623, and `Morphology::exception_features`'s doc for why both are valid).
fn check_rule_feature_ref(
    guid: &Guid,
    reg: &Registries,
    context: &str,
    warnings: &mut Vec<Warning>,
) {
    if !reg.inflection_classes.contains(guid) && !reg.exception_features.contains(guid) {
        warnings.push(Warning::new(RULE_FEATURE_UNRESOLVED, format!(
            "{context}: rule/exception feature {guid:?} does not resolve to a known inflection class or exception feature"
        )));
    }
}

fn check_feature_structure(
    fs: &FeatureStructure,
    closed: &[(Guid, HashSet<Guid>)],
    complex: &HashSet<Guid>,
    context: &str,
    warnings: &mut Vec<Warning>,
) {
    for value in &fs.values {
        let closed_hit = closed.iter().find(|(g, _)| *g == value.feature);
        match (&value.value, closed_hit) {
            (FeatureValueKind::Closed { value: v }, Some((_, values))) => {
                if !values.contains(v) {
                    warnings.push(Warning::new(FEATURE_STRUCTURE_UNRESOLVED, format!(
                        "{context}: feature value {v:?} does not resolve within feature {:?}",
                        value.feature
                    )));
                }
            }
            (FeatureValueKind::Closed { .. }, None) => {
                warnings.push(Warning::new(FEATURE_STRUCTURE_UNRESOLVED, format!(
                    "{context}: closed feature {:?} does not resolve to any closed feature",
                    value.feature
                )));
            }
            (FeatureValueKind::Complex { value: nested }, _) => {
                if !complex.contains(&value.feature) {
                    warnings.push(Warning::new(FEATURE_STRUCTURE_UNRESOLVED, format!(
                        "{context}: complex feature {:?} does not resolve to any complex feature",
                        value.feature
                    )));
                }
                check_feature_structure(nested, closed, complex, context, warnings);
            }
        }
    }
}

fn check_phon_context(
    ctx: &PhonContext,
    reg: &Registries,
    context: &str,
    warnings: &mut Vec<Warning>,
) {
    match ctx {
        PhonContext::Sequence { members } => {
            for m in members {
                check_phon_context(m, reg, context, warnings);
            }
        }
        PhonContext::Iteration { member, .. } => check_phon_context(member, reg, context, warnings),
        PhonContext::Segment { phoneme } => {
            if !reg.phonemes.contains(phoneme) {
                warnings.push(Warning::new(
                    DANGLING_REFERENCE,
                    format!("{context}: phoneme {phoneme:?} does not resolve"),
                ));
            }
        }
        PhonContext::NaturalClass {
            natural_class,
            plus_variables,
            minus_variables,
        } => {
            if !reg.natural_classes.contains(natural_class) {
                warnings.push(Warning::new(
                    DANGLING_REFERENCE,
                    format!(
                        "{context}: natural class {natural_class:?} does not resolve"
                    ),
                ));
            }
            for v in plus_variables.iter().chain(minus_variables) {
                if !reg.feature_constraints.contains(v) {
                    warnings.push(Warning::new(
                        DANGLING_REFERENCE,
                        format!("{context}: feature constraint {v:?} does not resolve"),
                    ));
                }
            }
        }
        PhonContext::Boundary { marker } => {
            if !reg.boundary_markers.contains(marker) {
                warnings.push(Warning::new(
                    DANGLING_REFERENCE,
                    format!("{context}: boundary marker {marker:?} does not resolve"),
                ));
            }
        }
        PhonContext::WordBoundary | PhonContext::Variable => {}
    }
}

fn check_pos_ref(guid: &Guid, reg: &Registries, context: &str, warnings: &mut Vec<Warning>) {
    if !reg.parts_of_speech.contains(guid) {
        warnings.push(Warning::new(
            DANGLING_REFERENCE,
            format!("{context}: part of speech {guid:?} does not resolve"),
        ));
    }
}

fn check_infl_class_ref(guid: &Guid, reg: &Registries, context: &str, warnings: &mut Vec<Warning>) {
    if !reg.inflection_classes.contains(guid) {
        warnings.push(Warning::new(
            DANGLING_REFERENCE,
            format!("{context}: inflection class {guid:?} does not resolve"),
        ));
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
            check_feature_structure(
                fs,
                &reg.phon_closed,
                &reg.phon_complex,
                &format!("phoneme {:?} features", ph.guid),
                &mut warnings,
            );
        }
    }
    for nc in &snap.phonology.natural_classes {
        match nc {
            crate::phonology::NaturalClass::Segments { guid, phonemes, .. } => {
                for p in phonemes {
                    if !reg.phonemes.contains(p) {
                        warnings.push(Warning::new(
                            DANGLING_REFERENCE,
                            format!(
                                "natural class {guid:?}: member phoneme {p:?} does not resolve"
                            ),
                        ));
                    }
                }
            }
            crate::phonology::NaturalClass::Features { guid, features, .. } => {
                check_feature_structure(
                    features,
                    &reg.phon_closed,
                    &reg.phon_complex,
                    &format!("natural class {guid:?} features"),
                    &mut warnings,
                );
            }
        }
    }
    for rule in &snap.phonology.rules {
        match rule {
            crate::phonology::PhonologicalRule::Rewrite(r) => {
                let ctx = format!("rewrite rule {:?}", r.guid);
                for c in &r.structural_description {
                    check_phon_context(c, &reg, &ctx, &mut warnings);
                }
                for v in &r.feature_constraint_variables {
                    if !reg.feature_constraints.contains(v) {
                        warnings.push(Warning::new(
                            DANGLING_REFERENCE,
                            format!("{ctx}: feature constraint variable {v:?} does not resolve"),
                        ));
                    }
                }
                for rhs in &r.right_hand_sides {
                    for c in &rhs.structural_change {
                        check_phon_context(c, &reg, &ctx, &mut warnings);
                    }
                    if let Some(c) = &rhs.left_context {
                        check_phon_context(c, &reg, &ctx, &mut warnings);
                    }
                    if let Some(c) = &rhs.right_context {
                        check_phon_context(c, &reg, &ctx, &mut warnings);
                    }
                    for p in &rhs.required_parts_of_speech {
                        check_pos_ref(p, &reg, &ctx, &mut warnings);
                    }
                    for f in rhs
                        .required_rule_features
                        .iter()
                        .chain(&rhs.excluded_rule_features)
                    {
                        check_rule_feature_ref(f, &reg, &ctx, &mut warnings);
                    }
                }
            }
            crate::phonology::PhonologicalRule::Metathesis(m) => {
                let ctx = format!("metathesis rule {:?}", m.guid);
                for c in &m.structural_description {
                    check_phon_context(c, &reg, &ctx, &mut warnings);
                }
            }
        }
    }

    // --- morphology --------------------------------------------------------------------------
    fn walk_pos(items: &[PartOfSpeech], reg: &Registries, warnings: &mut Vec<Warning>) {
        for p in items {
            let ctx = format!("part of speech {:?}", p.guid);
            if let Some(dic) = &p.default_inflection_class {
                check_infl_class_ref(dic, reg, &ctx, warnings);
            }
            for f in &p.inflectable_features {
                let known =
                    reg.syn_closed.iter().any(|(g, _)| g == f) || reg.syn_complex.contains(f);
                if !known {
                    warnings.push(Warning::new(
                        DANGLING_REFERENCE,
                        format!("{ctx}: inflectable feature {f:?} does not resolve"),
                    ));
                }
            }
            for tmpl in &p.affix_templates {
                let tctx = format!("affix template {:?}", tmpl.guid);
                for slot in tmpl.prefix_slots.iter().chain(&tmpl.suffix_slots) {
                    if !reg.affix_slots.contains(slot) {
                        warnings.push(Warning::new(
                            DANGLING_REFERENCE,
                            format!("{tctx}: slot {slot:?} does not resolve"),
                        ));
                    }
                }
            }
            walk_pos(&p.children, reg, warnings);
        }
    }
    walk_pos(&snap.morphology.parts_of_speech, &reg, &mut warnings);

    for rule in &snap.morphology.compound_rules {
        let ctx = format!("compound rule {:?}", rule.guid());
        let (left, right, out_pos, out_infl) = match rule {
            crate::morphology::CompoundRule::Endocentric {
                left,
                right,
                overriding,
                ..
            } => (
                left,
                right,
                &overriding.part_of_speech,
                &overriding.inflection_class,
            ),
            crate::morphology::CompoundRule::Exocentric {
                left, right, to, ..
            } => (left, right, &to.part_of_speech, &to.inflection_class),
        };
        for side in [left, right] {
            if let Some(p) = &side.part_of_speech {
                check_pos_ref(p, &reg, &ctx, &mut warnings);
            }
            for f in &side.exception_features {
                if !reg.exception_features.contains(f) {
                    warnings.push(Warning::new(
                        DANGLING_REFERENCE,
                        format!("{ctx}: exception feature {f:?} does not resolve"),
                    ));
                }
            }
        }
        if let Some(p) = out_pos {
            check_pos_ref(p, &reg, &ctx, &mut warnings);
        }
        if let Some(c) = out_infl {
            check_infl_class_ref(c, &reg, &ctx, &mut warnings);
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
                if !reg.allomorphs.contains(primary) {
                    warnings.push(Warning::new(
                        DANGLING_REFERENCE,
                        format!("{ctx}: primary allomorph {primary:?} does not resolve"),
                    ));
                }
                for o in others {
                    if !reg.allomorphs.contains(o) {
                        warnings.push(Warning::new(
                            DANGLING_REFERENCE,
                            format!("{ctx}: allomorph {o:?} does not resolve"),
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
                if !reg.msas.contains(primary) {
                    warnings.push(Warning::new(
                        DANGLING_REFERENCE,
                        format!("{ctx}: primary morpheme {primary:?} does not resolve"),
                    ));
                }
                for o in others {
                    if !reg.msas.contains(o) {
                        warnings.push(Warning::new(
                            DANGLING_REFERENCE,
                            format!("{ctx}: morpheme {o:?} does not resolve"),
                        ));
                    }
                }
            }
        }
    }

    for t in &snap.morphology.lex_entry_infl_types {
        for slot in &t.slots {
            if !reg.affix_slots.contains(slot) {
                warnings.push(Warning::new(
                    DANGLING_REFERENCE,
                    format!(
                        "lexEntryInflType {:?}: slot {slot:?} does not resolve",
                        t.guid
                    ),
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
            warnings.push(Warning::new(
                DANGLING_REFERENCE,
                format!(
                    "parser parameters: maxApps compound rule {:?} does not resolve",
                    m.compound_rule
                ),
            ));
        }
    }

    // --- lexicon -------------------------------------------------------------------------
    for entry in &snap.lexicon.entries {
        let entry_ctx = format!("lex entry {:?}", entry.guid);
        for allo in &entry.allomorphs {
            let ctx = format!("{entry_ctx} allomorph {:?}", allo.guid);
            for e in allo.environments.iter().chain(&allo.positions) {
                if !reg.environments.contains(e) {
                    warnings.push(Warning::new(
                        DANGLING_REFERENCE,
                        format!("{ctx}: environment {e:?} does not resolve"),
                    ));
                }
            }
            if let Some(sn) = &allo.stem_name {
                if !reg.stem_names.contains(sn) {
                    warnings.push(Warning::new(
                        DANGLING_REFERENCE,
                        format!("{ctx}: stem name {sn:?} does not resolve"),
                    ));
                }
            }
            for ic in &allo.inflection_classes {
                check_infl_class_ref(ic, &reg, &ctx, &mut warnings);
            }
            if let Some(pos) = &allo.ms_env_part_of_speech {
                check_pos_ref(pos, &reg, &ctx, &mut warnings);
            }
            if let Some(fs) = &allo.ms_env_features {
                check_feature_structure(fs, &reg.syn_closed, &reg.syn_complex, &ctx, &mut warnings);
            }
            if let Some(proc) = &allo.process {
                for c in &proc.input {
                    check_phon_context(c, &reg, &ctx, &mut warnings);
                }
                for step in &proc.output {
                    match step {
                        crate::lexicon::RuleMapping::InsertNaturalClass { natural_class }
                        | crate::lexicon::RuleMapping::ModifyFromInput { natural_class, .. }
                            if !reg.natural_classes.contains(natural_class) =>
                        {
                            warnings.push(Warning::new(
                                DANGLING_REFERENCE,
                                format!("{ctx}: natural class {natural_class:?} does not resolve"),
                            ));
                        }
                        _ => {}
                    }
                }
            }
        }
        for msa in &entry.msas {
            let ctx = format!("{entry_ctx} msa {:?}", msa.guid());
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
                        check_pos_ref(p, &reg, &ctx, &mut warnings);
                    }
                    if let Some(ic) = inflection_class {
                        check_infl_class_ref(ic, &reg, &ctx, &mut warnings);
                    }
                    if let Some(fs) = features {
                        check_feature_structure(
                            fs,
                            &reg.syn_closed,
                            &reg.syn_complex,
                            &ctx,
                            &mut warnings,
                        );
                    }
                    for f in exception_features {
                        if !reg.exception_features.contains(f) {
                            warnings.push(Warning::new(
                                DANGLING_REFERENCE,
                                format!("{ctx}: exception feature {f:?} does not resolve"),
                            ));
                        }
                    }
                    for s in slots {
                        if !reg.affix_slots.contains(s) {
                            warnings.push(Warning::new(
                                DANGLING_REFERENCE,
                                format!("{ctx}: slot {s:?} does not resolve"),
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
                        check_pos_ref(p, &reg, &ctx, &mut warnings);
                    }
                    for s in slots {
                        if !reg.affix_slots.contains(s) {
                            warnings.push(Warning::new(
                                DANGLING_REFERENCE,
                                format!("{ctx}: slot {s:?} does not resolve"),
                            ));
                        }
                    }
                    if let Some(fs) = features {
                        check_feature_structure(
                            fs,
                            &reg.syn_closed,
                            &reg.syn_complex,
                            &ctx,
                            &mut warnings,
                        );
                    }
                    for f in exception_features {
                        if !reg.exception_features.contains(f) {
                            warnings.push(Warning::new(
                                DANGLING_REFERENCE,
                                format!("{ctx}: exception feature {f:?} does not resolve"),
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
                    for p in [from_part_of_speech, to_part_of_speech]
                        .into_iter()
                        .flatten()
                    {
                        check_pos_ref(p, &reg, &ctx, &mut warnings);
                    }
                    for fs in [from_features, to_features].into_iter().flatten() {
                        check_feature_structure(
                            fs,
                            &reg.syn_closed,
                            &reg.syn_complex,
                            &ctx,
                            &mut warnings,
                        );
                    }
                    for ic in [from_inflection_class, to_inflection_class]
                        .into_iter()
                        .flatten()
                    {
                        check_infl_class_ref(ic, &reg, &ctx, &mut warnings);
                    }
                    for f in from_exception_features.iter().chain(to_exception_features) {
                        if !reg.exception_features.contains(f) {
                            warnings.push(Warning::new(
                                DANGLING_REFERENCE,
                                format!("{ctx}: exception feature {f:?} does not resolve"),
                            ));
                        }
                    }
                    if let Some(sn) = from_stem_name {
                        if !reg.stem_names.contains(sn) {
                            warnings.push(Warning::new(
                                DANGLING_REFERENCE,
                                format!("{ctx}: stem name {sn:?} does not resolve"),
                            ));
                        }
                    }
                }
                Msa::Unclassified { part_of_speech, .. } => {
                    if let Some(p) = part_of_speech {
                        check_pos_ref(p, &reg, &ctx, &mut warnings);
                    }
                }
            }
        }
        for sense in &entry.senses {
            if let Some(m) = &sense.msa {
                let found = entry.msas.iter().any(|msa| msa.guid() == m);
                if !found {
                    warnings.push(Warning::new(
                        REFERENCE_OUT_OF_SCOPE,
                        format!(
                            "{entry_ctx} sense {:?}: msa {m:?} does not resolve within this entry",
                            sense.guid
                        ),
                    ));
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
            for c in components {
                if !reg.entries.contains(c) && !reg.senses.contains(c) {
                    warnings.push(Warning::new(
                        DANGLING_REFERENCE,
                        format!("{ctx}: component {c:?} does not resolve to an entry or sense"),
                    ));
                }
            }
            // `variant_entry_types`/`complex_entry_types` may reference either a
            // `LexEntryInflType` (checkable against `lex_entry_infl_types`) or a plain
            // `LexEntryType` possibility list item (not enumerated anywhere in this format), so
            // an unresolved guid there is not necessarily dangling — intentionally not checked.
        }
    }

    warnings
}
