//! Natural classes, plus the synthetic "Any" class HCLoader always creates
//! (`TryLoadNaturalClass`, HCLoader.cs:2788-2829; `m_any`, HCLoader.cs:200-202).

use hashbrown::HashMap;

use hc_featstruct::SymbolBits;

use pg_snapshot::phonology::NaturalClass as SnapNaturalClass;
use pg_snapshot::Snapshot;

use crate::chardef::CharDefId;
use crate::featsys::{FlatIndex, PhonFeatureSystem, TYPE_SEGMENT_SYMBOL};
use crate::model::{NatClassId, NaturalClass, NaturalClassKind};

pub(crate) struct NatClassBuild {
    pub defs: Vec<NaturalClass>,
    pub by_guid: HashMap<String, NatClassId>,
    /// By `<Name>`/abbreviation text — the key environment strings' `[Abbr]` notation and lexical
    /// patterns resolve against (`m_naturalClassLookup`, keyed by `Abbreviation`, HCLoader.cs:88-90).
    pub by_name: HashMap<String, NatClassId>,
    pub any: NatClassId,
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

    for nc in &snapshot.phonology.natural_classes {
        match nc {
            SnapNaturalClass::Segments { guid, name, phonemes } => {
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
                defs.push(NaturalClass {
                    xml_id: guid.clone(),
                    name: Some(name.clone()),
                    kind: NaturalClassKind::Segments(resolved),
                });
            }
            SnapNaturalClass::Features { guid, name, features } => {
                let pairs = feature_constraint_pairs(features, phon, warnings, guid);
                let id = NatClassId(defs.len() as u32);
                by_guid.insert(guid.clone(), id);
                by_name.entry(name.clone()).or_insert(id);
                defs.push(NaturalClass {
                    xml_id: guid.clone(),
                    name: Some(name.clone()),
                    kind: NaturalClassKind::Feature(pairs),
                });
            }
        }
    }

    // Synthetic "Any" natural class (`m_any`, HCLoader.cs:200-202): matches any segment, i.e. no
    // constraint beyond the mandatory `Type=Segment` every `FeatureNaturalClass` carries.
    let any_id = NatClassId(defs.len() as u32);
    defs.push(NaturalClass {
        xml_id: "__any__".to_string(),
        name: Some("Any".to_string()),
        kind: NaturalClassKind::Feature(vec![(phon.type_flat(), SymbolBits(1u64 << TYPE_SEGMENT_SYMBOL))]),
    });

    NatClassBuild {
        defs,
        by_guid,
        by_name,
        any: any_id,
    }
}

/// Sparse `(lane, symbols)` constraints from a `FeatureStructure` against the phonological
/// feature system, unioning repeats and unconditionally requiring `Type=Segment` — mirrors
/// `crate::load::load_phon_constraints` (see plan §13.1 Tier-1 #1 there for why `Type` is always
/// injected).
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
