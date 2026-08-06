//! Natural classes, plus the synthetic "Any" class HCLoader always creates; `compact_to_referenced` keeps every named class, the synthetic "Any" class, the last-declared unnamed class, and every other class actually referenced structurally, matching HCLoader's own mixed eager/lazy population rather than `pg-fwdata`'s eager extraction of every declared class.
//! See docs/research/pg-grammar-natclass-compaction-design-notes.md for HCLoader's two-mechanism population rule, the confirmed Amharic divergence, and the one known gap in the reference sweep.

use std::collections::HashMap as StdHashMap;

use hashbrown::HashMap;

use pg_featstruct::SymbolBits;

use pg_snapshot::phonology::NaturalClass as SnapNaturalClass;
use pg_snapshot::Snapshot;

use crate::chardef::CharDefId;
use crate::featsys::{FlatIndex, PhonFeatureSystem, TYPE_SEGMENT_SYMBOL};
use crate::model::{
    AffixAllomorphDef, EnvironmentDef, Grammar, MorphRuleDef, NatClassId, NaturalClass,
    NaturalClassKind, OutputAction, Pattern, PatternNode, PhonRuleDef,
};

pub(crate) struct NatClassBuild {
    pub defs: Vec<NaturalClass>,
    pub by_guid: HashMap<String, NatClassId>,
    /// By `<Name>`/abbreviation text -- the key environment strings' `[Abbr]` notation and lexical patterns resolve against.
    pub by_name: HashMap<String, NatClassId>,
    pub any: NatClassId,
    /// The last-declared natural class (in snapshot order) whose name/abbreviation is empty, if any (see module doc for why it survives regardless of reference).
    pub last_unnamed: Option<NatClassId>,
}

pub(crate) fn build(
    snapshot: &Snapshot,
    phon: &PhonFeatureSystem,
    phoneme_of: &HashMap<String, CharDefId>,
    warnings: &mut Vec<String>,
) -> NatClassBuild {
    let mut defs = Vec::new();
    let mut by_guid = HashMap::new();
    let mut by_name = HashMap::new();
    let mut last_unnamed = None;

    for nc in &snapshot.phonology.natural_classes {
        match nc {
            SnapNaturalClass::Segments {
                guid,
                name,
                phonemes,
            } => {
                let mut resolved = Vec::with_capacity(phonemes.len());
                let mut ok = true;
                for p in phonemes {
                    match phoneme_of.get(p) {
                        Some(&cd) => resolved.push(cd),
                        None => {
                            warnings.push(format!(
                                "natural class {guid:?} ({name:?}): member phoneme {p:?} does not \
                                 resolve; class skipped"
                            ));
                            ok = false;
                            break;
                        }
                    }
                }
                if !ok {
                    continue;
                }
                let id = NatClassId(defs.len() as u32);
                by_guid.insert(guid.clone(), id);
                by_name.entry(name.clone()).or_insert(id);
                if name.is_empty() {
                    last_unnamed = Some(id);
                }
                defs.push(NaturalClass {
                    xml_id: guid.clone(),
                    name: Some(name.clone()),
                    kind: NaturalClassKind::Segments(resolved),
                });
            }
            SnapNaturalClass::Features {
                guid,
                name,
                features,
            } => {
                let pairs = feature_constraint_pairs(features, phon, warnings, guid);
                let id = NatClassId(defs.len() as u32);
                by_guid.insert(guid.clone(), id);
                by_name.entry(name.clone()).or_insert(id);
                if name.is_empty() {
                    last_unnamed = Some(id);
                }
                defs.push(NaturalClass {
                    xml_id: guid.clone(),
                    name: Some(name.clone()),
                    kind: NaturalClassKind::Feature(pairs),
                });
            }
        }
    }

    // Synthetic "Any" natural class: matches any segment, no constraint beyond the mandatory Type=Segment every FeatureNaturalClass carries.
    let any_id = NatClassId(defs.len() as u32);
    defs.push(NaturalClass {
        xml_id: "__any__".to_string(),
        name: Some("Any".to_string()),
        kind: NaturalClassKind::Feature(vec![(
            phon.type_flat(),
            SymbolBits(1u64 << TYPE_SEGMENT_SYMBOL),
        )]),
    });

    NatClassBuild {
        defs,
        by_guid,
        by_name,
        any: any_id,
        last_unnamed,
    }
}

/// Sparse `(lane, symbols)` constraints from a `FeatureStructure` against the phonological feature system, unioning repeats and unconditionally requiring `Type=Segment`; mirrors `crate::load::load_phon_constraints`.
fn feature_constraint_pairs(
    fs: &pg_snapshot::feature::FeatureStructure,
    phon: &PhonFeatureSystem,
    warnings: &mut Vec<String>,
    nc_guid: &str,
) -> Vec<(FlatIndex, SymbolBits)> {
    let mut map: HashMap<u32, u64> = HashMap::new();
    for v in &fs.values {
        let Some(flat) = phon.flat_index(&v.feature) else {
            warnings.push(format!(
                "natural class {nc_guid:?}: unknown phonological feature {:?}; value ignored",
                v.feature
            ));
            continue;
        };
        match &v.value {
            pg_snapshot::feature::FeatureValueKind::Closed { value } => {
                let Some(idx) = phon.symbol_index(flat, value) else {
                    warnings.push(format!(
                        "natural class {nc_guid:?}: unknown feature value {value:?}; ignored"
                    ));
                    continue;
                };
                *map.entry(flat.0).or_insert(0) |= 1u64 << idx;
            }
            pg_snapshot::feature::FeatureValueKind::Complex { .. } => {
                warnings.push(format!(
                    "natural class {nc_guid:?}: complex feature value not supported; ignored"
                ));
            }
        }
    }
    map.insert(phon.type_flat().0, 1u64 << TYPE_SEGMENT_SYMBOL);
    let mut out: Vec<(FlatIndex, SymbolBits)> = map
        .into_iter()
        .map(|(k, v)| (FlatIndex(k), SymbolBits(v)))
        .collect();
    out.sort_by_key(|(f, _)| f.0);
    out
}

// Post-hoc reachability compaction (see this module's top doc).

fn walk_pattern_mut(p: &mut Pattern, f: &mut dyn FnMut(&mut NatClassId)) {
    for n in &mut p.nodes {
        walk_node_mut(n, f);
    }
}

fn walk_node_mut(n: &mut PatternNode, f: &mut dyn FnMut(&mut NatClassId)) {
    match n {
        PatternNode::Context(sc) => f(&mut sc.nat_class),
        PatternNode::Quantifier { children, .. } => {
            for c in children {
                walk_node_mut(c, f);
            }
        }
        PatternNode::CharDef(_) | PatternNode::Segments { .. } | PatternNode::Anchor(_) => {}
    }
}

fn walk_env_mut(e: &mut EnvironmentDef, f: &mut dyn FnMut(&mut NatClassId)) {
    if let Some(p) = &mut e.left {
        walk_pattern_mut(p, f);
    }
    if let Some(p) = &mut e.right {
        walk_pattern_mut(p, f);
    }
}

fn walk_envs_mut(envs: &mut [EnvironmentDef], f: &mut dyn FnMut(&mut NatClassId)) {
    for e in envs {
        walk_env_mut(e, f);
    }
}

fn walk_output_mut(a: &mut OutputAction, f: &mut dyn FnMut(&mut NatClassId)) {
    match a {
        OutputAction::Modify(_, sc) | OutputAction::InsertContext(sc) => f(&mut sc.nat_class),
        OutputAction::Copy(_) | OutputAction::InsertSegments { .. } => {}
    }
}

fn walk_affix_allos_mut(allos: &mut [AffixAllomorphDef], f: &mut dyn FnMut(&mut NatClassId)) {
    for a in allos {
        walk_envs_mut(&mut a.environments, f);
        for p in &mut a.lhs {
            walk_pattern_mut(p, f);
        }
        for o in &mut a.rhs {
            walk_output_mut(o, f);
        }
    }
}

/// Visits every `NatClassId` occurrence anywhere in `grammar`'s structural data; used both to collect the referenced set and to rewrite it in place, one traversal, so the two passes can never drift apart on which fields count.
fn walk_all_natclass_ids_mut(grammar: &mut Grammar, f: &mut dyn FnMut(&mut NatClassId)) {
    for p in &mut grammar.prules {
        match p {
            PhonRuleDef::Rewrite(r) => {
                walk_pattern_mut(&mut r.lhs, f);
                for s in &mut r.subrules {
                    walk_pattern_mut(&mut s.rhs, f);
                    if let Some(p) = &mut s.left_env {
                        walk_pattern_mut(p, f);
                    }
                    if let Some(p) = &mut s.right_env {
                        walk_pattern_mut(p, f);
                    }
                }
            }
            PhonRuleDef::Metathesis(m) => walk_pattern_mut(&mut m.pattern, f),
        }
    }
    for r in &mut grammar.mrules {
        match r {
            MorphRuleDef::AffixProcess(d) => walk_affix_allos_mut(&mut d.allomorphs, f),
            MorphRuleDef::Realizational(d) => walk_affix_allos_mut(&mut d.allomorphs, f),
            MorphRuleDef::Compounding(d) => {
                for s in &mut d.subrules {
                    for p in &mut s.head_lhs {
                        walk_pattern_mut(p, f);
                    }
                    for p in &mut s.non_head_lhs {
                        walk_pattern_mut(p, f);
                    }
                    for o in &mut s.rhs {
                        walk_output_mut(o, f);
                    }
                }
            }
        }
    }
    for e in &mut grammar.entries {
        for a in &mut e.allomorphs {
            walk_envs_mut(&mut a.environments, f);
        }
    }
}

/// Drops every natural class in `grammar.natural_classes` that HCLoader would never load (module doc), remapping every surviving `NatClassId` to a dense index. Kept unconditionally: `any_nc`, `last_unnamed`, and every named class; everything else is kept only if `walk_all_natclass_ids_mut` finds a structural reference to it.
pub(crate) fn compact_to_referenced(
    grammar: &mut Grammar,
    any_nc: NatClassId,
    last_unnamed: Option<NatClassId>,
) {
    let mut used: hashbrown::HashSet<u32> = hashbrown::HashSet::new();
    used.insert(any_nc.0);
    if let Some(id) = last_unnamed {
        used.insert(id.0);
    }
    for (i, def) in grammar.natural_classes.iter().enumerate() {
        if def.name.as_deref().is_some_and(|n| !n.is_empty()) {
            used.insert(i as u32);
        }
    }
    walk_all_natclass_ids_mut(grammar, &mut |id| {
        used.insert(id.0);
    });

    let old_defs = std::mem::take(&mut grammar.natural_classes);
    let mut old_to_new: StdHashMap<u32, u32> = StdHashMap::with_capacity(used.len());
    let mut new_defs = Vec::with_capacity(used.len());
    for (old_id, def) in old_defs.into_iter().enumerate() {
        if used.contains(&(old_id as u32)) {
            old_to_new.insert(old_id as u32, new_defs.len() as u32);
            new_defs.push(def);
        }
    }
    grammar.natural_classes = new_defs;

    walk_all_natclass_ids_mut(grammar, &mut |id| {
        id.0 = *old_to_new
            .get(&id.0)
            .expect("nat class id referenced but not marked used -- compaction sweep bug");
    });
}
