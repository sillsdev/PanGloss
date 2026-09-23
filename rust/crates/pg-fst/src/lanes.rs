//! Flat symbolic-feature lane arithmetic used by the FST.
//!
//! A *constraint* / segment is a slice of `u64` lanes, one lane per symbolic feature indexed by
//! `FlatIndex` (§5.3). A lane beyond a slice's length is **unconstrained** (all-ones), exactly as
//! C# `EnsureFlat` seeds absent features to `ulong.MaxValue`. These helpers are the lane-wise
//! ports of the three FS predicates the FST touches:
//!
//! - matching an arc against a segment — `pg_featstruct::flat_unifiable` (`IsUnifiable`, the
//!   `Matcher.UseUnification == true` path; see `Input.Matches`, Input.cs:49-58);
//! - combining two arc conditions during determinization — `flat_unify` (`FeatureStruct.Unify`,
//!   used in `DeterministicGetArcs`, Fst.cs:631);
//! - pruning unsatisfiable determinized arcs / negated conditions — `flat_subsumes`
//!   (`FeatureStruct.Subsumes`, used by `Input.IsSatisfiable`, Input.cs:60-63).
//!
//! Lanes are canonicalized by trimming trailing all-ones lanes (an unconstrained tail is the
//! "absent feature" case), so structurally-equal constraints intern to the same id.

/// One symbolic feature's "all symbols" sentinel: an absent/unconstrained lane.
pub const UNCONSTRAINED: u64 = u64::MAX;

/// Trim trailing unconstrained (all-ones) lanes so equal constraints share a canonical form.
pub fn canonicalize(mut lanes: Vec<u64>) -> Vec<u64> {
    while matches!(lanes.last(), Some(&UNCONSTRAINED)) {
        lanes.pop();
    }
    lanes
}

/// Lane-wise unify (intersect) two flat constraints. Returns `None` if any lane's intersection is
/// empty (the two are not unifiable), else the canonicalized intersection. Port of
/// `FeatureStruct.Unify` on the flat/no-variable path (a lane AND, absent = all-ones).
pub fn flat_unify(a: &[u64], b: &[u64]) -> Option<Vec<u64>> {
    let n = a.len().max(b.len());
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let av = a.get(i).copied().unwrap_or(UNCONSTRAINED);
        let bv = b.get(i).copied().unwrap_or(UNCONSTRAINED);
        let v = av & bv;
        if v == 0 {
            return None;
        }
        out.push(v);
    }
    Some(canonicalize(out))
}

/// Lane-wise subsumption: does `sup` subsume `sub` (i.e. `sup`'s allowed symbols ⊇ `sub`'s, on
/// every lane)? Port of `FeatureStruct.Subsumes` on the flat path. Absent lane = all-ones.
pub fn flat_subsumes(sup: &[u64], sub: &[u64]) -> bool {
    let n = sup.len().max(sub.len());
    for i in 0..n {
        let supv = sup.get(i).copied().unwrap_or(UNCONSTRAINED);
        let subv = sub.get(i).copied().unwrap_or(UNCONSTRAINED);
        if (supv & subv) != subv {
            return false;
        }
    }
    true
}

/// C# `FeatureStruct.IsEmpty` for a flat constraint: the constraint places no restriction on any
/// feature (all lanes unconstrained). After `canonicalize` this is exactly the empty slice.
#[inline]
pub fn is_empty(lanes: &[u64]) -> bool {
    lanes.is_empty()
}

#[cfg(test)]
mod tests;
