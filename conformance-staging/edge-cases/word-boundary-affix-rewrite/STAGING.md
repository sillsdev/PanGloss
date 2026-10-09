# Staged conformance case: word-boundary rewrite at affix edges

## Why this fixture exists

This pins the actual word-edge anchor behavior requested by the implementation brief. It uses
HermitCrab's `initialBoundaryCondition` and `finalBoundaryCondition`, rather than a user-created
boundary marker whose representation happens to be `#`. The one-segment prefix and root-initial
root produce `ta`; a two-segment prefix produces `taa`; the internal `r` control stays `ara`; and
the suffix mirror produces `at`.

`literal-hash-affix-context` is the companion fixture for Motif's current authored context. It
uses a literal `PhBdryMarker` with code `#`, which the oracle treats as a shape boundary, not as a
word-edge anchor.

## Founding-oracle evidence

`words.yaml` signatures and rule traces were checked by the C# HermitCrab conformance harness built
from the pinned Machine commit `18cf242f4b114b0eb9bac304b4b171ca2f499a39` on 2026-10-09. The
targeted self-check passed: 1 fixture, 0 failed. The oracle accepted `ta`, `taa`, `ara`, `at`, and
`a`, and rejected the unchanged or misplaced-context controls `ra`, `raa`, `ata`, and `ar`.

## Graduation

Candidate destination: `machine/conformance/edge-cases/word-boundary-affix-rewrite/`. No upstream PR
has been opened.
