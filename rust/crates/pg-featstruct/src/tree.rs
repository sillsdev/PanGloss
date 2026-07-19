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
//! A [`FeatureStruct`] is a sorted `(FeatId, FeatureValue)` vector — deterministic order,
//! cheap `Eq`/`Hash` for the grammar-tier hash-cons interner (`Interner<FeatureStruct>` →
//! `FsId`), and cache-friendly linear walks (authored FS have single-digit entry counts).
//!
//! The *operations* (unify / subsumption / priority-union, ported from C#
//! `FeatureStruct.cs` `NondestructiveUnify`/`IsUnifiable`/`Subsumes`/`PriorityUnion`)
//! live in [`crate::ops`]; this module is the frozen data model they and the grammar
//! loader share.

use crate::bitvec::SymbolBits;

/// Index of a feature within one feature *system* (the syntactic system here; the
/// phonological system uses `FlatIndex` lanes instead). Assigned densely at grammar load.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct FeatId(pub u16);

/// One feature's value inside a tree FS.
///
/// Symbolic values are a [`SymbolBits`] set over that feature's symbols (bit `i` = symbol
/// with index `i` in the feature's declaration order), exactly like the segment-domain
/// lanes — POS max across the reference grammars is 37 symbols, well under 64.
/// Complex values are nested structs (C# `ComplexFeature`, e.g. Amharic's `obj`/`sbj`).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum FeatureValue {
    Symbolic(SymbolBits),
    Complex(FeatureStruct),
}

/// A frozen, tree-shaped feature structure: entries sorted by [`FeatId`], no duplicates.
///
/// Construct through [`FeatureStructBuilder`] (which enforces the sorted-unique invariant)
/// or [`FeatureStruct::EMPTY`]. Grammar-tier instances are interned to `FsId` so equality
/// on the hot path is an integer compare; this type's own `Eq`/`Hash` are what the
/// interner pays once per distinct value.
#[derive(Clone, PartialEq, Eq, Hash, Debug, Default)]
pub struct FeatureStruct {
    entries: Vec<(FeatId, FeatureValue)>,
}

impl FeatureStruct {
    /// The empty feature structure (unifies with everything, subsumes everything).
    pub const EMPTY: FeatureStruct = FeatureStruct {
        entries: Vec::new(),
    };

    /// Entries in ascending `FeatId` order.
    #[inline]
    pub fn entries(&self) -> &[(FeatId, FeatureValue)] {
        &self.entries
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// The value of `feat`, if present (binary search over the sorted entries).
    pub fn get(&self, feat: FeatId) -> Option<&FeatureValue> {
        self.entries
            .binary_search_by_key(&feat, |(f, _)| *f)
            .ok()
            .map(|i| &self.entries[i].1)
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

    pub fn add(&mut self, feat: FeatId, value: FeatureValue) -> &mut Self {
        match self.entries.binary_search_by_key(&feat, |(f, _)| *f) {
            Ok(i) => self.entries[i].1 = value,
            Err(i) => self.entries.insert(i, (feat, value)),
        }
        self
    }

    pub fn build(self) -> FeatureStruct {
        FeatureStruct {
            entries: self.entries,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_sorts_and_dedups() {
        let mut b = FeatureStructBuilder::new();
        b.add(FeatId(3), FeatureValue::Symbolic(SymbolBits(0b10)));
        b.add(FeatId(1), FeatureValue::Symbolic(SymbolBits(0b01)));
        b.add(FeatId(3), FeatureValue::Symbolic(SymbolBits(0b11))); // overwrite
        let fs = b.build();
        assert_eq!(fs.len(), 2);
        assert_eq!(fs.entries()[0].0, FeatId(1));
        assert_eq!(
            fs.get(FeatId(3)),
            Some(&FeatureValue::Symbolic(SymbolBits(0b11)))
        );
        assert_eq!(fs.get(FeatId(2)), None);
    }

    #[test]
    fn structural_equality_is_order_independent() {
        let mut a = FeatureStructBuilder::new();
        a.add(FeatId(1), FeatureValue::Symbolic(SymbolBits(1)));
        a.add(FeatId(2), FeatureValue::Complex(FeatureStruct::EMPTY));
        let mut b = FeatureStructBuilder::new();
        b.add(FeatId(2), FeatureValue::Complex(FeatureStruct::EMPTY));
        b.add(FeatId(1), FeatureValue::Symbolic(SymbolBits(1)));
        assert_eq!(a.build(), b.build());
    }
}
