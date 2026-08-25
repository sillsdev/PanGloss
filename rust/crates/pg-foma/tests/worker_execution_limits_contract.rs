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
    assert_eq!(WORKER_PROTOCOL_VERSION, 6);
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
fn protocol_five_request_frames_are_rejected_before_compile() {
    let mut request = CompileWorkerRequest::new("stale.xml", GrammarFormat::Xml);
    request.protocol_version = 5;
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
            assert!(detail.contains("5"), "detail: {detail}");
            assert!(detail.contains("6"), "detail: {detail}");
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
fn selected_compile_request_wire_is_closed_and_identity_bearing_only() {
    let source = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/worker.rs"));
    let start = source
        .find("struct SelectedCompileRequest")
        .expect("worker source must declare SelectedCompileRequest");
    let end = source[start..]
        .find('}')
        .map(|offset| start + offset)
        .expect("SelectedCompileRequest declaration must be closed");
    let declaration = &source[start..=end];
    let attributes = &source[start.saturating_sub(256)..start];

    assert!(
        attributes.contains("#[serde(deny_unknown_fields)]"),
        "SelectedCompileRequest must reject removed wire fields"
    );
    assert!(declaration.contains("attempt_id:"));
    assert!(declaration.contains("route:"));
    for removed in [
        "schema_version:",
        "envelope_id:",
        "envelope_digest:",
        "watchdog:",
        "communication:",
        "compose:",
        "enumeration:",
        "backend:",
    ] {
        assert!(
            !declaration.contains(removed),
            "SelectedCompileRequest must not retain removed field {removed}"
        );
    }
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

#[test]
fn selected_compile_result_keeps_payload_out_of_the_bounded_result_frame() {
    let worker = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/worker.rs"));
    let completed_build = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/completed_build.rs"
    ));

    let wire_start = completed_build
        .find("pub struct CompletedBackendBuildWire")
        .expect("completed-build source must declare the selected-build metadata wire");
    let wire_end = completed_build[wire_start..]
        .find('}')
        .map(|offset| wire_start + offset)
        .expect("selected-build metadata wire declaration must be closed");
    let wire_declaration = &completed_build[wire_start..=wire_end];

    assert!(
        !wire_declaration.contains("payload_bytes:"),
        "the 16-MiB JSON result must carry metadata, never the serialized FST bytes"
    );
    assert!(
        worker.contains("artifact_path:") || worker.contains("artifact_token:"),
        "the selected request must carry a parent-controlled out-of-band artifact destination"
    );
    assert!(
        worker.contains("SelectedSuccess") && worker.contains("artifact"),
        "selected success must report an out-of-band artifact, not inline payload bytes"
    );
}

#[test]
fn selected_compile_enforces_the_serialized_fst_limit_with_a_typed_failure() {
    let source = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/worker.rs"));
    let start = source
        .find("fn compile_selected_from_request(")
        .expect("worker source must declare selected compilation");
    let end = source[start..]
        .find("/// The worker CHILD")
        .map(|offset| start + offset)
        .expect("selected compilation must end before the worker entrypoint");
    let selected_compile = &source[start..end];
    assert!(
        selected_compile.contains("limits: &ExecutionLimits"),
        "selected compilation must receive the supervisor's execution limits"
    );
    assert!(
        selected_compile.contains("max_serialized_fst_bytes()"),
        "the selected worker must apply max_serialized_fst_bytes"
    );
    assert!(
        selected_compile.contains("payload_bytes.len()")
            || selected_compile.contains("serialized_fst_bytes"),
        "the limit must be measured against the actual serialized selected payload"
    );
    assert!(
        source.contains("SelectedExecutionLimitExceeded"),
        "an over-limit selected build must have a typed worker failure, not a detail-only compile failure"
    );
}

#[test]
fn selected_artifact_failures_remove_partial_and_final_transport_files() {
    let source = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/worker.rs"));
    let start = source
        .find("fn compile_selected_from_request(")
        .expect("worker source must declare selected compilation");
    let end = source[start..]
        .find("/// The worker CHILD")
        .map(|offset| start + offset)
        .expect("selected compilation must end before the worker entrypoint");
    let selected_compile = &source[start..end];
    let writer_start = source
        .find("fn write_selected_artifact(")
        .expect("worker source must declare the artifact writer");
    let writer_end = source[writer_start..]
        .find("fn compile_selected_from_request(")
        .map(|offset| writer_start + offset)
        .expect("artifact writer must end before selected compilation");
    let artifact_writer = &source[writer_start..writer_end];

    assert!(
        selected_compile.contains("remove_file") || selected_compile.contains("cleanup"),
        "selected compile failures must remove temporary and final artifact paths"
    );
    assert!(selected_compile.contains("write_selected_artifact"));
    assert!(
        artifact_writer.contains("fs::rename"),
        "a completed artifact must be published atomically, never left as a partial file"
    );
    assert!(
        source.contains("cleanup_selected_transport_dir(&transport_dir)"),
        "the parent must remove the reserved transport directory after every outcome"
    );
    assert!(
        source.contains("artifact_path") || source.contains("artifact_token"),
        "cleanup must be tied to the selected worker's transport artifact"
    );
}

#[test]
fn selected_transport_fields_stay_out_of_compile_and_completed_build_identity() {
    let worker = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/worker.rs"));
    let completed_build = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/completed_build.rs"
    ));

    let selected_start = worker
        .find("struct SelectedCompileRequest")
        .expect("selected request declaration must remain explicit");
    let selected_end = worker[selected_start..]
        .find('}')
        .map(|offset| selected_start + offset)
        .expect("selected request declaration must be closed");
    let selected_declaration = &worker[selected_start..=selected_end];
    assert!(
        selected_declaration.contains("artifact_path:")
            || selected_declaration.contains("artifact_token:"),
        "the artifact destination belongs to transport request state"
    );

    for declaration_name in [
        "pub struct CompileAttempt",
        "pub struct CompletedBackendBuildEvidence",
        "pub struct CompletedBackendBuildWire",
    ] {
        let start = completed_build
            .find(declaration_name)
            .unwrap_or_else(|| panic!("completed-build source must declare {declaration_name}"));
        let end = completed_build[start..]
            .find('}')
            .map(|offset| start + offset)
            .unwrap_or_else(|| panic!("{declaration_name} declaration must be closed"));
        let declaration = &completed_build[start..=end];
        for transport_field in [
            "artifact_path",
            "artifact_token",
            "artifact_directory",
            "ExecutionLimits",
            "max_serialized_fst_bytes",
            "max_committed_memory_bytes",
            "max_wall_time",
        ] {
            assert!(
                !declaration.contains(transport_field),
                "{declaration_name} must not carry transport/execution field {transport_field}"
            );
        }
    }
}
