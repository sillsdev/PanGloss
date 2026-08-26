use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use pg_foma::worker::{
    limits_for_version, run_compile_worker, run_worker_child, CompileWorkerOutcome,
    CompileWorkerRequest, ExecutionLimitError, ExecutionLimits, GrammarFormat, WorkerOutcome,
    DEFAULT_EXECUTION_LIMITS, WORKER_PROTOCOL_LIMITS, WORKER_PROTOCOL_VERSION,
};

const GIB: u64 = 1024 * 1024 * 1024;
static CHILD_ENV_LOCK: Mutex<()> = Mutex::new(());

fn selected_request(payload_limit: u64) -> CompileWorkerRequest {
    let request = CompileWorkerRequest::new("unused.xml", GrammarFormat::Xml);
    let mut json = serde_json::to_value(request).expect("serialize selected request fixture");
    json["selected"] = serde_json::json!({
        "attempt_id": "attempt-test",
        "route": "templated-underlying-tokens",
        "max_serialized_fst_bytes": payload_limit,
    });
    serde_json::from_value(json).expect("deserialize selected request fixture")
}

fn run_selected_output_mode(mode: &str, wall_time: Duration) -> WorkerOutcome {
    let _guard = CHILD_ENV_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    std::env::set_var("PANGLOSS_WORKER_TEST_OUTPUT_MODE", mode);
    let request = selected_request(1024);
    let limits = ExecutionLimits::try_new(GIB, 10 * GIB, wall_time)
        .expect("positive test limits must be valid");
    let outcome = run_compile_worker(
        Path::new(env!("CARGO_BIN_EXE_worker_test_child")),
        &[],
        &request,
        &limits,
    );
    std::env::remove_var("PANGLOSS_WORKER_TEST_OUTPUT_MODE");
    outcome
}

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
    assert_eq!(WORKER_PROTOCOL_VERSION, 9);
    assert!(
        limits_for_version(1).is_none(),
        "pre-cleanup worker messages must be rejected, not migrated"
    );
    assert!(
        limits_for_version(2).is_none(),
        "protocol-2 worker messages must be rejected, not migrated"
    );
    assert!(limits_for_version(WORKER_PROTOCOL_VERSION).is_some());
}

#[test]
fn protocol_eight_request_frames_are_rejected_before_compile() {
    let mut request = CompileWorkerRequest::new("stale.xml", GrammarFormat::Xml);
    request.protocol_version = 8;
    let body = serde_json::to_vec(&request).expect("serialize stale request");
    let mut input = Vec::new();
    input.extend_from_slice(&(body.len() as u64).to_le_bytes());
    input.extend_from_slice(&body);

    let mut output = Vec::new();
    run_worker_child(std::io::Cursor::new(input), &mut output)
        .expect("stale request must receive a typed response");
    let len = u64::from_le_bytes(output[0..8].try_into().expect("result length")) as usize;
    let result: pg_foma::worker::CompileWorkerResult =
        serde_json::from_slice(&output[8..8 + len]).expect("decode protocol response");
    match result.outcome {
        CompileWorkerOutcome::ProtocolViolation { detail } => {
            assert!(detail.contains("protocol_version"), "detail: {detail}");
            assert!(detail.contains("8"), "detail: {detail}");
            assert!(detail.contains("9"), "detail: {detail}");
        }
        other => panic!("expected ProtocolViolation, got {other:?}"),
    }
}

#[test]
fn oversized_request_is_rejected_by_the_wire_frame_limit_before_spawning() {
    let huge_path = "x".repeat((WORKER_PROTOCOL_LIMITS.max_request_bytes + 1) as usize);
    let request = CompileWorkerRequest::new(huge_path, GrammarFormat::Xml);

    let outcome = run_compile_worker(
        Path::new("worker-child-must-not-start"),
        &[],
        &request,
        &DEFAULT_EXECUTION_LIMITS,
    );
    assert!(
        matches!(outcome, WorkerOutcome::ProtocolViolation { .. }),
        "wire-frame overflow must be rejected before worker spawn: {outcome:?}"
    );
}

#[test]
fn wall_limit_kills_a_slow_worker_process() {
    let _guard = CHILD_ENV_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    std::env::set_var("PANGLOSS_WORKER_TEST_SLEEP_MS", "5000");
    let request = CompileWorkerRequest::new("unused.xml", GrammarFormat::Xml);
    let limits = ExecutionLimits::try_new(GIB, 10 * GIB, Duration::from_millis(200))
        .expect("positive test limits must be valid");

    let started = Instant::now();
    let outcome = run_compile_worker(
        Path::new(env!("CARGO_BIN_EXE_worker_test_child")),
        &[],
        &request,
        &limits,
    );
    let elapsed = started.elapsed();
    std::env::remove_var("PANGLOSS_WORKER_TEST_SLEEP_MS");

    match outcome {
        WorkerOutcome::WallTimeoutKilled { limit, .. } => {
            assert_eq!(limit, limits.max_wall_time());
        }
        other => panic!("expected the execution wall limit to kill the child, got {other:?}"),
    }
    assert!(
        elapsed < Duration::from_secs(4),
        "the supervisor must return before the child's five-second sleep; took {elapsed:?}"
    );
}

#[test]
fn malformed_selected_payload_processes_never_complete() {
    for mode in ["selected-missing", "selected-truncated", "selected-trailing"] {
        let outcome = run_selected_output_mode(mode, Duration::from_secs(2));
        assert!(
            matches!(
                &outcome,
                WorkerOutcome::ProtocolViolation { .. } | WorkerOutcome::ChildCrashed { .. }
            ),
            "synthetic mode {mode:?} must not complete a selected payload: {outcome:?}"
        );
        assert!(
            !matches!(&outcome, WorkerOutcome::SelectedCompleted { .. }),
            "synthetic mode {mode:?} unexpectedly completed: {outcome:?}"
        );
    }
}

#[test]
fn selected_payload_process_stalling_after_header_is_killed_by_wall_limit() {
    let started = Instant::now();
    let outcome = run_selected_output_mode("selected-stall", Duration::from_millis(200));
    let elapsed = started.elapsed();
    match outcome {
        WorkerOutcome::WallTimeoutKilled { limit, .. } => {
            assert_eq!(limit, Duration::from_millis(200));
        }
        other => panic!("expected stalled selected worker to be killed, got {other:?}"),
    }
    assert!(
        elapsed < Duration::from_secs(4),
        "the supervisor must return before the synthetic child's five-second stall; took {elapsed:?}"
    );
}

#[test]
fn supervisor_accepts_execution_limits_as_its_only_execution_control_input() {
    let source = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/worker.rs"));
    let start = source
        .find("pub fn run_compile_worker(")
        .expect("worker source must declare the supervisor entrypoint");
    let end = source[start..]
        .find(") -> WorkerOutcome")
        .map(|offset| start + offset)
        .expect("supervisor signature must return WorkerOutcome");
    let signature = &source[start..=end];

    assert!(
        signature.contains("limits: &ExecutionLimits"),
        "supervisor must receive the finite execution limits directly: {signature}"
    );
    assert!(
        !signature.contains("WatchdogEnvelope")
            && !signature.contains("rss_limit")
            && !signature.contains("sample_interval"),
        "old watchdog controls must not remain supervisor inputs: {signature}"
    );

    let selected_start = source
        .find("pub fn run_selected_compile_worker(")
        .expect("selected worker entrypoint must remain explicit");
    let selected_end = source[selected_start..]
        .find(") -> Result<")
        .map(|offset| selected_start + offset)
        .expect("selected worker signature must return a result");
    let selected_signature = &source[selected_start..=selected_end];
    assert!(
        selected_signature.contains("limits: &ExecutionLimits"),
        "selected supervisor entrypoint must receive the same limits: {selected_signature}"
    );
    assert!(!selected_signature.contains("WatchdogEnvelope"));
}

#[test]
fn wire_frame_limits_remain_separate_from_execution_limits() {
    let source = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/worker.rs"));
    let start = source
        .find("pub struct WorkerProtocolLimits")
        .expect("worker source must declare wire-frame limits");
    let end = source[start..]
        .find('}')
        .map(|offset| start + offset)
        .expect("wire-frame limit declaration must be closed");
    let declaration = &source[start..=end];

    for field in [
        "max_request_bytes:",
        "max_result_bytes:",
        "max_captured_stderr_bytes:",
    ] {
        assert!(
            declaration.contains(field),
            "wire-frame limit {field} must remain explicit"
        );
    }
    for removed in [
        "max_wall_timeout_ms:",
        "max_rss_limit_mb:",
        "min_rss_sample_interval_ms:",
    ] {
        assert!(
            !declaration.contains(removed),
            "execution-control field {removed} must not be disguised as a wire-frame limit"
        );
    }
}

#[test]
fn compile_attempt_and_completed_build_identity_surfaces_do_not_carry_execution_limits() {
    let source = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/completed_build.rs"
    ));

    for declaration_name in [
        "pub struct CompileAttempt",
        "pub struct CompletedBackendBuildEvidence",
        "pub struct CompletedBackendBuildWire",
    ] {
        let start = source
            .find(declaration_name)
            .unwrap_or_else(|| panic!("completed-build source must declare {declaration_name}"));
        let end = source[start..]
            .find('}')
            .map(|offset| start + offset)
            .unwrap_or_else(|| panic!("{declaration_name} declaration must be closed"));
        let declaration = &source[start..=end];
        for removed in [
            "ExecutionLimits",
            "WatchdogEnvelope",
            "max_serialized_fst_bytes",
            "max_committed_memory_bytes",
            "max_wall_time",
        ] {
            assert!(
                !declaration.contains(removed),
                "{declaration_name} must not carry execution-control field {removed}"
            );
        }
    }
}
