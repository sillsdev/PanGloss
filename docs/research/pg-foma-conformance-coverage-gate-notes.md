# The conformance-coverage cross-check (`tests/conformance_coverage_gate.rs`)

`supported_construct_conformance_coverage_has_no_gaps` is build-breaking (ADR 0001, honest
capability boundary), not advisory: a green build-breaking gate that can silently start lying is
worse than an advisory report, because the green light is what gets cited.

## What flipping to build-breaking asserts

Zero `Uncovered` and zero `Unmappable` rows across all `CharacteristicKind`s, each graded against
a covering, passing conformance fixture. The flip depended on `Unmappable` reaching zero (every
`CharacteristicKind` has a `constructs.txt` row), on an unknown `exercises:` tag being a hard error
rather than a soft warning (`tests/exercises_tag_liveness.rs`), and on row ids that are each mapped
by two characteristics each having a mechanized grammar-shape witness so the finer one cannot
report `Covered` on the coarser sibling's evidence (`tests/structural_witness_gate.rs`) — full
reasoning for the last one in `docs/conformance/shared-construct-id-analysis.md`.
`tests/coverage_citation_liveness.rs` separately keeps the curated containment citations from
becoming dangling pointers.

## What it still does NOT assert

- That a fixture tags the RIGHT construct. A tag claiming something the fixture does not exercise
  stays a human-authoring risk; no string or shape check closes it in general.
- That `Covered` means `Admit`. Ten rows are `ConfigPredicate` and three `ConfirmOnly`; `Covered`
  means "evidenced at its own disposition" (`ConfirmOnly` → `Admit` is a separate, optional track).
- That every CONFIGURATION inside a covered row is closed. Row-level coverage and
  configuration-level completeness are different questions; several rows still have open
  configuration splits tracked elsewhere in this crate's conformance docs.

The full report prints on every run: a failure must say WHICH row regressed and how, not merely
that a count moved.
