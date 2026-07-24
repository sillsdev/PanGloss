## Decisions

Execution order, prerequisites, and exclusive ownership are governed by
`openspec/changes/STAGING.md`; budget foundation, worker watchdog, and terminal outcome routing are distinct merge units.

- Deterministic counters are the primary fail-fast defense. A separate compiler worker protects the
  host from opaque foma hangs, panics, and excessive sampled memory.
- A mutable tracker accumulates states, arcs, tuples, groups, emitted lines, elapsed build time,
  paths, outputs, and candidates across the grammar/word.
- Per `docs/adr/0003-apply-time-containment.md`, the tracker adds a **derivation/unapplication
  chain-depth** dimension: a per-word counter of nested apply/unapply steps, checked on every step
  rather than bounded only by the native call stack. This closes the stack-overflow failure class
  deterministically (the Aweti-shaped 24-level derivation chain; the 1 GiB-stack workaround) instead
  of merely raising the point at which it recurs. Chain-depth breach is a distinct typed outcome
  alongside the existing logical-budget outcomes, ported identically to Windows, Linux, and WASM.
- Configuration parsing is strict and reports effective versioned limits.
- Every configurable dimension has a hard-coded, versioned, deliberately high absolute ceiling.
  Defaults and caller/host-selected limits remain below it; `unlimited` is not a supported value.
  These ceilings contain emergencies and do not replace the earlier diagnostic budgets.
- Runtime application uses one portable ceiling set and budget schema across Windows, Linux, and
  WASM. Each embedding application may choose lower normal or retry limits; it cannot define a
  different maximum. Normal defaults are intended for ordinary user PCs.
- Timeouts, worker panics, spawn failure, input/output size breach, logical budget breach, and
  sampled-RSS termination are distinct typed outcomes.
- The synchronous parent uses `std::process::Child::try_wait` and `Child::kill`; no async runtime is added.
- Pin a `sysinfo` release compatible with workspace Rust 1.90 and refresh only the worker PID.
- The RSS guard records sampling interval and observed maximum. It is never described as a hard
  ceiling because allocation can occur between samples.
- Production compiler code launches no descendants. Process trees, Job Objects, cgroups, Tokio,
  `processkit`, CPU quotas, and process-count limits are non-goals.
- Timed-out strategies are terminal for that grammar and are never retried in-process.
- Production routing returns terminal resource outcomes directly and never invokes a second engine
  or strategy automatically. Alternate strategies require an explicit new caller request.
- Expose two named runtime pipelines: normal FST propose plus HermitCrab confirm, and explicit
  HermitCrab-only for integrating engines, parity, and richer explanations of why a word did not
  parse. The selected pipeline is immutable within a request and shares the budget/outcome schema.
- Native tooling is the sole compilation authority. WASM loads a precompiled artifact, enforces
  logical application budgets, and has no compiler or worker-watchdog mode.
- Windows and Linux implement the same worker protocol, timeout/kill loop, typed outcomes,
  sampled-RSS policy, and bounded communication. Neither is a subordinate or CI-only tier.
- Missing platform evidence is recorded as `not_run` and does not block independent work.
- Application is atomic per word, not per batch: budget exhaustion yields no definitive partial
  result for that word, while already completed words remain valid. Callers may explicitly retry the
  incomplete subset with different apply limits after consuming the easy results.
- Callers may also select a cumulative batch budget. Batch termination preserves complete words and
  distinguishes the currently incomplete word from later words that were never attempted, allowing
  either subset to be resubmitted without ambiguity.

## Dependencies

The error schema may proceed alongside `define-grammar-coverage-contract`; calibration depends on
  the later resource-envelope change.
- Deterministic logical counters are the normal fast-failure mechanism and must attribute growth to
  grammar constructs. Cooperative elapsed checks and the parent wall timeout are outer safeguards
  for stalls or work that cannot yet be instrumented; behavior does not promise an identical number
  of wall-clock seconds across machines.
