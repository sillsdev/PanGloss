# Worker process-tree containment implementation plan

Design: `docs/superpowers/specs/2026-08-26-worker-process-tree-containment-design.md`

## Task 1: Red contract tests and fixture behavior

Files:

- `rust/crates/pg-foma/src/bin/worker_test_child.rs`
- `rust/crates/pg-foma/src/worker.rs`
- `rust/crates/pg-foma/tests/worker_execution_limits_contract.rs`
- `rust/crates/pg-foma/tests/backend_selection_contract.rs`

- [x] Add descendant modes: idle holders for both stdout and stderr, delayed sentinel writer, and
      a bounded allocator that touches and retains every page.
- [x] Add executable red tests for descendant timeout cleanup, direct-child crash cleanup,
      successful selected payload, and no artifact on every observable failure.
- [x] Rewrite `ChildCrashed` docs/unit expectations so a crash is not classified as
      `HostContainmentFired` without OS evidence; audit the backend-selection contract for the same
      assumption.
- [x] Add red proof for a descendant surviving its direct parent's crash. Add the deterministic
      containment-unavailable, cleanup-deadline, and late-event proofs in Task 5 against the real
      helper API through a narrow test-only fake; do not create an orphan production abstraction.
- [ ] Replace the source-shape-only execution-control test with behavioral containment assertions.
- [x] Tighten malformed/truncated/trailing selected-output cases to `ProtocolViolation`; they may
      not fall through to `ChildCrashed` or a containment classification.
- [x] Run the narrow platform test and record the intended red failures before implementation.
- [x] Commit tests separately (`40897d45`).

Rewrite/delete ledger for this task:

- rewrite `wall_limit_kills_a_slow_worker_process` as a descendant-tree test;
- rewrite `malformed_selected_payload_processes_never_complete` to require protocol failure;
- rewrite `worker.rs::host_containment_is_not_a_grammar_verdict` and extend
  `spawn_and_protocol_failures_are_process_faults_not_host_limits` so bare child crashes are
  process faults;
- delete `supervisor_accepts_execution_limits_as_its_only_execution_control_input` only after its
  behavioral replacements are red;
- replace and then delete the standalone `PANGLOSS_WORKER_TEST_SLEEP_MS` and
  `PANGLOSS_WORKER_TEST_CRASH` fixture branches.

Keep the default/configurability, protocol-version, request/payload-limit, exact-payload,
wire-versus-execution-limit, and completed-build-identity tests. Keep
`backend_selection_contract::readiness_labels_stay_selectable_while_containment_and_representability_do_not`;
it protects an axis distinction rather than the old spawn seam.

## Task 2: Lock the typed outcomes before native implementation

Files:

- `rust/crates/pg-foma/src/worker.rs`
- `rust/crates/pg-foma/src/health.rs`
- `rust/crates/pg-foma/src/health_evaluator.rs`
- `rust/crates/pg-pack/src/manifest.rs`
- `rust/crates/pg-pack/src/format.rs`

- [x] Add `MemoryLimitKilled`, `ContainmentUnavailable`, and `ContainmentFailed`; retain configured
      limit, observed peak, and native trigger evidence on the worker outcome. Add the truthful
      worker-tree peak-memory health metric and advance only the affected health/pack schemas to v5;
      reject stale standalone and embedded health versions.
- [x] Keep `ChildCrashed` distinct absent an OS-proven
      containment event.
- [x] Do not change backend selection, compile refusal caps, apply budgets, or transport framing.
- [x] Preserve `pg-foma`'s `forbid(unsafe_code)` and place no native FFI in that crate.
- [x] Commit the typed contract separately (`b330892f`).

Rejected implementation (`2026-08-26`): do not recreate an uncalled generic
`pg-foma::worker_containment` module, a second `LifecycleOutcome`, or containment-owned `Vec<u8>`
payload staging. The reviewed 375-line attempt lost native evidence, omitted bounded pipe/reap
cleanup, and could not express the existing `WorkerOutcome`; it was deleted before commit. The
concrete safe process API belongs in `pg-worker-containment`, while `worker.rs` remains the sole
owner of protocol parsing, staged build metadata, failure precedence, and terminal `WorkerOutcome`.

## Task 3: Windows Job Object adapter

Files:

- workspace manifests/lockfile and new narrowly scoped `pg-worker-containment` crate
- Windows native adapter in `pg-worker-containment`
- safe `pg-foma` containment integration
- Windows fixture/integration tests

- [x] Add a target-scoped Windows API dependency to `pg-worker-containment` with only the Win32
      Foundation, Globalization, Security, IO, JobObjects, Pipes, and Threading families.
- [x] Set `deny(unsafe_op_in_unsafe_fn)` in the helper crate, confine unsafe blocks to target-specific
      modules, and require a `SAFETY:` justification at every block.
- [x] Define the concrete safe owned process API here: contained launch with owned stdio, direct
      child status with exit diagnostics, native memory-event evidence, bounded terminate/drain/
      child-reap operations, final evidence capture, and peak-memory query. Do not add another
      worker outcome or parse/stage protocol payloads in this crate.
- [x] Create/configure an unnamed job before launch, with an exact hard cap and a separately recorded
      guaranteed-notification threshold below it.
- [x] Launch through `CreateProcessW` + `STARTUPINFOEXW` with atomic job-list assignment and an
      explicit inherited-handle list.
- [x] Preserve Windows quoting, Unicode/space-containing paths, environment overrides, current
      directory, pipe behavior, and cleanup of attribute lists and temporary handles.
- [x] Enforce job memory and kill-on-close; implement terminate/wait/peak diagnostics.
- [x] Prove success, descendant termination, aggregate memory kill, bounded pipe EOF, and managed-wrapper
      nested-job behavior.
- [x] Commit and run only the Windows containment target through `rust/tools/pg.ps1`
      (`9c7330c2`): 14 containment tests and all 665 `pg-foma` library tests passed.

## Task 4: Linux cgroup-v2 adapter

Detailed correction plan:
`docs/superpowers/plans/2026-08-26-linux-cgroup-delegated-root-correction.md`

Files:

- target-scoped Linux dependencies in `pg-worker-containment`
- Linux native adapter in `pg-worker-containment`
- safe `pg-foma` containment integration
- Linux fixture/integration tests

- [x] Require `PANGLOSS_CGROUP_DELEGATED_ROOT` as an explicit canonical hierarchy path. Map it
      through the most-specific visible cgroup2 mount; require the supervisor's current membership
      to be a strict descendant and the configured root's own `cgroup.procs` to be empty. Never walk
      upward, infer authority from a leaf name, or enable controllers on the host's behalf.
- [x] Require `memory` in the configured root's `cgroup.subtree_control`; create each generated
      per-attempt worker directly below that root as a sibling of the supervisor leaf, then configure
      memory, OOM-group, and swap boundaries.
- [x] Implement race-free placement with `clone3(CLONE_INTO_CGROUP | CLONE_PIDFD)` as the only
      admitted route in this checkpoint. Prebuild all child inputs; the child may perform only raw
      no-allocation setup, `execve`, or `_exit`. Fail closed when unavailable; forbid ordinary
      spawn-then-move. Any pre-exec-handshake fallback is a separately reviewed future change.
- [x] Implement one complete cleanup sequence for orderly errors, unwinds, and `Drop`: attempt
      `cgroup.kill`, bounded populated-zero wait, direct-child reap, memory event/peak capture, and
      directory removal even when an earlier cleanup operation fails. Cleanup failure outranks the
      initiating failure while retaining its diagnostic. Use the caller deadline for owned
      operations and a fixed five-second emergency grace for launch errors and `Drop`.
- [x] Fail closed when delegation/controller/placement is unavailable.
- [x] Make direct-child termination signal-aware; only a real numeric exit code zero is success.
- [x] Read back page-rounded `memory.max`, require it not exceed the requested cap, require the
      `memory.events` `max` and `oom_kill` keys to exist and parse, and require both hierarchical
      deltas before emitting native memory evidence.
- [x] Expose only safe owned containment operations to `pg-foma`; native handles and unsafe launch
      machinery remain private to the helper crate.
- [ ] Prove success, descendant memory kill, timeout with inherited pipes, fork-race cleanup, and
      required-capability CI behavior on Linux.
- [x] Add the source-level Linux execution path to `rust/tools/pg.ps1`; it proves a finite effective
      host-managed ancestral cgroup cap before Cargo starts and has no bare-Cargo fallback. Committed
      as `694de90f` after the separate Linux contract commits. This check means source-complete only:
      host-service interruption cleanup and real Linux execution remain unproved.
- [ ] Commit and run the Linux containment target on a deliberately delegated Linux runner with
      `PANGLOSS_CGROUP_TEST_REQUIRED=1`. Generic `ubuntu-latest` workspace tests do not establish
      delegated cgroup authority. External dependency: a runner launched in a writable delegated
      cgroup with the memory controller enabled, `cgroup.kill`, and permitted `clone3`.
- [ ] Do not claim the Linux runtime gate from Windows or from a capability-skipped Linux run.

Source status at `694de90f`: the native containment adapter is compile-checked for the Linux target
from Windows, and the managed wrapper has a reviewed 25-case cross-platform contract. Neither is a
Linux runtime claim. Required-capability CI, a real delegated-host run, and host-service interruption
proof remain the gate before supervisor integration or deletion of the old spawn/kill loop.
The final-tip Windows Cargo run reported 5 unit and 11 Windows containment tests passing, but the
outer wrapper required interruption after procgov failed to exit; record the test results as green
and the wrapper lifecycle gate as open. The required Linux CI job cannot honestly be landed as proof
until its labeled delegated runner and service contract exist.

## Task 5: Supervisor integration and deletion audit

Files:

- `rust/crates/pg-foma/src/worker.rs`
- `rust/crates/pg-foma/src/lib.rs`
- `pg-worker-containment` safe API
- containment tests
- cleanup charter

- [ ] Route `run_compile_worker` exclusively through `ContainedWorkerProcess`.
- [ ] Add a narrow test-only adapter over the concrete operations. Script setup-unavailable before
      launch, cleanup-deadline failure, early and late native memory events, failed direct-child
      status, reader closure, bounded reap, and failure precedence. The production path must use the
      same state machine; tests may not exercise an isolated duplicate lifecycle.
- [ ] Poll containment, wall time, protocol output, and stderr without reader deadlock.
- [ ] Latch native memory evidence once observed; a later clean poll can never erase it.
- [ ] Keep `WorkerOutcome` as the sole terminal result. `worker.rs`, not containment, owns parsed
      output and selected-build metadata, and every non-success discards them.
- [ ] Bound termination, tree drain, child reap, and reader joins; close parent pipe endpoints before
      joining readers after failed termination. Cleanup failure takes precedence but lower-priority
      evidence remains available in diagnostics.
- [ ] Accept completion only after clean child exit, exact EOF, empty tree, and a final successful
      containment poll; prove that a late containment event discards an otherwise valid payload.
- [ ] Only after both platform adapters pass, delete the Windows and Linux direct spawn/kill
      branches, direct-child containment helpers, and caller-owned-process-tree documentation.
- [ ] Grep the artifact-worker route for unmanaged `Command::spawn`, `Child::kill`, RSS enforcement,
      named jobs, named envelopes, and silent fallback; run a repo-scoped stale-documentation grep,
      including `lib.rs`, and classify unrelated process spawns instead of treating them as Stage 2
      violations.
- [ ] Obtain independent Luna spec and code-quality reviews; fix findings test-first.
- [ ] Primary runs comment hygiene, diff checks, platform-focused gates, and the existing raw
      transport gates on the exact merged tip.
- [ ] Mark only the artifact-worker containment sub-slice verified after Windows and Linux evidence
      exists. Global Stage 2 remains partial until Stage 3 migrates every production build route.

Exact shared-loop deletion after both platform gates:

- remove `Command`/`Stdio` imports used solely by `run_compile_worker`;
- replace its direct `Command::new(...).spawn()`, direct pipe extraction, `Child::try_wait`, repeated
  `Child::kill`/`wait` pairs, and unbounded joins with the containment seam;
- retain request prevalidation, protocol-aware stdout decoding, capped-stderr semantics,
  `run_worker_child`, raw framing, and the public supervisor entrypoint;
- replace stale standard-library/caller-owned-containment claims in `worker.rs` and `lib.rs`;
- do not remove `sysinfo`: runtime analysis and recipe optimization still use it.

Explicitly exclude unrelated subprocesses from this deletion: recipe-optimizer supervision, Git
metadata commands, WSL oracle tests, integration-test launchers, thread spawns, and the hidden
worker child itself. Record each exclusion in the residue audit rather than deleting by grep alone.

## Task 6: Handoff to explicit backend/routing cleanup

- [ ] Inventory remaining direct in-process compilation routes without modifying them here.
- [ ] Hand the verified containment seam to Stage 3, which removes preference/fallback and migrates
      Pack first, followed by native CLI artifact-producing routes.
- [ ] Keep WASM, witnessed coverage, recipe optimization, and public convenience-constructor scope
      explicitly unresolved until their charter decisions are made.
- [ ] Rewrite Pack's two watchdog tests to classify explicit typed outcomes, then delete its
      `--watchdog` flag/state, health-only branch, `worker_containment_failed` bookkeeping,
      `worker_containment_fired`, `run_fst_health_under_watchdog`, placeholder explanations, and the
      stale hidden-child comment only as part of Stage 3 route migration—not during adapter work.
