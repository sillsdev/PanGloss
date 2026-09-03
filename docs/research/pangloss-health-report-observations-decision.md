# PanGloss HealthReport observation decision

Date: 2026-08-11

## Decision

A versioned `HealthReport` schema owns both health observations and the findings derived from them:

```text
HealthReport
  schema_version
  evidence_context
  observations[]
  findings[]
```

`HealthObservation` retains comparable measurements across healthy and unhealthy bands.
`HealthFinding` remains the policy/attention/admission statement and references the observation key
that supports it when the finding is measurement-derived.

Observations and findings are not emitted as independent sibling artifacts. They originate from the
same health evaluation, share one evidence context, and must not be allowed to drift or be joined
approximately.

## Observation identity

An observation key includes enough typed information to distinguish:

- finding family/reason where applicable;
- phase;
- metric and metric-definition version;
- typed subject or explicit grammar-wide scope;
- aggregation and value provenance;
- the relevant execution-context dimensions.

Findings reference keys, not array ordinals or prose. Policy evaluation records the health trigger,
reference/resource limit, derived severity, and policy version separately from the observed value.

## Evidence-family boundary

Assessment, golden comparison, HC rule profiling, and readiness remain separate artifacts. They have
different execution lifecycles and completeness semantics. The derived attention report joins them
only through verified shared evidence context and typed identities.

Acknowledgement evaluation consumes the `HealthReport` observations and findings without rewriting
either. Its derived state and informational presentation remain separate from raw health severity and
admission.

## Compatibility

Existing finding-only reports require an explicit schema migration/compatibility path. A missing
`observations` member in an old report means “observation unavailable,” never zero, healthy, or
cleared.

## Next granularity question

It remains undecided whether `HealthReport.observations` contains only canonical aggregate
measurements that can participate in health policy, or also carries detailed per-word/per-rule raw
samples already owned by assessment and profiling artifacts.
