use super::*;
use web_time::Instant;

/// An uncapped (`usize::MAX`) budget with a short deadline armed must break out promptly, not run the huge iteration bound to completion.
#[test]
fn wall_clock_deadline_fires_independent_of_an_uncapped_work_cap() {
    const N_HUGE: u64 = 200_000_000; // large enough to run far longer than the deadline below on any dev/CI machine
    let timeout = Duration::from_millis(30);
    let budget = WordBudget::new(usize::MAX).with_timeout(Some(timeout));

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
        "the work cap (usize::MAX) must never fire — timeout and work-cap are independent bounds"
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
    let budget = WordBudget::new(usize::MAX).with_timeout(Some(Duration::from_millis(0)));
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
    let budget = WordBudget::new(usize::MAX).with_timeout(Some(timeout));

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
        "the work cap (usize::MAX) must never fire"
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

/// `with_timeout(None)` must be a complete no-op, behaving exactly as a plain `WordBudget::new(cap)` would.
#[test]
fn no_timeout_never_times_out() {
    let budget = WordBudget::new(5).with_timeout(None);
    for _ in 0..5 {
        assert!(!budget.over_budget());
        assert!(budget.tick());
    }
    assert!(
        budget.over_budget(),
        "the work cap must still fire on its own"
    );
    assert!(budget.capped());
    assert!(
        !budget.timed_out(),
        "no deadline was armed, so timed_out() must stay false"
    );
}

/// An analysis tick and a synthesis check spend the one allowance, a unit each, and nothing else.
#[test]
fn analysis_and_synthesis_spend_one_work_allowance() {
    let budget = WordBudget::new(7);
    for _ in 0..3 {
        assert!(budget.tick());
    }
    assert_eq!(budget.work_steps(), 3);
    for _ in 0..4 {
        assert!(!budget.synthesis_over_budget());
    }
    assert_eq!(budget.work_steps(), 7);
    assert!(budget.synthesis_over_budget());
    assert!(!budget.tick());
    assert!(budget.capped());
    assert_eq!(
        budget.work_steps(),
        7,
        "a refused check never spends a unit"
    );
}

#[test]
fn a_word_capped_on_work_alone_stops_analysis_and_reports_capped() {
    let budget = WordBudget::new(2);
    assert!(budget.tick());
    assert!(budget.tick());
    assert!(!budget.tick());
    assert!(budget.capped());
    assert_eq!(budget.work_steps(), 2);
}

/// Analysis may spend the whole cap, and a word within the cap never trips the analysis limit.
#[test]
fn analysis_spends_the_whole_cap_and_a_word_within_it_never_stops() {
    let budget = WordBudget::new(160);
    for _ in 0..160 {
        assert!(budget.tick());
    }
    assert_eq!(budget.work_steps(), 160);
    assert!(!budget.capped(), "a word of exactly the cap is complete");
    assert!(!budget.tick(), "a 161st analysis unit is refused");
    assert!(budget.capped());
    assert_eq!(budget.work_steps(), 160);
}

/// The reserve sits on top of the cap: confirmation spends it after analysis stopped, and nothing spends past it.
#[test]
fn confirmation_spends_the_reserve_beyond_the_cap_and_no_further() {
    let budget = WordBudget::new(160);
    while budget.tick() {}
    assert!(budget.capped(), "analysis reached the cap");
    assert!(!budget.analysis_unit(), "analysis stays stopped");
    for _ in 0..10 {
        assert!(
            !budget.synthesis_over_budget(),
            "confirmation spends the reserve"
        );
    }
    assert!(budget.synthesis_over_budget());
    assert_eq!(budget.work_steps(), 170, "the bound is cap + cap / 16");
}

/// A word that finished analysis within the cap and needs more in confirmation uses the reserve without analysis having capped.
#[test]
fn confirmation_within_the_reserve_is_not_a_cap() {
    let budget = WordBudget::new(160);
    for _ in 0..100 {
        assert!(budget.tick());
    }
    for _ in 0..70 {
        assert!(!budget.synthesis_over_budget());
    }
    assert!(!budget.capped(), "170 of 170 units is not past the reserve");
    assert!(budget.synthesis_over_budget());
    assert!(budget.capped(), "the 171st unit is past cap + reserve");
}

#[test]
fn a_cap_under_sixteen_has_no_reserve() {
    let budget = WordBudget::new(15);
    for _ in 0..15 {
        assert!(!budget.synthesis_over_budget());
    }
    assert!(budget.synthesis_over_budget());
    assert_eq!(budget.work_steps(), 15);
}
#[test]
fn an_unbounded_allowance_counts_without_capping() {
    let budget = WordBudget::new(usize::MAX);
    for _ in 0..64 {
        assert!(!budget.synthesis_over_budget());
    }
    assert_eq!(budget.work_steps(), 64);
    assert!(!budget.capped());
}
