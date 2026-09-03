# PanGloss acknowledgement bound decision

Date: 2026-08-11

## Decision

PanGloss acknowledgements use metric-aware, explicitly stored bounds.

For a comparable deterministic, finite, higher-is-worse integer metric, PanGloss suggests a
provisional 25% guardband:

```text
relative_delta = ceil(accepted_value * 0.25)
raw_bound = accepted_value + max(1 native unit, relative_delta)
effective_bound = min(raw_bound, next Error/Critical or hard-budget boundary)
```

The suggestion is not an automatic policy judgement. The acknowledgement event stores the accepted
native value and the actual chosen native-unit bound. A caller may choose another bound explicitly.

A 200% default is rejected. If a user explicitly chooses a multiple, presentation uses unambiguous
language such as `2.0×`, not “200% more.”

## Matching and resurfacing

- The accepted baseline and bound never ratchet upward automatically.
- Re-accepting a resurfaced finding creates a new immutable event superseding the earlier event.
- Exceeding the bound resurfaces the finding even while it remains a `Warning`.
- Becoming `Error` or `Critical` always bypasses acknowledgement and uses the override workflow.
- A resurfaced finding reports change from both the accepted value and accepted bound.
- Incomparable context makes the acknowledgement stale rather than applying it approximately.

## Metric-specific limits

- Deterministic counts, bytes, and work may use the 25% suggestion.
- Corpus/word-dependent counts require matching corpus/suite, selection, pipeline, budgets, and
  aggregation context.
- Ratios use stored native ratio bounds and percentage-point presentation, not the integer formula.
- One-shot elapsed time and sampled RSS receive no derived percentage bound in v1; repeated,
  comparable measurement is required.
- Predicted heuristic and unbounded/categorical findings may receive structural acknowledgements,
  but no claim about a measured percentage increase.
- A construct-scoped acknowledgement requires stable typed construct identity; a grammar-wide
  finding cannot be silently treated as construct-specific.

The detailed rationale and examples remain in `pangloss-acknowledgement-bound-proposal.md`.

## Next unresolved identity question

It remains undecided whether an acknowledgement survives a direct semantic edit to the same
GUID-identified construct when the observed metric remains below the stored bound, or becomes stale
whenever that construct's semantic fingerprint changes.
