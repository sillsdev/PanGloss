# Cost-based compilation planner with plan caching and profile-guided tuning

## Decision

When more than one legal composition topology (**compilation plan**) passes capability for a
grammar, PanGloss selects among them like a cost-based query optimizer fused with
profile-guided autotuning: rank candidates by **projected cost**, build the top candidate(s),
measure at creation, and keep the winner under a declared objective. The chosen plan is
recorded as a **committed plan** in grammar configuration (authoritative, reviewed,
reproducible); a **derived plan cache** is a disposable, fail-safe local optimization that is
never authoritative over it. A normal rebuild uses the committed plan; a full rerun
re-explores (all, or the top few).

## Why

"One system, all languages" has no single best compilation strategy. A grammar may have two
or more topologies that both pass capability and differ only in cost (compile size, apply
speed, proposal volume, duplicate counts). Choosing well — and caching the choice — is what
separates a production system from a demo.

## Key consequences

- **Plan selection is correctness-safe by construction.** Because every capability-passing
  plan is recall-preserving, `confirm(propose_A) = confirm(propose_B) = V` (the valid set):
  all plans produce the identical confirmed answer; only cost varies. Selection can never
  pick a fast-but-wrong plan.
- **Multi-plan building is a free differential correctness oracle.** If two plans ever
  disagree on a word's confirmed set, one violates the capability invariant — a
  predicate bug caught automatically. "Test all compositions" strengthens never-overclaim,
  not just cost.
- **A plan must be reified as first-class, enumerable data.** Today topology is hardcoded
  per-grammar branching (`should_run`, `probe_would_refuse`, `partition_entries`); "try both"
  has nothing to iterate over until a strategy enumerator emits the legal candidates. This
  refactor is the prerequisite.
- **Projected cost is a calibrated heuristic with an error bound, never a point estimate.**
  FST compose size is worst-case product/exponential, so estimates will sometimes be wrong by
  orders of magnitude. "Close" = overlapping bounds → build and measure. A point estimate
  never prunes a candidate alone; the estimator is periodically re-validated against measured
  reality (via `calibrate-fst-resource-envelopes`) or it silently rots.
- **Determinism policy.** Deterministic objectives (states+arcs, size, proposal volume) are
  the default and keep builds reproducible. Timing ("fastest on this corpus") is inherently
  noisy; it is confined to an explicit **tuning run** whose output — chosen plan id + corpus
  fingerprint + benchmark environment — is committed to config, so downstream builds are
  reproducible even though the measurement that chose the plan was not.
- **Exploration is resource-governed.** Building several candidates costs several × compile
  budget/peak memory. Each candidate builds under its own `ComposeBudget`; a candidate that
  trips its budget is **eliminated (a valid cost signal), not a build failure** — survivors
  compete.
- **The cache fails safe.** Cache key = hash(grammar + stem-data + compiler-version +
  capability-manifest-version + objective [+ workload fingerprint for latency]). Any mismatch
  → re-plan, or at minimum re-validate that the cached plan still passes capability (a grammar
  change can make a cached plan not merely slow but *incapable*). A stale plan is never
  trusted blindly.
- **Governance mirrors calibration.** A tuning run *proposes* a plan diff; committing it is
  an explicit human-reviewed action. A tuning run never silently rewrites config.
