use super::*;

fn selected_build(payload_fingerprint: String) -> CompletedBackendBuildWire {
    CompletedBackendBuildWire {
        requested_strategy: "templated-underlying-tokens".to_string(),
        realized_strategy: "templated-underlying-tokens".to_string(),
        grammar_identity: "grammar".to_string(),
        attempt_id: "attempt".to_string(),
        completion_proof: crate::completed_build::CompletionProofWire::TemplatedFullEmission {
            uncovered_count: 0,
            skipped_count: 0,
        },
        state_count: 1,
        arc_count: 1,
        model_fingerprint: "model".to_string(),
        payload_fingerprint,
    }
}

fn selected_success_with(
    payload_byte_len: u64,
    payload_sha256: String,
    payload_fingerprint: String,
) -> CompileWorkerResult {
    CompileWorkerResult {
        protocol_version: WORKER_PROTOCOL_VERSION,
        outcome: CompileWorkerOutcome::SelectedSuccess {
            build: selected_build(payload_fingerprint),
            payload_byte_len,
            payload_sha256,
        },
    }
}

fn selected_success(payload: &[u8]) -> CompileWorkerResult {
    let digest = sha256_hex(payload);
    selected_success_with(payload.len() as u64, digest.clone(), digest)
}

#[test]
fn selected_payload_exactly_at_limit_is_completed() {
    let payload = b"four".to_vec();

    let output = finish_selected_payload(
        selected_build(sha256_hex(&payload)),
        payload.clone(),
        payload.len() as u64,
    );

    assert!(matches!(
        output,
        WorkerChildOutput {
            outcome: CompileWorkerOutcome::SelectedSuccess { payload_byte_len: 4, .. },
            selected_payload: Some(actual),
        } if actual == payload
    ));
}

#[test]
fn selected_payload_one_byte_over_limit_emits_no_raw_frame() {
    let payload = b"four".to_vec();

    let output = finish_selected_payload(selected_build(sha256_hex(&payload)), payload, 3);

    assert!(matches!(
        output,
        WorkerChildOutput {
            outcome: CompileWorkerOutcome::SelectedExecutionLimitExceeded {
                actual_bytes: 4,
                limit_bytes: 3,
            },
            selected_payload: None,
        }
    ));
}

/// The readiness outcome must write exactly ONE frame; a second frame would BE the artifact.
#[test]
fn selected_not_production_ready_writes_no_raw_payload_frame() {
    let health = HealthReport::new(vec![HealthFinding::new(
        FindingCode::PartialMorphemeProductionPolicy,
        Severity::NotProductionReady,
        Phase::Compile,
        Metric::PartialMorphemeCount,
        MetricValue::Count(2),
        ValueProvenance::Observed,
        "synthetic partial-morpheme readiness refusal".to_string(),
    )]);
    let mut output = Vec::new();
    write_child_output(
        &mut output,
        WorkerChildOutput {
            outcome: CompileWorkerOutcome::SelectedNotProductionReady {
                health: health.clone(),
            },
            selected_payload: None,
        },
    )
    .expect("writing to an in-memory buffer must not fail");

    let mut cursor = std::io::Cursor::new(output);
    let metadata = read_frame(&mut cursor, WORKER_PROTOCOL_LIMITS.max_request_bytes)
        .expect("the metadata frame must be present");
    let result: CompileWorkerResult =
        serde_json::from_slice(&metadata).expect("result must deserialize");
    match result.outcome {
        CompileWorkerOutcome::SelectedNotProductionReady { health: reported } => {
            assert_eq!(
                reported, health,
                "the typed readiness report must survive the wire"
            );
        }
        other => panic!("expected SelectedNotProductionReady; got {other:?}"),
    }
    assert!(
        read_frame(&mut cursor, WORKER_PROTOCOL_LIMITS.max_request_bytes).is_err(),
        "no raw payload frame may escape a readiness refusal"
    );
}

#[test]
fn selected_request_json_contains_no_filesystem_destination() {
    let request = SelectedCompileRequest {
        attempt_id: "attempt-0123456789abcdef0123456789abcdef".to_string(),
        route: "tuned-surface-probed".to_string(),
        max_serialized_fst_bytes: 4,
    };

    let json = serde_json::to_value(request).expect("serialize selected request");

    assert!(json.get("artifact_path").is_none());
    assert!(json.get("artifact_directory").is_none());
    assert!(json.get("artifact_token").is_none());
}

#[test]
fn selected_success_json_contains_transport_metadata_but_no_payload() {
    let payload = b"fst!";
    let result = CompileWorkerResult {
        protocol_version: WORKER_PROTOCOL_VERSION,
        outcome: selected_success(payload).outcome,
    };

    let json = serde_json::to_value(result).expect("serialize selected success");

    assert_eq!(json["outcome"]["SelectedSuccess"]["payload_byte_len"], 4);
    assert_eq!(
        json["outcome"]["SelectedSuccess"]["payload_sha256"],
        sha256_hex(payload)
    );
    assert!(
        !json.to_string().contains("payload_bytes"),
        "selected result must contain metadata only: {json}"
    );
}

// Framing: bounded, validate-before-allocate, mirroring `pg_pack::format`'s own test shapes.

#[test]
fn frame_round_trips_small_payload() {
    let mut buf = Vec::new();
    write_frame(&mut buf, b"synthetic-payload").expect("write");
    let mut cursor = std::io::Cursor::new(buf);
    let read = read_frame(&mut cursor, 1024).expect("read");
    assert_eq!(read, b"synthetic-payload");
}

#[test]
fn read_frame_rejects_declared_length_over_limit_before_allocating() {
    // Declares a length far beyond any reasonable limit with no payload bytes following, so an allocate-before-validate bug would attempt a many-exabyte allocation instead of a clean error.
    let mut buf = Vec::new();
    buf.extend_from_slice(&u64::MAX.to_le_bytes());
    let mut cursor = std::io::Cursor::new(buf);
    let err = read_frame(&mut cursor, 1024).expect_err("must reject before allocating");
    match err {
        FrameError::LengthExceedsLimit { declared, limit } => {
            assert_eq!(declared, u64::MAX);
            assert_eq!(limit, 1024);
        }
        other => panic!("expected LengthExceedsLimit, got {other:?}"),
    }
}

#[test]
fn read_frame_rejects_length_exceeding_protocol_limit_with_short_buffer() {
    let mut buf = Vec::new();
    let huge = WORKER_PROTOCOL_LIMITS.max_request_bytes + 1;
    buf.extend_from_slice(&huge.to_le_bytes());
    // No body bytes -- proves rejection happens strictly from the header, before any attempt to `read_exact` a body.
    let mut cursor = std::io::Cursor::new(buf);
    let err = read_frame(&mut cursor, WORKER_PROTOCOL_LIMITS.max_request_bytes)
        .expect_err("must reject oversized declared length");
    assert!(matches!(err, FrameError::LengthExceedsLimit { .. }));
}

#[test]
fn decode_frame_body_rejects_malformed_json() {
    let err = decode_frame_body::<CompileWorkerRequest>(b"not valid json at all { { {")
        .expect_err("malformed JSON must not deserialize");
    assert!(matches!(err, FrameError::Json(_)));
}

#[test]
fn selected_request_rejects_removed_envelope_fields() {
    let error = serde_json::from_value::<SelectedCompileRequest>(serde_json::json!({
        "attempt_id": "attempt-test",
        "route": "tuned-surface-probed",
        "artifact_directory": "reserved-directory",
        "max_serialized_fst_bytes": 4,
        "envelope_id": "managed-v1",
    }))
    .expect_err("removed envelope fields must be rejected as unknown");
    assert!(
        error.to_string().contains("unknown field"),
        "expected deny_unknown_fields rejection, got {error}"
    );
}

// `run_worker_child` in-process: protocol handling plus the grammar-content outcomes reachable without a real adversarial grammar.

fn call_child(request_bytes: &[u8]) -> CompileWorkerResult {
    let mut input = std::io::Cursor::new(request_bytes.to_vec());
    let mut output = Vec::new();
    run_worker_child(&mut input, &mut output).expect(
        "run_worker_child must not I/O-error \
        against in-memory buffers",
    );
    let len = u64::from_le_bytes(output[0..8].try_into().unwrap()) as usize;
    serde_json::from_slice(&output[8..8 + len]).expect("result must deserialize")
}

#[test]
fn run_worker_child_reports_protocol_violation_for_oversized_request_frame() {
    let mut buf = Vec::new();
    buf.extend_from_slice(&(WORKER_PROTOCOL_LIMITS.max_request_bytes + 1).to_le_bytes());
    let result = call_child(&buf);
    match result.outcome {
        CompileWorkerOutcome::ProtocolViolation { detail } => {
            assert!(detail.contains("oversized") || detail.contains("exceeds"));
        }
        other => panic!("expected ProtocolViolation, got {other:?}"),
    }
}

#[test]
fn run_worker_child_reports_protocol_violation_for_malformed_request_json() {
    let mut buf = Vec::new();
    let body = b"not json";
    buf.extend_from_slice(&(body.len() as u64).to_le_bytes());
    buf.extend_from_slice(body);
    let result = call_child(&buf);
    assert!(matches!(
        result.outcome,
        CompileWorkerOutcome::ProtocolViolation { .. }
    ));
}

#[test]
fn run_worker_child_reports_protocol_violation_for_wrong_protocol_version() {
    let mut request = CompileWorkerRequest::new("does-not-matter.xml", GrammarFormat::Xml);
    request.protocol_version = WORKER_PROTOCOL_VERSION + 1;
    let json = serde_json::to_vec(&request).unwrap();
    let mut buf = Vec::new();
    buf.extend_from_slice(&(json.len() as u64).to_le_bytes());
    buf.extend_from_slice(&json);
    let result = call_child(&buf);
    match result.outcome {
        CompileWorkerOutcome::ProtocolViolation { detail } => {
            assert!(detail.contains("protocol_version"));
        }
        other => panic!("expected ProtocolViolation, got {other:?}"),
    }
}

#[test]
fn run_worker_child_reports_grammar_load_failed_for_missing_file() {
    let request =
        CompileWorkerRequest::new("this-file-does-not-exist-synthetic.xml", GrammarFormat::Xml);
    let json = serde_json::to_vec(&request).unwrap();
    let mut buf = Vec::new();
    buf.extend_from_slice(&(json.len() as u64).to_le_bytes());
    buf.extend_from_slice(&json);
    let result = call_child(&buf);
    assert!(matches!(
        result.outcome,
        CompileWorkerOutcome::GrammarLoadFailed { .. }
    ));
}

fn scratch_grammar_file(tag: &str, xml: &str) -> std::path::PathBuf {
    static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "pg-foma-worker-test-{tag}-{}-{n}.xml",
        std::process::id()
    ));
    std::fs::write(&path, xml).expect("write scratch grammar");
    path
}

#[test]
fn run_worker_child_reports_success_for_a_clean_small_grammar() {
    const CLEAN_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>WorkerSuccessFixture</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="table1">
      <Name>Orthography</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="segA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="segK"><Representations><Representation>k</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="segT"><Representations><Representation>t</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses></NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="table1">
        <Name>main</Name>
        <LexicalEntries>
          <LexicalEntry id="e1">
            <Allomorphs><Allomorph id="e1-1"><PhoneticShape>kat</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>kat</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;
    let path = scratch_grammar_file("success", CLEAN_XML);
    let request =
        CompileWorkerRequest::new(path.to_string_lossy().into_owned(), GrammarFormat::Xml);
    let json = serde_json::to_vec(&request).unwrap();
    let mut buf = Vec::new();
    buf.extend_from_slice(&(json.len() as u64).to_le_bytes());
    buf.extend_from_slice(&json);

    let result = call_child(&buf);
    match result.outcome {
        CompileWorkerOutcome::Success { health, .. } => {
            assert_eq!(health.admission(), Severity::WithinLimits);
        }
        other => panic!("expected Success, got {other:?}"),
    }
    let _ = std::fs::remove_file(&path);
}
