//! `CharacterDefinitionTable` — segment/boundary inventory and the NFD segmentation lookup
//! (plan §5.2, §8 layer 1).
//!
//! Ports C# `CharacterDefinitionTable.Add` (`CharacterDefinitionTable.cs:59-84`): every
//! representation is NFD-normalized before it is used as a lookup key, and it is an error for
//! two character definitions in the same table to claim the same normalized representation
//! (`_charDefLookup.ContainsKey` check). Multiple `<Representation>` elements on one
//! `SegmentDefinition`/`BoundaryDefinition` (e.g. Sena's `char4` = `m`/`n`, or its boundary
//! `char42` = `^0`/`*0`) all resolve to the *same* `CharDefId` — this is exercised by the
//! module tests and by the real Sena grammar.

use hashbrown::HashMap;

use crate::featsys::{FlatIndex, PhonFeatureSystem, TYPE_BOUNDARY_BITS, TYPE_SEGMENT_BITS};
use crate::nfd::nfd;
use crate::ModelError;
use pg_featstruct::flat_unifiable;
use pg_shape::CdBits;

/// Dense per-table identity of a character definition (segment or boundary).
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct CharDefId(pub u32);

/// Whether a character definition is a phonetic segment or a morpheme-boundary marker.
///
/// Mirrors C#'s `HCFeatureSystem.Segment`/`Boundary` type symbols. Boundaries become optional
/// shape nodes at segmentation time (`pg_shape::ShapeBuilder::push_boundary`).
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum CharDefKind {
    Segment,
    Boundary,
}

/// One `FeatureValue feature="..." symbolValues="..."` inside a `SegmentDefinition`
/// (loader-internal, pre-resolution against the feature system).
#[doc(hidden)]
#[derive(Clone)]
pub struct RawFeatureValue {
    pub feature_xml_id: String,
    /// Space-separated symbol xml ids, already split.
    pub symbol_xml_ids: Vec<String>,
}

/// One `<SegmentDefinition>`/`<BoundaryDefinition>` as read off the XML (loader-internal).
#[doc(hidden)]
#[derive(Clone)]
pub struct RawCharDef {
    pub xml_id: String,
    pub kind: CharDefKind,
    /// `<Representation>` text, in document order, unescaped but *not yet* NFD-normalized.
    pub representations: Vec<String>,
    /// Only populated for segments (`LoadCharacterDefinitionTable` never attaches a
    /// `FeatureStruct` to boundaries — `CharacterDefinitionTable.AddBoundary` always passes
    /// `fs: null`).
    pub feature_values: Vec<RawFeatureValue>,
}

/// A compiled character definition: its representations (original and NFD) and, for segments,
/// its feature lanes.
#[derive(Debug)]
pub struct CharDef {
    xml_id: String,
    kind: CharDefKind,
    /// Original (as-authored) representations, document order.
    representations: Vec<String>,
    /// NFD-normalized representations, parallel to `representations`.
    representations_nfd: Vec<String>,
    /// Per-`FlatIndex` symbolic-feature lane, `pg_featstruct::SymbolBits` bits packed into `u64`; always `feat_sys.len()` wide for every char def, defaulting to `full_mask` unless an explicit `FeatureValue` overrides it. The `Type` lane is always pinned to Segment-only or Boundary-only bits regardless of authored `FeatureValue`s -- an empty `Vec` here previously let `flat_unifiable` read a boundary as matching any segment.
    feature_lanes: Vec<u64>,
}

impl CharDef {
    #[inline]
    pub fn xml_id(&self) -> &str {
        &self.xml_id
    }

    #[inline]
    pub fn kind(&self) -> CharDefKind {
        self.kind
    }

    /// Original (as-authored) representations, document order.
    #[inline]
    pub fn representations(&self) -> &[String] {
        &self.representations
    }

    /// NFD-normalized representations, parallel to [`representations`](Self::representations).
    #[inline]
    pub fn representations_nfd(&self) -> &[String] {
        &self.representations_nfd
    }

    /// Per-`FlatIndex` symbolic-feature lanes (see the field doc for the default/override rule).
    #[inline]
    pub fn feature_lanes(&self) -> &[u64] {
        &self.feature_lanes
    }
}

/// A compiled `CharacterDefinitionTable`: the segment/boundary inventory plus the NFD
/// segmentation lookup (`nfd(representation) -> CharDefId`).
#[derive(Debug)]
pub struct CharDefTable {
    xml_id: String,
    name: Option<String>,
    defs: Vec<CharDef>,
    /// NFD-normalized representation -> owning char def, the exact lookup `CharacterDefinitionTable._charDefLookup` performs.
    lookup: HashMap<String, CharDefId>,
    /// Static unifiability closure over segment char-defs. `None` when the grammar declared zero authored phonological features, so identity gating stays bit-for-bit today's behavior; `Some(v)`: `v[i].contains(j)` iff `flat_unifiable(lanes_i, lanes_j)`. Boundary rows are left empty since C# boundaries always carry `StrRep`.
    unif_closure: Option<Vec<CdBits>>,
}

impl CharDefTable {
    #[doc(hidden)]
    pub fn from_raw(
        xml_id: String,
        name: Option<String>,
        raw_defs: Vec<RawCharDef>,
        feat_sys: &PhonFeatureSystem,
    ) -> Result<Self, ModelError> {
        let mut defs = Vec::with_capacity(raw_defs.len());
        let mut lookup: HashMap<String, CharDefId> = HashMap::with_capacity(raw_defs.len() * 2);

        for raw in raw_defs {
            let representations_nfd: Vec<String> =
                raw.representations.iter().map(|r| nfd(r)).collect();

            // C# CharacterDefinitionTable.Add: collision on any normalized representation is an error, checked before the char def is admitted to the table.
            for norm in &representations_nfd {
                if lookup.contains_key(norm) {
                    return Err(ModelError::DuplicateRepresentation(format!(
                        "table '{xml_id}': representation {norm:?} (from char def '{}') is already \
                         claimed by another character definition",
                        raw.xml_id
                    )));
                }
            }

            // Every char def gets a full `feat_sys.len()`-wide lane row: segments resolve their authored `<FeatureValue>`s, boundaries author none so every lane defaults to full mask; the Type lane is then unconditionally pinned below, which never overwrites real author intent since it can never come from an authored `FeatureValue`.
            let mut feature_lanes = if matches!(raw.kind, CharDefKind::Segment) {
                Self::build_feature_lanes(&raw.feature_values, feat_sys)?
            } else {
                (0..feat_sys.len())
                    .map(|i| feat_sys.mask(FlatIndex(i as u32)))
                    .collect()
            };
            let type_idx = feat_sys.type_flat().0 as usize;
            feature_lanes[type_idx] = match raw.kind {
                CharDefKind::Segment => TYPE_SEGMENT_BITS,
                CharDefKind::Boundary => TYPE_BOUNDARY_BITS,
            };

            let id = CharDefId(defs.len() as u32);
            for norm in &representations_nfd {
                lookup.insert(norm.clone(), id);
            }
            defs.push(CharDef {
                xml_id: raw.xml_id,
                kind: raw.kind,
                representations: raw.representations,
                representations_nfd,
                feature_lanes,
            });
        }

        // Precompute the static unifiability closure over segment char-defs, but only for a feature-bearing grammar: building it for a zero-authored-feature grammar would make char-def identity gating a no-op, since every segment's non-Type lanes are the same full mask and would spuriously cross-unify.
        let unif_closure = if !feat_sys.is_empty() {
            let n = defs.len();
            let mut closure: Vec<CdBits> = vec![CdBits::empty(); n];
            for i in 0..n {
                if !matches!(defs[i].kind, CharDefKind::Segment) {
                    continue;
                }
                for j in i..n {
                    if !matches!(defs[j].kind, CharDefKind::Segment) {
                        continue;
                    }
                    if flat_unifiable(&defs[i].feature_lanes, &defs[j].feature_lanes) {
                        closure[i].insert(j as u32);
                        closure[j].insert(i as u32);
                    }
                }
            }
            Some(closure)
        } else {
            None
        };

        Ok(CharDefTable {
            xml_id,
            name,
            defs,
            lookup,
            unif_closure,
        })
    }

    fn build_feature_lanes(
        values: &[RawFeatureValue],
        feat_sys: &PhonFeatureSystem,
    ) -> Result<Vec<u64>, ModelError> {
        // Default every lane to "uninstantiated" (all symbols allowed), matching C#'s `EnsureFlat` seeding of absent features.
        let mut lanes: Vec<u64> = (0..feat_sys.len())
            .map(|i| feat_sys.mask(FlatIndex(i as u32)))
            .collect();

        for fv in values {
            let flat = feat_sys.flat_index(&fv.feature_xml_id).ok_or_else(|| {
                ModelError::Semantic(format!(
                    "FeatureValue references unknown feature id '{}'",
                    fv.feature_xml_id
                ))
            })?;
            let mut bits: u64 = 0;
            for sym_id in &fv.symbol_xml_ids {
                let idx = feat_sys.symbol_index(flat, sym_id).ok_or_else(|| {
                    ModelError::Semantic(format!(
                        "FeatureValue references unknown symbol id '{sym_id}' on feature '{}'",
                        fv.feature_xml_id
                    ))
                })?;
                bits |= 1u64 << idx;
            }
            lanes[flat.0 as usize] = bits;
        }

        Ok(lanes)
    }

    #[inline]
    pub fn xml_id(&self) -> &str {
        &self.xml_id
    }

    #[inline]
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.defs.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.defs.is_empty()
    }

    #[inline]
    pub fn get(&self, id: CharDefId) -> &CharDef {
        &self.defs[id.0 as usize]
    }

    /// The segmentation lookup: NFD-normalized representation -> owning char def, exactly
    /// `CharacterDefinitionTable.TryGetValue` (representations are normalized before matching).
    pub fn lookup_nfd(&self, nfd_rep: &str) -> Option<CharDefId> {
        self.lookup.get(nfd_rep).copied()
    }

    /// Iterate char defs in table order, for the layer-1 loader dump (plan §8).
    pub fn iter(&self) -> impl Iterator<Item = (CharDefId, &CharDef)> {
        self.defs
            .iter()
            .enumerate()
            .map(|(i, d)| (CharDefId(i as u32), d))
    }

    /// The static unifiability closure of `cd` (Design A, P5 — see `unif_closure`'s field doc).
    /// `O(1)`. `None` when the closure is disabled (zero-feature grammar) or `cd` names a
    /// boundary (boundaries stay identity-gated in every grammar, mirroring C#'s `StrRep`-always
    /// regime for `AddBoundary`).
    #[inline]
    pub fn unifiable_cds(&self, cd: CharDefId) -> Option<&CdBits> {
        let closure = self.unif_closure.as_ref()?;
        if !matches!(self.defs[cd.0 as usize].kind, CharDefKind::Segment) {
            return None;
        }
        Some(&closure[cd.0 as usize])
    }

    /// The full per-cd closure rows, indexed by `CharDefId`, `None` when disabled. Threaded
    /// through `pg_parse::root_trie::RootAllomorphTrie::search_segs_opt` (P5), whose edges are
    /// already known to be `Segment`-kind (the trie's own `Segment`-only filter), so the direct
    /// row lookup there doesn't need `Self::unifiable_cds`'s per-call boundary re-check.
    #[inline]
    pub fn unif_closure_rows(&self) -> Option<&[CdBits]> {
        self.unif_closure.as_deref()
    }
}

#[cfg(test)]
mod tests;
