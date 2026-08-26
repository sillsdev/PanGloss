# Worker process-tree containment implementation plan

Design: `docs/superpowers/specs/2026-08-26-worker-process-tree-containment-design.md`

## Task 1: Red contract tests and fixture behavior

Files:

- `rust/crates/pg-foma/src/bin/worker_test_child.rs`
- `rust/crates/pg-foma/src/worker.rs`
- `rust/crates/pg-foma/tests/worker_execution_limits_contract.rs`
- `rust/crates/pg-foma/tests/backend_selection_contract.rs`

- [ ] Add descendant modes: idle holders for both stdout and stderr, delayed sentinel writer, and
      a bounded allocator that touches and retains every page.
- [ ] Add tests for containment unavailable, descendant timeout cleanup, descendant memory kill,
      successful selected payload, and no artifact on every failure.
- [ ] Rewrite `ChildCrashed` docs/unit expectations so a crash is not classified as
      `HostContainmentFired` without OS evidence; audit the backend-selection contract for the same
      assumption.
- [ ] Add red proofs for a late event after clean direct-child exit, a descendant surviving its
      direct parent's crash, and cleanup-deadline failure before production changes.
- [ ] Replace the source-shape-only execution-control test with behavioral containment assertions.
- [ ] Tighten malformed/truncated/trailing selected-output cases to `ProtocolViolation`; they may
      not fall through to `ChildCrashed` or a containment classification.
- [ ] Run the narrow platform test and record the intended red failures before implementation.
- [ ] Commit tests separately.

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

## Task 2: Define the internal containment seam and outcomes

Files:

- `rust/crates/pg-foma/src/worker.rs`
- new `rust/crates/pg-foma/src/worker_containment.rs` and platform submodules as needed

- [ ] Add `ContainedWorkerProcess` with spawn, poll, terminate-tree, wait-empty, and peak-memory
      operations; keep platform types private.
- [ ] Add `MemoryLimitKilled`, `ContainmentUnavailable`, and `ContainmentFailed` outcomes and health
      mappings with configured limit, observed peak, and provenance.
- [ ] Make every non-success discard parsed selected payload.
- [ ] Bound termination, tree drain, child reap, and reader joins; close parent pipe endpoints before
      joining readers after failed termination.
- [ ] Encode deterministic failure precedence and keep `ChildCrashed` distinct absent an OS-proven
      containment event.
- [ ] Do not change backend selection, compile refusal caps, apply budgets, or transport framing.
- [ ] Commit before platform implementation.

## Task 3: Windows Job Object adapter

Files:

- workspace and `pg-foma` Cargo manifests/lockfile
- Windows containment module
- Windows fixture/integration tests

- [ ] Add a direct, target-scoped Windows API dependency with only Foundation, Security,
      JobObjects, and Threading features.
- [ ] Create/configure an unnamed job before launch.
- [ ] Launch through `CreateProcessW` + `STARTUPINFOEXW` with atomic job-list assignment and an
      explicit inherited-handle list.
- [ ] Preserve Windows quoting, Unicode/space-containing paths, environment overrides, current
      directory, pipe behavior, and cleanup of attribute lists and temporary handles.
- [ ] Enforce job memory and kill-on-close; implement terminate/wait/peak diagnostics.
- [ ] Prove success, descendant timeout cleanup, memory kill, pipe closure, and managed-wrapper
      nested-job behavior.
- [ ] Commit and run only the Windows containment target through `rust/tools/pg.ps1`.

## Task 4: Linux cgroup-v2 adapter

Files:

- target-scoped Linux dependencies
- Linux containment module
- Linux fixture/integration tests

- [ ] Discover cgroup2 mount/current membership and validate an explicitly delegated parent.
- [ ] Require `memory` in `cgroup.subtree_control`; create a generated per-attempt child cgroup and
      configure memory, OOM-group, and swap boundaries.
- [ ] Implement race-free placement with `clone3(CLONE_INTO_CGROUP)` or a blocked pre-exec
      handshake whose child side performs no allocation/fork/setup and cannot escape on parent
      death; forbid ordinary spawn-then-move.
- [ ] Implement orderly error/unwind cleanup, `cgroup.kill`, bounded populated-zero wait, memory
      event/peak capture, and bounded directory cleanup; do not promise abrupt-supervisor-death
      cleanup on Linux without a separate external lifecycle mechanism and proof.
- [ ] Fail closed when delegation/controller/placement is unavailable.
- [ ] Prove success, descendant memory kill, timeout with inherited pipes, fork-race cleanup, and
      required-capability CI behavior on Linux.
- [ ] Commit and run the Linux containment target in Linux CI; do not claim this gate from Windows.

## Task 5: Supervisor integration and deletion audit

Files:

- `rust/crates/pg-foma/src/worker.rs`
- `rust/crates/pg-foma/src/lib.rs`
- containment modules
- containment tests
- cleanup charter

- [ ] Route `run_compile_worker` exclusively through `ContainedWorkerProcess`.
- [ ] Poll containment, wall time, protocol output, and stderr without reader deadlock.
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
