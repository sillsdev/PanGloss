//! One row per parser-relevant reference to a grammar statement, built from the Snapshot and compiler outcomes.

use std::collections::{BTreeMap, BTreeSet};

use pg_grammar::compile::{
    AllomorphGateKind, AllomorphGateOutcome, EnvironmentResolution, EnvironmentResolutionStatus,
};
use pg_snapshot::feature::{FeatureStructure, FeatureValueKind};
use pg_snapshot::lexicon::{EntryRef, Msa, RuleMapping};
use pg_snapshot::morphology::{
    AdhocProhibition, AffixTemplate, CompoundConstituentRequirement, CompoundOutcome, CompoundRule,
    InflectionClass,
};
use pg_snapshot::phonology::{NaturalClass, PhonContext, PhonologicalRule, RewriteRhs};
use pg_snapshot::{canonical_guid, InventoryKey, InventoryKind, LoadDecision, Snapshot};
use rusqlite::{params, Transaction};

use crate::lexicon::{form_class, reads_positions, GateEffects, MS_ENV_PART_OF_SPEECH_EFFECT};
use crate::morphology::all_parts_of_speech;
use crate::phonology::{
    environment_literals, feature_class_phonemes, inventory_owner_loaded, natural_class_guid,
    rule_feature_target_kind, EnvironmentEdges,
};
use crate::FactsError;

/// What the compiler produced: the grammar, its output ids, and the source mappings of those outputs.
pub(crate) struct CompiledInputs<'a> {
    pub(crate) grammar: Option<&'a pg_grammar::model::Grammar>,
    pub(crate) mappings: &'a [pg_grammar::compile::CompiledMapping],
    pub(crate) form_segments: &'a [(u32, Vec<crate::compiled::FormSegment>)],
}

/// Records every class under `classes` with the GUIDs of all its descendants.
fn index_descendants(classes: &[InflectionClass], out: &mut BTreeMap<String, Vec<String>>) {
    for class in classes {
        out.insert(guid_text(&class.guid), descendants_of(class));
        index_descendants(&class.children, out);
    }
}

fn descendants_of(class: &InflectionClass) -> Vec<String> {
    class
        .children
        .iter()
        .flat_map(|child| std::iter::once(guid_text(&child.guid)).chain(descendants_of(child)))
        .collect()
}

/// Writes every reference a statement makes, with the effect the compiler gave its referrer.
pub(crate) fn insert(
    tx: &Transaction<'_>,
    snapshot: &Snapshot,
    decisions: &[LoadDecision],
    environment_resolutions: &[EnvironmentResolution],
    allomorph_gates: &[AllomorphGateOutcome],
    compiled: CompiledInputs<'_>,
) -> Result<(), FactsError> {
    let mut descendants = BTreeMap::new();
    for part in all_parts_of_speech(&snapshot.morphology.parts_of_speech) {
        index_descendants(&part.inflection_classes, &mut descendants);
    }
    let mut writer = Writer {
        tx,
        known: Known::new(snapshot),
        decisions,
        descendants,
        next_ordinal: BTreeMap::new(),
    };
    let grammar = compiled.grammar;
    writer.allomorphs(snapshot, environment_resolutions, allomorph_gates)?;
    writer.environments(snapshot, environment_resolutions, grammar)?;
    writer.classes(snapshot, grammar)?;
    writer.form_segments(&compiled)?;
    writer.msas(snapshot)?;
    writer.entries(snapshot)?;
    writer.categories(snapshot)?;
    writer.infl_types(snapshot)?;
    writer.adhoc_prohibitions(snapshot)?;
    writer.compound_rules(snapshot)?;
    writer.phonological_rules(snapshot)?;
    writer.feature_constraints(snapshot)?;
    Ok(())
}

/// The GUID as stored: canonical lowercase when well formed, otherwise kept as authored.
fn guid_text(value: &str) -> String {
    canonical_guid(value).unwrap_or_else(|| value.to_string())
}

/// Target GUIDs the Snapshot authors, by target kind, so a dangling reference is `unresolved`.
struct Known(BTreeSet<(&'static str, String)>);

impl Known {
    fn new(snapshot: &Snapshot) -> Self {
        let mut known = BTreeSet::new();
        let mut add = |kind: &'static str, guid: &str| {
            known.insert((kind, guid_text(guid)));
        };
        for environment in &snapshot.phonology.environments {
            add("environment", &environment.guid);
        }
        for class in &snapshot.phonology.natural_classes {
            add("naturalClass", natural_class_guid(class));
        }
        for phoneme in &snapshot.phonology.phonemes {
            add("phoneme", &phoneme.guid);
        }
        for marker in &snapshot.phonology.boundary_markers {
            add("boundary", &marker.guid);
        }
        for constraint in &snapshot.phonology.feature_constraints {
            add("featureConstraint", &constraint.guid);
        }
        for system in [
            &snapshot.feature_systems.phonological,
            &snapshot.feature_systems.morphosyntactic,
        ] {
            for feature in &system.closed_features {
                add("feature", &feature.guid);
                for value in &feature.values {
                    add("featureValue", &value.guid);
                }
            }
            for feature in &system.complex_features {
                add("feature", &feature.guid);
            }
        }
        for exception in &snapshot.morphology.exception_features {
            add("exceptionFeature", &exception.guid);
        }
        for infl_type in &snapshot.morphology.lex_entry_infl_types {
            add("inflType", &infl_type.guid);
        }
        for entry in &snapshot.lexicon.entries {
            add("entry", &entry.guid);
            for sense in &entry.senses {
                add("sense", &sense.guid);
            }
            for allomorph in &entry.allomorphs {
                add("allomorph", &allomorph.guid);
            }
            for msa in &entry.msas {
                add("msa", msa.guid());
            }
        }
        for part in all_parts_of_speech(&snapshot.morphology.parts_of_speech) {
            add("category", &part.guid);
            for slot in &part.affix_slots {
                add("slot", &slot.guid);
            }
            for template in &part.affix_templates {
                add("template", &template.guid);
            }
            for stem in &part.stem_names {
                add("stemName", &stem.guid);
            }
            add_classes(&part.inflection_classes, &mut add);
        }
        Self(known)
    }

    fn has(&self, kind: &'static str, guid: &str) -> bool {
        self.0.contains(&(kind, guid_text(guid)))
    }
}

fn add_classes(classes: &[InflectionClass], add: &mut impl FnMut(&'static str, &str)) {
    for class in classes {
        add("inflectionClass", &class.guid);
        add_classes(&class.children, add);
    }
}

/// One statement reference before its ordinal is assigned.
struct Reference<'r> {
    target_kind: &'static str,
    target: &'r str,
    referrer_kind: &'static str,
    referrer: &'r str,
    role: &'static str,
    effect: &'static str,
}

struct Writer<'a, 'c> {
    tx: &'a Transaction<'c>,
    known: Known,
    decisions: &'a [LoadDecision],
    /// Every class GUID mapped to its descendants, which the compiler's subclass closure can use.
    descendants: BTreeMap<String, Vec<String>>,
    /// Next ordinal per referrer and role, so repeated references stay distinct by key.
    next_ordinal: BTreeMap<(&'static str, String, &'static str), i64>,
}

impl Writer<'_, '_> {
    /// Publishes a reference, and each descendant of a referenced class as `via_ancestor`.
    fn add(&mut self, reference: Reference<'_>) -> Result<(), FactsError> {
        let descendants = if reference.target_kind == "inflectionClass" {
            self.descendants
                .get(&guid_text(reference.target))
                .cloned()
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let (target_kind, referrer_kind, referrer, effect) = (
            reference.target_kind,
            reference.referrer_kind,
            reference.referrer,
            reference.effect,
        );
        self.insert_row(reference)?;
        for descendant in &descendants {
            self.insert_row(Reference {
                target_kind,
                target: descendant,
                referrer_kind,
                referrer,
                role: "via_ancestor",
                effect,
            })?;
        }
        Ok(())
    }

    fn insert_row(&mut self, reference: Reference<'_>) -> Result<(), FactsError> {
        let target = guid_text(reference.target);
        let referrer = guid_text(reference.referrer);
        let slot = self
            .next_ordinal
            .entry((reference.referrer_kind, referrer.clone(), reference.role))
            .or_insert(0);
        let ordinal = *slot;
        *slot += 1;
        self.tx.execute(
            "INSERT INTO statement_reference(target_kind, target_guid, referrer_kind, referrer_guid, role, ordinal, parser_effect) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                reference.target_kind,
                target,
                reference.referrer_kind,
                referrer,
                reference.role,
                ordinal,
                reference.effect
            ],
        )?;
        Ok(())
    }

    /// The effect of a reference made by `owner`: the compiler's fixed effect when it gave one.
    fn effect(
        &self,
        owner: &InventoryKey,
        target_kind: &'static str,
        target: &str,
        fixed: Option<&'static str>,
    ) -> &'static str {
        fixed.unwrap_or_else(|| {
            owner_effect(self.decisions, owner, self.known.has(target_kind, target))
        })
    }

    /// A reference whose effect follows its owner's compile state, with `unresolved` for dangling targets.
    fn owned(
        &mut self,
        owner: &InventoryKey,
        target_kind: &'static str,
        target: &str,
        referrer_kind: &'static str,
        referrer: &str,
        role: &'static str,
    ) -> Result<(), FactsError> {
        let effect = self.effect(owner, target_kind, target, None);
        self.add(Reference {
            target_kind,
            target,
            referrer_kind,
            referrer,
            role,
            effect,
        })
    }

    fn allomorphs(
        &mut self,
        snapshot: &Snapshot,
        resolutions: &[EnvironmentResolution],
        gate_outcomes: &[AllomorphGateOutcome],
    ) -> Result<(), FactsError> {
        let edges = EnvironmentEdges::new(snapshot, resolutions);
        let gates = GateEffects::new(gate_outcomes);
        for entry in &snapshot.lexicon.entries {
            for allomorph in &entry.allomorphs {
                let guid = allomorph.guid.as_str();
                let owner = InventoryKey::object(InventoryKind::Allomorph, guid);
                for (role, environments) in [
                    ("phone_env", &allomorph.environments),
                    ("position_env", &allomorph.positions),
                ] {
                    let consumed = role == "phone_env" || reads_positions(allomorph);
                    for environment in environments {
                        let edge = edges.outcome(guid, environment, consumed, self.decisions)?;
                        self.add(Reference {
                            target_kind: "environment",
                            target: environment,
                            referrer_kind: "allomorph",
                            referrer: guid,
                            role,
                            effect: edge_effect(edge.result)?,
                        })?;
                    }
                }
                for (index, class) in allomorph.inflection_classes.iter().enumerate() {
                    let (effect, _) =
                        gates.effect(guid, AllomorphGateKind::InflectionClass, index as u32);
                    self.add(Reference {
                        target_kind: "inflectionClass",
                        target: class,
                        referrer_kind: "allomorph",
                        referrer: guid,
                        role: "inflection_class",
                        effect,
                    })?;
                }
                if let Some(category) = &allomorph.ms_env_part_of_speech {
                    self.add(Reference {
                        target_kind: "category",
                        target: category,
                        referrer_kind: "allomorph",
                        referrer: guid,
                        role: "required_category",
                        effect: MS_ENV_PART_OF_SPEECH_EFFECT,
                    })?;
                }
                if let Some(stem_name) = &allomorph.stem_name {
                    let (effect, _) = gates.effect(guid, AllomorphGateKind::StemName, 0);
                    self.add(Reference {
                        target_kind: "stemName",
                        target: stem_name,
                        referrer_kind: "allomorph",
                        referrer: guid,
                        role: "stem_name",
                        effect,
                    })?;
                }
                if let Some(features) = &allomorph.ms_env_features {
                    // Mirrors the gate table: only an affix reads its required features.
                    let fixed = if form_class(allomorph) == "affix" {
                        gates.effect(guid, AllomorphGateKind::RequiredFeatures, 0).0
                    } else {
                        "not_attempted"
                    };
                    self.features(
                        &owner,
                        "allomorph",
                        guid,
                        "required_features",
                        features,
                        Some(fixed),
                    )?;
                }
                if let Some(process) = &allomorph.process {
                    for context in &process.input {
                        self.pattern(&owner, "affixProcess", guid, "process_input", context)?;
                    }
                    for mapping in &process.output {
                        match mapping {
                            RuleMapping::InsertNaturalClass { natural_class } => self.owned(
                                &owner,
                                "naturalClass",
                                natural_class,
                                "affixProcess",
                                guid,
                                "process_insert",
                            )?,
                            RuleMapping::ModifyFromInput { natural_class, .. } => self.owned(
                                &owner,
                                "naturalClass",
                                natural_class,
                                "affixProcess",
                                guid,
                                "process_modify",
                            )?,
                            RuleMapping::CopyFromInput { .. }
                            | RuleMapping::InsertSegments { .. } => {}
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Environment class tokens and literal segments; their effect follows whether an edge uses them.
    fn environments(
        &mut self,
        snapshot: &Snapshot,
        resolutions: &[EnvironmentResolution],
        grammar: Option<&pg_grammar::model::Grammar>,
    ) -> Result<(), FactsError> {
        let edges = EnvironmentEdges::new(snapshot, resolutions);
        let mut used = BTreeSet::new();
        for entry in &snapshot.lexicon.entries {
            for allomorph in &entry.allomorphs {
                let positions_read = reads_positions(allomorph);
                for (consumed, environments) in [
                    (true, &allomorph.environments),
                    (positions_read, &allomorph.positions),
                ] {
                    for environment in environments {
                        if edges
                            .outcome(&allomorph.guid, environment, consumed, self.decisions)?
                            .result
                            == "represented"
                        {
                            used.insert(guid_text(environment));
                        }
                    }
                }
            }
        }
        let resolved: BTreeMap<&str, &EnvironmentResolution> = resolutions
            .iter()
            .map(|resolution| (resolution.environment_guid.as_str(), resolution))
            .collect();
        for environment in &snapshot.phonology.environments {
            let Some(resolution) = resolved.get(environment.guid.as_str()) else {
                continue;
            };
            let effect = environment_effect(
                resolution.status == EnvironmentResolutionStatus::Valid,
                used.contains(&guid_text(&environment.guid)),
            );
            for token in &resolution.class_tokens {
                let Some(class) = &token.natural_class_guid else {
                    continue;
                };
                self.add(Reference {
                    target_kind: "naturalClass",
                    target: class,
                    referrer_kind: "environment",
                    referrer: &environment.guid,
                    role: "env_token",
                    effect,
                })?;
            }
        }
        for literal in environment_literals(snapshot, resolutions, grammar)? {
            let effect = environment_effect(
                literal.valid,
                used.contains(&guid_text(&literal.environment)),
            );
            let (target_kind, target) = match (&literal.phoneme, &literal.boundary) {
                (Some(phoneme), _) => ("phoneme", phoneme),
                (None, Some(boundary)) => ("boundary", boundary),
                (None, None) => continue,
            };
            self.add(Reference {
                target_kind,
                target,
                referrer_kind: "environment",
                referrer: &literal.environment,
                role: "env_segment",
                effect,
            })?;
        }
        Ok(())
    }

    /// Segment-list membership, and the features of phonemes and feature-defined classes.
    fn classes(
        &mut self,
        snapshot: &Snapshot,
        grammar: Option<&pg_grammar::model::Grammar>,
    ) -> Result<(), FactsError> {
        for (class, phoneme) in feature_class_phonemes(snapshot, grammar)? {
            let owner = InventoryKey::object(InventoryKind::NaturalClass, class.as_str());
            self.owned(
                &owner,
                "phoneme",
                &phoneme,
                "naturalClass",
                &class,
                "class_effective_member",
            )?;
        }
        for class in &snapshot.phonology.natural_classes {
            let guid = natural_class_guid(class);
            let owner = InventoryKey::object(InventoryKind::NaturalClass, guid);
            match class {
                NaturalClass::Segments { phonemes, .. } => {
                    for phoneme in phonemes {
                        self.owned(
                            &owner,
                            "phoneme",
                            phoneme,
                            "naturalClass",
                            guid,
                            "class_member",
                        )?;
                    }
                }
                NaturalClass::Features { features, .. } => {
                    self.features(&owner, "naturalClass", guid, "features", features, None)?;
                }
            }
        }
        for phoneme in &snapshot.phonology.phonemes {
            if let Some(features) = &phoneme.features {
                let owner = InventoryKey::object(InventoryKind::Phoneme, phoneme.guid.as_str());
                self.features(&owner, "phoneme", &phoneme.guid, "features", features, None)?;
            }
        }
        Ok(())
    }

    /// Phonemes, boundaries and classes in each compiled allomorph's form, from the compiler's own segments.
    fn form_segments(&mut self, compiled: &CompiledInputs<'_>) -> Result<(), FactsError> {
        let mut sources: BTreeMap<u32, Vec<String>> = BTreeMap::new();
        for mapping in compiled.mappings {
            if mapping.source_kind == "allomorph" {
                if let Some(source) = &mapping.source_guid {
                    sources
                        .entry(mapping.output_id)
                        .or_default()
                        .push(source.clone());
                }
            }
        }
        for (output_id, segments) in compiled.form_segments {
            for allomorph in sources.get(output_id).into_iter().flatten() {
                let owner = InventoryKey::object(InventoryKind::Allomorph, allomorph.as_str());
                for segment in segments {
                    let target = match (
                        &segment.phoneme_guid,
                        &segment.boundary_guid,
                        &segment.natural_class_guid,
                    ) {
                        (Some(phoneme), _, _) => ("phoneme", phoneme),
                        (None, Some(boundary), _) => ("boundary", boundary),
                        (None, None, Some(class)) => ("naturalClass", class),
                        (None, None, None) => continue,
                    };
                    self.owned(
                        &owner,
                        target.0,
                        target.1,
                        "allomorph",
                        allomorph,
                        "form_segment",
                    )?;
                }
            }
        }
        Ok(())
    }

    /// Every feature and closed value a feature structure names; complex values are walked in turn.
    fn features(
        &mut self,
        owner: &InventoryKey,
        referrer_kind: &'static str,
        referrer: &str,
        role: &'static str,
        structure: &FeatureStructure,
        fixed: Option<&'static str>,
    ) -> Result<(), FactsError> {
        for value in &structure.values {
            let effect = self.effect(owner, "feature", &value.feature, fixed);
            self.add(Reference {
                target_kind: "feature",
                target: &value.feature,
                referrer_kind,
                referrer,
                role,
                effect,
            })?;
            match &value.value {
                FeatureValueKind::Closed { value: closed } => {
                    let effect = self.effect(owner, "featureValue", closed, fixed);
                    self.add(Reference {
                        target_kind: "featureValue",
                        target: closed,
                        referrer_kind,
                        referrer,
                        role,
                        effect,
                    })?;
                }
                FeatureValueKind::Complex { value: child } => {
                    self.features(owner, referrer_kind, referrer, role, child, fixed)?;
                }
            }
        }
        Ok(())
    }

    fn msas(&mut self, snapshot: &Snapshot) -> Result<(), FactsError> {
        for entry in &snapshot.lexicon.entries {
            for msa in &entry.msas {
                let guid = msa.guid();
                let owner = InventoryKey::object(InventoryKind::Msa, guid);
                let mut features: Vec<(&FeatureStructure, &'static str)> = Vec::new();
                let links: Vec<(&'static str, &str, &'static str)> = match msa {
                    Msa::Stem {
                        part_of_speech,
                        inflection_class,
                        exception_features,
                        from_parts_of_speech,
                        slots,
                        features: msa_features,
                        ..
                    } => {
                        features.extend(msa_features.iter().map(|value| (value, "features")));
                        let mut links = Vec::new();
                        links.extend(optional(part_of_speech, "category", "pos"));
                        links.extend(many(from_parts_of_speech, "category", "clitic_from"));
                        links.extend(optional(inflection_class, "inflectionClass", "class"));
                        links.extend(many(exception_features, "exceptionFeature", "required"));
                        links.extend(many(slots, "slot", "clitic_slot"));
                        links
                    }
                    Msa::Inflectional {
                        part_of_speech,
                        slots,
                        exception_features,
                        features: msa_features,
                        ..
                    } => {
                        features.extend(msa_features.iter().map(|value| (value, "features")));
                        let mut links = Vec::new();
                        links.extend(optional(part_of_speech, "category", "pos"));
                        links.extend(many(slots, "slot", "slot"));
                        links.extend(many(exception_features, "exceptionFeature", "required"));
                        links
                    }
                    Msa::Derivational {
                        from_part_of_speech,
                        to_part_of_speech,
                        from_inflection_class,
                        to_inflection_class,
                        from_exception_features,
                        to_exception_features,
                        from_stem_name,
                        from_features,
                        to_features,
                        ..
                    } => {
                        features.extend(from_features.iter().map(|value| (value, "from_features")));
                        features.extend(to_features.iter().map(|value| (value, "to_features")));
                        let mut links = Vec::new();
                        links.extend(optional(from_part_of_speech, "category", "from_pos"));
                        links.extend(optional(to_part_of_speech, "category", "to_pos"));
                        links.extend(optional(
                            from_inflection_class,
                            "inflectionClass",
                            "from_class",
                        ));
                        links.extend(optional(to_inflection_class, "inflectionClass", "to_class"));
                        links.extend(optional(from_stem_name, "stemName", "from_stem_name"));
                        links.extend(many(
                            from_exception_features,
                            "exceptionFeature",
                            "from_required",
                        ));
                        links.extend(many(
                            to_exception_features,
                            "exceptionFeature",
                            "to_required",
                        ));
                        links
                    }
                    Msa::Unclassified { part_of_speech, .. } => {
                        optional(part_of_speech, "category", "pos")
                    }
                };
                for (target_kind, target, role) in links {
                    self.owned(&owner, target_kind, target, "msa", guid, role)?;
                }
                for (structure, role) in features {
                    self.features(&owner, "msa", guid, role, structure, None)?;
                }
            }
        }
        Ok(())
    }

    /// Sense MSA links, and variant components and types; a component is an entry or a sense.
    fn entries(&mut self, snapshot: &Snapshot) -> Result<(), FactsError> {
        let entries: BTreeSet<String> = snapshot
            .lexicon
            .entries
            .iter()
            .map(|entry| guid_text(&entry.guid))
            .collect();
        let senses: BTreeSet<String> = snapshot
            .lexicon
            .entries
            .iter()
            .flat_map(|entry| entry.senses.iter().map(|sense| guid_text(&sense.guid)))
            .collect();
        for entry in &snapshot.lexicon.entries {
            let entry_owner = InventoryKey::object(InventoryKind::Entry, entry.guid.as_str());
            for sense in &entry.senses {
                if let Some(msa) = &sense.msa {
                    let owner = InventoryKey::object(InventoryKind::Sense, sense.guid.as_str());
                    self.owned(&owner, "msa", msa, "sense", &sense.guid, "msa")?;
                }
            }
            for entry_ref in &entry.entry_refs {
                let EntryRef::Variant {
                    component_lexemes,
                    variant_entry_types,
                    ..
                } = entry_ref
                else {
                    continue;
                };
                for component in component_lexemes {
                    let (target_kind, effect) = if entries.contains(&guid_text(component)) {
                        ("entry", self.effect(&entry_owner, "entry", component, None))
                    } else if senses.contains(&guid_text(component)) {
                        ("sense", self.effect(&entry_owner, "sense", component, None))
                    } else {
                        ("entry", "unresolved")
                    };
                    self.add(Reference {
                        target_kind,
                        target: component,
                        referrer_kind: "entry",
                        referrer: &entry.guid,
                        role: "variant_component",
                        effect,
                    })?;
                }
                for variant_type in variant_entry_types {
                    // Only an irregular inflection type is read by a compiler path; other types are not.
                    let (target_kind, effect) = if self.known.has("inflType", variant_type) {
                        (
                            "inflType",
                            self.effect(&entry_owner, "inflType", variant_type, None),
                        )
                    } else {
                        ("variantType", "not_attempted")
                    };
                    self.add(Reference {
                        target_kind,
                        target: variant_type,
                        referrer_kind: "entry",
                        referrer: &entry.guid,
                        role: "variant_type",
                        effect,
                    })?;
                }
            }
        }
        Ok(())
    }

    /// Category defaults, inflectable features, and the templates each category owns.
    fn categories(&mut self, snapshot: &Snapshot) -> Result<(), FactsError> {
        for part in all_parts_of_speech(&snapshot.morphology.parts_of_speech) {
            let owner = InventoryKey::object(InventoryKind::PartOfSpeech, part.guid.as_str());
            if let Some(class) = &part.default_inflection_class {
                self.owned(
                    &owner,
                    "inflectionClass",
                    class,
                    "category",
                    &part.guid,
                    "default_class",
                )?;
            }
            for feature in &part.inflectable_features {
                self.owned(
                    &owner,
                    "feature",
                    feature,
                    "category",
                    &part.guid,
                    "inflectable_feature",
                )?;
            }
            for template in &part.affix_templates {
                let template_owner =
                    InventoryKey::object(InventoryKind::Template, template.guid.as_str());
                self.owned(
                    &template_owner,
                    "template",
                    &template.guid,
                    "category",
                    &part.guid,
                    "template",
                )?;
                self.template_slots(&template_owner, template)?;
            }
        }
        Ok(())
    }

    fn template_slots(
        &mut self,
        owner: &InventoryKey,
        template: &AffixTemplate,
    ) -> Result<(), FactsError> {
        for (role, slots) in [
            ("prefix_slot", &template.prefix_slots),
            ("suffix_slot", &template.suffix_slots),
        ] {
            for slot in slots {
                self.owned(owner, "slot", slot, "template", &template.guid, role)?;
            }
        }
        Ok(())
    }

    fn infl_types(&mut self, snapshot: &Snapshot) -> Result<(), FactsError> {
        for infl_type in &snapshot.morphology.lex_entry_infl_types {
            let owner = InventoryKey::object(InventoryKind::RuleFeature, infl_type.guid.as_str());
            for slot in &infl_type.slots {
                self.owned(&owner, "slot", slot, "inflType", &infl_type.guid, "slot")?;
            }
            if let Some(features) = &infl_type.inflection_features {
                self.features(
                    &owner,
                    "inflType",
                    &infl_type.guid,
                    "features",
                    features,
                    None,
                )?;
            }
        }
        Ok(())
    }

    fn adhoc_prohibitions(&mut self, snapshot: &Snapshot) -> Result<(), FactsError> {
        for prohibition in &snapshot.morphology.adhoc_prohibitions {
            let (guid, primary, others, target_kind, owner_kind) = match prohibition {
                AdhocProhibition::Allomorph {
                    guid,
                    primary,
                    others,
                    ..
                } => (
                    guid,
                    primary,
                    others,
                    "allomorph",
                    InventoryKind::AllomorphCoOccurrence,
                ),
                AdhocProhibition::Morpheme {
                    guid,
                    primary,
                    others,
                    ..
                } => (
                    guid,
                    primary,
                    others,
                    "msa",
                    InventoryKind::MorphemeCoOccurrence,
                ),
            };
            let owner = InventoryKey::object(owner_kind, guid.as_str());
            self.owned(
                &owner,
                target_kind,
                primary,
                "adhocProhibition",
                guid,
                "primary",
            )?;
            for other in others {
                self.owned(
                    &owner,
                    target_kind,
                    other,
                    "adhocProhibition",
                    guid,
                    "other",
                )?;
            }
        }
        Ok(())
    }

    fn compound_rules(&mut self, snapshot: &Snapshot) -> Result<(), FactsError> {
        for rule in &snapshot.morphology.compound_rules {
            let guid = rule.guid();
            let owner = InventoryKey::object(InventoryKind::CompoundRule, guid);
            let (left, right, outcome) = match rule {
                CompoundRule::Endocentric {
                    left,
                    right,
                    overriding,
                    ..
                } => (left, right, overriding),
                CompoundRule::Exocentric {
                    left, right, to, ..
                } => (left, right, to),
            };
            self.constituent(&owner, guid, left, "left_category", "left_exception")?;
            self.constituent(&owner, guid, right, "right_category", "right_exception")?;
            self.outcome(&owner, guid, outcome)?;
        }
        Ok(())
    }

    fn constituent(
        &mut self,
        owner: &InventoryKey,
        rule: &str,
        requirement: &CompoundConstituentRequirement,
        category_role: &'static str,
        exception_role: &'static str,
    ) -> Result<(), FactsError> {
        if let Some(category) = &requirement.part_of_speech {
            self.owned(
                owner,
                "category",
                category,
                "compoundRule",
                rule,
                category_role,
            )?;
        }
        for feature in &requirement.exception_features {
            self.owned(
                owner,
                "exceptionFeature",
                feature,
                "compoundRule",
                rule,
                exception_role,
            )?;
        }
        Ok(())
    }

    fn outcome(
        &mut self,
        owner: &InventoryKey,
        rule: &str,
        outcome: &CompoundOutcome,
    ) -> Result<(), FactsError> {
        if let Some(category) = &outcome.part_of_speech {
            self.owned(
                owner,
                "category",
                category,
                "compoundRule",
                rule,
                "outcome_category",
            )?;
        }
        if let Some(class) = &outcome.inflection_class {
            self.owned(
                owner,
                "inflectionClass",
                class,
                "compoundRule",
                rule,
                "outcome_class",
            )?;
        }
        Ok(())
    }

    fn phonological_rules(&mut self, snapshot: &Snapshot) -> Result<(), FactsError> {
        for rule in &snapshot.phonology.rules {
            match rule {
                PhonologicalRule::Rewrite(rule) => {
                    let owner =
                        InventoryKey::object(InventoryKind::PhonologicalRule, rule.guid.as_str());
                    for variable in &rule.feature_constraint_variables {
                        self.owned(
                            &owner,
                            "featureConstraint",
                            variable,
                            "phonologicalRule",
                            &rule.guid,
                            "rule_variable",
                        )?;
                    }
                    for context in &rule.structural_description {
                        self.pattern(
                            &owner,
                            "phonologicalRule",
                            &rule.guid,
                            "rewrite_lhs",
                            context,
                        )?;
                    }
                    for rhs in &rule.right_hand_sides {
                        self.rewrite_rhs(snapshot, &owner, &rule.guid, rhs)?;
                    }
                }
                PhonologicalRule::Metathesis(rule) => {
                    let owner =
                        InventoryKey::object(InventoryKind::PhonologicalRule, rule.guid.as_str());
                    for context in &rule.structural_description {
                        self.pattern(
                            &owner,
                            "phonologicalRule",
                            &rule.guid,
                            "metathesis_pattern",
                            context,
                        )?;
                    }
                }
            }
        }
        Ok(())
    }

    fn rewrite_rhs(
        &mut self,
        snapshot: &Snapshot,
        owner: &InventoryKey,
        rule: &str,
        rhs: &RewriteRhs,
    ) -> Result<(), FactsError> {
        for context in &rhs.structural_change {
            self.pattern(owner, "phonologicalRule", rule, "rewrite_sc", context)?;
        }
        if let Some(context) = &rhs.left_context {
            self.pattern(
                owner,
                "phonologicalRule",
                rule,
                "rewrite_left_context",
                context,
            )?;
        }
        if let Some(context) = &rhs.right_context {
            self.pattern(
                owner,
                "phonologicalRule",
                rule,
                "rewrite_right_context",
                context,
            )?;
        }
        for category in &rhs.required_parts_of_speech {
            self.owned(
                owner,
                "category",
                category,
                "phonologicalRule",
                rule,
                "rewrite_pos",
            )?;
        }
        for (features, role) in [
            (&rhs.required_rule_features, "rule_feature_required"),
            (&rhs.excluded_rule_features, "rule_feature_excluded"),
        ] {
            for feature in features {
                // A feature naming no class, exception feature or value keeps its row as `mprFeature`.
                let kind = rule_feature_target_kind(snapshot, feature);
                let effect = owner_effect(self.decisions, owner, kind != "unresolved");
                self.add(Reference {
                    target_kind: if kind == "unresolved" {
                        "mprFeature"
                    } else {
                        kind
                    },
                    target: feature,
                    referrer_kind: "phonologicalRule",
                    referrer: rule,
                    role,
                    effect,
                })?;
            }
        }
        Ok(())
    }

    /// Natural classes, phonemes, boundaries and alpha constraints a pattern names.
    fn pattern(
        &mut self,
        owner: &InventoryKey,
        referrer_kind: &'static str,
        referrer: &str,
        role: &'static str,
        context: &PhonContext,
    ) -> Result<(), FactsError> {
        match context {
            PhonContext::Sequence { members } => {
                for member in members {
                    self.pattern(owner, referrer_kind, referrer, role, member)?;
                }
            }
            PhonContext::Iteration { member, .. } => {
                self.pattern(owner, referrer_kind, referrer, role, member)?;
            }
            PhonContext::Segment { phoneme } => {
                self.owned(owner, "phoneme", phoneme, referrer_kind, referrer, role)?;
            }
            PhonContext::NaturalClass {
                natural_class,
                plus_variables,
                minus_variables,
            } => {
                self.owned(
                    owner,
                    "naturalClass",
                    natural_class,
                    referrer_kind,
                    referrer,
                    role,
                )?;
                for variable in plus_variables.iter().chain(minus_variables) {
                    self.owned(
                        owner,
                        "featureConstraint",
                        variable,
                        referrer_kind,
                        referrer,
                        "pattern_variable",
                    )?;
                }
            }
            PhonContext::Boundary { marker } => {
                self.owned(owner, "boundary", marker, referrer_kind, referrer, role)?;
            }
            PhonContext::WordBoundary | PhonContext::Variable => {}
        }
        Ok(())
    }

    /// The feature each alpha constraint names.
    fn feature_constraints(&mut self, snapshot: &Snapshot) -> Result<(), FactsError> {
        for constraint in &snapshot.phonology.feature_constraints {
            let owner =
                InventoryKey::object(InventoryKind::FeatureConstraint, constraint.guid.as_str());
            self.owned(
                &owner,
                "feature",
                &constraint.feature,
                "featureConstraint",
                &constraint.guid,
                "constraint_feature",
            )?;
        }
        Ok(())
    }
}

/// `owner_not_loaded` unless the compiler loaded the owner; then `applied`, or `unresolved` if dangling.
fn owner_effect(decisions: &[LoadDecision], owner: &InventoryKey, resolves: bool) -> &'static str {
    match inventory_owner_loaded(owner, decisions) {
        Some(true) if resolves => "applied",
        Some(true) => "unresolved",
        _ => "owner_not_loaded",
    }
}

/// An environment's references: `unresolved` when invalid, `applied` when an edge uses it, else `not_attempted`.
fn environment_effect(valid: bool, used: bool) -> &'static str {
    match (valid, used) {
        (false, _) => "unresolved",
        (true, true) => "applied",
        (true, false) => "not_attempted",
    }
}

/// The statement effect of one environment edge, from the usage outcome the edge table records.
fn edge_effect(result: &str) -> Result<&'static str, FactsError> {
    Ok(match result {
        "represented" => "applied",
        "invalid" | "unresolved" => "unresolved",
        "owner_not_loaded" => "owner_not_loaded",
        "not_attempted" => "not_attempted",
        other => {
            return Err(FactsError::Serialization(format!(
                "unknown environment usage result {other}"
            )))
        }
    })
}

fn optional<'a>(
    guid: &'a Option<String>,
    target_kind: &'static str,
    role: &'static str,
) -> Vec<(&'static str, &'a str, &'static str)> {
    guid.iter()
        .map(|guid| (target_kind, guid.as_str(), role))
        .collect()
}

fn many<'a>(
    guids: &'a [String],
    target_kind: &'static str,
    role: &'static str,
) -> Vec<(&'static str, &'a str, &'static str)> {
    guids
        .iter()
        .map(|guid| (target_kind, guid.as_str(), role))
        .collect()
}
