use super::*;

/// An uncapped (`usize::MAX`) budget with a short deadline armed must break out promptly, not run the huge iteration bound to completion.
#[test]
fn wall_clock_deadline_fires_independent_of_an_uncapped_step_cap() {
    const N_HUGE: u64 = 200_000_000; // large enough to run far longer than the deadline below on any dev/CI machine
    let timeout = Duration::from_millis(30);
    let budget = StepBudget::new(usize::MAX).with_timeout(Some(timeout));

    let start = Instant::now();
    let mut i: u64 = 0;
    while i < N_HUGE {
        if budget.over_budget() {
            break;
        }
        budget.tick();
        i += 1;
    }
    let elapsed = start.elapsed();

    assert!(
        budget.timed_out(),
        "budget must report timed_out() once the deadline elapses (i={i} of {N_HUGE})"
    );
    assert!(
        !budget.capped(),
        "the step cap (usize::MAX) must never fire — timeout and step-cap are independent bounds"
    );
    assert!(
        i < N_HUGE,
        "the loop must break out well before the artificial huge bound, not run to completion"
    );
    // Generous for slow CI machines, but far tighter than a full N_HUGE run.
    assert!(
        elapsed < Duration::from_secs(2),
        "elapsed {elapsed:?} should stay close to the {timeout:?} deadline, not balloon toward \
         an unbounded run"
    );
}

/// A deadline already in the past at construction time must fire on the first `over_budget()` check.
#[test]
fn zero_deadline_fires_on_the_first_check() {
    let budget = StepBudget::new(usize::MAX).with_timeout(Some(Duration::from_millis(0)));
    // Give the already-past deadline a moment's daylight against timer granularity.
    std::thread::sleep(Duration::from_millis(1));
    assert!(
        budget.over_budget(),
        "an already-past deadline must fire on the first check"
    );
    assert!(budget.timed_out());
    assert!(!budget.capped());
}

/// Fewer ticks than one cadence interval, with real time elapsing between them: reading the clock only at step 0 would run to completion.
#[test]
fn wall_clock_deadline_fires_even_when_total_ticks_never_reach_the_old_check_interval() {
    const N: u64 = 200; // well under the old 1024-tick cadence interval
    let timeout = Duration::from_millis(50);
    let budget = StepBudget::new(usize::MAX).with_timeout(Some(timeout));

    let start = Instant::now();
    let mut fired = false;
    let mut i: u64 = 0;
    while i < N {
        if budget.over_budget() {
            fired = true;
            break;
        }
        budget.tick();
        std::thread::sleep(Duration::from_millis(1));
        i += 1;
    }
    let elapsed = start.elapsed();

    assert!(
        fired,
        "the 50ms deadline must fire even though the loop only reaches {i} of {N} ticks -- \
         far short of the old 1024-tick cadence interval"
    );
    assert!(budget.timed_out());
    assert!(
        !budget.capped(),
        "the step cap (usize::MAX) must never fire"
    );
    assert!(
        i < N,
        "must break out before exhausting all {N} ticks (i={i}) -- the pre-fix cadence ran to \
         completion here because it never re-sampled the clock after step 0"
    );
    assert!(
        elapsed < Duration::from_secs(2),
        "elapsed {elapsed:?} should stay close to the {timeout:?} deadline, not run all {N} \
         ticks worth of sleeps (~{N}ms) unchecked"
    );
}

/// `with_timeout(None)` must be a complete no-op, behaving exactly as a plain `StepBudget::new(cap)` would.
#[test]
fn no_timeout_never_times_out() {
    let budget = StepBudget::new(5).with_timeout(None);
    for _ in 0..5 {
        assert!(!budget.over_budget());
        budget.tick();
    }
    assert!(budget.over_budget(), "step cap must still fire on its own");
    assert!(budget.capped());
    assert!(
        !budget.timed_out(),
        "no deadline was armed, so timed_out() must stay false"
    );
}
