# PanGloss acknowledgement bound proposal

Status: design-grill proposal, informed by two independent Luna xhigh reviews. The default remains
subject to user approval.

## Proposed product rule

An acknowledgement suppresses a finite, comparable `Info` or `Warning` observation only while the
same finding remains at or below an explicitly stored maximum in the metric's native units.

For deterministic, finite, higher-is-worse integer metrics, suggest a provisional 25% guardband:

```text
relative_delta = ceil(accepted_value * 0.25)
raw_bound = accepted_value + max(1 native unit, relative_delta)
effective_bound = min(raw_bound, next Error/Critical or hard-budget boundary)
```

The acknowledgement stores both `accepted_value` and `effective_bound`. It never recomputes an old
bound from a newer policy and never ratchets the baseline upward. A user may choose a different
explicit bound; accepting a resurfaced finding creates a new immutable event.

Twenty-five percent is a provisional convenience, not a claim that every metric may safely grow by
that amount. It has one useful relationship to current compile-health policy: warnings for several
budget dimensions begin at 80% of the budget, and a 25% increase from that point reaches 100% of the
budget. The bound is still capped before an acknowledgement could conceal an `Error`, `Critical`, or
hard containment boundary.

A 200% default is rejected. “200% more” ambiguously means three times the baseline, while “200% of”
means twice the baseline; either permits very large regressions. User-facing text should use an
unambiguous multiplier such as `2.0×` and the native values when such a comparison is requested.

## Metric-specific behavior

| Metric kind | Default behavior |
| --- | --- |
| Exact deterministic count/bytes/work | Suggest +25%, minimum one native unit, capped at the next non-acknowledgeable boundary |
| Corpus- or word-dependent count | Same only when corpus/suite, case selection, pipeline, budgets, and aggregation match exactly |
| Ratio | Store a native ratio bound and report change in percentage points; use a metric-specific suggestion rather than the generic integer formula |
| One-shot duration or sampled RSS | No derived percentage acknowledgement in v1; require repeated comparable measurements before offering a bound |
| Predicted heuristic value | Structural acknowledgement only in v1; do not present the heuristic as accepted measured cost |
| Unbounded/categorical finding | Structural acknowledgement only, keyed to unchanged construct semantics and execution recipe; no numeric increase claim |

Apply-side raw totals are not comparable across different corpora. Rule-profile counts likewise
require the same corpus or a defined normalized unit. An acknowledgement cannot claim construct
scope when the source finding has no stable construct identity.

## Resurfacing

Suppose a construct was acknowledged at `1,600,000` states with a stored bound of `2,000,000`, and a
later comparable run observes `2,192,000` states:

```text
increase from accepted value = 37.0%
increase beyond accepted bound = 9.6%
```

Present both facts:

> Warning: this construct was acknowledged at 1,600,000 states, with an accepted bound of
> 2,000,000. It now costs 2,192,000 states—37.0% above the accepted value and 9.6% above the
> accepted bound. The acknowledgement no longer applies.

For a ratio, distinguish percentage points from relative change:

> Rejection share is 13.7%; it was accepted at 10.0% with a 15.0% bound. That is +3.7 percentage
> points (+37% relative), and it remains within the accepted bound.

The finding resurfaces whenever the bound is exceeded, severity becomes `Error`/`Critical`, context
is incomparable, construct semantics change, or the acknowledgement expires. Improvement does not
rewrite the historical baseline or widen the bound.

## Required event fields

- metric, comparator/direction, accepted native value, and explicit native bound;
- accepted severity and maximum acknowledgeable severity;
- finding code, phase, typed construct key, and construct semantic fingerprint;
- grammar fingerprint for audit;
- pipeline/backend/profile, compiler, policy/budget, and aggregation digests;
- corpus/suite/case selection for apply and rule-profile evidence;
- author, timestamp, rationale, review/expiry, and supersession references.

## Current implementation cautions

Current `HealthFinding.affected` values are not uniformly typed or populated, so some findings can
only be acknowledged at exact project/observation scope until construct attribution improves.
Timing and sampled RSS are explicitly measurement-sensitive. The `RejectionShare` schema comment and
current producer also disagree about whether the value is confirmed share or rejected share; resolve
that mismatch before making ratio acknowledgements durable.
