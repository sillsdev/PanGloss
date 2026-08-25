use std::time::Duration;

use pg_foma::worker::{
    limits_for_version, ExecutionLimitError, ExecutionLimits, DEFAULT_EXECUTION_LIMITS,
    WORKER_PROTOCOL_VERSION,
};

const GIB: u64 = 1024 * 1024 * 1024;

#[test]
fn execution_limits_have_the_ratified_finite_defaults() {
    assert_eq!(
        DEFAULT_EXECUTION_LIMITS.max_serialized_fst_bytes(),
        GIB
    );
    assert_eq!(
        DEFAULT_EXECUTION_LIMITS.max_committed_memory_bytes(),
        10 * GIB
    );
    assert_eq!(
        DEFAULT_EXECUTION_LIMITS.max_wall_time(),
        Duration::from_secs(10 * 60)
    );
}

#[test]
fn execution_limits_are_configurable_but_cannot_be_disabled() {
    let custom = ExecutionLimits::try_new(
        2 * GIB,
        12 * GIB,
        Duration::from_secs(15 * 60),
    )
    .expect("positive custom limits are valid");
    assert_eq!(custom.max_serialized_fst_bytes(), 2 * GIB);
    assert_eq!(custom.max_committed_memory_bytes(), 12 * GIB);
    assert_eq!(custom.max_wall_time(), Duration::from_secs(15 * 60));

    assert_eq!(
        ExecutionLimits::try_new(0, 12 * GIB, Duration::from_secs(60)),
        Err(ExecutionLimitError::ZeroSerializedFstBytes)
    );
    assert_eq!(
        ExecutionLimits::try_new(GIB, 0, Duration::from_secs(60)),
        Err(ExecutionLimitError::ZeroCommittedMemoryBytes)
    );
    assert_eq!(
        ExecutionLimits::try_new(GIB, 10 * GIB, Duration::ZERO),
        Err(ExecutionLimitError::ZeroWallTime)
    );
}

#[test]
fn cleanup_breaks_the_old_worker_protocol_in_lockstep() {
    assert_eq!(WORKER_PROTOCOL_VERSION, 2);
    assert!(
        limits_for_version(1).is_none(),
        "pre-cleanup worker messages must be rejected, not migrated"
    );
    assert!(limits_for_version(WORKER_PROTOCOL_VERSION).is_some());
}
