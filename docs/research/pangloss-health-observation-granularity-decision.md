# PanGloss health-observation granularity decision

Date: 2026-08-11

## Decision

`HealthReport.observations` contains canonical aggregate measurements that participate in health
policy or acknowledgement comparison. It does not duplicate detailed samples owned by other
evidence artifacts.

Examples appropriate for `HealthReport.observations` include:

- final payload bytes;
- final/intermediate state and arc counts at a named compile scope;
- emitted-line count at a named compile scope;
- alpha-tuple, gate-group, ordering-rule, or compound-pair count for a typed construct/scope;
- aggregate proposal, confirmation, rejection, or duplicate measurements over an explicitly named
  workload;
- canonical latency percentile summaries produced under a recorded measurement method.

Examples that remain outside `HealthReport` include:

- every word's latency samples;
- every rule execution event;
- rule-by-word work matrices;
- trace events and derivation narratives;
- per-case structured analyses and golden expectations.

Those details remain in rule-profile, assessment, golden, investigation, or trace artifacts. The
derived attention report may link to them through typed identities and matching evidence context;
it does not copy them into health.

## Completeness

Each aggregate observation declares its scope, aggregation method, workload identity where
applicable, and completion status. An aggregate derived from incomplete or censored detailed evidence
cannot be presented as a complete measurement.

This boundary keeps `HealthReport` bounded, preserves one owner for detailed evidence, and prevents
two artifacts from claiming different completeness semantics for duplicated samples.

## Next absence question

It remains undecided how a health run represents a metric that was expected or previously
acknowledged but was not measured in the current invocation: omit it and infer from run context, or
emit an explicit `not_observed`/reason record.
