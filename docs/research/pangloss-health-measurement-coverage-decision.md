# PanGloss health measurement-coverage decision

Date: 2026-08-11

## Decision

`HealthReport` explicitly records measurement coverage for each applicable metric family. Consumers
must not infer measurement status from the presence or absence of observations or findings.

The minimum status vocabulary is:

```text
measured
not_requested
unsupported
failed
censored
```

Each non-`measured` status carries a stable reason code and factual detail. Examples include no
workload supplied for apply metrics, unsupported backend instrumentation, worker failure, logical
budget termination, or wall-clock timeout.

## Semantics

- `measured` means the report contains the canonical complete observation for that declared scope.
- `not_requested` means the invocation did not request or supply the inputs needed for that metric.
- `unsupported` means the selected pipeline/backend cannot produce it.
- `failed` means measurement was attempted but did not produce usable evidence.
- `censored` means partial evidence exists but termination prevents treating it as a complete value.

An absent observation never means zero, healthy, cleared, or unsupported. Old finding-only report
schemas are treated as observation status unknown.

Coverage records identify metric family, requested scope/workload, measurement method, and any
produced observation keys. Detailed failure/termination evidence remains available without turning a
partial aggregate into a complete measurement.

## Interaction with acknowledgements

Acknowledgement evaluation consumes measurement coverage before comparing values. A previously
acknowledged observation with no comparable `measured` current value cannot be described as
revalidated, improved, unchanged, or breached.

Human output may explain:

```text
info: previously acknowledged at 10,000, but this metric was not evaluated in the current build
      (no workload supplied)
```

## Next lifecycle question

It remains undecided whether a run that does not remeasure an acknowledged observation leaves the
acknowledgement active-but-unverified, or immediately marks it stale until a comparable measurement
is produced.
