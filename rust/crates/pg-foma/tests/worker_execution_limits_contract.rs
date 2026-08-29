use std::time::Duration;

use pg_foma::worker::{
    run_worker_child, CompileWorkerOutcome, CompileWorkerRequest, GrammarFormat,
};
use pg_worker_containment::{ExecutionLimitError, ExecutionLimits, DEFAULT_EXECUTION_LIMITS};

const GIB: u64 = 1024 * 1024 * 1024;

#[test]
fn execution_limits_have_the_ratified_finite_defaults() {
    assert_eq!(DEFAULT_EXECUTION_LIMITS.max_serialized_fst_bytes(), GIB);
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
    let custom = ExecutionLimits::try_new(2 * GIB, 12 * GIB, Duration::from_secs(15 * 60))
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

    for field in ["max_request_bytes:", "max_result_bytes:"] {
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
