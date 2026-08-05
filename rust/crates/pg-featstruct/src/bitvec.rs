//! Bit-vector symbolic feature structures (plan §5.3) — the primary, hot-path representation
//! for closed-class segment-domain features.
//!
//! This module is a faithful port of C# `UlongSymbolicFeatureValueFlags` (per-feature symbol
//! set) and `FeatureStruct.TryFastUnifiable`/`EnsureFlat` (the whole-FS flat-unify fast path),
//! both on branch `hc-rustify` (PR #446). The port keeps the exact bit arithmetic — including
//! the 64-symbol mask boundary #446 pinned with a regression test — because parse results are
//! not allowed to change (plan §8).
//!
//! ## Representation split
//! - `SymbolBits` is one symbolic feature's allowed-symbol set: bit `i` = the symbol with
//!   `Index i`. It is a plain `u64` newtype (8 bytes, `Copy`); the per-feature *mask* (which
//!   depends on the feature's symbol count) is **not** stored here — it lives in grammar-tier
//!   feature metadata and is passed into the ops that need it (`not`/`Not`/`has_all`). This
//!   mirrors C#'s `_mask` field while keeping the value type minimal and cache-friendly.
//! - A whole segment-domain FS is a lane array (`&[u64]`), one lane per symbolic feature indexed
//!   by `FlatIndex`. `flat_unifiable` is the hot-path operation. An absent lane (index beyond
//!   the slice) is unconstrained = all-ones, exactly as C#'s `EnsureFlat` seeds `ulong.MaxValue`.

/// Full mask for a symbolic feature with `count` possible symbols (`0..=64`).
///
/// A feature with exactly 64 symbols occupies bits `0..=63` (the whole `u64`). Computing the
/// mask as `(1 << count) - 1` is wrong at 64 because a shift count of 64 is undefined/masked
/// on most ISAs (`1u64 << 64` wraps to `1u64 << 0` on x86, giving mask 0 — silently breaking
/// every mask-dependent op). C# guards this in the `UlongSymbolicFeatureValueFlags` ctor; we
/// use `checked_shl`, which returns `None` at 64 and we map that to all-ones.
#[inline]
pub const fn full_mask(count: u32) -> u64 {
    match 1u64.checked_shl(count) {
        Some(v) => v - 1,
        None => u64::MAX,
    }
}

/// A bit-packed set of allowed symbols for one symbolic feature (port of the `_flags` field of
/// C# `UlongSymbolicFeatureValueFlags`). Bit `i` corresponds to the `FeatureSymbol` with `Index i`.
#[derive(
    Copy,
    Clone,
    PartialEq,
    Eq,
    Hash,
    Debug,
    Default,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize,
)]
pub struct SymbolBits(pub u64);

impl SymbolBits {
    /// The empty set (no symbol allowed). An empty symbolic value is *unsatisfiable*.
    pub const EMPTY: SymbolBits = SymbolBits(0);

    /// A set containing exactly the symbol with the given index.
    #[inline]
    pub const fn single(index: u32) -> SymbolBits {
        SymbolBits(1u64 << index)
    }

    /// The all-symbols set for a feature with `count` symbols (= `full_mask`). An uninstantiated
    /// symbolic value (`IsUninstantiated`) holds exactly this.
    #[inline]
    pub const fn all(count: u32) -> SymbolBits {
        SymbolBits(full_mask(count))
    }

    /// Raw bitset. Mirrors `UlongSymbolicFeatureValueFlags.RawFlags`.
    #[inline]
    pub const fn raw(self) -> u64 {
        self.0
    }

    /// C# `HasAnySet`: at least one symbol allowed (i.e. the value is satisfiable).
    #[inline]
    pub const fn has_any(self) -> bool {
        self.0 != 0
    }

    /// C# `HasAllSet`: every symbol of the feature allowed (uninstantiated). Needs the feature mask.
    #[inline]
    pub const fn has_all(self, mask: u64) -> bool {
        self.0 == mask
    }

    /// C# `Get(symbol)`: is the symbol with this index allowed?
    #[inline]
    pub const fn get(self, index: u32) -> bool {
        (self.0 & (1u64 << index)) != 0
    }

    /// C# `GetFirst`: the lowest-index allowed symbol, or `None` if empty. Iteration order is
    /// symbol `Index` order, so this is the least-significant set bit.
    #[inline]
    pub const fn first(self) -> Option<u32> {
        if self.0 == 0 {
            None
        } else {
            Some(self.0.trailing_zeros())
        }
    }

    /// Add a symbol (C# `Set` for one symbol).
    #[inline]
    pub fn set(&mut self, index: u32) {
        self.0 |= 1u64 << index;
    }

    // --- The four not/notOther-parameterized set predicates and mutators, ported verbatim from
    //     UlongSymbolicFeatureValueFlags. `not` negates `self`, `not_other` negates `other`;
    //     negation is within the feature's `mask`. See that file for the branch derivation.

    /// C# `IsSupersetOf(not, other, notOther)`. (Used by subsumption on the symbolic path.)
    #[inline]
    pub fn is_superset_of(self, not: bool, other: SymbolBits, not_other: bool, mask: u64) -> bool {
        let a = self.0;
        let b = other.0;
        match (not, not_other) {
            (false, false) => (a & b) == b,
            (false, true) => (a & (!b & mask)) == (!b & mask),
            (true, false) => ((!a & mask) & b) == b,
            (true, true) => ((!a & mask) & (!b & mask)) == (!b & mask),
        }
    }

    /// C# `Overlaps(not, other, notOther)`. This is the symbolic-value unifiability test
    /// (non-empty intersection ⇒ unifiable).
    #[inline]
    pub fn overlaps(self, not: bool, other: SymbolBits, not_other: bool, mask: u64) -> bool {
        let a = self.0;
        let b = other.0;
        match (not, not_other) {
            (false, false) => (a & b) != 0,
            (false, true) => (a & (!b & mask)) != 0,
            (true, false) => ((!a & mask) & b) != 0,
            (true, true) => ((!a & mask) & (!b & mask)) != 0,
        }
    }

    /// C# `IntersectWith(not, other, notOther)` — returns the new flags (this is unify).
    #[inline]
    pub fn intersect_with(
        self,
        not: bool,
        other: SymbolBits,
        not_other: bool,
        mask: u64,
    ) -> SymbolBits {
        let a = self.0;
        let b = other.0;
        SymbolBits(match (not, not_other) {
            (false, false) => a & b,
            (false, true) => a & (!b & mask),
            (true, false) => (!a & mask) & b,
            (true, true) => (!a & mask) & (!b & mask),
        })
    }

    /// C# `UnionWith(not, other, notOther)` — returns the new flags.
    #[inline]
    pub fn union_with(
        self,
        not: bool,
        other: SymbolBits,
        not_other: bool,
        mask: u64,
    ) -> SymbolBits {
        let a = self.0;
        let b = other.0;
        SymbolBits(match (not, not_other) {
            (false, false) => a | b,
            (false, true) => a | (!b & mask),
            (true, false) => (!a & mask) | b,
            (true, true) => (!a & mask) | (!b & mask),
        })
    }

    /// C# `ExceptWith(not, other, notOther)` — returns the new flags.
    #[inline]
    pub fn except_with(
        self,
        not: bool,
        other: SymbolBits,
        not_other: bool,
        mask: u64,
    ) -> SymbolBits {
        let a = self.0;
        let b = other.0;
        SymbolBits(match (not, not_other) {
            (false, false) => a & (!b & mask),
            (false, true) => a & b,
            (true, false) => (!a & mask) & (!b & mask),
            (true, true) => (!a & mask) & b,
        })
    }

    /// C# `Not()`: complement within the feature mask.
    #[inline]
    pub fn not(self, mask: u64) -> SymbolBits {
        SymbolBits(!self.0 & mask)
    }
}

/// Whole-FS bit-packed unifiability fast path — the port of C# `FeatureStruct.TryFastUnifiable`.
///
/// `seg` is the segment being matched (a shape node's feature lanes); `constraint` is the FST
/// arc input's lanes. Each lane is one symbolic feature's `SymbolBits` indexed by `FlatIndex`;
/// a lane beyond a slice's length is *unconstrained* (all-ones), matching C#'s `EnsureFlat`
/// seeding absent features to `ulong.MaxValue`.
///
/// Returns `true` iff the two are unifiable, i.e. **no lane has an empty intersection**. This is
/// provably identical to `IsUnifiable(other, useDefaults:false, varBindings:null)` for the
/// simple/no-variable case (a feature absent on either side is the "no constraint" branch, and
/// overlap == unifiable).
///
/// # Precondition (the caller's responsibility, mirroring C#'s guard)
/// The C# fast path only fires when `constraint._flatComplete && seg._flatSafeSegment`: the
/// constraint must be fully bit-packable (no variables / >64-symbol / string / complex features)
/// and every non-packable feature on the segment must be non-symbolic (so a symbolic constraint
/// can't touch it). In this port a FS is stored as flat lanes *only if* it satisfies that; a FS
/// with a non-packable feature is held in the DAG representation and routed to the slow unifier
/// instead. Callers must not pass a non-flat constraint here.
#[inline]
pub fn flat_unifiable(seg: &[u64], constraint: &[u64]) -> bool {
    let n = seg.len().max(constraint.len());
    for i in 0..n {
        let av = seg.get(i).copied().unwrap_or(u64::MAX);
        let bv = constraint.get(i).copied().unwrap_or(u64::MAX);
        if (av & bv) == 0 {
            return false; // a feature has no common symbol ⇒ not unifiable
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_boundary() {
        // The exact boundary #446's regression test guards: 64 symbols ⇒ all-ones, not 0.
        assert_eq!(full_mask(64), u64::MAX);
        assert_eq!(full_mask(63), (1u64 << 63) - 1);
        assert_eq!(full_mask(20), (1u64 << 20) - 1); // Sena's widest feature (20 symbols)
        assert_eq!(full_mask(1), 1);
        assert_eq!(full_mask(0), 0);
    }

    #[test]
    fn basic_set_ops() {
        let mut s = SymbolBits::EMPTY;
        assert!(!s.has_any());
        assert_eq!(s.first(), None);
        s.set(2);
        s.set(5);
        assert!(s.has_any());
        assert!(s.get(2) && s.get(5) && !s.get(3));
        assert_eq!(s.first(), Some(2));
        assert_eq!(
            s,
            SymbolBits::single(2).union_with(false, SymbolBits::single(5), false, full_mask(8))
        );
    }

    #[test]
    fn has_all_needs_mask() {
        let mask = full_mask(3); // 3 symbols, bits 0..=2
        assert!(SymbolBits(0b111).has_all(mask));
        assert!(!SymbolBits(0b011).has_all(mask));
        assert!(SymbolBits::all(3).has_all(mask));
    }

    #[test]
    fn not_is_complement_within_mask() {
        let mask = full_mask(3);
        assert_eq!(SymbolBits(0b010).not(mask), SymbolBits(0b101));
        // double negation is identity within the mask
        assert_eq!(SymbolBits(0b010).not(mask).not(mask), SymbolBits(0b010));
    }

    /// Exhaustively verify the four not/notOther branches of every op against a brute-force
    /// reference over a small symbol universe — this is the parity anchor for the C# formulas.
    #[test]
    fn not_matrix_matches_reference() {
        let count = 5u32;
        let mask = full_mask(count);
        let universe: Vec<u64> = (0..(1u64 << count)).collect();
        // brute-force symbol-set helpers
        let neg = |x: u64| !x & mask;
        for &a in &universe {
            for &b in &universe {
                for &not in &[false, true] {
                    for &noth in &[false, true] {
                        let ea = if not { neg(a) } else { a };
                        let eb = if noth { neg(b) } else { b };
                        let sa = SymbolBits(a);
                        let sb = SymbolBits(b);
                        assert_eq!(
                            sa.overlaps(not, sb, noth, mask),
                            (ea & eb) != 0,
                            "overlaps a={a:#07b} b={b:#07b} not={not} noth={noth}"
                        );
                        assert_eq!(
                            sa.is_superset_of(not, sb, noth, mask),
                            (ea & eb) == eb,
                            "superset a={a:#07b} b={b:#07b} not={not} noth={noth}"
                        );
                        assert_eq!(sa.intersect_with(not, sb, noth, mask).0, ea & eb);
                        assert_eq!(sa.union_with(not, sb, noth, mask).0, ea | eb);
                        assert_eq!(sa.except_with(not, sb, noth, mask).0, ea & neg(eb));
                    }
                }
            }
        }
    }

    #[test]
    fn flat_unify_absent_lane_is_unconstrained() {
        // A shorter slice = trailing features unconstrained (all-ones), so length mismatch alone
        // never blocks unification — matches EnsureFlat's ulong.MaxValue seeding.
        let seg = [0b0011u64, 0b0101u64];
        let constraint = [0b0001u64]; // only constrains lane 0; lane 1 absent = unconstrained
        assert!(flat_unifiable(&seg, &constraint));
        assert!(flat_unifiable(&constraint, &seg));
    }

    #[test]
    fn flat_unify_conflict_blocks() {
        let seg = [0b0010u64, 0b0100u64];
        let constraint = [0b0001u64, 0b0100u64]; // lane 0: 0b0010 & 0b0001 == 0 ⇒ conflict
        assert!(!flat_unifiable(&seg, &constraint));
    }

    #[test]
    fn flat_unify_all_lanes_overlap() {
        let seg = [0b0110u64, 0b1100u64, 0b0001u64];
        let constraint = [0b0100u64, 0b1000u64, 0b0001u64];
        assert!(flat_unifiable(&seg, &constraint));
    }

    #[test]
    fn flat_unify_empty_slices() {
        assert!(flat_unifiable(&[], &[])); // vacuously unifiable
        assert!(flat_unifiable(&[0b101], &[])); // constraint says nothing
    }
}
