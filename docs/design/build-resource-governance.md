# Build resource governance

This document describes the current resource controls for `rust/tools/pg.ps1`.
The design separates admission, concurrency, and process launching so each
mechanism has one clear responsibility.

## Current contract

| Concern | Mechanism | Effect |
| --- | --- | --- |
| Compiler width | `Get-CargoJobBudget`, `CARGO_BUILD_JOBS`, and `-TestThreads` | Limits the compiler's requested parallelism. |
| Build concurrency | Build-slot pool | Allows at most two managed builds by default and makes contention explicit. |
| Run concurrency | Run-slot pool | Keeps light tools such as formatters and probes from competing with a heavy build. |
| Memory admission | `Test-MemoryReserve`, `Get-MemoryProcessBudget`, and `Get-MemoryPerProcessGB` | Refuses a new operation when the measured machine headroom is too small. |
| Process priority | `BelowNormal` by default; `Idle` for low-impact helpers | Gives ordinary interactive work preference without changing compiler correctness. |
| Cache pressure | sccache is started and checked at `BelowNormal` priority | Keeps the cache service from becoming the foreground workload. |
| Disk pressure | Worktree and target-volume checks | Refuses operations that would exhaust the build volume. |
| Child cleanup | Direct-process exit cleanup with `taskkill /T /F` | Prevents a failed launcher from leaving its descendant tree behind. |

Admission is deliberately a spawn gate, not a promise that memory remains
constant for the lifetime of a process. A memory estimate is used to choose a
safe concurrency budget; the process itself is not given a kernel-enforced
memory ceiling.

## Managed process launch

`Invoke-ManagedProcess` is retained as the stable PowerShell seam because
callers and tests already use that name, but it is now a direct-process
adapter. On Windows it:

1. starts the requested executable with `Start-Process -NoNewWindow -PassThru`;
2. supplies the working directory and optional stdout redirection;
3. applies the requested priority, defaulting to `BelowNormal`;
4. waits with `WaitForExit()` and returns the child exit code; and
5. if the launcher is still alive while leaving the seam, terminates its tree
   with `taskkill /T /F /PID`.

The Linux adapter has the same argument-level contract and uses the platform's
normal child-process wait. `Invoke-CargoWithReaper` remains the public cargo
entry point and forwards to this seam; it does not compile or launch cargo in
the test suite.

The direct adapter intentionally has no hidden CPU-rate, process-membership,
or per-invocation memory control. Compiler width and slot admission are the
controls that remain observable and testable.

## Why the wrapper was removed

The removed Windows process governor imposed a 25% CPU-rate ceiling on cold
builds. That ceiling was the dominant throughput limit even when the machine
had enough physical memory and available compiler slots. The wrapper also
added a teardown wait to every managed invocation: its polling loop could wait
up to ten seconds, and the wrapper did not exit until the descendant set had
drained. Measured teardown added roughly 5--12 seconds per managed call.

The new design keeps the useful controls—compiler width, slot pools, memory
admission, priority, cache handling, disk checks, and explicit child cleanup—
while removing those two sources of avoidable latency. This is a production
readiness and throughput correction, not a change to the memory-admission
evidence or an attempt to hide an incomplete build.

## Slot pools and budgets

The build pool is acquired before a managed compile and released in a
`finally` path. The run pool is separate, so an executable launched with
`run -Exe` does not consume the compile-time memory estimate or compiler-job
budget. A heavy build consumes a build slot even when its compiler command has
been reduced to a small width; the slot records machine contention, not a
kernel process limit.

`Get-CargoJobBudget` combines the requested maximum, compiler reservation, and
available memory. Its result is passed to cargo as `CARGO_BUILD_JOBS` and is
also visible in preflight diagnostics. `TestThreads` remains the test-only
override for narrow reproducible runs.

## Cache and orphan handling

The cache service is managed independently of the cargo child. The wrapper
does not need to be present for cache health checks or priority management.
When a managed operation is abandoned, the orphan scanner considers only
known compiler and linker descendants (`cargo`, `rustc`, `rust-lld`, `link`,
`cc1`, and the configured C/C++ compiler names). It never kills an unrelated
process based only on a shared name, and it reports failures to perform a
requested cleanup.

## Verification expectations

PowerShell tests mock `Start-Process` for the cargo-launch contract and verify
the exact executable, arguments, working directory, priority, wait call, and
exit code. Process-tree activity tests cover stale build-slot holders without
starting cargo. Memory, disk, cache, and orphan-safety tests remain focused on
their respective controls.
