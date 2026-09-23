use super::*;
use crate::bitvec::SymbolBits;
use crate::tree::FeatId;

// Symbol i (1-based) is bit (i-1), in declaration order, matching tree.rs's convention.

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

// "simple" fixture (FeatureStructTests.cs:614-629): a=FeatId(0), b=FeatId(1), c=FeatId(2), 3 symbols each.

const FA: FeatId = FeatId(0);
const FB: FeatId = FeatId(1);
const FC: FeatId = FeatId(2);

/// Mirrors `Unify` case 0 (FeatureStructTests.cs:29-30): disjoint on `a` -> whole unify fails.
#[test]
fn unify_simple_disjoint_fails() {
    let a = fs(&[(FA, sym(0b001)), (FB, sym(0b001))]);
    let b = fs(&[(FA, sym(0b010)), (FC, sym(0b010))]);
    assert_eq!(unify(&a, &b), None);
}

/// Mirrors `IsUnifiable` case 0 (FeatureStructTests.cs:213-214): same fixture, expects `false`.
#[test]
fn is_unifiable_simple_disjoint_is_false() {
    let a = fs(&[(FA, sym(0b001)), (FB, sym(0b001))]);
    let b = fs(&[(FA, sym(0b010)), (FC, sym(0b010))]);
    assert!(!is_unifiable(&a, &b));
}

/// Mirrors `Unify` case 1 (FeatureStructTests.cs:30): `a` intersects, `b` passes through -> {a2,b1,c2}.
#[test]
fn unify_simple_overlap_succeeds() {
    let a = fs(&[(FA, sym(0b011)), (FB, sym(0b001)), (FC, sym(0b010))]);
    let b = fs(&[(FA, sym(0b010)), (FC, sym(0b010))]);
    let expected = fs(&[(FA, sym(0b010)), (FB, sym(0b001)), (FC, sym(0b010))]);
    assert_eq!(unify(&a, &b), Some(expected));
}

/// Mirrors `IsUnifiable` case 1 (FeatureStructTests.cs:214): expects `true`, cross-checked against `unify`.
#[test]
fn is_unifiable_simple_overlap_is_true() {
    let a = fs(&[(FA, sym(0b011)), (FB, sym(0b001)), (FC, sym(0b010))]);
    let b = fs(&[(FA, sym(0b010)), (FC, sym(0b010))]);
    assert!(is_unifiable(&a, &b));
    assert_eq!(is_unifiable(&a, &b), unify(&a, &b).is_some());
}

/// Mirrors `PriorityUnion` case 0 (FeatureStructTests.cs:264-265): `b` overwrites the shared feature.
#[test]
fn priority_union_simple_overwrite() {
    let a = fs(&[(FA, sym(0b001)), (FB, sym(0b001))]);
    let b = fs(&[(FA, sym(0b010)), (FC, sym(0b010))]);
    let expected = fs(&[(FA, sym(0b010)), (FB, sym(0b001)), (FC, sym(0b010))]);
    assert_eq!(priority_union(&a, &b), expected);
}

// "complex" fixture (FeatureStructTests.cs:631-683): cx1..cx4 = FeatId(0..3), each wrapping FeatId(10).

const CX1: FeatId = FeatId(0);
const CX2: FeatId = FeatId(1);
const CX3: FeatId = FeatId(2);
const CX4: FeatId = FeatId(3);
const LEAF: FeatId = FeatId(10);

fn leaf(bits: u64) -> FeatureValue {
    FeatureValue::Complex(fs(&[(LEAF, sym(bits))]))
}

/// Mirrors `Unify` case 2 (FeatureStructTests.cs:32-44): `cx1`'s nested values are disjoint -> fails.
#[test]
fn unify_complex_disjoint_fails_at_depth2() {
    let a = fs(&[(CX1, leaf(0b001)), (CX2, leaf(0b001)), (CX4, leaf(0b001))]);
    let b = fs(&[(CX1, leaf(0b010)), (CX3, leaf(0b010)), (CX4, leaf(0b110))]);
    assert_eq!(unify(&a, &b), None);
}

/// Mirrors `IsUnifiable` case 2 (FeatureStructTests.cs:216): same fixture, expects `false`.
#[test]
fn is_unifiable_complex_disjoint_is_false() {
    let a = fs(&[(CX1, leaf(0b001)), (CX2, leaf(0b001)), (CX4, leaf(0b001))]);
    let b = fs(&[(CX1, leaf(0b010)), (CX3, leaf(0b010)), (CX4, leaf(0b110))]);
    assert!(!is_unifiable(&a, &b));
}

/// Mirrors `Unify` case 3 (FeatureStructTests.cs:33-44): nested intersections all succeed at depth 2.
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

/// Mirrors `IsUnifiable` case 3 (FeatureStructTests.cs:217): same fixture, expects `true`.
#[test]
fn is_unifiable_complex_succeeds_is_true() {
    let a = fs(&[(CX1, leaf(0b011)), (CX2, leaf(0b001)), (CX4, leaf(0b001))]);
    let b = fs(&[(CX1, leaf(0b010)), (CX3, leaf(0b010)), (CX4, leaf(0b011))]);
    assert!(is_unifiable(&a, &b));
}

/// Mirrors `PriorityUnion` case 2 (FeatureStructTests.cs:267-278): recursion bottoms out at a symbolic overwrite, `b` wins.
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

/// Both sides complex at `cx1`: exercises `priority_union`'s recursive-merge branch, not the overwrite branch.
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

// subsumes has no direct C# unit test; ported from FeatureStruct.cs:930-957 reading.

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
    // a constrains FeatId(1), absent from b -> false immediately (FeatureStruct.cs:951-954).
    let a = fs(&[(FA, sym(0b001)), (FB, sym(0b001))]);
    let b = fs(&[(FA, sym(0b001))]);
    assert!(!subsumes(&a, &b));
    // But the reverse holds: b has no feature that isn't also satisfied/absent-in-a's walk.
    assert!(subsumes(&b, &a));
}

// add has no direct C# unit test; hand-ported from FeatureStruct.cs:453-505 reading.

/// Every fixture feature here has 3 symbols (bits 0..=2) -> full domain is `0b111`.
fn mask3(_: FeatId) -> u64 {
    0b111
}

/// `a`-only feature passes through untouched; `b`-only feature is seeded from empty and unioned in.
#[test]
fn add_singleton_keys_pass_through_or_are_seeded() {
    let a = fs(&[(FA, sym(0b001))]);
    let b = fs(&[(FB, sym(0b010))]);
    let expected = fs(&[(FA, sym(0b001)), (FB, sym(0b010))]);
    assert_eq!(add(&a, &b, &mask3), expected);
}

/// Unlike `unify` (intersect), `add` unions the two sets; here the union misses full domain coverage, so the key survives.
#[test]
fn add_conflicting_values_unions_not_intersects() {
    let a = fs(&[(FA, sym(0b001))]);
    let b = fs(&[(FA, sym(0b010))]);
    assert_eq!(add(&a, &b, &mask3), fs(&[(FA, sym(0b011))]));
}

/// Reuses `unify_simple_disjoint_fails`'s fixture, where `unify` fails outright; `add` has no failure path.
#[test]
fn add_never_fails_where_unify_would() {
    let a = fs(&[(FA, sym(0b001)), (FB, sym(0b001))]);
    let b = fs(&[(FA, sym(0b010)), (FC, sym(0b010))]);
    assert!(unify(&a, &b).is_none());
    let expected = fs(&[(FA, sym(0b011)), (FB, sym(0b001)), (FC, sym(0b010))]);
    assert_eq!(add(&a, &b, &mask3), expected);
}

/// A union covering every declared symbol is "uninstantiated"; `AddImpl` deletes the key rather than keeping "all allowed".
#[test]
fn add_full_domain_union_deletes_the_key() {
    let a = fs(&[(FA, sym(0b011)), (FB, sym(0b001))]);
    let b = fs(&[(FA, sym(0b100))]); // 0b011 | 0b100 == 0b111 == the full 3-symbol domain.
    let expected = fs(&[(FB, sym(0b001))]); // FA is gone entirely, not left at 0b111.
    assert_eq!(add(&a, &b, &mask3), expected);
}

/// Same deletion rule for a seed-from-empty key: if `b` alone spans the full domain, the key is dropped too.
#[test]
fn add_seed_from_empty_at_full_domain_still_deletes() {
    let b = fs(&[(FA, sym(0b111))]);
    assert_eq!(add(&FeatureStruct::EMPTY, &b, &mask3), FeatureStruct::EMPTY);
}

/// A nested value on both sides recurses (mirroring `unify`/`priority_union`), but the leaf op is still union.
#[test]
fn add_recurses_into_nested_complex_values() {
    let a = fs(&[(CX1, leaf(0b001))]);
    let b = fs(&[(CX1, leaf(0b010))]);
    assert_eq!(add(&a, &b, &mask3), fs(&[(CX1, leaf(0b011))]));
}

/// Nested-struct analogue of `add_full_domain_union_deletes_the_key`: deletion cascades up to the outer key too.
#[test]
fn add_nested_complex_deletion_cascades_to_parent_key() {
    let a = fs(&[(CX1, leaf(0b011)), (CX2, leaf(0b001))]);
    let b = fs(&[(CX1, leaf(0b100))]); // inner LEAF union 0b011|0b100 == 0b111 -> inner empty.
    let expected = fs(&[(CX2, leaf(0b001))]); // CX1 gone entirely, CX2 untouched (a-only).
    assert_eq!(add(&a, &b, &mask3), expected);
}

// union has no direct C# unit test; hand-ported from FeatureStruct.cs:384-414 reading.

/// Unlike `add`, a key present on only one side is dropped entirely rather than passed through.
#[test]
fn union_singleton_keys_are_dropped() {
    let a = fs(&[(FA, sym(0b001)), (FB, sym(0b010))]);
    let b = fs(&[(FA, sym(0b010)), (FC, leaf(0b001))]);
    assert_eq!(union(&a, &b, &mask3), fs(&[(FA, sym(0b011))]));
}

/// A shared symbolic feature gets the bitwise OR of both sides' symbol sets, same leaf op as `add`.
#[test]
fn union_shared_symbolic_feature_is_bitwise_or() {
    let a = fs(&[(FA, sym(0b001)), (FB, sym(0b100))]);
    let b = fs(&[(FA, sym(0b010)), (FB, sym(0b001))]);
    assert_eq!(
        union(&a, &b, &mask3),
        fs(&[(FA, sym(0b011)), (FB, sym(0b101))])
    );
}

/// A shared feature whose OR covers the full declared domain is deleted, same rule as `add`.
#[test]
fn union_shared_feature_covering_full_domain_is_deleted() {
    let a = fs(&[(FA, sym(0b011)), (FB, sym(0b001))]);
    let b = fs(&[(FA, sym(0b100)), (FB, sym(0b001))]);
    assert_eq!(union(&a, &b, &mask3), fs(&[(FB, sym(0b001))]));
}

/// A shared nested complex value recurses through the same `union`/`add_value` pairing.
#[test]
fn union_recurses_into_nested_complex_values() {
    let a = fs(&[(CX1, leaf(0b001)), (CX2, leaf(0b001))]);
    let b = fs(&[(CX1, leaf(0b010)), (CX3, leaf(0b010))]);
    // CX2/CX3 are one-sided -> dropped; CX1 is shared -> leaf union.
    assert_eq!(union(&a, &b, &mask3), fs(&[(CX1, leaf(0b011))]));
}

/// Keys intersect at depth too: a nested key on one side only is dropped, where `add` would keep it (the HC `{head: {...}}` shape).
#[test]
fn union_intersects_nested_keys_unlike_add() {
    let a = fs(&[(
        CX1,
        FeatureValue::Complex(fs(&[(FA, sym(0b001)), (FB, sym(0b001))])),
    )]);
    let b = fs(&[(CX1, FeatureValue::Complex(fs(&[(FA, sym(0b010))])))]);
    let want = fs(&[(CX1, FeatureValue::Complex(fs(&[(FA, sym(0b011))])))]);
    assert_eq!(union(&a, &b, &mask3), want);
    assert_ne!(
        add(&a, &b, &mask3),
        want,
        "add keeps the one-sided nested FB; union must not"
    );
}

/// A nested struct whose every key unions away (full domain) is itself dropped, as C# `UnionImpl` returns `_definite.Count > 0`.
#[test]
fn union_drops_nested_struct_that_unions_to_empty() {
    let a = fs(&[(CX1, leaf(0b011)), (CX2, leaf(0b001))]);
    let b = fs(&[(CX1, leaf(0b100)), (CX2, leaf(0b010))]);
    assert_eq!(union(&a, &b, &mask3), fs(&[(CX2, leaf(0b011))]));
}

/// `union(x, x) == x` over fixtures below the full-domain deletion edge case (see `universe()`).
#[test]
fn union_of_x_with_itself_is_x() {
    let x = fs(&[(FA, sym(0b011)), (FB, sym(0b001)), (CX1, leaf(0b010))]);
    assert_eq!(union(&x, &x, &mask3), x);
}

// Universe: FA/FB symbolic (absent or one of 7 non-empty 3-bit subsets), FC a depth-1 nested FA.

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

/// `add(a, EMPTY)` is a no-op, the one identity `add` shares with `unify`/`priority_union` despite its different leaf op.
#[test]
fn property_add_with_empty_b_is_identity() {
    let u = universe();
    for a in &u {
        assert_eq!(add(a, &FeatureStruct::EMPTY, &mask3), a.clone());
    }
}

/// Union only loosens or copies `b`'s constraint through, so `add(a, b)` is always unifiable with `b` itself.
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

/// `subtract(a, EMPTY)` is the identity: `b` has no features to walk, so `a` passes through unchanged.
#[test]
fn property_subtract_with_empty_b_is_identity() {
    let u = universe();
    for a in &u {
        assert_eq!(subtract(a, &FeatureStruct::EMPTY), a.clone());
    }
}

/// `subtract(a, a)` empties every feature (symbolic ExceptWith-self, complex recurses), leaving `EMPTY`.
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

/// Hand case: `a`'s `a`-lane loses the bit shared with `b`, keeping the rest; `b`-only lanes pass through.
#[test]
fn subtract_removes_bits_present_in_b() {
    let a = fs(&[(FA, sym(0b011)), (FB, sym(0b001))]);
    let b = fs(&[(FA, sym(0b001))]);
    let result = subtract(&a, &b);
    assert_eq!(result, fs(&[(FA, sym(0b010)), (FB, sym(0b001))]));
}

/// Subtracting away every allowed symbol drops that feature's key rather than leaving an empty `SymbolBits(0)`.
#[test]
fn subtract_drops_feature_emptied_to_zero_bits() {
    let a = fs(&[(FA, sym(0b011)), (FB, sym(0b001))]);
    let b = fs(&[(FA, sym(0b011))]);
    let result = subtract(&a, &b);
    assert_eq!(result, fs(&[(FB, sym(0b001))]));
}

// remove_paths: port of `AnalysisSyntacticFeatureMerge.RemovePaths` (research C#, no direct unit test there).

/// A top-level feature named in `paths` is removed outright, regardless of `a`'s value there.
#[test]
fn remove_paths_top_level_key_removed() {
    let a = fs(&[(FA, sym(0b011)), (FB, sym(0b001))]);
    let paths = fs(&[(FA, sym(0b111))]);
    assert_eq!(remove_paths(&a, &paths), fs(&[(FB, sym(0b001))]));
}

/// A feature `paths` doesn't mention is left exactly as `a` had it.
#[test]
fn remove_paths_untouched_keys_pass_through() {
    let a = fs(&[(FA, sym(0b011)), (FB, sym(0b001)), (FC, leaf(0b010))]);
    let paths = fs(&[(FA, sym(0b001))]);
    assert_eq!(
        remove_paths(&a, &paths),
        fs(&[(FB, sym(0b001)), (FC, leaf(0b010))])
    );
}

/// Both sides nested `FeatureStruct`s: recurse instead of dropping the whole key.
#[test]
fn remove_paths_recurses_into_nested_structs() {
    let inner_a = fs(&[(LEAF, sym(0b011)), (CX2, sym(0b001))]);
    let a = fs(&[(CX1, FeatureValue::Complex(inner_a))]);
    let inner_paths = fs(&[(LEAF, sym(0b111))]);
    let paths = fs(&[(CX1, FeatureValue::Complex(inner_paths))]);
    let expected_inner = fs(&[(CX2, sym(0b001))]);
    let expected = fs(&[(CX1, FeatureValue::Complex(expected_inner))]);
    assert_eq!(remove_paths(&a, &paths), expected);
}

/// If recursing into a nested struct empties it entirely, the parent key is dropped too.
#[test]
fn remove_paths_emptied_nested_struct_drops_parent_key() {
    let inner_a = fs(&[(LEAF, sym(0b011))]);
    let a = fs(&[(CX1, FeatureValue::Complex(inner_a)), (CX2, leaf(0b001))]);
    let inner_paths = fs(&[(LEAF, sym(0b111))]);
    let paths = fs(&[(CX1, FeatureValue::Complex(inner_paths))]);
    assert_eq!(remove_paths(&a, &paths), fs(&[(CX2, leaf(0b001))]));
}

/// `paths` empty: identity over `a`.
#[test]
fn remove_paths_empty_paths_is_identity() {
    let u = universe();
    for a in &u {
        assert_eq!(remove_paths(a, &FeatureStruct::EMPTY), a.clone());
    }
}

/// A feature `paths` names that `a` lacks entirely is a no-op (nothing to remove).
#[test]
fn remove_paths_feature_absent_from_a_is_noop() {
    let a = fs(&[(FB, sym(0b001))]);
    let paths = fs(&[(FA, sym(0b111))]);
    assert_eq!(remove_paths(&a, &paths), a);
}
