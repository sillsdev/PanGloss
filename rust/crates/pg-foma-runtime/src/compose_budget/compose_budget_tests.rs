use super::*;
// Chain-depth dimension: exercises `check_chain_depth` directly, no `Fsm`/foma call involved.

#[test]
fn chain_depth_unbounded_budget_never_trips() {
    // `unbounded()` must also leave chain depth off, like the size dimensions above.
    let budget = ComposeBudget::unbounded();
    // Well past the motivating Aweti 24-level chain and still `Ok`.
    budget
        .check_chain_depth(1_000_000, "chain_depth_unbounded_budget_never_trips")
        .expect("unbounded chain-depth budget must never trip, at any depth");
}

#[test]
fn chain_depth_is_off_on_an_unbounded_budget() {
    let budget = ComposeBudget::unbounded();
    budget
        .check_chain_depth(usize::MAX, "chain_depth_is_off_on_an_unbounded_budget")
        .expect("an unbounded budget must leave chain depth off");
}

#[test]
fn chain_depth_explicit_cap_does_not_trip_at_or_below_limit() {
    let budget = ComposeBudget::unbounded().with_chain_depth_cap(24);
    budget
        .check_chain_depth(
            24,
            "chain_depth_explicit_cap_does_not_trip_at_or_below_limit",
        )
        .expect("depth == cap must be accepted, mirroring every other cap's <= convention");
    budget
        .check_chain_depth(
            1,
            "chain_depth_explicit_cap_does_not_trip_at_or_below_limit",
        )
        .expect("depth well below cap must be accepted");
}

#[test]
fn chain_depth_explicit_cap_trips_one_past_limit() {
    // 24: the motivating Aweti derivation-chain depth (module doc).
    let budget = ComposeBudget::unbounded().with_chain_depth_cap(24);
    let err = budget
        .check_chain_depth(25, "chain_depth_explicit_cap_trips_one_past_limit")
        .expect_err("depth == cap + 1 must trip");
    match err {
        ComposeError::ChainDepthExceeded { depth, limit, site } => {
            assert_eq!(depth, 25);
            assert_eq!(limit, 24);
            assert_eq!(site, "chain_depth_explicit_cap_trips_one_past_limit");
        }
    }
}

#[test]
fn chain_depth_absolute_ceiling_clamps_excessive_configured_cap() {
    // A cap far above the absolute ceiling must clamp down to it, not pass through verbatim.
    let budget =
        ComposeBudget::unbounded().with_chain_depth_cap(CHAIN_DEPTH_ABSOLUTE_CEILING + 1_000);
    // One past the clamped ceiling must trip, reporting the ceiling as the limit, not the original request.
    let err = budget
        .check_chain_depth(
            CHAIN_DEPTH_ABSOLUTE_CEILING + 1,
            "chain_depth_absolute_ceiling_clamps_excessive_configured_cap",
        )
        .expect_err("one past the clamped ceiling must trip");
    match err {
        ComposeError::ChainDepthExceeded { limit, .. } => {
            assert_eq!(limit, CHAIN_DEPTH_ABSOLUTE_CEILING);
        }
    }
}

#[test]
fn chain_depth_cap_from_env_clamps_to_absolute_ceiling() {
    // Exercises `chain_depth_cap_from_env`'s clamp via the pure function, without touching process-global env state.
    assert_eq!(
        clamp_chain_depth_cap(CHAIN_DEPTH_ABSOLUTE_CEILING + 1_000),
        CHAIN_DEPTH_ABSOLUTE_CEILING
    );
    assert_eq!(
        clamp_chain_depth_cap(24),
        24,
        "a cap under the ceiling must pass through unchanged"
    );
}

#[test]
fn chain_depth_exceeded_display_is_specific() {
    let err = ComposeError::ChainDepthExceeded {
        depth: 25,
        limit: 24,
        site: "unit-test-site",
    };
    let msg = err.to_string();
    assert!(msg.contains("unit-test-site"));
    assert!(msg.contains("25"));
    assert!(msg.contains("24"));
    assert!(msg.contains("HC_COMPOSE_CHAIN_DEPTH_BUDGET"));
}

// Apply-path dimension: schema/budget-type tests only; decode-loop wiring is `analyzer.rs`'s own `propose_budgeted` tests.

#[test]
fn apply_budget_unbounded_has_no_caps() {
    let budget = ApplyBudget::unbounded();
    assert_eq!(budget.path_cap(), None);
    assert_eq!(budget.candidate_cap(), None);
}

#[test]
fn apply_budget_with_caps_round_trips_each_dimension_independently() {
    let budget = ApplyBudget::with_caps(Some(10), None);
    assert_eq!(budget.path_cap(), Some(10));
    assert_eq!(budget.candidate_cap(), None);

    let budget = ApplyBudget::with_caps(None, Some(5));
    assert_eq!(budget.path_cap(), None);
    assert_eq!(budget.candidate_cap(), Some(5));
}

#[test]
fn apply_budget_from_env_defaults_to_unbounded_when_unset() {
    // Reads real env (only `from_env` does); asserts the fallback only when the var is genuinely unset in this test process.
    if std::env::var("HC_APPLY_PATH_BUDGET").is_err() {
        assert_eq!(ApplyBudget::from_env().path_cap(), None);
    }
    if std::env::var("HC_APPLY_CANDIDATE_BUDGET").is_err() {
        assert_eq!(ApplyBudget::from_env().candidate_cap(), None);
    }
}

#[test]
fn apply_dimension_label_is_stable_and_distinct() {
    assert_eq!(
        ApplyDimension::DecodedPaths.label(),
        "decoded apply_up paths"
    );
    assert_eq!(ApplyDimension::Candidates.label(), "distinct candidates");
    assert_ne!(
        ApplyDimension::DecodedPaths.label(),
        ApplyDimension::Candidates.label()
    );
}

#[test]
fn apply_outcome_complete_and_incomplete_are_distinguishable() {
    let complete: ApplyOutcome<Vec<u32>> = ApplyOutcome::Complete(vec![1, 2, 3]);
    assert_eq!(complete, ApplyOutcome::Complete(vec![1, 2, 3]));

    let incomplete: ApplyOutcome<Vec<u32>> = ApplyOutcome::Incomplete {
        dimension: ApplyDimension::Candidates,
        value: 11,
        limit: 10,
    };
    match incomplete {
        ApplyOutcome::Incomplete {
            dimension,
            value,
            limit,
        } => {
            assert_eq!(dimension, ApplyDimension::Candidates);
            assert_eq!(value, 11);
            assert_eq!(limit, 10);
        }
        ApplyOutcome::Complete(_) => panic!("expected Incomplete"),
    }
}
