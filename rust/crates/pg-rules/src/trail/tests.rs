use super::*;
use std::hash::BuildHasher;

fn r(id: u32) -> Option<MRuleId> {
    Some(MRuleId(id))
}

fn a(id: u32) -> Option<AllomorphId> {
    Some(AllomorphId(id))
}

fn hash_of(view: &SlotView) -> u64 {
    std::hash::BuildHasherDefault::<std::collections::hash_map::DefaultHasher>::default()
        .hash_one(view)
}

#[test]
fn pushes_are_indexed_in_forward_order() {
    let mut t = Trail::default();
    assert!(t.is_empty());
    t.push(r(1), None);
    t.push_count_only(MRuleId(7));
    t.push(None, a(5));
    t.push(r(2), a(6));
    assert_eq!(t.len(), 3);
    assert_eq!(t.rules(), vec![r(1), None, r(2)]);
    assert_eq!(t.allomorphs(), vec![None, a(5), a(6)]);
    assert_eq!(t.rule(0), Some(r(1)));
    assert_eq!(t.rule(1), Some(None));
    assert_eq!(t.allomorph(2), Some(a(6)));
    assert_eq!(t.rule(3), None);
    assert_eq!(t.last_rule(), Some(r(2)));
    assert_eq!(t.slots_rev().count(), 3);
}

#[test]
fn count_only_steps_count_but_are_not_slots() {
    let mut t = Trail::default();
    t.push(r(1), None);
    t.push_count_only(MRuleId(1));
    t.push(None, None);
    assert_eq!(t.len(), 2);
    assert_eq!(t.count_of(MRuleId(1)), 2);
    assert_eq!(t.count_of(MRuleId(9)), 0);
    assert_eq!(t.rule_counts(), BTreeMap::from([(MRuleId(1), 2)]));
}

#[test]
fn a_count_only_step_does_not_change_the_slot_view_but_changes_equality() {
    let plain = Trail::from_slots([(r(1), a(2))]);
    let mut counted = Trail::from_slots([(r(1), a(2))]);
    counted.push_count_only(MRuleId(3));
    assert_ne!(plain, counted);
    assert!(plain.same_slots(&counted));
    assert_eq!(plain.slot_view(), counted.slot_view());
    assert_eq!(hash_of(&plain.slot_view()), hash_of(&counted.slot_view()));
    let other = Trail::from_slots([(r(1), a(3))]);
    assert_ne!(plain.slot_view(), other.slot_view());
}

#[test]
fn separately_built_equal_trails_are_equal_and_hash_alike() {
    let left = Trail::from_slots([(r(1), None), (None, a(4))]);
    let right = Trail::from_slots([(r(1), None), (None, a(4))]);
    assert!(!left.same_allocation(&right));
    assert_eq!(left, right);
    assert_eq!(hash_of(&left.slot_view()), hash_of(&right.slot_view()));
    assert_eq!(format!("{left:?}"), format!("{right:?}"));
}

#[test]
fn siblings_share_their_prefix() {
    let base = Trail::from_slots([(r(1), None)]);
    let mut x = base.clone();
    let mut y = base.clone();
    x.push(r(2), None);
    y.push(r(3), None);
    assert_ne!(x, y);
    assert_eq!(x.count_of(MRuleId(1)), 1);
}

#[test]
fn a_very_deep_trail_drops_without_overflowing_the_stack() {
    let mut t = Trail::default();
    for i in 0..200_000 {
        t.push(r(i % 7), None);
    }
    assert_eq!(t.len(), 200_000);
    drop(t);
}

#[test]
fn a_packed_step_keeps_absent_and_zero_ids_apart() {
    assert_eq!(std::mem::size_of::<Step>(), 20);
    let slots = [
        (None, None),
        (Some(MRuleId(0)), None),
        (None, Some(AllomorphId(0))),
        (Some(MRuleId(0)), Some(AllomorphId(0))),
        (Some(MRuleId(u32::MAX)), Some(AllomorphId(u32::MAX))),
    ];
    let mut trail = Trail::from_slots(slots);
    trail.push_count_only(MRuleId(0));
    assert_eq!(trail.len(), slots.len());
    for (i, (rule, allomorph)) in slots.into_iter().enumerate() {
        assert_eq!(trail.rule(i), Some(rule));
        assert_eq!(trail.allomorph(i), Some(allomorph));
    }
    assert_eq!(trail.count_of(MRuleId(0)), 3);
    assert_eq!(trail.count_of(MRuleId(u32::MAX)), 1);
    for (i, a) in slots.iter().enumerate() {
        for (j, b) in slots.iter().enumerate() {
            assert_eq!(Trail::from_slots([*a]) == Trail::from_slots([*b]), i == j);
        }
    }
    assert_ne!(trail, Trail::from_slots(slots));
    assert!(trail.same_slots(&Trail::from_slots(slots)));
}

#[test]
fn dropped_steps_are_recycled_and_shared_prefixes_survive() {
    let mut base = Trail::default();
    base.push(r(1), None);
    let mut child = base.clone();
    child.push(r(2), a(3));
    drop(base);
    assert_eq!(child.rules(), vec![r(1), r(2)]);
    drop(child);
    let mut again = Trail::default();
    for id in 0..(2 * CHUNK as u32) {
        again.push(r(id), None);
    }
    assert_eq!(again.len(), 2 * CHUNK);
    assert_eq!(again.rule(CHUNK + 5), Some(r(CHUNK as u32 + 5)));
    drop(again);
    assert_eq!(SLAB.with(|slab| slab.borrow().live), 0);
}
