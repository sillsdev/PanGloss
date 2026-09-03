# PanGloss health observations and edit-insensitive acknowledgements

Status: design-grill proposal, informed by two independent Luna xhigh reviews. This corrects the
earlier suggestion that every semantic edit to an acknowledged construct should make its
acknowledgement stale.

## Core correction

An acknowledgement accepts a bounded recurring cost observation. It does not approve or fingerprint
the complete linguistic semantics of a construct.

A source edit is therefore not itself a reason to resurface an acknowledgement. If the same
comparable observation remains within its accepted bound, the acknowledgement remains active.
Grammar/model and construct-semantic fingerprints are retained for audit, but are not active match
keys for numeric cost acknowledgements.

## Comparable observation identity

Two numeric observations are comparable only when these fields match:

- finding code and phase;
- metric and metric-definition version;
- typed source construct key and construct kind, or an explicit grammar-wide scope;
- scope and aggregation method;
- value-provenance class;
- pipeline, backend/recipe, and profile;
- effective resource envelope and budget policy;
- corpus/suite/case selection for workload-dependent metrics;
- measurement method, device class, and aggregation for timing/RSS evidence.

The `.fwdata` source hash and compiled-model fingerprint are recorded as provenance, not used as
automatic invalidators. The importer must still establish that a source GUID continues to identify
the same construct kind; deletion, reuse for a different kind, or loss of stable identity is not
continuity.

## Required observation layer

Current health output is finding-oriented: measurements below a warning threshold are commonly
omitted. That cannot support an honest statement such as “accepted at 10,000; now 320,” because the
320 measurement may generate no finding at all.

Add a raw measurement layer separate from findings:

```text
HealthObservation
  observation_key
    finding_code
    phase
    metric
    metric_definition_version
    typed_subject_or_global_scope
    aggregation
  value
  value_provenance
  current_health_trigger
  current_reference_or_resource_limit
  current_severity
  evidence_context
```

`HealthFinding` remains the derived attention/admission statement. `HealthObservation` supplies
comparable values across healthy and unhealthy bands. Absence of a finding is never interpreted as a
zero or improvement; a current observation is required.

Keep three thresholds distinct in presentation:

1. **health trigger** — where PanGloss begins producing the warning;
2. **reference/resource limit** — budget or severity boundary from the producer;
3. **accepted bound** — the durable project decision.

The current `HealthFinding.threshold` field does not consistently represent all three concepts and
must not be relabeled generically in the UI.

## State under later edits

| Later result | Acknowledgement state |
| --- | --- |
| Accepted 10,000; current 320 | Active; report improvement |
| Accepted 10,000; current 10,000 | Active; report unchanged |
| Accepted 10,000; current 12,000; bound 12,500 | Active; report increase within bound |
| Accepted 10,000; current 13,000; bound 12,500 | Breached; resurface warning |
| Same source GUID edited, comparable value still within bound | Active |
| Subject deleted or not measured | `not_observed`; do not invent a result |
| Identity reused for a different construct kind | Stale/incomparable |
| Metric/pipeline/budget/corpus/aggregation changed | Stale/incomparable |
| Severity becomes `Error` or `Critical` | Resurface; acknowledgement cannot override |

## Human presentation

Acknowledgement should demote the default presentation of a matching `Info`/`Warning` observation to
an informational statement rather than making it invisible. Raw severity and findings remain
unchanged in machine output.

Examples:

```text
info: acknowledged PGF0003 emitted lines
      accepted 10,000; bound 12,500; current 320; improved 96.8%

info: acknowledged PGF0002 network states
      accepted 10,000; bound 12,500; current 10,000; unchanged
```

When many acknowledgements match, normal output may aggregate unchanged entries while retaining
material changes:

```text
info: 12 acknowledged observations remain within accepted bounds
      3 improved; 1 increased within bound; 8 unchanged
      use --show-acknowledged for details
```

A breach is explicit:

```text
warning: rule {GUID} was accepted at 10,000 states with a bound of 12,500.
         It now measures 13,000: 30% above the accepted value and 4% above the bound.
         The acknowledgement no longer applies.
```

Machine-readable output retains the raw finding/severity, current observation, matching
acknowledgement event ID, and derived state.

## Categorical findings

Unbounded and categorical findings have no numeric health observation to compare. Their
acknowledgements use a narrower structural key:

```text
finding code + phase + typed construct + structural condition + execution recipe
```

If that structural condition changes, the acknowledgement resolves or becomes incomparable. No
percentage-change claim is made.

## Proposed correction to the prior grill

Reject the earlier recommendation that any semantic fingerprint change makes a numeric
acknowledgement stale. Direct edits are irrelevant when observation identity remains comparable and
the value remains within the stored bound. Semantic fingerprints remain useful audit evidence and
may still be required for non-numeric structural acknowledgements.
