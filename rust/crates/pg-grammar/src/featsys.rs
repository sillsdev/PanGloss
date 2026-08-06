//! `PhonologicalFeatureSystem` — the phonetic feature census (plan §5.3).
//!
//! Ports `XmlLanguageLoader.LoadPhonologicalFeatureSystem`/`LoadFeature` (only the
//! `SymbolicFeature` case: no `ComplexFeature` ever appears directly under
//! `PhonologicalFeatureSystem` in HC XML — `XmlLanguageLoader.cs:620-624` only iterates
//! `SymbolicFeature` children). Each feature gets a dense **`FlatIndex`** (document order) and
//! each of its symbols a dense **symbol index** (document order); the feature's `mask` is
//! `pg_featstruct::full_mask(symbol_count)`.
//!
//! A grammar may legitimately have **zero** phonological features (Sena's real XML has no
//! `<PhonologicalFeatureSystem>` element at all — its `SymbolicFeature`s live under
//! `HeadFeatures`, the syntactic feature system, out of scope here). C# handles this by never
//! attaching a segment `FeatureStruct` (`LoadCharacterDefinitionTable`'s
//! `if (_language.PhonologicalFeatureSystem.Count > 0)` guard); the Rust port mirrors it with an
//! empty `PhonFeatureSystem` rather than an error.

use hashbrown::HashMap;
use pg_featstruct::full_mask;

use crate::GrammarError;

/// Dense per-grammar index of a symbolic feature, assigned in XML document order (plan §5.3).
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct FlatIndex(pub u32);

/// One `<SymbolicFeature>` as read off the XML, before dense-indexing (loader-internal).
pub(crate) struct RawFeature {
    pub(crate) xml_id: String,
    pub(crate) name: String,
    /// `(symbol xml id, symbol name)` in document order.
    pub(crate) symbols: Vec<(String, String)>,
    /// Finding N2 (phase2 audit C): `SymbolicFeature@defaultSymbol`, the XML `id` of one of this
    /// same feature's own `<Symbol>`s (C# `LoadFeature`, `XmlLanguageLoader.cs:632-654`:
    /// `feature.DefaultSymbolID = defValId` resolves via the feature's own `_possibleSymbols`
    /// dict, `SymbolicFeature.cs:57-60`). `None` when the attribute is absent (the common case —
    /// C#'s `string.IsNullOrEmpty(defValId)` guard is equivalent to "no default configured", not
    /// "default is the empty string").
    pub(crate) default_symbol: Option<String>,
}

#[derive(Debug)]
struct FeatureDef {
    xml_id: String,
    name: String,
    /// symbol xml id -> dense symbol index (document order)
    symbol_index: HashMap<String, u32>,
    symbol_names: Vec<String>,
    mask: u64,
    /// A single-bit mask (the default symbol's dense index), or `None` if the feature declared no default; consumed by `pg_rules::rewrite`'s `UseDefaults` confirm step.
    default_bits: Option<u64>,
}

/// The internal XML-id the synthetic `Type` feature is registered under; chosen so no authored `<SymbolicFeature id="...">` can collide with it.
const TYPE_XML_ID: &str = "__hc_type__";

/// Dense symbol index of the `Type` feature's `Segment` symbol (see `PhonFeatureSystem::type_flat`).
pub const TYPE_SEGMENT_SYMBOL: u32 = 0;
/// Dense symbol index of the `Type` feature's `Boundary` symbol.
pub const TYPE_BOUNDARY_SYMBOL: u32 = 1;
/// `SymbolBits`-style raw bits for `Type=Segment` (bit 0 only).
pub const TYPE_SEGMENT_BITS: u64 = 1 << TYPE_SEGMENT_SYMBOL;
/// `SymbolBits`-style raw bits for `Type=Boundary` (bit 1 only).
pub const TYPE_BOUNDARY_BITS: u64 = 1 << TYPE_BOUNDARY_SYMBOL;

/// The compiled phonological feature system: features and their symbols, dense-indexed.
///
/// **Lint:** a `SymbolicFeature` with >= 64 symbols is rejected as `GrammarError::Unsupported`
/// rather than silently mis-masked — the 64-symbol mask boundary is a known parity hazard
/// (`pg_featstruct::full_mask` doc comment; #446's regression fixture). None of the three
/// reference grammars come close (widest is Sena's HeadFeatures `genro` at 20 symbols, and that
/// one isn't even phonological); this guard exists for grammars this milestone hasn't seen.
///
/// **`Type` (plan §13.1 Tier-1 #1):** every `PhonFeatureSystem` carries one synthetic extra
/// feature, always appended **last** (at `FlatIndex(real_feature_count)`), with exactly two
/// symbols `Segment`/`Boundary` — mirroring C# `CharacterDefinitionTable.Add`'s unconditional
/// `fs.AddValue(HCFeatureSystem.Type, type)` (`CharacterDefinitionTable.cs:56-89`, including the
/// `fs == null` branch) and `NaturalClass`'s unconditional `AddValue(Type, Segment)`
/// (`NaturalClass.cs:7-15`). It is never read from XML — no `<SymbolicFeature>` ever declares it
/// — so `len()`/`iter()` always report at least 1 feature, even for a grammar with zero authored
/// `<PhonologicalFeatureSystem>` features (Sena). Appended, not prepended, so every existing
/// `FlatIndex` computed against authored features is unaffected.
#[derive(Debug)]
pub struct PhonFeatureSystem {
    features: Vec<FeatureDef>,
    id_to_flat: HashMap<String, u32>,
    /// `FlatIndex` of the synthetic `Type` feature — always `features.len() - 1`.
    type_flat: FlatIndex,
}

impl Default for PhonFeatureSystem {
    /// Zero authored features still carry the synthetic `Type` feature; hand-written because `FlatIndex` deliberately has no `Default` (a bare `FlatIndex(0)` would be a footgun).
    fn default() -> Self {
        Self::from_raw(Vec::new()).expect("zero raw features never errors")
    }
}

impl PhonFeatureSystem {
    pub(crate) fn from_raw(raw: Vec<RawFeature>) -> Result<Self, GrammarError> {
        let mut features = Vec::with_capacity(raw.len() + 1);
        let mut id_to_flat = HashMap::with_capacity(raw.len() + 1);
        for (flat, f) in raw.into_iter().enumerate() {
            if f.symbols.len() >= 64 {
                return Err(GrammarError::Unsupported(format!(
                    "SymbolicFeature '{}' ({}) has {} symbols; the flat bit-vector representation \
                     supports at most 63 symbols per feature (the 64-symbol mask boundary is a \
                     known parity hazard) — grammar must fall back to the managed engine",
                    f.name,
                    f.xml_id,
                    f.symbols.len()
                )));
            }
            let mut symbol_index = HashMap::with_capacity(f.symbols.len());
            let mut symbol_names = Vec::with_capacity(f.symbols.len());
            for (idx, (sym_id, sym_name)) in f.symbols.into_iter().enumerate() {
                symbol_index.insert(sym_id, idx as u32);
                symbol_names.push(sym_name);
            }
            let mask = full_mask(symbol_names.len() as u32);
            // `defaultSymbol` resolves against this feature's own symbols; an unresolvable id becomes a `GrammarError::Semantic`, matching this loader's malformed-reference convention.
            let default_bits = match &f.default_symbol {
                Some(sym_id) => {
                    let idx = symbol_index.get(sym_id).ok_or_else(|| {
                        GrammarError::Semantic(format!(
                            "SymbolicFeature '{}' ({}): defaultSymbol '{sym_id}' is not one of its own symbols",
                            f.name, f.xml_id
                        ))
                    })?;
                    Some(1u64 << idx)
                }
                None => None,
            };
            id_to_flat.insert(f.xml_id.clone(), flat as u32);
            features.push(FeatureDef {
                xml_id: f.xml_id,
                name: f.name,
                symbol_index,
                symbol_names,
                mask,
                default_bits,
            });
        }

        // Append the synthetic `Type` feature (see struct docs) unconditionally.
        let type_flat = FlatIndex(features.len() as u32);
        let mut type_symbol_index = HashMap::with_capacity(2);
        type_symbol_index.insert("Segment".to_string(), TYPE_SEGMENT_SYMBOL);
        type_symbol_index.insert("Boundary".to_string(), TYPE_BOUNDARY_SYMBOL);
        id_to_flat.insert(TYPE_XML_ID.to_string(), type_flat.0);
        features.push(FeatureDef {
            xml_id: TYPE_XML_ID.to_string(),
            name: "Type".to_string(),
            symbol_index: type_symbol_index,
            // Display strings only (lookup keys above are unrelated, unchanged sentinels); C# gives `Segment`/`Boundary` lower-case descriptions, which is what dumps print.
            symbol_names: vec!["segment".to_string(), "boundary".to_string()],
            mask: full_mask(2),
            default_bits: None,
        });

        Ok(PhonFeatureSystem {
            features,
            id_to_flat,
            type_flat,
        })
    }

    /// The synthetic `Type` feature's `FlatIndex` (see struct docs) — always the last feature.
    #[inline]
    pub fn type_flat(&self) -> FlatIndex {
        self.type_flat
    }

    /// Number of symbolic features in the system, **including** the always-present synthetic
    /// `Type` feature (see struct docs) — so this is never 0, even for a grammar with zero
    /// authored `<PhonologicalFeatureSystem>` features (Sena: `len() == 1`, `type_flat() ==
    /// FlatIndex(0)`).
    #[inline]
    pub fn len(&self) -> usize {
        self.features.len()
    }

    /// Whether the grammar declared zero authored (real, XML-visible) phonological features —
    /// i.e. only the synthetic `Type` feature is present (Sena's real XML has no
    /// `<PhonologicalFeatureSystem>` element at all — see module docs). Distinct from `len() ==
    /// 0`, which can no longer happen now that `Type` is always appended.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.type_flat.0 == 0
    }

    /// The feature's dense `FlatIndex`, by its XML `id` attribute.
    pub fn flat_index(&self, feature_xml_id: &str) -> Option<FlatIndex> {
        self.id_to_flat.get(feature_xml_id).map(|&i| FlatIndex(i))
    }

    /// The full mask (`pg_featstruct::full_mask(symbol_count)`) for a feature.
    pub fn mask(&self, flat: FlatIndex) -> u64 {
        self.features[flat.0 as usize].mask
    }

    /// The feature's `defaultSymbol`, as a single-bit `SymbolBits`-style mask, or `None` if the
    /// feature declared no default. Type, and every feature of a grammar with no defaultSymbol
    /// attributes at all (all three reference grammars), always returns None.
    #[inline]
    pub fn default_bits(&self, flat: FlatIndex) -> Option<u64> {
        self.features[flat.0 as usize].default_bits
    }

    /// Number of symbols the feature at `flat` declares.
    pub fn symbol_count(&self, flat: FlatIndex) -> usize {
        self.features[flat.0 as usize].symbol_names.len()
    }

    /// The feature's name (`<Name>`), by `FlatIndex`.
    pub fn feature_name(&self, flat: FlatIndex) -> &str {
        &self.features[flat.0 as usize].name
    }

    /// The feature's original XML `id` attribute, by `FlatIndex`.
    pub fn feature_xml_id(&self, flat: FlatIndex) -> &str {
        &self.features[flat.0 as usize].xml_id
    }

    /// A symbol's display name (`<Symbol id="...">NAME</Symbol>` text — C# `FeatureSymbol.
    /// Description`), by the owning feature's `FlatIndex` and the symbol's dense index (as used
    /// in a feature-lane bitmask). F2 prerequisite (HYBRID_FST_RUST_PLAN.md §7.1 bullet 4): needed
    /// to render a char-def's `FeatureStruct` in C#'s `FeatureStruct.ToString()` format
    /// (`SurfacePhonology`'s `DeletionJunctions` dump prints the deleted neighbor's own feature
    /// struct this way). Panics on an out-of-range `idx` (a caller bug — every `idx` in this crate
    /// comes from iterating that same feature's own mask/symbol_count).
    pub fn symbol_name(&self, flat: FlatIndex, idx: u32) -> &str {
        &self.features[flat.0 as usize].symbol_names[idx as usize]
    }

    /// Dense symbol index of a symbol, by the feature's `FlatIndex` and the symbol's XML `id`.
    pub fn symbol_index(&self, flat: FlatIndex, symbol_xml_id: &str) -> Option<u32> {
        self.features[flat.0 as usize]
            .symbol_index
            .get(symbol_xml_id)
            .copied()
    }

    /// Iterate features in `FlatIndex` order, for the layer-1 loader dump (plan §8).
    pub fn iter(&self) -> impl Iterator<Item = (FlatIndex, &str, usize)> {
        self.features
            .iter()
            .enumerate()
            .map(|(i, f)| (FlatIndex(i as u32), f.name.as_str(), f.symbol_names.len()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(xml_id: &str, name: &str, symbols: &[(&str, &str)]) -> RawFeature {
        raw_with_default(xml_id, name, symbols, None)
    }

    fn raw_with_default(
        xml_id: &str,
        name: &str,
        symbols: &[(&str, &str)],
        default_symbol: Option<&str>,
    ) -> RawFeature {
        RawFeature {
            xml_id: xml_id.to_string(),
            name: name.to_string(),
            symbols: symbols
                .iter()
                .map(|(id, n)| (id.to_string(), n.to_string()))
                .collect(),
            default_symbol: default_symbol.map(str::to_string),
        }
    }

    #[test]
    fn zero_authored_features_still_carries_type() {
        // A grammar with zero authored `<PhonologicalFeatureSystem>` features (Sena) still gets `len() == 1` for the synthetic `Type` feature, not 0.
        let fs = PhonFeatureSystem::from_raw(vec![]).unwrap();
        assert!(fs.is_empty(), "zero *authored* features");
        assert_eq!(
            fs.len(),
            1,
            "Type is always appended, even with zero authored features"
        );
        assert_eq!(fs.flat_index("nope"), None);
        assert_eq!(fs.type_flat(), FlatIndex(0));
        assert_eq!(fs.feature_name(fs.type_flat()), "Type");
        assert_eq!(
            fs.symbol_index(fs.type_flat(), "Segment"),
            Some(TYPE_SEGMENT_SYMBOL)
        );
        assert_eq!(
            fs.symbol_index(fs.type_flat(), "Boundary"),
            Some(TYPE_BOUNDARY_SYMBOL)
        );
        assert_eq!(fs.mask(fs.type_flat()), 0b11);
    }

    // --- Finding N2: SymbolicFeature@defaultSymbol -----------------------------------------

    #[test]
    fn no_default_symbol_yields_none() {
        let fs = PhonFeatureSystem::from_raw(vec![raw("f", "voice", &[("p", "+"), ("m", "-")])])
            .unwrap();
        assert_eq!(fs.default_bits(FlatIndex(0)), None);
        // The always-appended synthetic Type feature never has a default either.
        assert_eq!(fs.default_bits(fs.type_flat()), None);
    }

    #[test]
    fn default_symbol_resolves_to_its_own_dense_bit() {
        let fs = PhonFeatureSystem::from_raw(vec![raw_with_default(
            "f",
            "voice",
            &[("p", "+"), ("m", "-")],
            Some("m"),
        )])
        .unwrap();
        // "m" is dense symbol index 1 -> bit 0b10.
        assert_eq!(fs.default_bits(FlatIndex(0)), Some(0b10));
    }

    #[test]
    fn default_symbol_referencing_an_unknown_id_is_a_semantic_error() {
        let err = PhonFeatureSystem::from_raw(vec![raw_with_default(
            "f",
            "voice",
            &[("p", "+"), ("m", "-")],
            Some("nope"),
        )])
        .unwrap_err();
        assert!(matches!(err, GrammarError::Semantic(_)));
    }

    #[test]
    fn dense_indices_in_document_order() {
        let fs = PhonFeatureSystem::from_raw(vec![
            raw("feat200", "dr", &[("sym1", "+"), ("sym2", "-")]),
            raw(
                "feat271",
                "OrthPlace",
                &[("symA", "velar"), ("symB", "labial")],
            ),
        ])
        .unwrap();
        // 2 authored features + the always-appended synthetic `Type` feature.
        assert!(!fs.is_empty());
        assert_eq!(fs.len(), 3);
        assert_eq!(fs.flat_index("feat200"), Some(FlatIndex(0)));
        assert_eq!(fs.flat_index("feat271"), Some(FlatIndex(1)));
        assert_eq!(fs.feature_name(FlatIndex(0)), "dr");
        assert_eq!(fs.symbol_index(FlatIndex(0), "sym1"), Some(0));
        assert_eq!(fs.symbol_index(FlatIndex(0), "sym2"), Some(1));
        assert_eq!(fs.symbol_index(FlatIndex(1), "symB"), Some(1));
        assert_eq!(fs.mask(FlatIndex(0)), 0b11);
        // `Type` is appended last, never shifting the authored features' indices.
        assert_eq!(fs.type_flat(), FlatIndex(2));
    }

    #[test]
    fn sixty_four_symbols_is_unsupported() {
        let symbols: Vec<(String, String)> =
            (0..64).map(|i| (format!("s{i}"), format!("{i}"))).collect();
        let symbols_ref: Vec<(&str, &str)> = symbols
            .iter()
            .map(|(a, b)| (a.as_str(), b.as_str()))
            .collect();
        let err =
            PhonFeatureSystem::from_raw(vec![raw("featBig", "big", &symbols_ref)]).unwrap_err();
        assert!(matches!(err, GrammarError::Unsupported(_)));
    }

    #[test]
    fn sixty_three_symbols_is_supported() {
        let symbols: Vec<(String, String)> =
            (0..63).map(|i| (format!("s{i}"), format!("{i}"))).collect();
        let symbols_ref: Vec<(&str, &str)> = symbols
            .iter()
            .map(|(a, b)| (a.as_str(), b.as_str()))
            .collect();
        let fs = PhonFeatureSystem::from_raw(vec![raw("featBig", "big", &symbols_ref)]).unwrap();
        assert_eq!(fs.symbol_count(FlatIndex(0)), 63);
    }
}
