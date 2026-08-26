# Worker process-tree containment design

Status: approved by the cleanup charter; implementation pending.

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
  try_wait_direct_child() -> exit status with diagnostics
  poll_containment() -> native memory-limit evidence or clean state
  terminate_tree(deadline)
  wait_tree_empty(deadline)
  reap_direct_child(deadline)
  final_evidence_and_peak()
```

The helper owns all native handles and returns intrinsic Windows/Linux evidence without reducing it
to a Boolean event. `pg-foma` keeps its crate-wide `forbid(unsafe_code)`, owns protocol parsing and
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

Use cgroup v2 only. Discover the cgroup2 mount from `/proc/self/mountinfo` and the current cgroup
from `/proc/self/cgroup`. Create a generated per-attempt child beneath an explicitly delegated,
writable parent; its generated name is an implementation detail, not caller identity or an
execution envelope. Require `memory` in the parent's `cgroup.subtree_control` and writable
`memory.max`, `memory.oom.group`, `cgroup.procs`, `cgroup.events`, and `cgroup.kill` surfaces.
For this checkpoint, the supervisor's current unified cgroup is the delegated parent: never walk
upward looking for writable authority and never enable a missing controller. The service/container
manager must launch the supervisor inside a cgroup it has delegated; otherwise containment is
unavailable. This is host infrastructure, not a caller-selected build envelope.

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
cleanup deadline. Orderly supervisor errors and unwinds must run this cleanup; abrupt supervisor
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

Only after both adapters and their platform gates pass may direct `Command::spawn`/`Child::kill`
and “caller supplies process-tree policy” documentation be deleted.
