# Worker process-tree containment design

Status: approved; Windows verified, corrected Linux topology approved, Linux implementation pending.

## Goal

This stage installs and proves the OS-enforced process-tree boundary used by the worker compile
route. A build attempt has three caller-configurable limits: final serialized payload bytes,
aggregate worker-tree memory charge, and wall time. The existing fixed stderr bound is a separate
transport-safety stop. Failure to establish containment fails closed and produces no completed
artifact.

This replaces direct-child `Command::spawn`/`Child::kill` as the production containment seam. It
does not add a named envelope, retry, fallback, automatic backend choice, or a second scheduler.

## Portable contract

`ExecutionLimits` remains the only caller-facing limit configuration. The memory field is defined
portably as the aggregate OS-enforced memory charge for the worker containment tree:

- Windows: Job Object committed-memory accounting and `JOB_OBJECT_LIMIT_JOB_MEMORY`.
- Linux: cgroup-v2 hierarchical memory charge with `memory.max`; set `memory.swap.max=0` when the
  delegated controller exposes it, so the configured boundary cannot be escaped through swap.

The worker outcome records the configured limit plus intrinsic platform-native trigger evidence and
peak memory charge. Its evidence type makes Windows/Linux mismatches unconstructible. The two
kernels need not count the same categories byte-for-byte; both enforce one finite aggregate
boundary over the complete worker tree. Health serialization gains one truthful versioned
worker-tree peak-memory metric now because no existing metric can carry the observation without
lying; native trigger evidence remains on the typed worker outcome. This narrowly advances the
required health/pack version bump from Stage 8, not the broader compatibility sweep. Existing
internal compile caps remain until Stage 4; this stage does not legitimize them or remove them
early.

The concrete safe API in `pg-worker-containment` owns:

```text
ContainedWorkerProcess
  spawn(executable, args, stdio, execution limits)
  take_stdio()
  try_wait_direct_child(deadline) -> exit status with diagnostics
  poll_containment(deadline) -> native memory-limit evidence or clean state
  terminate_tree(deadline)
  wait_tree_empty(deadline)
  reap_direct_child(deadline)
  final_evidence_and_peak(deadline)
```

The helper owns all native handles and returns intrinsic Windows/Linux evidence without reducing it
to a Boolean event. Every fallible lifecycle method takes the caller's absolute cleanup deadline;
when one returns an operational error it must attempt the complete bounded cleanup sequence with
that same deadline before returning. The fixed emergency deadline remains internal to launch
failure guards and `Drop` only; it is never substituted for a caller deadline or exposed as a
named execution envelope. `pg-foma` keeps its crate-wide `forbid(unsafe_code)`, owns protocol parsing and
selected-build metadata, and produces the existing `WorkerOutcome` as the only terminal outcome.
It latches the first native memory event so a later clean poll cannot erase it. Do not add a second
`LifecycleOutcome`, stage `Vec<u8>` inside containment, or create an uncalled generic production
trait solely for fake tests. A narrow test-only adapter may script the concrete operations, but it
must drive the same supervisor state machine used by production. Do not leak Job/cgroup handles
through either crate's public API.

Containment is established before worker code can run. There is no successful unmanaged fallback.
The supervisor accepts a selected artifact only after the direct child exits successfully, stdout
parsing reaches exact EOF, the containment tree is empty, and a final containment poll reports no
memory-limit or containment error. A late event after direct-child exit discards the payload. Every
failure path bounds tree termination, tree drain, direct-child reap, parent-pipe closure, and reader
joins; cleanup failure has precedence without destroying lower-priority diagnostic evidence.

## Typed outcomes

Add distinct outcomes for:

- `MemoryLimitKilled { limit_bytes, evidence }`, where intrinsic evidence records the native
  limit-trigger proof and peak memory charge;
- `ContainmentUnavailable { detail }`;
- `ContainmentFailed { detail }`.

Keep wall timeout, payload-size refusal, stderr overflow, protocol violation, spawn failure, and
child crash distinct. Do not infer a memory kill merely from an abnormal exit; `ChildCrashed` is a
process fault unless OS containment evidence says otherwise. Every non-success discards any parsed
payload. Resolve simultaneous failures deterministically: containment setup/poll/cleanup failure,
then OS-proven memory kill, wall timeout, stderr overflow, protocol failure, and child crash;
preserve lower-priority evidence as diagnostics.

## Windows adapter

Support Windows 10 or newer. Create an unnamed Job Object and configure it before launch with:

- `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`;
- `JOB_OBJECT_LIMIT_JOB_MEMORY`;
- `JobMemoryLimit = ExecutionLimits.max_committed_memory_bytes()`.

The configured value is the exact hard ceiling. Also configure
`JobObjectNotificationLimitInformation2` below that ceiling with headroom equal to the smaller of
64 MiB or half the configured cap, consume
`JOB_OBJECT_MSG_NOTIFICATION_LIMIT`, and query `JobObjectLimitViolationInformation2` before
constructing memory-limit evidence. Record both the notification threshold and observed job-memory
peak. Ordinary job-limit completion messages are not guaranteed and therefore never prove a clean
final poll by their absence; see [Microsoft's completion-port contract](https://learn.microsoft.com/en-us/windows/win32/api/winnt/ns-winnt-jobobject_associate_completion_port).

Launch with `CreateProcessW` and `STARTUPINFOEXW`, using `PROC_THREAD_ATTRIBUTE_JOB_LIST` so the
worker belongs to the job before its first instruction. Use
`PROC_THREAD_ATTRIBUTE_HANDLE_LIST` to inherit only the child stdin/stdout/stderr handles. Do not
use ordinary `Command::spawn` followed by `AssignProcessToJobObject`; that permits allocation and
forking before assignment. Do not use named jobs or breakaway flags.

On any supervisor failure, call `TerminateJobObject`, wait for the direct child and job to drain,
close the supervisor's parent pipe endpoints, then join pipe readers within a bounded cleanup
deadline. Hold the final job handle until status and diagnostics are captured. Closing the job
handle kills the tree even if the supervisor exits abruptly. Preserve ordinary command semantics for Windows quoting,
Unicode paths, spaces, environment inheritance/overrides, current directory, and stdin/stdout/
stderr plumbing; release attribute-list and temporary handle resources on every path. Nested jobs
are supported; an incompatible host job yields `ContainmentUnavailable`, never an unmanaged build.

## Linux adapter

### Delegation-topology correction (ratified 2026-08-26)

The original draft treated the supervisor's current cgroup as the delegated parent. That topology
is rejected: [cgroup v2's no-internal-process rule](https://docs.kernel.org/admin-guide/cgroup-v2.html#no-internal-process-constraint)
forbids a normal non-root domain cgroup from both containing the supervisor and distributing the
memory controller to children. An empty explicit delegation root plus a separate supervisor leaf
is therefore a correctness requirement, not an optional deployment preference.

Two alternatives remain rejected for this stage. The adapter does not infer authority by moving one
level above a specially named supervisor leaf, because that turns a naming convention into ambient
authority. It also does not require an inherited directory descriptor, because the explicit
hierarchy path can be validated against kernel-reported membership and keeps the `pg.ps1`/runner
contract substantially smaller. Revisit the descriptor design only if path validation proves
insufficient on a real delegated host.

Use cgroup v2 only. The host supplies `PANGLOSS_CGROUP_DELEGATED_ROOT` as the absolute hierarchy
path reported by `/proc/*/cgroup`, not as a filesystem path. It identifies one empty cgroup that the
host has explicitly delegated to PanGloss. There is no inferred default. Missing, relative,
non-canonical, inaccessible, or ambiguous configuration yields `ContainmentUnavailable`.

The host topology is fixed:

```text
configured delegated root (empty; memory enabled for children)
├── supervisor leaf (PanGloss, runner, and their non-worker processes)
└── .pangloss-worker-<generated attempt id> (one contained build tree)
```

The adapter discovers cgroup2 mounts from `/proc/self/mountinfo` and current membership from
`/proc/self/cgroup`, then maps the configured hierarchy path through the most-specific visible
cgroup2 mount whose mount root contains it. It validates that the supervisor's current membership
is a strict descendant of the configured root and that the configured root's own `cgroup.procs` is
empty. It never searches ancestors for writable authority, derives authority by stripping the
current path, or enables a controller. A service manager such as systemd must create the empty
delegated root, place the supervisor in a leaf (`DelegateSubgroup=` is one valid mechanism), enable
the memory controller for the root's children, and pass the exact root path.

Create each generated worker cgroup directly beneath the configured root, as a sibling of the
supervisor leaf. The generated name is an implementation detail, not caller identity or an
execution envelope. Require `memory` in the root's `cgroup.subtree_control` and writable
`memory.max`, `memory.oom.group`, `cgroup.procs`, `cgroup.events`, `memory.events`, and `cgroup.kill`
surfaces on the worker cgroup. This authority locator is host infrastructure, not a caller-selected
build envelope, semantic build configuration, resource limit, backend-selection input, or artifact
identity field.

Set `memory.max`, `memory.oom.group=1`, and `memory.swap.max=0` where available. Place the child in
the cgroup before it can execute compile work. This checkpoint uses
`clone3(CLONE_INTO_CGROUP | CLONE_PIDFD)` as its sole launch route and fails closed when the kernel,
architecture, seccomp policy, or delegation does not support it. Do not add a spawn-then-move
fallback. A future gated pre-exec fallback requires a separate design and parent-death proof before
it can be admitted. Prebuild argv, environment, paths, and file descriptors before `clone3`; the
child performs only raw no-allocation setup, `execve`, or `_exit`.

Read back `memory.max` after configuration. It may be page-rounded, so require the effective value
to be no greater than the requested cap and record it in Linux native evidence. Baseline and final
reads use hierarchical `memory.events`, not `memory.events.local`; construct memory-limit evidence
only when both `max` and `oom_kill` increased. `memory.peak` alone and an abnormal child exit are not
proof. An ancestor-cgroup kill without local evidence remains a process failure.

On failure, write `1` to `cgroup.kill`, wait for `cgroup.events` to report `populated 0`, reap the
direct child, capture `memory.peak` and `memory.events`, then remove the cgroup within the bounded
cleanup deadline. Orderly launch errors, later supervisor errors, and Rust unwinds must attempt the
complete sequence. Cleanup continues after an individual cleanup operation fails; a cleanup failure
takes precedence over the initiating error while retaining that lower-priority diagnostic. `Drop`
performs the same bounded best-effort sequence for an owned live process rather than only sending
`cgroup.kill` when no explicit cleanup attempt has already claimed that process. An explicit cleanup
attempt continues through every cleanup operation once under the caller's deadline, even after an
individual operation fails; `Drop` does not silently retry that sequence under a second five-second
deadline. On Windows, closing the owned job handle retains the kernel's kill-on-close backstop after
such a failed explicit attempt. Launch-error and otherwise-unclaimed `Drop` cleanup use a fixed
five-second emergency grace; this is an internal lifecycle bound, not a fourth configurable
execution limit. Operations reached through the
owned API continue to use the supervisor-supplied absolute cleanup deadline. Abrupt supervisor
process death is outside this stage's portable guarantee because cgroup v2 has no job-handle-close
equivalent. Missing delegation, controller support, `cgroup.kill`, permission, or race-free
placement yields `ContainmentUnavailable`. Process groups, RSS polling, `RLIMIT_AS`, and
`RLIMIT_RSS` may aid cleanup but do not satisfy aggregate memory containment and cannot permit
publication.

Direct-child status is platform-neutral: clean numeric exits and signal termination are distinct.
Only exit code zero is success; SIGKILL/SIGSEGV may never be converted into a fabricated exit code.

## Stage boundary

This slice replaces the artifact worker's spawn/kill seam and proves both platform adapters. It
does not yet delete automatic backend preference or migrate every CLI route; that is the
immediately following explicit-backend/routing stage. Until migration finishes, direct in-process
compilation remains a tracked violation and cannot be described as production compliant.

Protected behavior:

- protocol-v9 raw payload validation;
- sequential independent explicit attempts;
- apply-time and reduplication safety budgets;
- grammar-required correctness routing and the real build pre-expansion;
- within-backend tuning, deferred unchanged.

## Acceptance proof

Tests are written before the spawn seam is replaced:

1. containment setup failure returns a typed failure before compile;
2. a successful selected child returns its exact payload;
3. a descendant exceeding a small memory cap kills the complete tree and yields no artifact;
4. wall timeout kills a descendant that keeps stdout/stderr open, and reader joins finish;
5. direct-child crash with a living descendant leaves no process and no artifact;
6. malformed/truncated/trailing output remains a protocol failure, not a memory kill;
7. peak memory and configured limit are recorded with platform provenance;
8. unavailable Linux delegation fails closed; `PANGLOSS_CGROUP_TEST_REQUIRED=1` turns capability
   skips into failures in the designated Linux gate;
9. Windows nested-job execution under the managed build wrapper succeeds or reports a typed
   unsupported-host failure;
10. a direct child that exits cleanly before a late containment event still loses its payload;
11. memory fixtures touch and retain their pages, and descendants retain both stdout and stderr.
12. the configured Linux delegation root is empty, the supervisor is in a strict child leaf, and
    every worker reports membership in a sibling attempt cgroup without upward authority
    discovery; atomic first-instruction placement is reviewed structurally from the clone3-only
    implementation rather than claimed by this behavioral topology test;
13. missing-executable, launch-handshake, and orderly-unwind failures leave no direct child,
    descendant, or per-attempt cgroup, and cleanup failure outranks the initiating error.

Only after both adapters and their platform gates pass may direct `Command::spawn`/`Child::kill`
and “caller supplies process-tree policy” documentation be deleted.
