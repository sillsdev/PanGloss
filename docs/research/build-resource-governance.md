# Build resource governance research

## Decision

Managed Windows cargo launches use the direct-process adapter in
`rust/tools/_common.ps1`. The adapter starts the requested executable,
assigns priority, waits for its exit code, and performs tree cleanup only if
the launcher remains alive during teardown. Linux keeps the equivalent native
child-process adapter.

The retained governance controls are compiler width, build and run slot
pools, memory admission, process priority, cache priority, disk checks, and
targeted orphan scanning.

## Evidence and classification

The change is primarily a production-readiness and throughput correction.

- The removed Windows process governor imposed a 25% CPU-rate ceiling on cold
  builds. That was the main throughput limit when physical memory and compiler
  slots were available.
- Its teardown path added approximately 5--12 seconds to each managed call.
  The wait loop polled for up to ten seconds, and the launcher remained alive
  until its descendants drained.
- Memory admission remains a resource-containment control. It measures
  available physical memory and refuses a new operation when the configured
  reserve would be violated; it is not a promise that a process has a fixed
  lifetime memory cap.
- Compiler width and slot ownership remain correctness-neutral scheduling
  controls. They bound requested concurrency and make machine contention
  visible without changing the compiler's output.

The distinction matters: a readiness problem must not be used to avoid a
contained stress attempt, and a larger limit must not be used to excuse an
incomplete output. The direct adapter removes an unnecessary runtime ceiling;
it does not weaken the evidence required for output correctness.

## Retained mechanisms

`Get-CargoJobBudget` derives `CARGO_BUILD_JOBS` from the configured maximum,
the compiler reservation, and measured memory headroom. `Get-MemoryPerProcessGB`
supplies the compile estimate used by the memory admission calculation.
Build slots and run slots are separate so an executable launched by `run -Exe`
does not consume compiler capacity.

The default process priority is `BelowNormal`; low-impact helpers may request
`Idle`. sccache is treated as an independent service and remains at
`BelowNormal`. Disk admission and the orphan scanner remain active. The
scanner is limited to recognized cargo, compiler, linker, and configured C/C++
process names so it cannot act on an unrelated process merely because it is
old.

## Direct-launch contract

The Windows adapter is tested with a mocked `Start-Process`, not with cargo.
The test verifies `-NoNewWindow`, `-PassThru`, working-directory and stdout
forwarding, the default priority, a zero-argument `WaitForExit()` call, and
the child's exit code. The cargo forwarding test verifies that the stable
`Invoke-CargoWithReaper` seam preserves arguments and priority without hidden
thread or memory-limit parameters.

## Scope boundary

This document records the current design and its measured rationale. Dated
historical documents may retain the terminology and measurements of the older
implementation; they are not normative instructions for current managed
launches.
