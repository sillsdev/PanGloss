//! Character-definition table synthesis from the snapshot's phoneme/boundary inventory
//! (`HCLoader.LoadCharacterDefinitionTable`, HCLoader.cs:2669-2743).
//!
//! Beyond the phonemes/boundary markers themselves, HCLoader always appends: the "null" boundary
//! (representations `^0`, `*0`, `&0`, `∅` — used by `AnyPlus`/`AnyStar`/`PrefixNull`/`SuffixNull`
//! to make morpheme-boundary matching optional around a phonological/environment context) and a
//! standalone `.` boundary (the space-replacement character, `FormatForm`). The user-authored
//! morpheme-boundary marker (conventionally `+`) is looked up by representation afterward.
//!
//! Dotted-circle (U+25CC) stripping already happened in `pg-fwdata` (see
//! `pg_snapshot::phonology::Phoneme::representations`'s doc) — this module does not repeat it.
//! Space -> `.` replacement (`FormatForm`) applies to *forms* (allomorph/entry text), not
//! phoneme/boundary representations themselves, so it is not applied here either.

use hashbrown::HashMap;

use pg_snapshot::phonology::{BoundaryMarker, Phoneme};
use pg_snapshot::Snapshot;

use crate::chardef::{CharDefKind, CharDefTable, CharDefId, RawCharDef, RawFeatureValue};
use crate::featsys::PhonFeatureSystem;
use crate::nfd::nfd;
use crate::GrammarError;

use super::ws_forms;

pub(crate) struct CharDefBuild {
    pub table: CharDefTable,
    pub phoneme_of: HashMap<String, CharDefId>,
    pub boundary_of: HashMap<String, CharDefId>,
    pub null_bdry: CharDefId,
    pub morph_bdry: CharDefId,
}

pub(crate) fn build(
    snapshot: &Snapshot,
    phon: &PhonFeatureSystem,
    warnings: &mut Vec<String>,
) -> Result<CharDefBuild, GrammarError> {
    let default_ws = snapshot.project.vernacular_writing_systems.first().map(String::as_str);

    let mut raw_defs: Vec<RawCharDef> = Vec::new();
    let mut seen_nfd: hashbrown::HashSet<String> = hashbrown::HashSet::new();
    // xml_id (here: the phoneme/boundary guid) -> index into `raw_defs`, so we can map back to a
    // `CharDefId` once the table is built (dense ids match `raw_defs` order 1:1).
    let mut phoneme_of: HashMap<String, CharDefId> = HashMap::new();
    let mut boundary_of: HashMap<String, CharDefId> = HashMap::new();

    for ph in &snapshot.phonology.phonemes {
        let reps = ws_forms(&ph.representations, default_ws);
        if reps.is_empty() {
            warnings.push(format!("phoneme {:?} has no grapheme representation; skipped", ph.guid));
            continue;
        }
        let norm: Vec<String> = reps.iter().map(|r| nfd(r)).collect();
        if norm.iter().any(|n| seen_nfd.contains(n)) {
            warnings.push(format!(
                "phoneme {:?}: representation collides with an earlier phoneme/boundary; skipped",
                ph.guid
            ));
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
    }

    for bd in &snapshot.phonology.boundary_markers {
        let reps = boundary_representations(bd, default_ws);
        if reps.is_empty() {
            // HCLoader silently omits a boundary marker with no representation (no `InvalidPhoneme`-
            // style logger call for boundaries) — not even a warning.
            continue;
        }
        let norm: Vec<String> = reps.iter().map(|r| nfd(r)).collect();
        if norm.iter().any(|n| seen_nfd.contains(n)) {
            warnings.push(format!(
                "boundary marker {:?}: representation collides with an earlier phoneme/boundary; \
                 skipped",
                bd.guid
            ));
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
    }

    // Synthetic boundaries HCLoader always appends (HCLoader.cs:2710-2712).
    let null_idx = raw_defs.len();
    push_synthetic_boundary(&mut raw_defs, &mut seen_nfd, "__null__", &["^0", "*0", "&0", "\u{2205}"], warnings);
    let null_bdry = CharDefId(null_idx as u32);
    push_synthetic_boundary(&mut raw_defs, &mut seen_nfd, "__dot__", &["."], warnings);

    let table = CharDefTable::from_raw("main".to_string(), None, raw_defs, phon)?;

    let morph_bdry = table
        .lookup_nfd(&nfd("+"))
        .unwrap_or_else(|| {
            warnings.push(
                "no boundary marker representation '+' found; morpheme-boundary matching will \
                 fall back to the null boundary"
                    .to_string(),
            );
            null_bdry
        });

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
) {
    let norm: Vec<String> = reps.iter().map(|r| nfd(r)).collect();
    let free: Vec<String> = reps
        .iter()
        .zip(norm.iter())
        .filter(|(_, n)| !seen_nfd.contains(*n))
        .map(|(r, _)| r.to_string())
        .collect();
    if free.is_empty() {
        warnings.push(format!(
            "synthetic boundary {xml_id:?} ({reps:?}) fully collides with authored phonemes/\
             boundaries; skipped"
        ));
        return;
    }
    for f in &free {
        seen_nfd.insert(nfd(f));
    }
    raw_defs.push(RawCharDef {
        xml_id: xml_id.to_string(),
        kind: CharDefKind::Boundary,
        representations: free,
        feature_values: Vec::new(),
    });
}

/// HCLoader's boundary-marker representation rule uses `BestVernacularAlternative`, distinct from
/// phonemes' `VernacularDefaultWritingSystem` (HCLoader.cs:2700-2702) — both fold to "prefer the
/// project's default vernacular WS, else whatever's there" in this snapshot format.
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
