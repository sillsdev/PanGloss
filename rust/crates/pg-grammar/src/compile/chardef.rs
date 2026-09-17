//! Character-definition table synthesis from the snapshot's phoneme/boundary inventory (`HCLoader.LoadCharacterDefinitionTable`); beyond the phonemes/boundary markers themselves, HCLoader always appends the "null" boundary and a standalone `.` boundary, with the user-authored morpheme-boundary marker looked up by representation afterward; dotted-circle stripping and space->`.` replacement both happen elsewhere and are not repeated here.

use hashbrown::HashMap;

use pg_snapshot::phonology::{BoundaryMarker, Phoneme};
use pg_snapshot::{InventoryKey, InventoryKind, IssueClass, SelectionRecorder, Snapshot};

use crate::chardef::{CharDefId, CharDefKind, CharDefTable, RawCharDef, RawFeatureValue};
use crate::featsys::PhonFeatureSystem;
use crate::nfd::nfd;
use crate::GrammarError;

use super::{inventory, issue_codes, ws_forms};

pub(crate) struct CharDefBuild {
    pub table: CharDefTable,
    pub phoneme_of: HashMap<String, CharDefId>,
    pub boundary_of: HashMap<String, CharDefId>,
    pub null_bdry: CharDefId,
    pub morph_bdry: CharDefId,
}

/// Every `RawCharDef` authored so far, before the table is built -- `substrate::complete` appends onto this.
pub(crate) struct RawCharDefBuild {
    pub raw_defs: Vec<RawCharDef>,
    pub seen_nfd: hashbrown::HashSet<String>,
    pub phoneme_of: HashMap<String, CharDefId>,
    pub boundary_of: HashMap<String, CharDefId>,
    pub null_bdry: CharDefId,
}

/// The authored/synthesized `RawCharDef` inventory alone, before `finalize` builds the real table.
pub(crate) fn build_raw(
    snapshot: &Snapshot,
    phon: &PhonFeatureSystem,
    warnings: &mut Vec<String>,
    recorder: &mut SelectionRecorder,
) -> Result<RawCharDefBuild, GrammarError> {
    let default_ws = snapshot
        .project
        .vernacular_writing_systems
        .first()
        .map(String::as_str);

    let mut raw_defs: Vec<RawCharDef> = Vec::new();
    let mut seen_nfd: hashbrown::HashSet<String> = hashbrown::HashSet::new();
    // xml_id -> index into `raw_defs`, so we can map back to a `CharDefId` once the table is built (dense ids match `raw_defs` order 1:1).
    let mut phoneme_of: HashMap<String, CharDefId> = HashMap::new();
    let mut boundary_of: HashMap<String, CharDefId> = HashMap::new();

    for ph in &snapshot.phonology.phonemes {
        let key = InventoryKey::object(InventoryKind::Phoneme, ph.guid.clone());
        recorder.considered(key.clone());
        let reps = ws_forms(&ph.representations, default_ws);
        if reps.is_empty() {
            inventory::reject(
                recorder,
                warnings,
                key,
                issue_codes::PHONEME_NO_REPRESENTATION,
                IssueClass::InvalidSource,
                format!(
                    "phoneme {:?} has no grapheme representation; skipped",
                    ph.guid
                ),
            );
            continue;
        }
        recorder.selected(key.clone());
        let norm: Vec<String> = reps.iter().map(|r| nfd(r)).collect();
        if norm.iter().any(|n| seen_nfd.contains(n)) {
            inventory::reject(
                recorder,
                warnings,
                key,
                issue_codes::PHONEME_NFD_COLLISION,
                IssueClass::AmbiguousSource,
                format!(
                    "phoneme {:?}: representation collides with an earlier phoneme/boundary; skipped",
                    ph.guid
                ),
            );
            continue;
        }
        let feature_values = phoneme_feature_values(ph, phon, warnings);
        for n in &norm {
            seen_nfd.insert(n.clone());
        }
        let idx = raw_defs.len();
        phoneme_of.insert(ph.guid.clone(), CharDefId(idx as u32));
        raw_defs.push(RawCharDef {
            xml_id: ph.guid.clone(),
            kind: CharDefKind::Segment,
            representations: reps.into_iter().map(str::to_string).collect(),
            feature_values,
        });
        recorder.represented(key);
    }

    for bd in &snapshot.phonology.boundary_markers {
        let key = InventoryKey::object(InventoryKind::BoundaryMarker, bd.guid.clone());
        recorder.considered(key.clone());
        let reps = boundary_representations(bd, default_ws);
        if reps.is_empty() {
            // HCLoader silently omits a boundary marker with no representation, not even a warning.
            recorder.selected(key.clone());
            inventory::reject_quietly(
                recorder,
                key,
                issue_codes::BOUNDARY_NO_REPRESENTATION,
                IssueClass::InvalidSource,
                "boundary marker has no grapheme representation",
            );
            continue;
        }
        recorder.selected(key.clone());
        let norm: Vec<String> = reps.iter().map(|r| nfd(r)).collect();
        if norm.iter().any(|n| seen_nfd.contains(n)) {
            inventory::reject(
                recorder,
                warnings,
                key,
                issue_codes::BOUNDARY_NFD_COLLISION,
                IssueClass::AmbiguousSource,
                format!(
                    "boundary marker {:?}: representation collides with an earlier phoneme/boundary; \
                     skipped",
                    bd.guid
                ),
            );
            continue;
        }
        for n in &norm {
            seen_nfd.insert(n.clone());
        }
        let idx = raw_defs.len();
        boundary_of.insert(bd.guid.clone(), CharDefId(idx as u32));
        raw_defs.push(RawCharDef {
            xml_id: bd.guid.clone(),
            kind: CharDefKind::Boundary,
            representations: reps,
            feature_values: Vec::new(),
        });
        recorder.represented(key);
    }

    // Synthetic boundaries HCLoader always appends (HCLoader.cs:2710-2712).
    let null_idx = raw_defs.len();
    push_synthetic_boundary(
        &mut raw_defs,
        &mut seen_nfd,
        "__null__",
        &["^0", "*0", "&0", "\u{2205}"],
        warnings,
        recorder,
    );
    let null_bdry = CharDefId(null_idx as u32);
    push_synthetic_boundary(
        &mut raw_defs,
        &mut seen_nfd,
        "__dot__",
        &["."],
        warnings,
        recorder,
    );

    Ok(RawCharDefBuild {
        raw_defs,
        seen_nfd,
        phoneme_of,
        boundary_of,
        null_bdry,
    })
}

/// Builds the real table from a (possibly substrate-completed) `RawCharDefBuild`; pairs with `build_raw`.
pub(crate) fn finalize(
    raw: RawCharDefBuild,
    phon: &PhonFeatureSystem,
    warnings: &mut Vec<String>,
    recorder: &mut SelectionRecorder,
) -> Result<CharDefBuild, GrammarError> {
    let RawCharDefBuild {
        raw_defs,
        phoneme_of,
        boundary_of,
        null_bdry,
        ..
    } = raw;
    let table = CharDefTable::from_raw("main".to_string(), None, raw_defs, phon)?;

    // The morph-boundary lookup is a derived fact, not a snapshot object -- there is nothing upstream to author it against.
    let morph_bdry_key = InventoryKey::setting(InventoryKind::BoundaryMarker, "morph-boundary");
    recorder.synthesized(morph_bdry_key.clone());
    recorder.considered(morph_bdry_key.clone());
    recorder.selected(morph_bdry_key.clone());
    let morph_bdry = match table.lookup_nfd(&nfd("+")) {
        Some(id) => {
            recorder.represented(morph_bdry_key);
            id
        }
        None => {
            inventory::reject(
                recorder,
                warnings,
                morph_bdry_key,
                issue_codes::BOUNDARY_MORPH_MARKER_UNRESOLVED,
                IssueClass::InvalidSource,
                "no boundary marker representation '+' found; morpheme-boundary matching will \
                 fall back to the null boundary",
            );
            null_bdry
        }
    };

    Ok(CharDefBuild {
        table,
        phoneme_of,
        boundary_of,
        null_bdry,
        morph_bdry,
    })
}

fn push_synthetic_boundary(
    raw_defs: &mut Vec<RawCharDef>,
    seen_nfd: &mut hashbrown::HashSet<String>,
    xml_id: &str,
    reps: &[&str],
    warnings: &mut Vec<String>,
    recorder: &mut SelectionRecorder,
) {
    let key = InventoryKey::object(InventoryKind::BoundaryMarker, xml_id.to_string());
    recorder.synthesized(key.clone());
    recorder.considered(key.clone());
    let norm: Vec<String> = reps.iter().map(|r| nfd(r)).collect();
    let free: Vec<String> = reps
        .iter()
        .zip(norm.iter())
        .filter(|(_, n)| !seen_nfd.contains(*n))
        .map(|(r, _)| r.to_string())
        .collect();
    if free.is_empty() {
        recorder.selected(key.clone());
        inventory::reject(
            recorder,
            warnings,
            key,
            issue_codes::BOUNDARY_NFD_COLLISION,
            IssueClass::AmbiguousSource,
            format!(
                "synthetic boundary {xml_id:?} ({reps:?}) fully collides with authored phonemes/\
                 boundaries; skipped"
            ),
        );
        return;
    }
    recorder.selected(key.clone());
    for f in &free {
        seen_nfd.insert(nfd(f));
    }
    raw_defs.push(RawCharDef {
        xml_id: xml_id.to_string(),
        kind: CharDefKind::Boundary,
        representations: free,
        feature_values: Vec::new(),
    });
    recorder.represented(key);
}

/// HCLoader's boundary-marker representation rule uses `BestVernacularAlternative`, distinct from phonemes' `VernacularDefaultWritingSystem`, but both fold to "prefer the project's default vernacular WS, else whatever's there" in this snapshot format.
fn boundary_representations(bd: &BoundaryMarker, default_ws: Option<&str>) -> Vec<String> {
    ws_forms(&bd.representations, default_ws)
        .into_iter()
        .map(str::to_string)
        .collect()
}

fn phoneme_feature_values(
    ph: &Phoneme,
    phon: &PhonFeatureSystem,
    warnings: &mut Vec<String>,
) -> Vec<RawFeatureValue> {
    let Some(fs) = &ph.features else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for v in &fs.values {
        let Some(flat) = phon.flat_index(&v.feature) else {
            warnings.push(format!(
                "phoneme {:?}: unknown phonological feature {:?}; value ignored",
                ph.guid, v.feature
            ));
            continue;
        };
        match &v.value {
            pg_snapshot::feature::FeatureValueKind::Closed { value } => {
                if phon.symbol_index(flat, value).is_none() {
                    warnings.push(format!(
                        "phoneme {:?}: unknown feature value {value:?} on feature {:?}; ignored",
                        ph.guid, v.feature
                    ));
                    continue;
                }
                out.push(RawFeatureValue {
                    feature_xml_id: v.feature.clone(),
                    symbol_xml_ids: vec![value.clone()],
                });
            }
            pg_snapshot::feature::FeatureValueKind::Complex { .. } => {
                warnings.push(format!(
                    "phoneme {:?}: complex feature value on {:?} not supported; ignored",
                    ph.guid, v.feature
                ));
            }
        }
    }
    out
}
