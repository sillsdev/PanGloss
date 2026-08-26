# Selected worker raw-payload transport

## Purpose

Delete the selected-build filesystem transport and replace it with one strict, bounded worker
protocol. The worker already communicates over stdin/stdout and the parent ultimately needs the
serialized FST as `Vec<u8>`; a second raw stdout frame is therefore the smallest honest transport.

This is one implementation slice of [`docs/simplification-rip-list.md`](../../simplification-rip-list.md).
It does not implement process-tree containment, remove internal compile budgets, or change backend
selection. Those remain separate cleanup slices.

## Rejected designs

### Direct or directory-based temporary files

Rejected. A path precheck cannot establish later ownership. Another same-user process can create,
unlink, or replace the path between checking, opening, reading, and cleanup. Fixing that requires
platform-specific open-handle transfer and deletion semantics, preserving a filesystem subsystem
whose only purpose is moving bytes already available on stdout.

Delete the path, directory, temporary-file, hard-link, canonicalization, ownership-probing, and
cleanup machinery. Do not retain it behind an abstraction or compatibility path.

### JSON or base64 payloads

Rejected. Base64 inflates a payload by roughly one third and requires large encoded and decoded
buffers. A JSON byte array is worse. Both conflate the small control-frame limit with the separately
configurable serialized-model limit.

## Protocol v9

The protocol remains strict lockstep. Version 8 is rejected; there is no v8 parser, alias, default,
or migration shim.

The parent writes the existing single JSON request frame. Output depends on the terminal outcome:

1. An ordinary worker outcome is one bounded JSON result frame followed by EOF.
2. A selected-build failure is one bounded JSON result frame followed by EOF.
3. A selected-build success is:
   - one bounded JSON result frame containing `SelectedSuccess { build, payload_byte_len,
     payload_sha256 }`;
   - one length-prefixed raw payload frame;
   - EOF.

The JSON result limit remains independent of the serialized-model limit. The raw frame is not JSON,
base64, a filesystem artifact, or part of the generic control-frame allowance.

The selected-success JSON frame is the child's commit point: construction and serialization are
complete and the child has declared the exact payload it will send. It is not parent acceptance.
The parent exposes a completed build only after receiving and validating the entire raw frame and a
clean worker exit.

## Child behavior

The child constructs and serializes the selected backend once. It enforces
`max_serialized_fst_bytes` once against the final serialized bytes.

- If the payload is over the limit, emit only `SelectedExecutionLimitExceeded` and no raw frame.
- If result metadata cannot be serialized within the JSON-result limit, emit a typed failure and no
  raw frame.
- On success, emit the JSON result, then the raw frame, then close stdout.
- Never create an artifact path, directory, temporary file, sidecar, or ownership token.

The existing `CompletedBackendBuildWire::payload_fingerprint` remains authoritative build evidence.
The explicit payload length and digest in the success header describe the transport. Both digests
must agree with the received bytes.

## Parent reader and acceptance

Replace whole-stdout capture with a protocol-aware reader. Do not increase the existing aggregate
capture cap to 1 GB.

The reader must:

1. Read the first length prefix and reject values above `max_result_bytes` before allocation.
2. Convert lengths to `usize` with checked conversion and reserve memory fallibly.
3. Decode one protocol-v9 JSON result.
4. Require immediate EOF for ordinary outcomes and selected failures.
5. For selected success, require that the request was selected, then read a second length prefix.
6. Reject zero, over-limit, or header-mismatched payload lengths before payload allocation.
7. Read exactly the declared bytes into the final payload `Vec<u8>` without cloning an aggregate
   stdout buffer.
8. Verify the raw bytes against both `payload_sha256` and `build.payload_fingerprint`.
9. Require immediate EOF; any trailing byte is a protocol violation.

The supervisor continues enforcing wall time while the reader blocks. An early framing error must
notify the supervisor so it can terminate the worker; the reader must not stop consuming while a
live child remains blocked on a full pipe. The stderr cap remains separate.

`WorkerOutcome` gains a parent-only selected-success variant carrying the validated build wire and
payload. Generic callers continue receiving the existing completed outcome shape.

## Failure semantics

A missing second frame, truncated payload, length mismatch, digest mismatch, fingerprint mismatch,
trailing output, timeout, crash, output flood, or nonzero exit produces no completed build. Received
bytes are dropped in memory. There is no intermediate artifact to clean up and no recovery or retry
backend.

The three execution limits remain operational containment:

- final serialized payload size applies to the raw payload frame;
- wall time includes construction, serialization, and pipe transfer;
- process-tree committed-memory enforcement remains a later platform-containment slice.

## Test-first implementation

Before production edits, delete or replace tests that pin filesystem transport. Add behavioral tests
and verify they fail for the missing v9 behavior:

- ordinary outcomes and selected failures are one JSON frame plus EOF;
- selected success is one JSON header plus one raw frame plus EOF;
- exact-limit payload succeeds and one-byte-over emits only the typed limit outcome;
- oversized result and raw-frame declarations are rejected before allocation;
- missing, truncated, zero-length, mismatched-length, digest-mismatched, fingerprint-mismatched, and
  trailing-byte payloads are rejected;
- a raw frame after a non-success outcome is rejected;
- selected success for a non-selected request is rejected;
- timeout or crash during transfer yields no selected completion;
- protocol 8 requests and results are rejected;
- the ordinary worker and stderr-limit behavior remain unchanged.

Delete filesystem-oriented tests rather than translating them into source-string assertions. Tests
must exercise the wire behavior through readers/writers or the real worker process.

## Deletion boundary

Delete these concepts from production and tests:

- `SelectedArtifactDescriptor`;
- selected artifact path derivation and attempt-ID filename validation;
- temp-root canonicalization and path containment checks;
- exclusive artifact-file creation, syncing, hard-linking, and renaming;
- child and parent artifact cleanup;
- parent artifact metadata/reopen/read validation;
- filesystem destination fields and compatibility tests;
- whole-stdout cloning for selected payloads.

Retain only framing, bounded allocation, digest verification, and typed outcome handling required by
the actual worker contract. The expected implementation is net-negative in lines and removes the
entire filesystem-transport subsystem.

## Verification

The focused implementation gate is:

```powershell
& .\rust\tools\pg.ps1 -Mode test -Package pg-foma -Filter selected_ --lib
```

The protocol integration gate is:

```powershell
& .\rust\tools\pg.ps1 -Mode test -Package pg-foma -TestTarget worker_execution_limits_contract
```

Before accepting the slice, inspect the exact diff, run comment hygiene and `git diff --check`, and
obtain an independent Luna review of the final commit. The broader cleanup proceeds only after this
transport slice is green and the rejected filesystem code is absent.
