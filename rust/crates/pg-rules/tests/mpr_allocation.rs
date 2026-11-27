//! Stats-off MPR gates borrow wide candidates without allocating observation sets.

use pg_grammar_model::model::{
    mpr_excluded_ok, mpr_required_ok, MprGroup, MprGroupMatchType, MprGroupOutput, MprId, MprSet,
};

#[global_allocator]
static ALLOCATOR: pg_cost::CountingAllocator = pg_cost::CountingAllocator;

fn set(ids: &[u32]) -> MprSet {
    let mut value = MprSet::EMPTY;
    for &id in ids {
        value.insert(MprId(id));
    }
    value
}

fn measure(action: impl FnOnce() -> bool) -> (bool, pg_cost::Counts) {
    let meter = pg_cost::Meter::start().unwrap();
    let passed = std::hint::black_box(action());
    (passed, meter.finish(0))
}

#[test]
fn stats_off_mpr_allocation_regression() {
    let mut grammar = pg_grammar::load(include_str!(
        "../../../tools/fixtures/supplied-lexicon-host-smoke.xml"
    ))
    .unwrap();
    grammar.mpr_groups.clear();
    let empty = MprSet::EMPTY;
    let low = set(&[1]);
    let wide = set(&[64, 255, 256, 299]);
    let mixed = low.union(&wide);
    pg_cost::enable().unwrap();
    let mut failures = Vec::new();
    let mut check = |name: &str, expected: bool, action: &mut dyn FnMut() -> bool| {
        let (passed, counts) = measure(action);
        println!(
            "{name}: passed={passed}, allocations={}, bytes={}",
            counts.allocations, counts.bytes
        );
        if passed != expected || counts.allocations != 0 || counts.bytes != 0 {
            failures.push(format!("{name}: passed={passed}, {counts:?}"));
        }
    };
    for (name, test, have, excluded, expected) in [
        ("low-required-low", &low, &low, false, true),
        ("low-required-wide-pass", &low, &mixed, false, true),
        ("low-required-wide-block", &low, &wide, false, false),
        ("wide-required-wide-pass", &wide, &wide, false, true),
        ("wide-required-empty-block", &wide, &empty, false, false),
        ("wide-excluded-wide-block", &wide, &wide, true, false),
        ("wide-excluded-empty-pass", &wide, &empty, true, true),
        ("empty-required-wide", &empty, &wide, false, true),
    ] {
        check(name, expected, &mut || {
            if excluded {
                grammar.mpr_group_ok(&empty, test, have)
            } else {
                grammar.mpr_group_ok(test, &empty, have)
            }
        });
        check(name, expected, &mut || {
            if excluded {
                mpr_excluded_ok(&[], test, have)
            } else {
                mpr_required_ok(&[], test, have)
            }
        });
    }
    check("borrowed-subset", true, &mut || wide.is_subset_of(&mixed));
    check("borrowed-overlaps", true, &mut || wide.overlaps(&mixed));
    for match_type in [MprGroupMatchType::All, MprGroupMatchType::Any] {
        grammar.mpr_groups = vec![MprGroup {
            name: None,
            match_type,
            output: MprGroupOutput::Overwrite,
            members: low.clone(),
        }];
        for (label, have, expected) in [
            ("low-have", &low, true),
            ("wide-have", &mixed, true),
            ("wide-have-block", &wide, false),
        ] {
            let name = format!("grouped-{match_type:?}-{label}");
            check(&name, expected, &mut || {
                mpr_required_ok(&grammar.mpr_groups, &low, have)
            });
            check(&name, !expected, &mut || {
                mpr_excluded_ok(&grammar.mpr_groups, &low, have)
            });
        }
    }
    pg_cost::disable();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
