# Selected Worker Raw-Payload Transport Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Delete selected-build filesystem transport and carry a selected FST through one strictly bounded raw worker-output frame.

**Architecture:** Protocol v9 keeps the small JSON request/result frame and adds one raw payload frame only after `SelectedSuccess`. A protocol-aware stdout reader validates control and payload limits independently, returns an owned payload without cloning a whole stdout buffer, and accepts success only after digest, fingerprint, EOF, and process-exit validation.

**Tech Stack:** Rust, serde/serde_json, SHA-256 helpers already in `pg-foma`, length-prefixed stdin/stdout framing, `std::process`, nextest through `rust/tools/pg.ps1`.

---

## File map

- Modify `rust/crates/pg-foma/src/worker.rs`: v9 wire types, child output, bounded stdout parser, supervisor integration, filesystem-code deletion, and unit tests.
- Modify `rust/crates/pg-foma/src/worker_contract.rs`: strict protocol bump from 8 to 9; leave control-frame limits unchanged.
- Modify `rust/crates/pg-foma/src/bin/worker_test_child.rs`: test-only malformed/partial stdout modes needed by real-process tests.
- Modify `rust/crates/pg-foma/tests/worker_execution_limits_contract.rs`: v8 rejection and real-process partial/trailing payload behavior; delete source-string transport assertions where behavioral coverage replaces them.
- Modify `docs/simplification-rip-list.md`: mark A8 done only after verified implementation and record the raw-frame mechanism.

### Task 1: Replace filesystem tests with failing protocol-v9 acceptance tests

**Files:**
- Modify: `rust/crates/pg-foma/src/worker.rs`
- Modify: `rust/crates/pg-foma/tests/worker_execution_limits_contract.rs`

- [ ] **Step 1: Delete tests for the rejected transport**

Remove these unit tests and their scratch-path helpers from `worker.rs`:

```text
scratch_attempt_id
scratch_artifact_path
selected_artifact_publish_is_atomic_and_described_by_actual_bytes
selected_publish_never_clobbers_or_removes_an_existing_file
selected_output_cleanup_removes_the_attempt_owned_file
selected_artifact_path_is_a_fixed_direct_child_of_canonical_temp
selected_artifact_path_rejects_every_non_generated_attempt_id_shape
selected_parent_read_is_bounded_before_accepting_payload
```

Do not add source searches asserting that filenames disappeared. Replacement tests exercise bytes on the wire.

- [ ] **Step 2: Write the wished-for wire helpers and behavioral tests**

Add a fixture that creates a valid selected result:

```rust
fn selected_success(payload: &[u8]) -> CompileWorkerResult {
    let digest = sha256_hex(payload);
    CompileWorkerResult {
        protocol_version: 9,
        outcome: CompileWorkerOutcome::SelectedSuccess {
            build: CompletedBackendBuildWire {
                requested_strategy: "templated-underlying-tokens".to_string(),
                realized_strategy: "templated-underlying-tokens".to_string(),
                grammar_identity: "grammar".to_string(),
                attempt_id: "attempt".to_string(),
                completion_proof:
                    crate::completed_build::CompletionProofWire::TemplatedFullEmission {
                        uncovered_count: 0,
                        skipped_count: 0,
                    },
                state_count: 1,
                arc_count: 1,
                model_fingerprint: "model".to_string(),
                payload_fingerprint: digest.clone(),
            },
            payload_byte_len: payload.len() as u64,
            payload_sha256: digest,
        },
    }
}

fn framed_result(result: &CompileWorkerResult, payload: Option<&[u8]>) -> Vec<u8> {
    let mut bytes = Vec::new();
    write_frame(&mut bytes, &serde_json::to_vec(result).unwrap()).unwrap();
    if let Some(payload) = payload {
        write_frame(&mut bytes, payload).unwrap();
    }
    bytes
}
```

Add tests calling the wished-for `read_worker_output(Cursor<_>, selected_limit)` API:

```rust
#[test]
fn selected_success_is_one_json_frame_then_one_raw_frame() {
    let payload = b"fst!";
    let parsed = read_worker_output(
        std::io::Cursor::new(framed_result(&selected_success(payload), Some(payload))),
        Some(payload.len() as u64),
    )
    .expect("valid selected output");
    assert!(matches!(
        parsed,
        ParsedWorkerOutput::SelectedCompleted { payload: actual, .. } if actual == payload
    ));
}

#[test]
fn selected_payload_declaration_over_limit_is_rejected_before_body_read() {
    let payload = b"four";
    let mut bytes = framed_result(&selected_success(payload), None);
    bytes.extend_from_slice(&u64::MAX.to_le_bytes());
    let error = read_worker_output(std::io::Cursor::new(bytes), Some(3)).unwrap_err();
    assert!(error.contains("payload") && error.contains("limit"), "{error}");
}
```

Add table-driven cases for missing second frame, truncated body, zero length, header/frame length mismatch, header digest mismatch, build fingerprint mismatch, trailing byte, second frame after failure, and selected success for `None` expected payload limit. Add a generic-result test proving one frame plus EOF still succeeds.

- [ ] **Step 3: Update strict-lockstep expectations**

In `worker_execution_limits_contract.rs`, change the current version assertion to 9 and replace `protocol_seven_request_frames_are_rejected_before_compile` with a v8 request rejection test. Assert the response detail names both 8 and 9.

- [ ] **Step 4: Run the focused test and observe the intended red failure**

Run:

```powershell
& .\rust\tools\pg.ps1 -Mode test -Package pg-foma -Filter selected_ --lib
```

Expected: compilation fails because `read_worker_output`, `ParsedWorkerOutput`, and the v9 selected-success fields do not exist. A passing run or failure caused only by a typo is not an acceptable red gate.

- [ ] **Step 5: Commit tests before implementation**

```powershell
git add -- rust/crates/pg-foma/src/worker.rs rust/crates/pg-foma/tests/worker_execution_limits_contract.rs
git commit -m "test(worker): require raw selected payload frame"
```

### Task 2: Delete child-side filesystem publication and emit two frames

**Files:**
- Modify: `rust/crates/pg-foma/src/worker.rs`
- Modify: `rust/crates/pg-foma/src/worker_contract.rs`

- [ ] **Step 1: Change the strict wire shape**

Set `PROTOCOL_VERSION` to 9. Replace the selected-success variant and delete `SelectedArtifactDescriptor`:

```rust
SelectedSuccess {
    build: CompletedBackendBuildWire,
    payload_byte_len: u64,
    payload_sha256: String,
},
```

Keep request, result, and stderr limits unchanged.

- [ ] **Step 2: Represent child output without a filesystem descriptor**

Add a private child-only carrier:

```rust
struct WorkerChildOutput {
    outcome: CompileWorkerOutcome,
    selected_payload: Option<Vec<u8>>,
}
```

Refactor `compile_selected_from_request` to return `WorkerChildOutput`. After `into_wire_and_payload`, enforce `max_serialized_fst_bytes` exactly once. On success, compute the digest, populate `SelectedSuccess`, and retain the same `Vec<u8>` in `selected_payload`. Every failure has `selected_payload: None`.

- [ ] **Step 3: Write the raw frame only for selected success**

Change the final child write to:

```rust
let result = CompileWorkerResult {
    protocol_version: WORKER_PROTOCOL_VERSION,
    outcome: child_output.outcome,
};
write_result(&mut output, &result)?;
if let Some(payload) = child_output.selected_payload {
    write_frame(&mut output, &payload)?;
}
Ok(())
```

Generic outcomes and selected failures must retain exactly one result frame.

- [ ] **Step 4: Delete child filesystem code and imports**

Delete `SelectedArtifactDescriptor`, `cleanup_selected_output`, `publish_selected_payload`, `write_selected_artifact`, `selected_artifact_path_for_attempt`, `artifact_created`, and the `fs`/`OpenOptions`/`PathBuf` imports used only by them. Do not retain wrappers or deprecated aliases.

- [ ] **Step 5: Commit before running a build**

```powershell
git add -- rust/crates/pg-foma/src/worker.rs rust/crates/pg-foma/src/worker_contract.rs
git commit -m "feat(worker): emit raw selected payload frame"
```

### Task 3: Replace whole-stdout capture with a bounded protocol reader

**Files:**
- Modify: `rust/crates/pg-foma/src/worker.rs`

- [ ] **Step 1: Add allocation-safe frame errors**

Extend `FrameError` with addressability and allocation failures and change `read_frame` to convert `u64` to `usize`, call `try_reserve_exact`, resize only after successful reservation, then `read_exact`. Error messages must name the declared length and applicable limit.

- [ ] **Step 2: Add the parent-only parsed output**

```rust
#[derive(Debug)]
enum ParsedWorkerOutput {
    Completed(CompileWorkerOutcome),
    SelectedCompleted {
        build: CompletedBackendBuildWire,
        payload: Vec<u8>,
    },
}
```

Implement:

```rust
fn read_worker_output<R: Read>(
    mut reader: R,
    selected_payload_limit: Option<u64>,
) -> Result<ParsedWorkerOutput, String>
```

It must read the JSON frame with `max_result_bytes`, enforce protocol 9, then either require EOF or read exactly one raw frame with the selected payload limit. Validate nonzero length, declared/header equality, SHA-256, `build.payload_fingerprint`, and EOF. It returns the final payload vector directly; it never clones an aggregate stdout buffer.

- [ ] **Step 3: Make the stdout thread protocol-aware**

Replace stdout's `spawn_capped_reader` use with a thread that calls `read_worker_output`. Send its result over `std::sync::mpsc`. Keep `spawn_capped_reader` for stderr only. The supervisor poll loop must:

- keep polling the child and wall deadline while stdout blocks;
- kill the child immediately when the reader reports a protocol error before exit;
- wait for and join the reader before accepting success;
- classify a valid parsed result only after a successful exit;
- discard parsed selected bytes after any timeout, crash, or nonzero exit.

- [ ] **Step 4: Expose validated selected completion**

Add:

```rust
WorkerOutcome::SelectedCompleted {
    build: CompletedBackendBuildWire,
    payload: Vec<u8>,
},
```

Map it to an empty success `HealthReport`. `run_selected_compile_worker` must accept only this variant, call `CompletedBackendBuild::from_wire(build, payload)`, and then use `select_completed_build`. Delete artifact derivation, prechecks, reopen/read validation, and cleanup from this function.

- [ ] **Step 5: Delete obsolete parser/capture code**

Delete `parse_result_frame`, `classify_exit`, stdout `Arc<Mutex<Vec<u8>>>`, stdout overflow state, and `read_selected_artifact`. Retain only generic frame helpers and the stderr cap needed by the real contract.

- [ ] **Step 6: Commit before verification**

```powershell
git add -- rust/crates/pg-foma/src/worker.rs
git commit -m "refactor(worker): stream selected payload safely"
```

- [ ] **Step 7: Run the minimum sufficient gate**

Run only:

```powershell
& .\rust\tools\pg.ps1 -Mode test -Package pg-foma -Filter selected_ --lib
```

Expected: all selected unit tests pass. The full workspace, corpus tests, ignored tests, and release build are integration's responsibility and are cut from this task.

### Task 4: Prove partial and malformed subprocess output never completes

**Files:**
- Modify: `rust/crates/pg-foma/src/bin/worker_test_child.rs`
- Modify: `rust/crates/pg-foma/tests/worker_execution_limits_contract.rs`

- [x] **Step 1: Add explicit test-child output modes**

Add test-only environment modes that write: a valid selected-success header without a payload, a truncated raw payload, and valid frames plus a trailing byte. Each mode must use the production framing format; it must not call production compile code after emitting the synthetic stream.

- [x] **Step 2: Add real-process behavioral tests**

Build a selected request through the public serde surface without exposing a test-only production
constructor:

```rust
fn selected_request(payload_limit: u64) -> CompileWorkerRequest {
    let request = CompileWorkerRequest::new("unused.xml", GrammarFormat::Xml);
    let mut json = serde_json::to_value(request).expect("serialize request fixture");
    json["selected"] = serde_json::json!({
        "attempt_id": "attempt-test",
        "route": "templated-underlying-tokens",
        "max_serialized_fst_bytes": payload_limit,
    });
    serde_json::from_value(json).expect("deserialize selected request fixture")
}
```

For each mode, call `run_compile_worker` with this selected request and finite limits. Assert that
the outcome is `ProtocolViolation` or `ChildCrashed`, never `SelectedCompleted`. Keep the existing
wall-time test and add a mode that stalls after the success header; assert the wall limit kills it
and returns no selected completion. Hold `CHILD_ENV_LOCK` for every test that changes a child-mode
environment variable and remove that variable before releasing the guard.

- [x] **Step 3: Remove superseded source-string protocol tests**

Delete any source-inspection assertion whose invariant is now exercised through `run_compile_worker` or `run_worker_child`. Retain source checks only for identity/configuration surfaces that cannot be reached behaviorally in this slice.

- [x] **Step 4: Commit before verification**

```powershell
git add -- rust/crates/pg-foma/src/bin/worker_test_child.rs rust/crates/pg-foma/tests/worker_execution_limits_contract.rs
git commit -m "test(worker): reject incomplete payload streams"
```

- [ ] **Step 5: Run the protocol integration target**

```powershell
& .\rust\tools\pg.ps1 -Mode test -Package pg-foma -TestTarget worker_execution_limits_contract
```

Expected: every test in the target passes, including v8 rejection, timeout, truncated payload, missing payload, and trailing output.

### Task 5: Final deletion audit and handoff gate

**Files:**
- Modify: `docs/simplification-rip-list.md`

- [ ] **Step 1: Prove filesystem transport is absent**

Run:

```powershell
rg -n "SelectedArtifactDescriptor|selected_artifact|artifact_directory|hard_link|pangloss-selected" rust/crates/pg-foma/src/worker.rs rust/crates/pg-foma/src/worker_contract.rs
```

Expected: no production matches. Test fixtures may mention rejected v8 fields only when decoding stale data; remove source-string absence tests.

- [ ] **Step 2: Record A8 as done**

Change A8 to:

```markdown
**DONE** — selected payload uses a separately bounded raw stdout frame; filesystem transport deleted
```

- [ ] **Step 3: Run hygiene and diff checks**

```powershell
& .\rust\tools\comment-hygiene.ps1 -List
git diff --check
git diff --stat 1ab11ef9..HEAD
```

Expected: comment hygiene clean, no whitespace errors, and the replacement slice is net-negative.

- [ ] **Step 4: Commit the charter status**

```powershell
git add -- docs/simplification-rip-list.md
git commit -m "docs(cleanup): finish raw payload transport"
```

- [ ] **Step 5: Obtain independent Luna review**

Give a fresh read-only Luna reviewer the exact implementation range and the approved design. Require prioritized findings and GO/NO-GO. The primary must inspect every finding and every changed line; a green test run is not sufficient to override a correctness finding.

- [ ] **Step 6: Run authoritative merged-tip gates**

Measure physical memory, commit headroom, CPU load, and active Cargo/procgov trees. With safe headroom and only one managed build active, run:

```powershell
& .\rust\tools\pg.ps1 -Mode test -Package pg-foma -Filter selected_ --lib
& .\rust\tools\pg.ps1 -Mode test -Package pg-foma -TestTarget worker_execution_limits_contract
```

Capture complete output. If `pg.ps1` lingers only after a complete nextest summary, stop the wrapper and report the recorded test result separately from the wrapper exit.
