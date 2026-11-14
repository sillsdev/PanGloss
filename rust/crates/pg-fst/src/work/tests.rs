use super::*;

#[test]
fn word_memory_is_released_with_its_owner_and_the_stop_is_latched() {
    let budget = WorkBudget::new(100).with_word_cap(Some(10));
    let _scope = budget.enter();
    let mut first = WordMemory::active();
    assert!(first.charge(6));
    {
        let mut second = WordMemory::active();
        assert!(second.charge(4));
    }
    assert!(first.charge(4));
    assert!(!first.charge(1));
    assert!(budget.capped() && budget.word_capped());
    drop(first);
    let mut later = WordMemory::active();
    assert!(!later.charge(1));
    assert_eq!(budget.used(), 0);
}

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
    let outer_key = active_budget_key();
    assert_ne!(outer_key, 0);
    assert!(consume());
    let inner = WorkBudget::new(1);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _inner_scope = inner.enter();
        assert_ne!(active_budget_key(), outer_key);
        assert!(consume());
        assert!(!consume());
        panic!("exercise scope unwinding");
    }));
    assert!(result.is_err());
    assert_eq!(active_budget_key(), outer_key);
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

fn budget_past_its_deadline_mid_interval() -> WorkBudget {
    let budget = WorkBudget::new(1_000).with_timeout(Some(Duration::from_millis(200)));
    assert!(budget.consume());
    std::thread::sleep(std::time::Duration::from_millis(250));
    budget
}

#[test]
fn a_deadline_stops_reservations_within_one_poll_interval() {
    let budget = budget_past_its_deadline_mid_interval();
    let granted = (0..DEADLINE_POLL_INTERVAL)
        .take_while(|_| budget.consume())
        .count();
    assert_eq!(granted, DEADLINE_POLL_INTERVAL - 1);
    assert!(budget.timed_out());
    assert!(!budget.capped());
}

#[test]
fn a_poll_observes_the_deadline_between_reservation_polls() {
    let budget = budget_past_its_deadline_mid_interval();
    assert!(budget.stopped());
    assert!(budget.timed_out());
    assert_eq!(budget.used(), 1);
}

#[test]
fn the_cap_is_exact_between_clock_reads() {
    let budget =
        WorkBudget::new(DEADLINE_POLL_INTERVAL + 3).with_timeout(Some(Duration::from_secs(3600)));
    let granted = (0..DEADLINE_POLL_INTERVAL * 2)
        .take_while(|_| budget.consume())
        .count();
    assert_eq!(granted, DEADLINE_POLL_INTERVAL + 3);
    assert!(budget.capped());
}
