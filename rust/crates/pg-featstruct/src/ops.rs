//! Syntactic-domain feature-structure operations over the frozen tree model (`crate::tree`).
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
//! - **String features / `not`-negated symbolic values.** `crate::tree::FeatureValue::Symbolic`
//!   carries no negation flag (unlike C#'s `not`/`notOther`-parameterized
//!   `ISymbolicFeatureValueFlags` API), so every `crate::SymbolBits` call below passes
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
//!   `crate::SymbolBits::overlaps` — **non-empty intersection ⇒ unifiable; empty ⇒ the whole
//!   structure fails to unify** (this is the "unify of two symbolic values is set intersection;
//!   empty intersection fails" rule from the task brief).
//! - **`unify`** ports `FeatureStruct.NondestructiveUnify` (`FeatureStruct.cs:1010-1068`): the
//!   output holds every feature from *either* side; a feature present on only one side is copied
//!   through unchanged (`NondestructiveUnify:1056` for other-only, `:1060-1064` for this-only);
//!   a feature present on both sides is recursively unified and the whole operation fails if that
//!   recursive unify fails (`:1036-1041`). Leaf case ports
//!   `SimpleFeatureValue.NondestructiveUnify` (`SimpleFeatureValue.cs:397-415`), which clones and
//!   runs `DestructiveUnify`'s non-variable arm (`SimpleFeatureValue.cs:171-176`:
//!   `Overlaps` check then `IntersectWith`), i.e. `crate::SymbolBits::intersect_with` guarded by
//!   `crate::SymbolBits::overlaps`.
//! - **`subsumes(a, b)`**: **direction — `a` is the more general structure; `subsumes(a, b)` asks
//!   "does `a` (fewer/looser constraints) subsume `b` (as-or-more-specific)?"**, matching C#
//!   `a.Subsumes(b)`. Ports `FeatureStruct.SubsumesImpl` (`FeatureStruct.cs:930-957`), which walks
//!   **`this`'s (`a`'s) own features**: every feature `a` constrains must also be present in `b`
//!   (`:951-954`: missing ⇒ `false` immediately — this is *not* symmetric with `is_unifiable`),
//!   and recursively `a`'s value must subsume `b`'s value. `b`-only features are irrelevant (`a`
//!   doesn't constrain them). Leaf case ports `SimpleFeatureValue.SubsumesImpl`'s non-variable arm
//!   (`SimpleFeatureValue.cs:110-113`: `IsSupersetOf(false, otherSfv, false)`), i.e.
//!   `crate::SymbolBits::is_superset_of` — `a`'s allowed-symbol set must be a superset of `b`'s.
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
//! ## Feature-kind mismatch (`Symbolic` vs `Complex` under the same `FeatId`)
//! In a well-typed HC grammar a `Feature` is declared once, globally, as either a
//! `SymbolicFeature` or a `ComplexFeature` (`FeatureSystem`/`XmlLanguageLoader`), so the same
//! `FeatId` never holds a `FeatureValue::Symbolic` on one side and a
//! `FeatureValue::Complex` on the other in `is_unifiable`/`unify`/`subsumes`. C# reaches this
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
            // DestructiveUnify's non-variable arm: overlap-guard, then intersect (SimpleFeatureValue.cs:171-176).
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
/// sides, if both values are nested `FeatureStruct`s they're recursively priority-unioned,
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
/// happen to be complementary. A nested `FeatureStruct` value has the analogous condition —
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

/// `a` is `None` for a seed-from-empty key; a `None` result means uninstantiated, drop the key.
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
            // Plain bitset OR (SymbolicFeatureValue.cs:164-171); mask is unused in this un-negated arm.
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

/// Port of `FeatureStruct.Union` (`FeatureStruct.cs:384-414`): unlike `add`, keys are
/// **intersected** rather than accumulated at every depth — a feature present on only one side is
/// dropped, not passed through — while a shared symbolic value takes `add_value`'s leaf rule
/// (`SimpleFeatureValue.UnionImpl`/`AddImpl` are the same routine in C#), including deleting a
/// key whose union covers every declared symbol. A nested struct that unions to empty is dropped.
pub fn union(
    a: &FeatureStruct,
    b: &FeatureStruct,
    mask_of: &impl Fn(FeatId) -> u64,
) -> FeatureStruct {
    let ae = a.entries();
    let be = b.entries();
    let mut builder = FeatureStructBuilder::new();
    let (mut i, mut j) = (0usize, 0usize);
    while i < ae.len() && j < be.len() {
        match ae[i].0.cmp(&be[j].0) {
            Ordering::Less => i += 1,
            Ordering::Greater => j += 1,
            Ordering::Equal => {
                if let Some(v) = union_value(ae[i].0, &ae[i].1, &be[j].1, mask_of) {
                    builder.add(ae[i].0, v);
                }
                i += 1;
                j += 1;
            }
        }
    }
    builder.build()
}

/// Per-value half of `union`: nested structs recurse into `union` (not `add`), so keys intersect at depth too.
fn union_value(
    feat: FeatId,
    a: &FeatureValue,
    b: &FeatureValue,
    mask_of: &impl Fn(FeatId) -> u64,
) -> Option<FeatureValue> {
    match (a, b) {
        (FeatureValue::Complex(fa), FeatureValue::Complex(fb)) => {
            let merged = union(fa, fb, mask_of);
            if merged.is_empty() {
                None
            } else {
                Some(FeatureValue::Complex(merged))
            }
        }
        // Symbolic/Symbolic, plus the debug-asserted kind-mismatch arms `add_value` already handles.
        _ => add_value(feat, Some(a), b, mask_of),
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
/// `pg-rules::stratum::choose_inflectional_stem` (`SynthesisAffixTemplatesRule.cs:99-100`'s
/// `remainder`) and `pg-rules::word::Word::expand_alternatives` (`Word.cs:517-518`'s realizational-
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
                    // Feature-kind mismatch: keep a's value untouched, matching C#'s Dereference-failure no-op.
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

/// Port of `AnalysisSyntacticFeatureMerge.RemovePaths` (private, `AnalysisSyntacticFeatureMerge.cs`):
/// strips every leaf feature path `paths` defines from `a`. For each feature `paths` names: absent
/// from `a` -> skip; both sides nested `FeatureStruct`s -> recurse and drop the key only if the
/// recursion empties it; otherwise drop the key outright. A feature `paths` doesn't mention is
/// untouched, even if `a` has it. Used to invert synthesis's `out` contribution off an analysis
/// word's syntactic FS before re-unifying with `required` (`crate::morph::ana_syn_fs`, `pg-rules`).
pub fn remove_paths(a: &FeatureStruct, paths: &FeatureStruct) -> FeatureStruct {
    let ae = a.entries();
    let pe = paths.entries();
    let mut builder = FeatureStructBuilder::new();
    let (mut i, mut j) = (0usize, 0usize);
    while i < ae.len() && j < pe.len() {
        match ae[i].0.cmp(&pe[j].0) {
            Ordering::Less => {
                builder.add(ae[i].0, ae[i].1.clone());
                i += 1;
            }
            Ordering::Greater => {
                // `paths`-only feature: `a` doesn't have it, nothing to remove.
                j += 1;
            }
            Ordering::Equal => {
                match (&ae[i].1, &pe[j].1) {
                    (FeatureValue::Complex(fa), FeatureValue::Complex(fp)) => {
                        let nested = remove_paths(fa, fp);
                        if !nested.is_empty() {
                            builder.add(ae[i].0, FeatureValue::Complex(nested));
                        }
                    }
                    (_, FeatureValue::Complex(_)) | (FeatureValue::Complex(_), _) => {
                        debug_assert!(
                            false,
                            "feature-kind mismatch (Symbolic vs Complex under the same FeatId); a \
                             Feature is globally either symbolic or complex in a well-typed HC \
                             grammar — see ops.rs module docs"
                        );
                        // Drop the key, matching every other kind-mismatch degrade in this module.
                    }
                    _ => {
                        // Both symbolic (or the drop above already applied): remove the key outright.
                    }
                }
                i += 1;
                j += 1;
            }
        }
    }
    // Remaining `a`-only entries (paths exhausted): pass through untouched.
    for (f, v) in &ae[i..] {
        builder.add(*f, v.clone());
    }
    builder.build()
}

#[cfg(test)]
mod tests;
