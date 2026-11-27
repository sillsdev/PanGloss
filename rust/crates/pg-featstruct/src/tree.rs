//! Tree-shaped feature structures for the *syntactic* domain (plan §5.3): POS + head
//! features on words, rules' required/output feature structs.
//!
//! ## Why a tree, not a DAG
//! C# `FeatureStruct` supports re-entrancy (union-find `Forward` pointers, `Dereference`),
//! but authored HC grammars cannot express it: `XmlLanguageLoader.LoadFeatureStruct` builds
//! strictly nested values (symbolValues / nested `FeatureValue` elements) with no
//! by-reference resolution anywhere in the loader (verified — no `ref=`/IDREF handling).
//! Alpha variables are phonological-domain only (`LoadVariables` resolves against
//! `PhonologicalFeatureSystem`), so the syntactic tree carries **no variables** either.
//! Coreference via `VariableBindings` lives at the pattern-matching layer, not here.
//!
//! ## Representation
//! A `FeatureStruct` is a sorted `(FeatId, FeatureValue)` vector — deterministic order,
//! cheap `Eq`/`Hash` for the grammar-tier hash-cons interner (`Interner<FeatureStruct>` →
//! `FsId`), and cache-friendly linear walks (authored FS have single-digit entry counts).
//!
//! The *operations* (unify / subsumption / priority-union, ported from C#
//! `FeatureStruct.cs` `NondestructiveUnify`/`IsUnifiable`/`Subsumes`/`PriorityUnion`)
//! live in `crate::ops`; this module is the frozen data model they and the grammar
//! loader share.

use std::sync::Arc;

use crate::bitvec::SymbolBits;

/// Index of a feature within one feature *system* (syntactic here; phonological uses `FlatIndex` lanes), assigned densely at grammar load.
#[derive(
    Copy, Clone, PartialEq, Eq, Hash, Debug, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct FeatId(pub u16);

/// One feature's value inside a tree FS.
///
/// Symbolic values are a `SymbolBits` set over that feature's symbols (bit `i` = symbol
/// with index `i` in the feature's declaration order), exactly like the segment-domain
/// lanes — POS max across the reference grammars is 37 symbols, well under 64.
/// Complex values are nested structs (C# `ComplexFeature`, e.g. Amharic's `obj`/`sbj`).
#[derive(Clone, PartialEq, Eq, Hash, Debug, serde::Serialize, serde::Deserialize)]
pub enum FeatureValue {
    Symbolic(SymbolBits),
    Complex(FeatureStruct),
}

/// A frozen, tree-shaped feature structure: entries sorted by `FeatId`, no duplicates.
///
/// Construct through `FeatureStructBuilder` (which enforces the sorted-unique invariant)
/// or `FeatureStruct::EMPTY`. Grammar-tier instances are interned to `FsId` so equality
/// on the hot path is an integer compare; this type's own `Eq`/`Hash` are what the
/// interner pays once per distinct value.
#[derive(Clone, PartialEq, Eq, Hash, Default)]
pub struct FeatureStruct {
    /// `None` is the empty structure. Frozen entries are shared, so `Clone` is a reference-count bump.
    entries: Option<Arc<[(FeatId, FeatureValue)]>>,
}

/// What a `FeatureStruct` weighed while it held its entries in an inline `Vec`. Byte-budget arithmetic (`Word` capture estimates) keeps using this figure so eviction and capture ceilings do not move now that the entries are shared.
pub const INLINE_BYTES: usize = 24;

/// Prints exactly what the derive printed when the entries were a plain `Vec` (`FeatureStruct { entries: [..] }`); diagnostic text embeds it.
impl std::fmt::Debug for FeatureStruct {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FeatureStruct")
            .field("entries", &self.entries())
            .finish()
    }
}

impl serde::Serialize for FeatureStruct {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut out = serializer.serialize_struct("FeatureStruct", 1)?;
        out.serialize_field("entries", self.entries())?;
        out.end()
    }
}

impl<'de> serde::Deserialize<'de> for FeatureStruct {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct WireFeatureStruct {
            entries: Vec<(FeatId, FeatureValue)>,
        }

        let wire = WireFeatureStruct::deserialize(deserializer)?;
        let mut builder = FeatureStructBuilder::new();
        for (feat, value) in wire.entries {
            if builder
                .entries
                .binary_search_by_key(&feat, |(existing, _)| *existing)
                .is_ok()
            {
                return Err(serde::de::Error::custom(format_args!(
                    "duplicate feature {}",
                    feat.0
                )));
            }
            builder.add(feat, value);
        }
        Ok(builder.build())
    }
}

impl FeatureStruct {
    /// The empty feature structure (unifies with everything, subsumes everything).
    pub const EMPTY: FeatureStruct = FeatureStruct { entries: None };

    /// Whether the two structures hold one allocation of entries, which equal structures need not. Two empty structures hold none and answer true.
    #[inline]
    pub fn same_allocation(&self, other: &FeatureStruct) -> bool {
        match (&self.entries, &other.entries) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        }
    }

    /// Entries in ascending `FeatId` order.
    #[inline]
    pub fn entries(&self) -> &[(FeatId, FeatureValue)] {
        self.entries.as_deref().unwrap_or(&[])
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.entries.is_none()
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.entries().len()
    }

    /// The value of `feat`, if present (binary search over the sorted entries).
    pub fn get(&self, feat: FeatId) -> Option<&FeatureValue> {
        let entries = self.entries();
        entries
            .binary_search_by_key(&feat, |(f, _)| *f)
            .ok()
            .map(|i| &entries[i].1)
    }
}

/// Builder enforcing the sorted-unique invariant. `add` on an already-present feature
/// replaces the value (last write wins, matching C# `FeatureStruct.AddValue` semantics
/// where re-adding a feature overwrites its value).
#[derive(Debug, Default)]
pub struct FeatureStructBuilder {
    entries: Vec<(FeatId, FeatureValue)>,
}

impl FeatureStructBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// A builder that will hold up to `entries` features without regrowing.
    pub fn with_capacity(entries: usize) -> Self {
        FeatureStructBuilder {
            entries: Vec::with_capacity(entries),
        }
    }

    pub fn add(&mut self, feat: FeatId, value: FeatureValue) -> &mut Self {
        // The merge-walks in `crate::ops` add in ascending order: append without searching.
        if self.entries.last().is_none_or(|(last, _)| *last < feat) {
            self.entries.push((feat, value));
            return self;
        }
        match self.entries.binary_search_by_key(&feat, |(f, _)| *f) {
            Ok(i) => self.entries[i].1 = value,
            Err(i) => self.entries.insert(i, (feat, value)),
        }
        self
    }

    pub fn build(self) -> FeatureStruct {
        FeatureStruct {
            entries: if self.entries.is_empty() {
                None
            } else {
                Some(Arc::from(self.entries))
            },
        }
    }
}

#[cfg(test)]
mod tests;
