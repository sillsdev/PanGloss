use super::*;

#[test]
fn reservations_stop_at_the_exact_cap_and_remain_stopped() {
    let budget = WorkBudget::new(3);
    for _ in 0..3 {
        assert!(budget.consume());
    }
    for _ in 0..3 {
        assert!(!budget.consume());
    }
    assert_eq!(budget.used(), 3);
    assert!(budget.capped());
    assert!(!budget.timed_out());
}

#[test]
fn nested_scopes_restore_the_callers_meter_on_unwind() {
    let outer = WorkBudget::new(3);
    let _outer_scope = outer.enter();
    assert!(consume());
    let inner = WorkBudget::new(1);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _inner_scope = inner.enter();
        assert!(consume());
        assert!(!consume());
        panic!("exercise scope unwinding");
    }));
    assert!(result.is_err());
    assert!(consume());
    assert_eq!(outer.used(), 2);
    assert_eq!(inner.used(), 1);
    assert!(!outer.capped());
    assert!(inner.capped());
}

#[test]
fn a_word_meter_is_not_inherited_by_another_thread() {
    let budget = WorkBudget::new(0);
    let _scope = budget.enter();
    assert!(std::thread::spawn(consume).join().unwrap());
    assert!(!consume());
    assert!(budget.capped());
}

#[test]
fn zero_deadline_refuses_inner_search_work_without_a_step() {
    let budget = WorkBudget::new(100).with_timeout(Some(Duration::ZERO));
    let _scope = budget.enter();
    assert!(!consume());
    assert_eq!(budget.used(), 0);
    assert!(budget.timed_out());
    assert!(!budget.capped());
}
