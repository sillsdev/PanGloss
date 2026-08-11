## Why

PanGloss can currently report that a backend compiled a fixture without proving that the backend
proposed every analysis required by Rust HermitCrab. The strongest three-backend semantic gate covers
only a small fixture slice, so query-encoding or emitter defects can yield zero candidates while the
row-level characteristic and compile-witness accounts remain green.

## What Changes

- Add a fixture-backed backend semantic matrix that runs Rust HermitCrab-only and every applicable
  compiled backend over the same stable assessment cases.
- Make every claimed-support cell build-breaking: through a strict audit path that forbids fallback,
  a supported backend must realize the requested strategy, propose every required admission identity,
  and confirm a semantic
  analysis set equal to the Rust HermitCrab-only result and the fixture's checked-in expectation.
- Treat `CannotRepresent` as an honest blank rather than support evidence: the normal selector must
  refuse it with an attributable characteristic/configuration diagnostic. Unexpected refusal,
  compilation failure, truncation, empty positive oracle result, zero positive proposals, or semantic
  disagreement is red; there is no accepted-gap or report-only state.
- Reuse conformance-format grammars, words, stable case identifiers, and oracle provenance for every
  observable semantic claim. Rust-only assertions cover backend selection, encoder dispatch,
  proposals, budgets, counters, caches, supervision, and coverage accounting without duplicating the
  grammar's semantic expectations.
- Define a finite denominator: the per-characteristic/per-backend representation-status account is
  the floor; configuration partitions, declared non-orthogonal interactions, load-bearing ordering
  archetypes, and boundary cases are additional named obligations. This does not claim arbitrary
  Cartesian-product or factorial-permutation coverage.
- Provide an on-demand C# verification lane through the existing Machine conformance adapter. It
  verifies or authors fixture expectations and provenance when invoked; ordinary Rust gates consume
  the checked-in results and never claim C# provenance that was not actually obtained.
- Split a fast PR scope and a full managed scope, each fail-closed against its own declared case
  denominator. No test is ignored merely because a backend is currently broken; implementation must
  repair every unexpected red result before merge.

## Impact

- Primary code/test areas: `pg-foma` backend runtime/evidence support and integration tests,
  `pg-conformance-fixtures` case identity/provenance helpers, and small synthetic fixtures under
  `conformance-staging/` or the pinned Machine conformance tree.
- Existing sources of truth remain authoritative: `CharacteristicKind::ALL`,
  `strategy_coverage::{ALL_STRATEGIES, representation_of}`, selector diagnostics, fixture discovery,
  and checked-in conformance expectations. The matrix consumes these instead of creating a second
  representation-status ledger.
- Keep `RepresentsWithKnownGap` inside the represented, red denominator and repair every selected
  case to semantic parity. Only already-declared `CannotRepresent` cells are conceptual blanks;
  this change cannot downgrade a known gap into a refusal to make the matrix green.
- `witnessed_coverage` must stop conflating compile success with semantic witnessing; its report is
  either upgraded to consume semantic observations or renamed/narrowed so only one artifact owns the
  term `witnessed`.
- Depends on `add-capability-characteristics-check` for dispositions/refusals and on the existing
  conformance adapter/provenance procedure recorded by `add-reference-hermitcrab-parity`.
- Coordinates with, but does not replace, `add-pairwise-grammar-interaction-coverage` and
  `docs/rule-interaction-and-ordering-coverage-plan.md`; those publish stable interaction and
  ordering obligation IDs and own reachability/retirement. This change maps those externally owned
  IDs to executable backend-parity runs.
- PanGloss owns Machine and the conformance grammars, but another agent may concurrently change both.
  Fixture and grammar edits therefore require explicit file ownership, inspection of that agent's
  accepted tip, and rebase/integration before overlapping edits.
- Non-goals: adding new HermitCrab model features, converting `CannotRepresent` cells into supported
  cells, duplicating all matrix cases as inline XML, matching C# crashes/resource thresholds, or
  changing production backend selection policy.

## OpenSpec validation note

This repository's code-defined change schema intentionally has no spec-delta artifact.
`openspec status` completion is authoritative for the three planning artifacts; the generic strict validator's
`no deltas found` result is a known non-actionable mismatch and must not be worked around by
inventing a spec directory.
