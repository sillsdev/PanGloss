//! `CharacterDefinitionTable` — segment/boundary inventory and the NFD segmentation lookup
//! (plan §5.2, §8 layer 1).
//!
//! Ports C# `CharacterDefinitionTable.Add` (`CharacterDefinitionTable.cs:59-84`): every
//! representation is NFD-normalized before it is used as a lookup key, and it is an error for
//! two character definitions in the same table to claim the same normalized representation
//! (`_charDefLookup.ContainsKey` check). Multiple `<Representation>` elements on one
//! `SegmentDefinition`/`BoundaryDefinition` (e.g. Sena's `char4` = `m`/`n`, or its boundary
//! `char42` = `^0`/`*0`) all resolve to the *same* [`CharDefId`] — this is exercised by the
//! module tests and by the real Sena grammar.

use hashbrown::HashMap;

use crate::featsys::{FlatIndex, PhonFeatureSystem, TYPE_BOUNDARY_BITS, TYPE_SEGMENT_BITS};
use crate::nfd::nfd;
use crate::GrammarError;
use hc_featstruct::flat_unifiable;
use hc_shape::CdBits;

/// Dense per-table identity of a character definition (segment or boundary).
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct CharDefId(pub u32);

/// Whether a character definition is a phonetic segment or a morpheme-boundary marker.
///
/// Mirrors C#'s `HCFeatureSystem.Segment`/`Boundary` type symbols. Boundaries become optional
/// shape nodes at segmentation time (`hc_shape::ShapeBuilder::push_boundary`).
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum CharDefKind {
    Segment,
    Boundary,
}

/// One `FeatureValue feature="..." symbolValues="..."` inside a `SegmentDefinition`
/// (loader-internal, pre-resolution against the feature system).
pub(crate) struct RawFeatureValue {
    pub(crate) feature_xml_id: String,
    /// Space-separated symbol xml ids, already split.
    pub(crate) symbol_xml_ids: Vec<String>,
}

/// One `<SegmentDefinition>`/`<BoundaryDefinition>` as read off the XML (loader-internal).
pub(crate) struct RawCharDef {
    pub(crate) xml_id: String,
    pub(crate) kind: CharDefKind,
    /// `<Representation>` text, in document order, unescaped but *not yet* NFD-normalized.
    pub(crate) representations: Vec<String>,
    /// Only populated for segments (`LoadCharacterDefinitionTable` never attaches a
    /// `FeatureStruct` to boundaries — `CharacterDefinitionTable.AddBoundary` always passes
    /// `fs: null`).
    pub(crate) feature_values: Vec<RawFeatureValue>,
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
    /// Per-`FlatIndex` symbolic-feature lane, `hc_featstruct::SymbolBits` bits packed into `u64`.
    /// Length always equals `feat_sys.len()` for **every** char def, segment or boundary
    /// (`feat_sys.len() >= 1` always, since it includes the synthetic `Type` feature — see
    /// `featsys` module docs; a grammar with zero authored phonological features, e.g. Sena,
    /// still gets width 1 here, not 0). A feature not mentioned by an explicit `FeatureValue`
    /// defaults to `full_mask` (uninstantiated / unconstrained — C#'s `EnsureFlat` seeding); an
    /// explicit `FeatureValue` overrides that lane with the union of its symbol bits.
    ///
    /// **`Type` (plan §13.1 Tier-1 #1):** the `feat_sys.type_flat()` lane is *always* pinned —
    /// never left at the default full mask, and never settable by an authored `FeatureValue`
    /// (no `<FeatureValue feature="...">` in real HC XML ever names it) — to `Segment`-only bits
    /// for a `SegmentDefinition` or `Boundary`-only bits for a `BoundaryDefinition`. This mirrors
    /// C# `CharacterDefinitionTable.Add`'s unconditional `fs.AddValue(HCFeatureSystem.Type,
    /// type)` (`CharacterDefinitionTable.cs:56-89`), which fires even in the `fs == null` branch —
    /// i.e. even a boundary def (which authors no phonological `FeatureValue`s) gets a real,
    /// non-empty `FeatureStruct` purely from this `Type` tag. Before this fix boundaries stored
    /// an empty `Vec` here, which every lane-consuming matcher (`hc_fst`'s `flat_unifiable`
    /// treats an absent lane as unconstrained) silently read as "matches any segment" — the
    /// confirmed root cause of the `meN-`/`peN-`-prefix boundary-environment bug.
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
    /// NFD-normalized representation -> owning char def. This is the exact lookup
    /// `CharacterDefinitionTable._charDefLookup` performs (`TryGetValue` in `GetShapeNodes`).
    lookup: HashMap<String, CharDefId>,
    /// Static unifiability closure over segment char-defs (Design A, `docs/p5-crosstable-
    /// featurestruct-design.md` §6.1). `None` ⇔ the grammar declared zero authored phonological
    /// features (`PhonFeatureSystem::is_empty()`) — C#'s `StrRep` regime, so identity gating
    /// (char-def equality) stays exactly today's behavior, bit-for-bit. `Some(v)`: `v[i].contains
    /// (j)` ⇔ `flat_unifiable(lanes_i, lanes_j)` for segment char-defs `i`, `j` (reflexive,
    /// symmetric by construction). Boundary rows are left empty (`CdBits::empty()`) — C#
    /// boundaries always carry `StrRep` (`AddBoundary` always passes `fs: null`), so boundary
    /// identity gating is untouched by this closure; [`Self::unifiable_cds`] additionally guards
    /// this explicitly rather than relying on the row being empty.
    unif_closure: Option<Vec<CdBits>>,
}

impl CharDefTable {
    pub(crate) fn from_raw(
        xml_id: String,
        name: Option<String>,
        raw_defs: Vec<RawCharDef>,
        feat_sys: &PhonFeatureSystem,
    ) -> Result<Self, GrammarError> {
        let mut defs = Vec::with_capacity(raw_defs.len());
        let mut lookup: HashMap<String, CharDefId> = HashMap::with_capacity(raw_defs.len() * 2);

        for raw in raw_defs {
            let representations_nfd: Vec<String> =
                raw.representations.iter().map(|r| nfd(r)).collect();

            // C# CharacterDefinitionTable.Add: collision on any normalized representation is an
            // error, checked before the char def is admitted to the table.
            for norm in &representations_nfd {
                if lookup.contains_key(norm) {
                    return Err(GrammarError::DuplicateRepresentation(format!(
                        "table '{xml_id}': representation {norm:?} (from char def '{}') is already \
                         claimed by another character definition",
                        raw.xml_id
                    )));
                }
            }

            // Every char def gets a full `feat_sys.len()`-wide lane row (segments *and*
            // boundaries — see the `feature_lanes` field docs): segments resolve their authored
            // `<FeatureValue>`s (boundaries author none — C#'s `AddBoundary` always passes
            // `fs: null`, so every other lane defaults to the uninstantiated full mask, matching
            // a sparse FeatureStruct with no phonological features set). The `Type` lane is then
            // unconditionally pinned below, overriding whatever `build_feature_lanes` defaulted
            // it to (it can never come from an authored `FeatureValue`, so this never overwrites
            // real author intent).
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

        // Design A (P5): precompute the static unifiability closure over segment char-defs, but
        // only for a feature-bearing grammar — see `unif_closure`'s field doc for why a
        // zero-authored-feature grammar (Sena, en, sp) must NOT build this (it would make
        // char-def identity gating a no-op there, since every segment's non-`Type` lanes are the
        // same full mask and would spuriously cross-unify).
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
    ) -> Result<Vec<u64>, GrammarError> {
        // Default every lane to "uninstantiated" (all symbols allowed), matching C#'s
        // `EnsureFlat` seeding of absent features to `ulong.MaxValue`-equivalent.
        let mut lanes: Vec<u64> = (0..feat_sys.len())
            .map(|i| feat_sys.mask(FlatIndex(i as u32)))
            .collect();

        for fv in values {
            let flat = feat_sys.flat_index(&fv.feature_xml_id).ok_or_else(|| {
                GrammarError::Semantic(format!(
                    "FeatureValue references unknown feature id '{}'",
                    fv.feature_xml_id
                ))
            })?;
            let mut bits: u64 = 0;
            for sym_id in &fv.symbol_xml_ids {
                let idx = feat_sys.symbol_index(flat, sym_id).ok_or_else(|| {
                    GrammarError::Semantic(format!(
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

    /// The full per-cd closure rows, indexed by [`CharDefId`], `None` when disabled. Threaded
    /// through `hc_parse::root_trie::RootAllomorphTrie::search_segs_opt` (P5), whose edges are
    /// already known to be `Segment`-kind (the trie's own `Segment`-only filter), so the direct
    /// row lookup there doesn't need [`Self::unifiable_cds`]'s per-call boundary re-check.
    #[inline]
    pub fn unif_closure_rows(&self) -> Option<&[CdBits]> {
        self.unif_closure.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table_with(defs: Vec<RawCharDef>) -> Result<CharDefTable, GrammarError> {
        let feat_sys = PhonFeatureSystem::from_raw(vec![]).unwrap();
        CharDefTable::from_raw("table1".to_string(), None, defs, &feat_sys)
    }

    fn seg(xml_id: &str, reps: &[&str]) -> RawCharDef {
        RawCharDef {
            xml_id: xml_id.to_string(),
            kind: CharDefKind::Segment,
            representations: reps.iter().map(|s| s.to_string()).collect(),
            feature_values: vec![],
        }
    }

    fn bnd(xml_id: &str, reps: &[&str]) -> RawCharDef {
        RawCharDef {
            xml_id: xml_id.to_string(),
            kind: CharDefKind::Boundary,
            representations: reps.iter().map(|s| s.to_string()).collect(),
            feature_values: vec![],
        }
    }

    #[test]
    fn multi_representation_char_def_shares_one_id() {
        let table = table_with(vec![seg("char4", &["m", "n"])]).unwrap();
        assert_eq!(table.len(), 1);
        let id_m = table.lookup_nfd("m").unwrap();
        let id_n = table.lookup_nfd("n").unwrap();
        assert_eq!(id_m, id_n);
        assert_eq!(table.get(id_m).representations(), &["m", "n"]);
    }

    #[test]
    fn duplicate_representation_across_defs_is_an_error() {
        let err = table_with(vec![seg("char1", &["s"]), seg("char2", &["s"])]).unwrap_err();
        assert!(matches!(err, GrammarError::DuplicateRepresentation(_)));
    }

    #[test]
    fn boundary_kind_is_preserved() {
        // Plan §13.1 Tier-1 #1: a boundary's `feature_lanes()` is no longer empty — it is
        // `feat_sys.len()`-wide (1, here: just the synthetic `Type` feature, since `table_with`
        // builds a zero-authored-feature system), with `Type` pinned to `Boundary`-only bits.
        let table = table_with(vec![bnd("char41", &["+"])]).unwrap();
        let id = table.lookup_nfd("+").unwrap();
        let cd = table.get(id);
        assert_eq!(cd.kind(), CharDefKind::Boundary);
        assert_eq!(cd.feature_lanes(), &[crate::featsys::TYPE_BOUNDARY_BITS]);
    }

    #[test]
    fn representations_are_normalized_to_nfd_for_lookup() {
        // U+00E9 (precomposed é) must be found via its NFD key (e + combining acute).
        let table = table_with(vec![seg("char1", &["\u{00e9}"])]).unwrap();
        assert!(table.lookup_nfd("e\u{0301}").is_some());
        assert!(table.lookup_nfd("\u{00e9}").is_none()); // precomposed form is not the lookup key
    }

    #[test]
    fn feature_lanes_default_to_full_mask_and_override_on_explicit_value() {
        // Build a tiny 2-feature system by hand via the raw path used elsewhere in the crate.
        use crate::featsys::RawFeature;
        let feat_sys = PhonFeatureSystem::from_raw(vec![
            RawFeature {
                xml_id: "feat1".into(),
                name: "voice".into(),
                symbols: vec![("symP".into(), "+".into()), ("symM".into(), "-".into())],
                default_symbol: None,
            },
            RawFeature {
                xml_id: "feat2".into(),
                name: "place".into(),
                symbols: vec![("symA".into(), "lab".into()), ("symB".into(), "vel".into())],
                default_symbol: None,
            },
        ])
        .unwrap();
        let raw = vec![RawCharDef {
            xml_id: "char1".to_string(),
            kind: CharDefKind::Segment,
            representations: vec!["p".to_string()],
            feature_values: vec![RawFeatureValue {
                feature_xml_id: "feat1".to_string(),
                symbol_xml_ids: vec!["symM".to_string()],
            }],
        }];
        let table = CharDefTable::from_raw("t".to_string(), None, raw, &feat_sys).unwrap();
        let id = table.lookup_nfd("p").unwrap();
        let lanes = table.get(id).feature_lanes();
        // 2 authored features + the always-appended synthetic `Type` feature.
        assert_eq!(lanes.len(), 3);
        assert_eq!(lanes[0], 0b10); // feat1 explicitly set to symM (index 1)
        assert_eq!(lanes[1], feat_sys.mask(FlatIndex(1))); // feat2 unmentioned -> full mask
                                                           // Type is pinned to Segment-only bits, never left at the default full mask.
        assert_eq!(
            lanes[feat_sys.type_flat().0 as usize],
            crate::featsys::TYPE_SEGMENT_BITS
        );
    }

    #[test]
    fn greedy_longest_match_prefers_two_char_rep() {
        // The table itself doesn't do the matching (that's segment.rs), but confirm both "s",
        // "y", and "sy" are independently resolvable so segment.rs's greedy scan has real work
        // to do choosing between them.
        let table = table_with(vec![
            seg("c1", &["s"]),
            seg("c2", &["y"]),
            seg("c3", &["sy"]),
        ])
        .unwrap();
        assert!(table.lookup_nfd("s").is_some());
        assert!(table.lookup_nfd("y").is_some());
        assert!(table.lookup_nfd("sy").is_some());
        assert_ne!(table.lookup_nfd("s"), table.lookup_nfd("sy"));
    }
}
