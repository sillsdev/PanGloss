use super::{MprId, MprSet};

#[test]
fn mpr_ids_above_the_inline_word_keep_their_identity() {
    for id in [0, 63, 64, 127, 128, 255, 256, 299] {
        let mut single = MprSet::EMPTY;
        single.insert(MprId(id));
        for other in [0, 63, 64, 127, 128, 255, 256, 299] {
            assert_eq!(
                single.contains(MprId(other)),
                id == other,
                "MPR {id} versus {other}"
            );
        }
    }
    let mut set = MprSet::EMPTY;
    for id in [0, 63, 64, 127, 128, 255, 256, 299] {
        set.insert(MprId(id));
    }
    for id in [0, 63, 64, 127, 128, 255, 256, 299] {
        assert!(set.contains(MprId(id)), "missing MPR {id}");
    }
    for id in [1, 62, 65, 126, 129, 254] {
        assert!(!set.contains(MprId(id)), "aliased MPR {id}");
    }
}

fn set(ids: &[u32]) -> MprSet {
    let mut value = MprSet::EMPTY;
    for &id in ids {
        value.insert(MprId(id));
    }
    value
}

#[test]
fn mixed_set_operations_match_independent_ordered_sets() {
    use std::collections::BTreeSet;
    let cases: &[&[u32]] = &[
        &[],
        &[0],
        &[63],
        &[64],
        &[0, 63, 64],
        &[127, 128, 255, 256, 299],
        &[16384],
    ];
    let ids = |value: MprSet| value.iter().map(|id| id.0).collect::<Vec<_>>();
    for a in cases {
        for b in cases {
            let left = set(a);
            let right = set(b);
            let expected_left: BTreeSet<_> = a.iter().copied().collect();
            let expected_right: BTreeSet<_> = b.iter().copied().collect();
            assert_eq!(
                ids(left.union(&right)),
                expected_left
                    .union(&expected_right)
                    .copied()
                    .collect::<Vec<_>>()
            );
            assert_eq!(
                ids(left.intersection(&right)),
                expected_left
                    .intersection(&expected_right)
                    .copied()
                    .collect::<Vec<_>>()
            );
            assert_eq!(
                ids(left.difference(&right)),
                expected_left
                    .difference(&expected_right)
                    .copied()
                    .collect::<Vec<_>>()
            );
            assert_eq!(
                left.overlaps(&right),
                !expected_left.is_disjoint(&expected_right)
            );
            assert_eq!(
                left.is_subset_of(&right),
                expected_left.is_subset(&expected_right)
            );
        }
    }
}

#[test]
fn shared_clones_are_independent_and_low_results_keep_legacy_hashes() {
    use std::hash::{DefaultHasher, Hash, Hasher};
    fn hash(value: impl Hash) -> u64 {
        let mut h = DefaultHasher::new();
        value.hash(&mut h);
        h.finish()
    }
    let original = set(&[0, 63, 64, 299]);
    let mut changed = original.clone();
    changed.insert(MprId(256));
    assert!(!original.contains(MprId(256)));
    assert!(changed.contains(MprId(256)));
    let low = original.difference(&set(&[64, 299]));
    assert_eq!(low, set(&[0, 63]));
    assert_eq!(hash(&low), hash((1u64 << 63) | 1));
    assert_eq!(low.heap_bytes(), 0);
    assert_eq!(original.difference(&original), MprSet::EMPTY);
    assert_eq!(hash(&original), hash(set(&[299, 64, 63, 0])));
    assert_eq!(hash(original.clone()), hash(original));
}
