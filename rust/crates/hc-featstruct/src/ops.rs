//! Syntactic-domain feature-structure operations over the frozen tree model ([`crate::tree`]).
//!
//! Port of C# `SIL.Machine.FeatureModel.FeatureStruct`'s `IsUnifiable`/`Unify`/`Subsumes`/
//! `PriorityUnion`, restricted to the subset that is actually reachable for authored HC
//! grammars in the syntactic domain (see `tree.rs`'s module doc for why that restriction is
//! sound): trees, not DAGs (no re-entrancy), no variables, no string features, `useDefaults`
//! always `false`.
//!
//! ## What was skipped, and why it's exact (not an approximation)
//! - **Re-entrancy / `Forward` / `Dereference` / the `copies` union-find map.**
//!   `FeatureValue.UnifyImpl` (`FeatureValue.cs:64-118`) only takes the `NondestructiveUnify`
//!   branch when neither operand is already in the `copies` map; that map is populated solely by
//!   *shared* (re-entrant) sub-structures being visited a second time. A tree has no node reached
//!   twice, so `copies` is always empty at every call in this domain and the `NondestructiveUnify`
//!   branch is the *only* branch ever taken — recursing directly over the tree is exact, not an
//!   approximation of the DAG algorithm.
//! - **`VariableBindings`.** `SimpleFeatureValue.IsUnifiableImpl`/`SubsumesImpl`/
//!   `DestructiveUnify` (`SimpleFeatureValue.cs:52-102,104-154,156-235`) each branch on
//!   `IsVariable`; alpha variables are phonological-only in HC, so the syntactic path always
//!   takes the `!IsVariable && !otherSfv.IsVariable` arm.
//! - **`useDefaults`.** Always `false` on this path, so the `useDefaults && ... DefaultValue`
//!   branches in `FeatureStruct.cs` (`IsUnifiableImpl:855-859`, `SubsumesImpl:946-950`,
//!   `NondestructiveUnify:1043-1053`) never fire.
//! - **String features / `not`-negated symbolic values.** [`crate::tree::FeatureValue::Symbolic`]
//!   carries no negation flag (unlike C#'s `not`/`notOther`-parameterized
//!   `ISymbolicFeatureValueFlags` API), so every [`SymbolBits`] call below passes
//!   `not = false, not_other = false`. Inspecting the `(false, false)` arm of each op in
//!   `bitvec.rs` shows the `mask` parameter is unused in that arm, so callers here pass a dummy
//!   `NO_MASK` — this crate's tree model has no per-feature symbol-count metadata to give it.
//!
//! ## Ported semantics, with C# call sites
//! - **`is_unifiable`** ports `FeatureStruct.IsUnifiableImpl` (`FeatureStruct.cs:839-862`), which
//!   walks the *other* operand's features only: a feature present in just one side is vacuously
//!   compatible (unify would simply copy it through), so checking the two sides' *common*
//!   features via a merge-walk is exactly equivalent and symmetric in outcome.
//!   Leaf case ports `SimpleFeatureValue.IsUnifiableImpl`'s non-variable arm
//!   (`SimpleFeatureValue.cs:58-62`: `Overlaps(false, otherSfv, false)`), i.e.
//!   [`SymbolBits::overlaps`] — **non-empty intersection ⇒ unifiable; empty ⇒ the whole
//!   structure fails to unify** (this is the "unify of two symbolic values is set intersection;
//!   empty intersection fails" rule from the task brief).
//! - **`unify`** ports `FeatureStruct.NondestructiveUnify` (`FeatureStruct.cs:1010-1068`): the
//!   output holds every feature from *either* side; a feature present on only one side is copied
//!   through unchanged (`NondestructiveUnify:1056` for other-only, `:1060-1064` for this-only);
//!   a feature present on both sides is recursively unified and the whole operation fails if that
//!   recursive unify fails (`:1036-1041`). Leaf case ports
//!   `SimpleFeatureValue.NondestructiveUnify` (`SimpleFeatureValue.cs:397-415`), which clones and
//!   runs `DestructiveUnify`'s non-variable arm (`SimpleFeatureValue.cs:171-176`:
//!   `Overlaps` check then `IntersectWith`), i.e. [`SymbolBits::intersect_with`] guarded by
//!   [`SymbolBits::overlaps`].
//! - **`subsumes(a, b)`**: **direction — `a` is the more general structure; `subsumes(a, b)` asks
//!   "does `a` (fewer/looser constraints) subsume `b` (as-or-more-specific)?"**, matching C#
//!   `a.Subsumes(b)`. Ports `FeatureStruct.SubsumesImpl` (`FeatureStruct.cs:930-957`), which walks
//!   **`this`'s (`a`'s) own features**: every feature `a` constrains must also be present in `b`
//!   (`:951-954`: missing ⇒ `false` immediately — this is *not* symmetric with `is_unifiable`),
//!   and recursively `a`'s value must subsume `b`'s value. `b`-only features are irrelevant (`a`
//!   doesn't constrain them). Leaf case ports `SimpleFeatureValue.SubsumesImpl`'s non-variable arm
//!   (`SimpleFeatureValue.cs:110-113`: `IsSupersetOf(false, otherSfv, false)`), i.e.
//!   [`SymbolBits::is_superset_of`] — `a`'s allowed-symbol set must be a superset of `b`'s.
//!   Consequently `subsumes(EMPTY, x)` is always `true` (the walk over `EMPTY`'s zero features is
//!   vacuous).
//! - **`priority_union(a, b)`**: **`b`'s values win on conflict.** Ports the private recursive
//!   `FeatureStruct.PriorityUnion` (`FeatureStruct.cs:300-368`, called from the public
//!   `PriorityUnion` at `:286-298`). Unlike `unify`, a leaf conflict is **not** an intersection —
//!   `PriorityUnion` has no per-value merge for `SimpleFeatureValue` at all: whenever the two
//!   sides both have a feature and `b`'s value is *not* itself a nested `FeatureStruct` unified
//!   with a `FeatureStruct` on `a`'s side, `b`'s value simply **overwrites** `a`'s wholesale
//!   (`:340-343` when `b`'s value is complex but `a`'s isn't; `:345-361` whenever `b`'s value is a
//!   `SimpleFeatureValue`, taken regardless of what `a`'s value is). The **only** case that
//!   recurses instead of overwriting is both sides' value being a nested `FeatureStruct`
//!   (`:317-320` first pass, mutating `a`'s copy of the substruct in place before the second pass
//!   re-affirms it at `:334-339`). Features present on only one side pass through unchanged
//!   (`a`-only: implicit, `PriorityUnion` never touches keys absent from `other`; `b`-only:
//!   `:363-366`). This routine has no failure mode in C# (always returns/mutates in place), so
//!   there is no kind-mismatch assertion here — mismatch is already exactly what `b`-wins
//!   naturally produces.
//!
//! ## Feature-kind mismatch (`Symbolic` vs `Complex` under the same [`FeatId`])
//! In a well-typed HC grammar a `Feature` is declared once, globally, as either a
//! `SymbolicFeature` or a `ComplexFeature` (`FeatureSystem`/`XmlLanguageLoader`), so the same
//! [`FeatId`] never holds a [`FeatureValue::Symbolic`] on one side and a
//! [`FeatureValue::Complex`] on the other in `is_unifiable`/`unify`/`subsumes`. C# reaches this
//! only through a failed runtime type-check — `Dereference<T>`'s `as T` cast
//! (`FeatureValue.cs:120-127`) returning `null` — which every relevant override then treats as a
//! graceful `false`/failure, *not* an exception: `FeatureStruct.IsUnifiableImpl:841-843`,
//! `FeatureStruct.SubsumesImpl:932-934`, `FeatureStruct.NondestructiveUnify:1018-1023`, and
//! `SimpleFeatureValue`'s three mirror-image checks (`IsUnifiableImpl:54-56`,
//! `SubsumesImpl:106-108`, `NondestructiveUnify:404-408` via `DestructiveUnify:164-166`). We
//! mirror that exactly — deterministic `false`/`None`, matching C# bit for bit — and additionally
//! `debug_assert!` because reaching this branch in this crate means the grammar loader produced
//! an ill-typed tree, which is a bug worth catching in debug/test builds even though release
//! builds must still degrade gracefully like C# does.

use crate::tree::{FeatId, FeatureStruct, FeatureStructBuilder, FeatureValue};
use std::cmp::Ordering;

/// Dummy mask for the three `SymbolBits` ops used here ([`SymbolBits::overlaps`],
/// [`SymbolBits::is_superset_of`], [`SymbolBits::intersect_with`]): all are called with
/// `not = false, not_other = false`, and inspecting the `(false, false)` arm of each
/// (`bitvec.rs`) shows `mask` is never read on that arm. This tree model has no per-feature
/// symbol-count metadata to supply a real mask, and none is needed for the un-negated case.
const NO_MASK: u64 = 0;

/// Port of `FeatureStruct.IsUnifiableImpl` (`FeatureStruct.cs:839-862`) for the tree/no-variable
/// subset: `true` iff every feature the two structures have *in common* has unifiable values,
/// recursively. Features present on only one side never block unifiability (see module docs).
pub fn is_unifiable(a: &FeatureStruct, b: &FeatureStruct) -> bool {
    let ae = a.entries();
    let be = b.entries();
    let (mut i, mut j) = (0usize, 0usize);
    while i < ae.len() && j < be.len() {
        match ae[i].0.cmp(&be[j].0) {
            Ordering::Less => i += 1,
            Ordering::Greater => j += 1,
            Ordering::Equal => {
                if !value_is_unifiable(&ae[i].1, &be[j].1) {
                    return false;
                }
                i += 1;
                j += 1;
            }
        }
    }
    true
}

fn value_is_unifiable(a: &FeatureValue, b: &FeatureValue) -> bool {
    match (a, b) {
        (FeatureValue::Symbolic(sa), FeatureValue::Symbolic(sb)) => {
            // SimpleFeatureValue.IsUnifiableImpl, non-variable arm (SimpleFeatureValue.cs:58-62).
            sa.overlaps(false, *sb, false, NO_MASK)
        }
        (FeatureValue::Complex(fa), FeatureValue::Complex(fb)) => is_unifiable(fa, fb),
        _ => {
            debug_assert!(
                false,
                "feature-kind mismatch (Symbolic vs Complex under the same FeatId); a Feature \
                 is globally either symbolic or complex in a well-typed HC grammar — see ops.rs \
                 module docs"
            );
            false
        }
    }
}

/// Port of `FeatureStruct.NondestructiveUnify` (`FeatureStruct.cs:1010-1068`) for the
/// tree/no-variable subset. `Some(fs)` with `fs` holding the union of both sides' features
/// (common features recursively unified), or `None` if any common feature fails to unify
/// (empty symbolic intersection, or a nested unify failing).
pub fn unify(a: &FeatureStruct, b: &FeatureStruct) -> Option<FeatureStruct> {
    let ae = a.entries();
    let be = b.entries();
    let mut builder = FeatureStructBuilder::new();
    let (mut i, mut j) = (0usize, 0usize);
    loop {
        match (ae.get(i), be.get(j)) {
            (Some((fa, va)), Some((fb, vb))) => match fa.cmp(fb) {
                Ordering::Less => {
                    builder.add(*fa, va.clone());
                    i += 1;
                }
                Ordering::Greater => {
                    builder.add(*fb, vb.clone());
                    j += 1;
                }
                Ordering::Equal => {
                    let merged = unify_value(va, vb)?;
                    builder.add(*fa, merged);
                    i += 1;
                    j += 1;
                }
            },
            (Some((fa, va)), None) => {
                builder.add(*fa, va.clone());
                i += 1;
            }
            (None, Some((fb, vb))) => {
                builder.add(*fb, vb.clone());
                j += 1;
            }
            (None, None) => break,
        }
    }
    Some(builder.build())
}

fn unify_value(a: &FeatureValue, b: &FeatureValue) -> Option<FeatureValue> {
    match (a, b) {
        (FeatureValue::Symbolic(sa), FeatureValue::Symbolic(sb)) => {
            // SimpleFeatureValue.NondestructiveUnify -> DestructiveUnify's non-variable arm
            // (SimpleFeatureValue.cs:171-176): Overlaps guard, then IntersectWith.
            if sa.overlaps(false, *sb, false, NO_MASK) {
                Some(FeatureValue::Symbolic(
                    sa.intersect_with(false, *sb, false, NO_MASK),
                ))
            } else {
                None
            }
        }
        (FeatureValue::Complex(fa), FeatureValue::Complex(fb)) => {
            unify(fa, fb).map(FeatureValue::Complex)
        }
        _ => {
            debug_assert!(
                false,
                "feature-kind mismatch (Symbolic vs Complex under the same FeatId); a Feature \
                 is globally either symbolic or complex in a well-typed HC grammar — see ops.rs \
                 module docs"
            );
            None
        }
    }
}

/// Port of `FeatureStruct.SubsumesImpl` (`FeatureStruct.cs:930-957`) for the tree/no-variable
/// subset. **Direction**: `subsumes(a, b)` is `a.Subsumes(b)` in C# — `a` is the more general
/// structure; every feature `a` constrains must be present in `b` with a value `a`'s value
/// (symbolically) is a superset of, recursively. `b`-only features are unconstrained by `a` and
/// don't affect the result. `subsumes(FeatureStruct::EMPTY, _)` is always `true`.
pub fn subsumes(a: &FeatureStruct, b: &FeatureStruct) -> bool {
    for (fa, va) in a.entries() {
        match b.get(*fa) {
            Some(vb) => {
                if !value_subsumes(va, vb) {
                    return false;
                }
            }
            None => return false,
        }
    }
    true
}

fn value_subsumes(a: &FeatureValue, b: &FeatureValue) -> bool {
    match (a, b) {
        (FeatureValue::Symbolic(sa), FeatureValue::Symbolic(sb)) => {
            // SimpleFeatureValue.SubsumesImpl, non-variable arm (SimpleFeatureValue.cs:110-113).
            sa.is_superset_of(false, *sb, false, NO_MASK)
        }
        (FeatureValue::Complex(fa), FeatureValue::Complex(fb)) => subsumes(fa, fb),
        _ => {
            debug_assert!(
                false,
                "feature-kind mismatch (Symbolic vs Complex under the same FeatId); a Feature \
                 is globally either symbolic or complex in a well-typed HC grammar — see ops.rs \
                 module docs"
            );
            false
        }
    }
}

/// Port of the private recursive `FeatureStruct.PriorityUnion` (`FeatureStruct.cs:300-368`) for
/// the tree/no-variable subset. **`b`'s values win on conflict**: for a feature present on both
/// sides, if both values are nested [`FeatureStruct`]s they're recursively priority-unioned,
/// otherwise `b`'s value overwrites `a`'s wholesale (including type mismatches — see module
/// docs). Features present on only one side pass through unchanged. Always succeeds (matches C#,
/// which has no failure mode for this operation).
pub fn priority_union(a: &FeatureStruct, b: &FeatureStruct) -> FeatureStruct {
    let ae = a.entries();
    let be = b.entries();
    let mut builder = FeatureStructBuilder::new();
    let (mut i, mut j) = (0usize, 0usize);
    loop {
        match (ae.get(i), be.get(j)) {
            (Some((fa, va)), Some((fb, vb))) => match fa.cmp(fb) {
                Ordering::Less => {
                    builder.add(*fa, va.clone());
                    i += 1;
                }
                Ordering::Greater => {
                    builder.add(*fb, vb.clone());
                    j += 1;
                }
                Ordering::Equal => {
                    let merged = match (va, vb) {
                        (FeatureValue::Complex(cfa), FeatureValue::Complex(cfb)) => {
                            FeatureValue::Complex(priority_union(cfa, cfb))
                        }
                        _ => vb.clone(),
                    };
                    builder.add(*fa, merged);
                    i += 1;
                    j += 1;
                }
            },
            (Some((fa, va)), None) => {
                builder.add(*fa, va.clone());
                i += 1;
            }
            (None, Some((fb, vb))) => {
                builder.add(*fb, vb.clone());
                j += 1;
            }
            (None, None) => break,
        }
    }
    builder.build()
}

/// Port of `FeatureStruct.AddImpl`/`SimpleFeatureValue.AddImpl` (`FeatureStruct.cs:453-505`,
/// `SimpleFeatureValue.cs:237-244`, which delegates straight to `UnionImpl`/`UnionWith` for the
/// non-variable arm) — the analysis-side **widening** operator, as opposed to `unify`'s
/// narrowing. `add(a, b)` folds `b`'s features into `a`:
///
/// - A feature present **only in `a`** passes through untouched — `FeatureStruct.AddImpl` walks
///   `otherFS._definite` (i.e. `b`'s keys) exclusively, so an `a`-only key is never visited
///   (`FeatureStruct.cs:481`).
/// - A feature present **only in `b`** is copied in as if `a` held a fresh empty value of the
///   same kind at that key first (`FeatureStruct.cs:489-497`: `new FeatureStruct()` /
///   `new StringFeatureValue()` / `new SymbolicFeatureValue(feature)`, all of which start with
///   zero allowed symbols / zero sub-features) — for the symbolic leaf that is exactly
///   `union(EMPTY, b)`, i.e. `b`'s value verbatim; for a nested struct it is `add(EMPTY, b)`,
///   i.e. a deep structural copy of `b`.
/// - A feature present **on both sides** is replaced by the **union of its two value sets**
///   (`SymbolicFeatureValue.UnionWith`'s non-variable arm, `SymbolicFeatureValue.cs:164-171`: a
///   plain bitset OR) rather than an intersection.
///
/// Unlike `unify`, `add` **never fails**. Instead, whenever a symbolic value's post-union bitset
/// covers every symbol declared for that feature — `SimpleFeatureValue.IsUninstantiated`
/// (`SimpleFeatureValue.cs:543-546`) for a non-variable value reduces to
/// `SymbolicFeatureValue`'s override `HasAllSet()` (`SymbolicFeatureValue.cs:134-137`) — the
/// feature is **removed** from the result instead of being kept at "all values allowed"
/// (`FeatureStruct.cs:499-500`: `if (!thisValue.AddImpl(...)) _definite.Remove(featVal.Key)`).
/// "All values allowed" and "feature absent" are semantically identical to every other op in this
/// module (`is_unifiable`/`unify`/`subsumes` all treat a missing feature as unconstrained), but
/// they are *not* identical as an accumulator for a **later** `add`: C# deleting the key lets the
/// next rule's `add` on that feature start over from empty, whereas leaving it at "all" would
/// make the feature permanently unconstrained-and-stuck the first time two rules' required values
/// happen to be complementary. A nested [`FeatureStruct`] value has the analogous condition —
/// `_definite.Count > 0` (`FeatureStruct.cs:504`) — i.e. the recursively-added substruct becoming
/// empty deletes the parent key the same way.
///
/// This is the one operation in this module that needs a per-feature symbol-count mask (to test
/// "all bits set"); this tree model carries none, so callers supply `mask_of(feat)` (in this
/// crate's grammar-loading caller, `SynFeatureSystem::mask`).
pub fn add(
    a: &FeatureStruct,
    b: &FeatureStruct,
    mask_of: &impl Fn(FeatId) -> u64,
) -> FeatureStruct {
    let ae = a.entries();
    let be = b.entries();
    let mut builder = FeatureStructBuilder::new();
    let (mut i, mut j) = (0usize, 0usize);
    loop {
        match (ae.get(i), be.get(j)) {
            (Some((fa, va)), Some((fb, vb))) => match fa.cmp(fb) {
                Ordering::Less => {
                    // `a`-only key: untouched (FeatureStruct.AddImpl never visits it).
                    builder.add(*fa, va.clone());
                    i += 1;
                }
                Ordering::Greater => {
                    // `b`-only key: seed-from-empty then add (FeatureStruct.cs:489-500).
                    if let Some(v) = add_value(*fb, None, vb, mask_of) {
                        builder.add(*fb, v);
                    }
                    j += 1;
                }
                Ordering::Equal => {
                    if let Some(v) = add_value(*fa, Some(va), vb, mask_of) {
                        builder.add(*fa, v);
                    }
                    i += 1;
                    j += 1;
                }
            },
            (Some((fa, va)), None) => {
                builder.add(*fa, va.clone());
                i += 1;
            }
            (None, Some((fb, vb))) => {
                if let Some(v) = add_value(*fb, None, vb, mask_of) {
                    builder.add(*fb, v);
                }
                j += 1;
            }
            (None, None) => break,
        }
    }
    builder.build()
}

/// One key's `add`: `a` is `None` when the key is absent from the accumulator (seed-from-empty
/// case, `FeatureStruct.cs:489-497`). Returns `None` when the result is "uninstantiated" (all
/// symbols allowed / substruct empty), signaling the caller to delete the key
/// (`FeatureStruct.cs:499-500`).
fn add_value(
    feat: FeatId,
    a: Option<&FeatureValue>,
    b: &FeatureValue,
    mask_of: &impl Fn(FeatId) -> u64,
) -> Option<FeatureValue> {
    match b {
        FeatureValue::Symbolic(sb) => {
            let sa = match a {
                Some(FeatureValue::Symbolic(sa)) => *sa,
                Some(FeatureValue::Complex(_)) => {
                    debug_assert!(
                        false,
                        "feature-kind mismatch (Symbolic vs Complex under the same FeatId); a \
                         Feature is globally either symbolic or complex in a well-typed HC \
                         grammar — see ops.rs module docs"
                    );
                    return None;
                }
                None => crate::bitvec::SymbolBits::EMPTY,
            };
            // SymbolicFeatureValue.UnionWith's non-variable arm (SymbolicFeatureValue.cs:164-171):
            // plain bitset OR (the `(false, false)` arm ignores `mask`, see `NO_MASK`'s doc).
            let merged = sa.union_with(false, *sb, false, NO_MASK);
            if merged.has_all(mask_of(feat)) {
                None
            } else {
                Some(FeatureValue::Symbolic(merged))
            }
        }
        FeatureValue::Complex(fb) => {
            let fa = match a {
                Some(FeatureValue::Complex(fa)) => fa.clone(),
                Some(FeatureValue::Symbolic(_)) => {
                    debug_assert!(
                        false,
                        "feature-kind mismatch (Symbolic vs Complex under the same FeatId); a \
                         Feature is globally either symbolic or complex in a well-typed HC \
                         grammar — see ops.rs module docs"
                    );
                    return None;
                }
                None => FeatureStruct::EMPTY,
            };
            let merged = add(&fa, fb, mask_of);
            if merged.is_empty() {
                None
            } else {
                Some(FeatureValue::Complex(merged))
            }
        }
    }
}

/// Port of `FeatureStruct.Subtract`/`SubtractImpl` (`FeatureStruct.cs:507-549`) +
/// `SimpleFeatureValue.SubtractImpl` (`SimpleFeatureValue.cs:329-383`), restricted to the tree/
/// no-variable subset (see module docs): walks **`b`'s** features only (`FeatureStruct.cs:535`
/// iterates `otherFS._definite`); a feature `b` has that `a` lacks is vacuously skipped
/// (`FeatureStruct.cs:539`'s `TryGetValue` guard — `a` never gains a feature it didn't have).
/// A feature present on both sides is narrowed: a `Symbolic` leaf has `b`'s allowed symbols
/// removed from `a`'s (`ExceptWith(false, otherSfv, false)`, a plain `a & !b`); a `Complex`
/// value recurses. Either way, if the narrowed value becomes "unsatisfiable" — an empty symbol
/// set (`SimpleFeatureValue.IsSatisfiable` false) or an empty substruct (`_definite.Count == 0`)
/// — the key is dropped entirely from the result, mirroring `FeatureStruct.cs:542-543`'s
/// `_definite.Remove`. `a`-only features pass through unchanged. Used by
/// `hc-rules::stratum::choose_inflectional_stem` (`SynthesisAffixTemplatesRule.cs:99-100`'s
/// `remainder`) and `hc-rules::word::Word::expand_alternatives` (`Word.cs:517-518`'s realizational-
/// FS diff).
pub fn subtract(a: &FeatureStruct, b: &FeatureStruct) -> FeatureStruct {
    let mut builder = FeatureStructBuilder::new();
    for (feat, aval) in a.entries() {
        match b.get(*feat) {
            None => {
                builder.add(*feat, aval.clone());
            }
            Some(bval) => match (aval, bval) {
                (FeatureValue::Symbolic(abits), FeatureValue::Symbolic(bbits)) => {
                    let diff = crate::bitvec::SymbolBits(abits.0 & !bbits.0);
                    if diff.0 != 0 {
                        builder.add(*feat, FeatureValue::Symbolic(diff));
                    }
                    // else: emptied -> IsSatisfiable false -> key dropped (see doc above).
                }
                (FeatureValue::Complex(ac), FeatureValue::Complex(bc)) => {
                    let sub = subtract(ac, bc);
                    if !sub.is_empty() {
                        builder.add(*feat, FeatureValue::Complex(sub));
                    }
                }
                _ => {
                    // Feature-kind mismatch (see module docs' shared rationale): keep `a`'s value
                    // untouched rather than the C# graceful-false/no-op some of the sibling ops
                    // use — `SubtractImpl`'s `Dereference<SimpleFeatureValue>`/`<FeatureStruct>`
                    // failure returns `true` (`SimpleFeatureValue.cs:336-337`,
                    // `FeatureStruct.cs:528`'s implicit `Dereference` guard), i.e. "no change".
                    debug_assert!(
                        false,
                        "feature-kind mismatch (Symbolic vs Complex under the same FeatId); a \
                         Feature is globally either symbolic or complex in a well-typed HC \
                         grammar — see ops.rs module docs"
                    );
                    builder.add(*feat, aval.clone());
                }
            },
        }
    }
    builder.build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bitvec::SymbolBits;
    use crate::tree::FeatId;

    // ---- symbol-bit helpers for the hand-ported tests -------------------------------------
    // Each C# `FeatureSymbol` becomes one bit, in declaration order (bit i = i-th declared
    // symbol), matching tree.rs's documented convention.

    fn sym(bits: u64) -> FeatureValue {
        FeatureValue::Symbolic(SymbolBits(bits))
    }

    fn fs(entries: &[(FeatId, FeatureValue)]) -> FeatureStruct {
        let mut b = FeatureStructBuilder::new();
        for (f, v) in entries {
            b.add(*f, v.clone());
        }
        b.build()
    }

    // ---- hand cases ported from FeatureStructTests.cs --------------------------------------
    //
    // "simple" fixture (FeatureStructTests.cs TestBinaryOperation:614-629): features
    // a={a1,a2,a3}, b={b1,b2,b3}, c={c1,c2,c3} -> FeatId(0)=a, FeatId(1)=b, FeatId(2)=c;
    // symbol i (1-based, e.g. a1) -> bit (i-1) (e.g. a1 -> bit 0, a2 -> bit 1).

    const FA: FeatId = FeatId(0);
    const FB: FeatId = FeatId(1);
    const FC: FeatId = FeatId(2);

    /// Mirrors `Unify` test case 0 (FeatureStructTests.cs:29-30, fixture at :623-625):
    /// fs1={a:a1,b:b1}, fs2={a:a2,c:c2} -> disjoint on `a` -> whole unify fails.
    #[test]
    fn unify_simple_disjoint_fails() {
        let a = fs(&[(FA, sym(0b001)), (FB, sym(0b001))]);
        let b = fs(&[(FA, sym(0b010)), (FC, sym(0b010))]);
        assert_eq!(unify(&a, &b), None);
    }

    /// Mirrors `IsUnifiable` test case 0 (FeatureStructTests.cs:213-214): same fixture, expects
    /// `false`.
    #[test]
    fn is_unifiable_simple_disjoint_is_false() {
        let a = fs(&[(FA, sym(0b001)), (FB, sym(0b001))]);
        let b = fs(&[(FA, sym(0b010)), (FC, sym(0b010))]);
        assert!(!is_unifiable(&a, &b));
    }

    /// Mirrors `Unify` test case 1 (FeatureStructTests.cs:30, fixture at :627-629):
    /// fs1={a:{a1,a2},b:b1,c:c2}, fs2={a:a2,c:c2} -> a intersects to {a2}, b passes through
    /// (only in fs1), c intersects to {c2} (equal on both sides) -> {a2,b1,c2}.
    #[test]
    fn unify_simple_overlap_succeeds() {
        let a = fs(&[(FA, sym(0b011)), (FB, sym(0b001)), (FC, sym(0b010))]);
        let b = fs(&[(FA, sym(0b010)), (FC, sym(0b010))]);
        let expected = fs(&[(FA, sym(0b010)), (FB, sym(0b001)), (FC, sym(0b010))]);
        assert_eq!(unify(&a, &b), Some(expected));
    }

    /// Mirrors `IsUnifiable` test case 1 (FeatureStructTests.cs:214): same fixture, expects
    /// `true`, and cross-checks `is_unifiable(a,b) == unify(a,b).is_some()`.
    #[test]
    fn is_unifiable_simple_overlap_is_true() {
        let a = fs(&[(FA, sym(0b011)), (FB, sym(0b001)), (FC, sym(0b010))]);
        let b = fs(&[(FA, sym(0b010)), (FC, sym(0b010))]);
        assert!(is_unifiable(&a, &b));
        assert_eq!(is_unifiable(&a, &b), unify(&a, &b).is_some());
    }

    /// Mirrors `PriorityUnion` test case 0 (FeatureStructTests.cs:264-265): `b` overwrites the
    /// common feature `a` (not an intersection, unlike unify) -> {a2,b1,c2}.
    #[test]
    fn priority_union_simple_overwrite() {
        let a = fs(&[(FA, sym(0b001)), (FB, sym(0b001))]);
        let b = fs(&[(FA, sym(0b010)), (FC, sym(0b010))]);
        let expected = fs(&[(FA, sym(0b010)), (FB, sym(0b001)), (FC, sym(0b010))]);
        assert_eq!(priority_union(&a, &b), expected);
    }

    // "complex" fixture (FeatureStructTests.cs TestBinaryOperation:631-683): complex features
    // cx1, cx2, cx3, cx4, each holding one symbolic feature (a/b/c/d respectively, 3 symbols
    // each) -> FeatId(0)=cx1, FeatId(1)=cx2, FeatId(2)=cx3, FeatId(3)=cx4; the nested leaf
    // feature inside each is FeatId(10) (a single reusable slot, since each cx's payload is a
    // one-feature FS in the fixture).

    const CX1: FeatId = FeatId(0);
    const CX2: FeatId = FeatId(1);
    const CX3: FeatId = FeatId(2);
    const CX4: FeatId = FeatId(3);
    const LEAF: FeatId = FeatId(10);

    fn leaf(bits: u64) -> FeatureValue {
        FeatureValue::Complex(fs(&[(LEAF, sym(bits))]))
    }

    /// Mirrors `Unify` test case 2 (FeatureStructTests.cs:32-44, fixture at :645-663):
    /// fs1={cx1:{a1},cx2:{b1},cx4:{d1}}, fs2={cx1:{a2},cx3:{c2},cx4:{d2,d3}} -> `cx1`'s nested
    /// symbolic values ({a1} vs {a2}) are disjoint at depth 2 -> whole unify fails.
    #[test]
    fn unify_complex_disjoint_fails_at_depth2() {
        let a = fs(&[(CX1, leaf(0b001)), (CX2, leaf(0b001)), (CX4, leaf(0b001))]);
        let b = fs(&[(CX1, leaf(0b010)), (CX3, leaf(0b010)), (CX4, leaf(0b110))]);
        assert_eq!(unify(&a, &b), None);
    }

    /// Mirrors `IsUnifiable` test case 2 (FeatureStructTests.cs:216): same fixture, expects
    /// `false`.
    #[test]
    fn is_unifiable_complex_disjoint_is_false() {
        let a = fs(&[(CX1, leaf(0b001)), (CX2, leaf(0b001)), (CX4, leaf(0b001))]);
        let b = fs(&[(CX1, leaf(0b010)), (CX3, leaf(0b010)), (CX4, leaf(0b110))]);
        assert!(!is_unifiable(&a, &b));
    }

    /// Mirrors `Unify` test case 3 (FeatureStructTests.cs:33-44, fixture at :665-683):
    /// fs1={cx1:{a1,a2},cx2:{b1},cx4:{d1}}, fs2={cx1:{a2},cx3:{c2},cx4:{d1,d2}} -> cx1 intersects
    /// to {a2}, cx2/cx3 pass through (present on only one side each), cx4 intersects to {d1} ->
    /// succeeds at depth 2.
    #[test]
    fn unify_complex_succeeds_at_depth2() {
        let a = fs(&[(CX1, leaf(0b011)), (CX2, leaf(0b001)), (CX4, leaf(0b001))]);
        let b = fs(&[(CX1, leaf(0b010)), (CX3, leaf(0b010)), (CX4, leaf(0b011))]);
        let expected = fs(&[
            (CX1, leaf(0b010)),
            (CX2, leaf(0b001)),
            (CX3, leaf(0b010)),
            (CX4, leaf(0b001)),
        ]);
        assert_eq!(unify(&a, &b), Some(expected));
    }

    /// Mirrors `IsUnifiable` test case 3 (FeatureStructTests.cs:217): same fixture, expects
    /// `true`.
    #[test]
    fn is_unifiable_complex_succeeds_is_true() {
        let a = fs(&[(CX1, leaf(0b011)), (CX2, leaf(0b001)), (CX4, leaf(0b001))]);
        let b = fs(&[(CX1, leaf(0b010)), (CX3, leaf(0b010)), (CX4, leaf(0b011))]);
        assert!(is_unifiable(&a, &b));
    }

    /// Mirrors `PriorityUnion` test case 2 (FeatureStructTests.cs:267-278, fixture at
    /// :645-663): `cx1` and `cx4` are both-complex conflicts, but `PriorityUnion` still
    /// overwrites wholesale at the leaf (`b` wins, no intersection) since the *nested* leaf
    /// value is symbolic, not complex — only complex-vs-complex triggers the recursive merge,
    /// and here that recursion bottoms out at a symbolic overwrite one level down.
    #[test]
    fn priority_union_complex_overwrite_at_depth2() {
        let a = fs(&[(CX1, leaf(0b001)), (CX2, leaf(0b001)), (CX4, leaf(0b001))]);
        let b = fs(&[(CX1, leaf(0b010)), (CX3, leaf(0b010)), (CX4, leaf(0b110))]);
        let expected = fs(&[
            (CX1, leaf(0b010)),
            (CX2, leaf(0b001)),
            (CX3, leaf(0b010)),
            (CX4, leaf(0b110)),
        ]);
        assert_eq!(priority_union(&a, &b), expected);
    }

    /// A genuine both-complex conflict one level up: `cx1` in `a` is the *complex* fixture used
    /// as `LEAF`'s container, `cx1` in `b` wraps another complex layer — exercises the
    /// recursive-merge branch of `priority_union` (not the overwrite branch), confirming
    /// `b`'s inner leaf wins while unrelated inner features on each side pass through.
    #[test]
    fn priority_union_recurses_when_both_sides_complex_at_depth2() {
        // a.cx1 = { leaf: a1, cx2-nested-only-in-a: b1 }; b.cx1 = { leaf: a2 }.
        let inner_a = fs(&[(LEAF, sym(0b001)), (CX2, sym(0b001))]);
        let inner_b = fs(&[(LEAF, sym(0b010))]);
        let a = fs(&[(CX1, FeatureValue::Complex(inner_a))]);
        let b = fs(&[(CX1, FeatureValue::Complex(inner_b))]);

        let expected_inner = fs(&[(LEAF, sym(0b010)), (CX2, sym(0b001))]);
        let expected = fs(&[(CX1, FeatureValue::Complex(expected_inner))]);
        assert_eq!(priority_union(&a, &b), expected);
    }

    // ---- subsumes: direction and EMPTY behavior (no direct C# unit test exists for
    // FeatureStruct.Subsumes in FeatureStructTests.cs; ported straight from
    // FeatureStruct.cs:930-957 / SimpleFeatureValue.cs:104-154 reading) --------------------

    #[test]
    fn subsumes_empty_subsumes_everything() {
        let x = fs(&[(FA, sym(0b010)), (FC, leaf(0b001))]);
        assert!(subsumes(&FeatureStruct::EMPTY, &x));
    }

    #[test]
    fn subsumes_direction_more_general_subsumes_more_specific() {
        // a allows {a1,a2}; b is narrowed to just {a1} -> a (looser) subsumes b (tighter).
        let general = fs(&[(FA, sym(0b011))]);
        let specific = fs(&[(FA, sym(0b001))]);
        assert!(subsumes(&general, &specific));
        // The reverse does not hold: the tighter set is not a superset of the looser one.
        assert!(!subsumes(&specific, &general));
    }

    #[test]
    fn subsumes_fails_when_a_has_a_feature_b_lacks() {
        // a constrains `b` (FeatId(1)), which `other` doesn't have at all -> false immediately
        // (FeatureStruct.cs:951-954), regardless of any other feature agreeing.
        let a = fs(&[(FA, sym(0b001)), (FB, sym(0b001))]);
        let b = fs(&[(FA, sym(0b001))]);
        assert!(!subsumes(&a, &b));
        // But the reverse holds: b has no feature that isn't also satisfied/absent-in-a's walk.
        assert!(subsumes(&b, &a));
    }

    // ---- add: analysis-side widening (FeatureStruct.cs:453-505) -- no direct C# unit test
    // exists for FeatureStruct.Add in FeatureStructTests.cs; hand-ported from the source
    // reading, reusing the "simple" (FA/FB/FC, 3 symbols each) and "complex" (CX1..CX4/LEAF)
    // fixtures above so each case can be directly contrasted with the matching unify/
    // priority_union case.

    /// Every fixture feature here has 3 symbols (bits 0..=2) -> full domain is `0b111`.
    fn mask3(_: FeatId) -> u64 {
        0b111
    }

    /// A feature present only in `a` is untouched (`FeatureStruct.AddImpl` only walks `other`'s
    /// keys, `FeatureStruct.cs:481`); a feature present only in `b` is copied in verbatim (as if
    /// seeded from an empty value and unioned, `FeatureStruct.cs:489-500`).
    #[test]
    fn add_singleton_keys_pass_through_or_are_seeded() {
        let a = fs(&[(FA, sym(0b001))]);
        let b = fs(&[(FB, sym(0b010))]);
        let expected = fs(&[(FA, sym(0b001)), (FB, sym(0b010))]);
        assert_eq!(add(&a, &b, &mask3), expected);
    }

    /// Same feature on both sides: contrast with `unify`, which *intersects* -- `add` instead
    /// *unions* the two value sets (`SymbolicFeatureValue.cs:164-171`'s `UnionWith`), and here
    /// the union (`0b011`) doesn't cover the feature's full 3-symbol domain, so the key survives.
    #[test]
    fn add_conflicting_values_unions_not_intersects() {
        let a = fs(&[(FA, sym(0b001))]);
        let b = fs(&[(FA, sym(0b010))]);
        assert_eq!(add(&a, &b, &mask3), fs(&[(FA, sym(0b011))]));
    }

    /// Reuses `unify_simple_disjoint_fails`'s exact fixture (FeatureStructTests.cs:29-30, :623-
    /// 625: fs1={a:a1,b:b1}, fs2={a:a2,c:c2}) -- `unify` fails outright on this input because `a`
    /// is disjoint on both sides; `add` never fails (`FeatureStruct.cs`'s `Add`/`AddImpl` have no
    /// failure return path) and instead unions `a` (`0b011`), passes `b` through (only in `a`),
    /// and seeds `c` from empty (only in `b`).
    #[test]
    fn add_never_fails_where_unify_would() {
        let a = fs(&[(FA, sym(0b001)), (FB, sym(0b001))]);
        let b = fs(&[(FA, sym(0b010)), (FC, sym(0b010))]);
        assert!(unify(&a, &b).is_none());
        let expected = fs(&[(FA, sym(0b011)), (FB, sym(0b001)), (FC, sym(0b010))]);
        assert_eq!(add(&a, &b, &mask3), expected);
    }

    /// When the union of a shared feature's two value sets covers every symbol declared for that
    /// feature, the feature is *uninstantiated* (`SimpleFeatureValue.cs:543-546` composed with
    /// `SymbolicFeatureValue.cs:134-137`'s `HasAllSet`) and `FeatureStruct.AddImpl` deletes the
    /// key outright (`FeatureStruct.cs:499-500`) rather than keeping it at "all values allowed".
    #[test]
    fn add_full_domain_union_deletes_the_key() {
        let a = fs(&[(FA, sym(0b011)), (FB, sym(0b001))]);
        let b = fs(&[(FA, sym(0b100))]); // 0b011 | 0b100 == 0b111 == the full 3-symbol domain.
        let expected = fs(&[(FB, sym(0b001))]); // FA is gone entirely, not left at 0b111.
        assert_eq!(add(&a, &b, &mask3), expected);
    }

    /// Same deletion rule applies to a `b`-only key seeded from empty: if `b`'s own value already
    /// spans the full domain, "seed from empty and union" produces the same uninstantiated value,
    /// so the key is dropped instead of being materialized at "all values allowed".
    #[test]
    fn add_seed_from_empty_at_full_domain_still_deletes() {
        let b = fs(&[(FA, sym(0b111))]);
        assert_eq!(add(&FeatureStruct::EMPTY, &b, &mask3), FeatureStruct::EMPTY);
    }

    /// A nested `FeatureStruct` value on both sides recurses (mirroring `unify`'s and
    /// `priority_union`'s complex-vs-complex branches), but the leaf op is still union, not
    /// intersection or overwrite.
    #[test]
    fn add_recurses_into_nested_complex_values() {
        let a = fs(&[(CX1, leaf(0b001))]);
        let b = fs(&[(CX1, leaf(0b010))]);
        assert_eq!(add(&a, &b, &mask3), fs(&[(CX1, leaf(0b011))]));
    }

    /// The nested-struct analogue of `add_full_domain_union_deletes_the_key`: when the inner
    /// leaf's union spans its full domain, the inner key is deleted, the inner struct becomes
    /// empty, and `FeatureStruct.cs:504`'s `_definite.Count > 0` check then deletes the *outer*
    /// key too, cascading the "uninstantiated" deletion up one level.
    #[test]
    fn add_nested_complex_deletion_cascades_to_parent_key() {
        let a = fs(&[(CX1, leaf(0b011)), (CX2, leaf(0b001))]);
        let b = fs(&[(CX1, leaf(0b100))]); // inner LEAF union 0b011|0b100 == 0b111 -> inner empty.
        let expected = fs(&[(CX2, leaf(0b001))]); // CX1 gone entirely, CX2 untouched (a-only).
        assert_eq!(add(&a, &b, &mask3), expected);
    }

    // ---- property tests over a small universe (2-3 features, <=3 symbols, depth <=2) --------
    // Universe: FA, FB symbolic (3 symbols -> bits 0..=2, each either absent or one of the 7
    // non-empty subsets — authored grammars never produce an unsatisfiable/empty symbol value,
    // so we don't generate SymbolBits::EMPTY); FC complex, wrapping a depth-1 sub-FS that itself
    // only varies FA (absent, or one of 7 non-empty subsets) -> total nesting depth 2.

    fn symbol_options() -> Vec<Option<SymbolBits>> {
        let mut v = vec![None];
        for bits in 1u64..=0b111 {
            v.push(Some(SymbolBits(bits)));
        }
        v
    }

    fn universe() -> Vec<FeatureStruct> {
        let sym_opts = symbol_options();
        let mut nested_opts: Vec<Option<FeatureStruct>> = vec![None];
        for bits in sym_opts.iter().flatten() {
            let mut b = FeatureStructBuilder::new();
            b.add(FA, FeatureValue::Symbolic(*bits));
            nested_opts.push(Some(b.build()));
        }

        let mut out = Vec::new();
        for a_opt in &sym_opts {
            for b_opt in &sym_opts {
                for c_opt in &nested_opts {
                    let mut b = FeatureStructBuilder::new();
                    if let Some(bits) = a_opt {
                        b.add(FA, FeatureValue::Symbolic(*bits));
                    }
                    if let Some(bits) = b_opt {
                        b.add(FB, FeatureValue::Symbolic(*bits));
                    }
                    if let Some(nested) = c_opt {
                        b.add(FC, FeatureValue::Complex(nested.clone()));
                    }
                    out.push(b.build());
                }
            }
        }
        out
    }

    #[test]
    fn property_unify_is_commutative() {
        let u = universe();
        for a in &u {
            for b in &u {
                assert_eq!(
                    unify(a, b),
                    unify(b, a),
                    "unify not commutative for a={a:?} b={b:?}"
                );
            }
        }
    }

    #[test]
    fn property_unify_with_self_is_identity() {
        let u = universe();
        for a in &u {
            assert_eq!(
                unify(a, a),
                Some(a.clone()),
                "unify(a,a) != Some(a) for a={a:?}"
            );
        }
    }

    #[test]
    fn property_unify_with_empty_is_identity() {
        let u = universe();
        for a in &u {
            assert_eq!(unify(a, &FeatureStruct::EMPTY), Some(a.clone()));
            assert_eq!(unify(&FeatureStruct::EMPTY, a), Some(a.clone()));
        }
    }

    #[test]
    fn property_is_unifiable_matches_unify_is_some() {
        let u = universe();
        for a in &u {
            for b in &u {
                assert_eq!(
                    is_unifiable(a, b),
                    unify(a, b).is_some(),
                    "is_unifiable/unify disagree for a={a:?} b={b:?}"
                );
            }
        }
    }

    #[test]
    fn property_empty_subsumes_everything() {
        let u = universe();
        for x in &u {
            assert!(subsumes(&FeatureStruct::EMPTY, x));
        }
    }

    #[test]
    fn property_unify_result_is_subsumed_by_both_operands() {
        let u = universe();
        for a in &u {
            for b in &u {
                if let Some(unified) = unify(a, b) {
                    assert!(
                        subsumes(a, &unified),
                        "a doesn't subsume unify(a,b) for a={a:?} b={b:?}"
                    );
                    assert!(
                        subsumes(b, &unified),
                        "b doesn't subsume unify(a,b) for a={a:?} b={b:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn property_priority_union_with_empty_is_identity() {
        let u = universe();
        for a in &u {
            assert_eq!(priority_union(a, &FeatureStruct::EMPTY), a.clone());
            assert_eq!(priority_union(&FeatureStruct::EMPTY, a), a.clone());
        }
    }

    /// `add(a, EMPTY)` never touches anything (there's nothing in `b` to fold in) -- this is the
    /// one identity `add` shares with `unify`/`priority_union` despite its different leaf op,
    /// since an `a`-only key is always a verbatim passthrough.
    #[test]
    fn property_add_with_empty_b_is_identity() {
        let u = universe();
        for a in &u {
            assert_eq!(add(a, &FeatureStruct::EMPTY, &mask3), a.clone());
        }
    }

    /// Every feature `add(a, b)` still constrains must be a **superset** of what `b` alone
    /// specifies at that feature (union only ever loosens `a`'s constraint at a shared feature,
    /// or copies `b`'s constraint in unchanged at a `b`-only feature) -- i.e. `add(a, b)` is
    /// always unifiable with `b` itself (loosening can never create a new conflict), unless the
    /// deletion rule removed the feature entirely, which only makes the result *more* permissive.
    #[test]
    fn property_add_result_is_unifiable_with_b() {
        let u = universe();
        for a in &u {
            for b in &u {
                let added = add(a, b, &mask3);
                assert!(
                    is_unifiable(&added, b),
                    "add(a,b) not unifiable with b for a={a:?} b={b:?} added={added:?}"
                );
            }
        }
    }

    /// `subtract(a, EMPTY)` is the identity: `b` has no features to walk, so `a` passes through
    /// unchanged (`FeatureStruct.cs:535`'s loop over `otherFS._definite` never executes).
    #[test]
    fn property_subtract_with_empty_b_is_identity() {
        let u = universe();
        for a in &u {
            assert_eq!(subtract(a, &FeatureStruct::EMPTY), a.clone());
        }
    }

    /// `subtract(a, a)` removes every feature `a` has (each shared symbolic value ExceptWith's
    /// itself to the empty set, deleting the key; each shared complex value recurses to the same
    /// base case), leaving `EMPTY`.
    #[test]
    fn property_subtract_self_is_empty() {
        let u = universe();
        for a in &u {
            assert_eq!(
                subtract(a, a),
                FeatureStruct::EMPTY,
                "subtract(a,a) not empty for a={a:?}"
            );
        }
    }

    /// Hand case: `a={a: a1|a2, b: b1}`, `b={a: a1}` -> `a`'s `a`-lane loses bit a1, keeping a2;
    /// `b`-only feature is irrelevant to `a`; `a`'s `b`-only feature passes through untouched.
    #[test]
    fn subtract_removes_bits_present_in_b() {
        let a = fs(&[(FA, sym(0b011)), (FB, sym(0b001))]);
        let b = fs(&[(FA, sym(0b001))]);
        let result = subtract(&a, &b);
        assert_eq!(result, fs(&[(FA, sym(0b010)), (FB, sym(0b001))]));
    }

    /// Hand case: subtracting away *every* allowed symbol at a feature drops that feature's key
    /// entirely (C#'s `IsSatisfiable`-false removal), rather than leaving an empty/unsatisfiable
    /// `SymbolBits(0)` value sitting in the result.
    #[test]
    fn subtract_drops_feature_emptied_to_zero_bits() {
        let a = fs(&[(FA, sym(0b011)), (FB, sym(0b001))]);
        let b = fs(&[(FA, sym(0b011))]);
        let result = subtract(&a, &b);
        assert_eq!(result, fs(&[(FB, sym(0b001))]));
    }
}
