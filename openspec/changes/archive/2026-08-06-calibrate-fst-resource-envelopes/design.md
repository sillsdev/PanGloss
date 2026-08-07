## Decisions

Execution order, prerequisites, and exclusive ownership are governed by
`openspec/changes/STAGING.md`; this change is not dispatchable outside that graph.

- Sweep one vector first, then selected pairwise stress interactions; correctness gates remain active.
- Combine those synthetic sweeps with the real-language matrix and representative runtime words.
  Real workloads establish useful normal headroom; synthetic cases reveal isolated and interaction
  cliffs. Final policy needs both kinds of valid evidence.
- OS child-process high-water RSS is authoritative; states/arcs are structural correlates, not memory proxies.
- Runtime thresholds use one portable versioned policy across Windows, Linux, and WASM. Build
  profile and platform remain measurement metadata, not separate runtime policies.
- Over-budget variants must terminate with typed outcomes inside the outer hard limits.
- Performance results run serially on a quiet machine and retain raw metadata.
- Current final calibration runs on Windows. Linux evidence is explicitly `not_run`, not inferred or
  blocking. When Linux measurements become available, compare them and conservatively review the
  same portable policy rather than forking platform-specific runtime limits.
- Earlier stages use centralized, explicit provisional values only to build and test the machinery.
  Final scale sweeps and policy publication wait for every Stage-2 construct change and production
  cascade profile to merge. Provisional values cannot be presented as release policy.
- Calibration output is advisory: raw data, recipes, proposed values, headroom reasoning, and a
  current-to-proposed policy diff. It has no write path to production constants. A human-reviewed
  committed policy-version change is the only way to activate new values.

## Dependencies

Depends on completed `harden-foma-resource-safety`, ledger schema v1, every merged Stage-2 construct
change, production cascade profiling, the pinned post-Stage-2 ledger, and the pairwise covering-array/
gate infrastructure. Calibration harness implementation may proceed earlier, but serial sweeps and
policy publication are late merge/execution units.
