use super::*;

#[test]
fn enabling_without_the_allocator_refuses_instead_of_undercounting() {
    assert_eq!(enable(), Err(AllocatorNotInstalled));
    assert!(Meter::start().is_none());
}

#[test]
fn cost_applies_each_weight_to_its_own_count() {
    let counts = Counts {
        work: 3,
        bytes: 5,
        registers: 7,
        allocations: 11,
    };
    assert_eq!(
        counts.cost(),
        (3 * WORK_WEIGHT + 5 * BYTE_WEIGHT + 7 * REGISTER_WEIGHT + 11 * ALLOCATION_WEIGHT)
            / WEIGHT_SCALE
    );
}

#[test]
fn cost_saturates_rather_than_wrapping() {
    let counts = Counts {
        work: u64::MAX,
        bytes: u64::MAX,
        registers: u64::MAX,
        allocations: u64::MAX,
    };
    assert_eq!(counts.cost(), u64::MAX / WEIGHT_SCALE);
}

#[test]
fn thread_cpu_time_moves_when_this_thread_works() {
    let before = thread_cpu_time().expect("this platform reports thread CPU time");
    let mut spin = 0u64;
    while thread_cpu_time().unwrap() <= before {
        spin = std::hint::black_box(spin.wrapping_add(1));
    }
}

#[test]
fn registers_are_counted_but_cost_nothing() {
    let counts = Counts {
        registers: 1_000_000,
        ..Counts::default()
    };
    assert_eq!(counts.cost(), 0);
}
