# PanGloss profiler, health, configuration, and acknowledgement design freeze

Date: 2026-08-11

Status: product grill complete; ready for implementation.

> **Current FST policy note (2026-08-23).** The profiler records three separate axes: capability
> correctness, resource/size cost, and readiness health. `--allow-unproven` and
> `--remove-size-limits` are developer-build-only controls, absent and rejected in production.
> The former may lose valid parses and may write local developer evidence, but never production-publishes
> or certifies; the latter removes internal
> deterministic size/work caps only, while exact completion, external watchdog/RSS containment,
> bounded I/O, and the absolute ceiling remain mandatory. `Error` can be complete/accurate stress
> evidence but is production-unready; `Critical` is a correctness gap. The legacy
> `--no-enforce-capability` escape is developer-only. Acknowledgements never turn either result
> into production admission.

## Frozen product decisions

1. PanGloss works from `.fwdata` plus CLI arguments and built-in defaults; configuration is optional.
2. Shared project data lives under `ConfigurationSettings/PanGloss/`, outside `.fwdata`.
3. `project.toml` holds named pipeline/backend/resource profiles and report defaults.
4. Acknowledgements are immutable event files under `Acknowledgements/<event-id>.json`.
5. Chorus/FLExBridge registration is required eventually but deferred; v1 calls the files
   project-local.
6. Acknowledgements affect `Info`/`Warning` presentation only. `Error` remains a production-
   readiness failure; `Critical` is a correctness/capability gap. Only the explicit developer-only
   capability override can inspect that gap as unproven output, and it never creates production
   trust.
7. Numeric acknowledgements store accepted value and native-unit bound. A provisional +25% bound is
   suggested only for comparable deterministic finite higher-is-worse integer metrics.
8. Numeric validity follows comparable health-observation identity and context, not source edits.
9. Matching acknowledged warnings are presented as `Info`, with accepted/current/bound values; raw
   finding severity and admission remain unchanged.
10. `HealthReport` owns evidence context, measurement coverage, canonical aggregate observations,
    and derived findings. Detailed rule/word evidence stays in profiler/assessment artifacts.
11. Coverage states are `measured`, `not_requested`, `unsupported`, `failed`, and `censored`.
12. Missing comparable measurement leaves an acknowledgement `active_unverified`, not stale.
13. HC rule profiling uses normal parse semantics, per-word collectors, deterministic merge, and
    separates physical rule executions, memo events, output/fan-out, and advisory timing.
14. The attention report joins immutable health, readiness, assessment, and profiler evidence only
    through verified shared context and typed identities.
15. Neutral advice presents unordered possible approaches with applicability, tradeoffs, gotchas,
    equivalence cautions, and validation steps; it does not recommend an automatic grammar edit.

## Authoritative decision records

- `fieldworks-run-tests-backend-profiler-review.md`
- `pangloss-health-vs-hc-rule-stats.md`
- `pangloss-fieldworks-storage-decision.md`
- `pangloss-optional-configuration-decision.md`
- `pangloss-acknowledgement-severity-decision.md`
- `pangloss-acknowledgement-bound-decision.md`
- `pangloss-observation-based-acknowledgement-decision.md`
- `pangloss-health-report-observations-decision.md`
- `pangloss-health-observation-granularity-decision.md`
- `pangloss-health-measurement-coverage-decision.md`
- `pangloss-unmeasured-acknowledgement-decision.md`

## Superseded proposal language

Earlier proposal notes remain research history, not implementation authority. In particular, do not
implement:

- root `.pangloss/` storage;
- one mutable `acknowledgements.json` ledger;
- configuration required for normal commands;
- acknowledged findings becoming invisible;
- model/source hash or any semantic edit automatically staling a numeric acknowledgement;
- missing findings meaning zero, healthy, or cleared;
- observations as a separate artifact from their `HealthReport` findings;
- detailed rule-by-word samples duplicated into `HealthReport`;
- acknowledgement bypass of production Error readiness or Critical capability refusal.

## Engineering contracts to pin in schemas/tests

These do not require another product grill:

- exact `project.toml` wire schema, unknown-key/version behavior, and discovery rules;
- exact immutable acknowledgement event envelope, ID, timestamp, supersession, and canonicalization;
- shared `EvidenceContext`, `ConstructKey`, observation-key, and digest algorithms;
- initial metric list, comparator/direction, metric-definition versions, health triggers, and
  reference/resource-limit semantics;
- HealthReport v1-to-v2 compatibility and old-report unknown-observation behavior;
- current `--engine`/`--pipeline` normalization and explicit refusal of unsupported runtime backend
  selection.

## Implementation sequence

1. Shared evidence context and typed subject/observation identity.
2. Optional FieldWorks-side configuration discovery and resolved-profile provenance.
3. `HealthReport` v2: coverage, observations, findings referencing observations, and compatibility.
4. Immutable acknowledgement event store, matching, and health-only informational presentation.
5. HC rule profiler collector/report.
6. Cross-report attention view and neutral advice catalog.
7. Deferred Chorus/FLExBridge registration and real two-replica merge verification.

Implementation should begin with a narrow health-only vertical slice and tests. It must not begin by
embedding acknowledgements in `HealthFinding`, changing backend preference globally, or coupling the
profiler to trace mode.
