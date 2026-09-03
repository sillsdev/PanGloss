# PanGloss observation-based acknowledgement decision

Date: 2026-08-11

## Decision

Numeric acknowledgement validity is based on comparable health observations, not on whether the
FieldWorks project or acknowledged construct was edited.

A numeric acknowledgement remains active across arbitrary source edits while:

- finding code, phase, metric, and metric-definition version match;
- typed source construct identity/kind or explicit global scope match;
- scope and aggregation method match;
- provenance class, pipeline/backend/profile, and resource envelope match;
- corpus/suite/case selection matches for workload-dependent observations;
- measurement method/device context matches where required;
- the current value remains within the stored accepted bound; and
- current severity remains acknowledgeable (`Info` or `Warning`).

Source and compiled-model fingerprints remain audit provenance rather than active numeric-match keys.
Deletion, identity reuse for another construct kind, missing measurement, or incomparable execution
context prevents suppression.

## Presentation

A matching acknowledgement does not make a finding invisible. It demotes the default human-facing
presentation to an informational statement containing:

- accepted value;
- accepted bound;
- current value;
- improved, unchanged, increased-within-bound, or breached state;
- health trigger and reference/resource limit when available.

Machine-readable output retains the raw finding and severity, current observation, acknowledgement
event ID, and derived state. Large sets may be summarized by default with full details available on
request.

## Required evidence change

PanGloss must retain below-threshold measurements as `HealthObservation` records. A missing finding
does not prove improvement: current producers commonly omit values in healthy bands. Findings remain
derived attention/admission statements; observations provide the comparable measurement history.

The presentation distinguishes:

1. health trigger;
2. reference/resource limit;
3. project-accepted bound.

## Categorical exception

Unbounded and categorical findings have no numeric value to compare. Their acknowledgements remain
keyed to the structural condition and execution recipe. A changed structural condition resolves or
invalidates that acknowledgement without inventing a percentage comparison.

## Superseded recommendation

The earlier proposal that any semantic edit to the acknowledged construct should make a numeric
acknowledgement stale is rejected. Observable health and explicit comparison context govern numeric
acknowledgements.

## Next schema question

It remains undecided whether observations should be embedded beside findings in a versioned
`HealthReport` schema or emitted as a separate sibling artifact joined through evidence context.
