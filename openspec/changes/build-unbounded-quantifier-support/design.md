## Context

`compile-bounded-fst-quantifiers` left the unbounded case refused, citing ADR 0001. The decision to
build it, and the evidence that the original premise did not hold, is recorded in
`docs/conformance/needs-decision-resolutions.md` (row 14) — including the backend's native operator
citations and the fact that an absent upper bound is the loader's own default.

## Goals / Non-Goals

- **Goal:** an exact unbounded construction, so the propose-and-confirm contract holds without
  approximation in either direction.
- **Non-Goal:** promoting the quantifier characteristic to unconditional admission. It rests at
  confirm-only, per the promotion ladder in `plan-construct-coverage-completion` D1.
- **Non-Goal:** emptying any reference grammar's refusal set. All three carry a permanently
  carved-out construct (`docs/benchmark-matrix.md`), so closing this reduces but cannot eliminate
  their refusals.

## Decisions

- **One predicate, widened, rather than a second configuration id.** The decision record floated a
  separate `quantifier.unbounded-expansion` key. Widening the existing predicate uniformly was chosen
  instead: bounded and unbounded now share one exact construction, so two ids would describe one
  capability and invite exactly the shared-identifier confusion documented in
  `docs/conformance/shared-construct-id-analysis.md`. Recorded because it departs from the record this
  change implements.
- **The finite ceiling is skipped, not raised.** An unbounded repetition is not "a bound above the
  ceiling" — the native construction's compiled size does not scale with occurrence count, so the
  ceiling is inapplicable rather than merely generous. Raising it would have re-introduced the
  cutoff-as-semantics confusion in a subtler form.
- **Reproduce before fixing.** The right-to-left mirror defect was argued structurally before it was
  ever observed. It was reproduced concretely first and the fix landed only after. This matters
  because the existing right-to-left quantifier fixture could not detect it: its repetition group
  wraps a single element and is therefore palindromic by accident, so it passed either way.

## Risks / Trade-offs

- An unbounded net admits arbitrarily long matches, so apply-time cost is bounded by ADR 0003's apply
  budgets rather than by the pattern itself. That is the same containment every other construct
  already relies on, not a new exposure.
- A repetition with no upper bound inside a reversed slot list is a new combination created by these
  two changes together; both mirror variants are pinned so the interaction is covered rather than
  assumed.
