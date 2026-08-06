# pg-foma plan_interaction_coverage_gate.rs: design notes moved out of comments

Longer arguments pulled out of `rust/crates/pg-foma/tests/plan_interaction_coverage_gate.rs`
implementation comments so the source can carry a one- or two-line pointer instead of the full
argument.

## What this file does

Integration test for tree-structured node/subtree interaction coverage over the reified compilation
plan, rather than pairwise covering arrays over raw grammar "knobs". Computes
`pg_foma::plan_interaction_coverage::compute_interaction_coverage`'s report over every discoverable
conformance fixture, prints it, and fails the build if any required `AdjacencyTuple` is
`Uncovered`. This mirrors `conformance_coverage_gate.rs`'s own flip discipline: a green
build-breaking gate that can silently start lying is worse than an advisory report, because the
green light is what gets cited.

This flip does not face the sibling's "shared coarser construct id lets a finer characteristic
inherit unfalsifiable coverage" problem: every `AdjacencyTuple` is already this module's own
finest-grained unit, and a tuple can only be credited from an actual parent-child edge present in a
caller-supplied, per-fixture reified `Plan`, never from the mere co-presence of both node kinds
somewhere in the same grammar.

## What this gate does NOT assert

- That every tag on a tuple's `tags` field was itself exercised by that specific edge — `tags` is
  informative context, never the coverage signal itself.
- That every characteristic/configuration reachable through a covered tuple is itself proven — e.g.
  `(Union, Leaf/StructuralCompositeMarker)` being `Covered` says a fixture's plan realizes that
  shape, not that every circumfix candidate-selection gap is closed. Tuple-level coverage and
  configuration-level completeness are different questions.

## The fuzz slice — also a hard assertion

For every discovered fixture whose plan's `Gate` node has >=2 partition groups,
`fuzz_gate_group_reordering_for_grammar` builds the grammar's default plan and its
`permute_gate_groups` twin and asserts `differential_oracle` reports `Agree`. This re-confirms a
mechanized correctness property (Gate-group order-invariance plus union commutativity) on every
real corpus grammar, not a coverage-completeness claim. A real disagreement here would be a genuine
regression, never something to paper over.

## What this file does NOT do

- Does not modify `machine/conformance/` fixtures, `conformance-staging/`, or any production compile
  path — those are read/reused only.
- Does not touch `conformance_coverage_gate.rs`/`conformance_fixtures_gate.rs`: a separate,
  independent cross-check over a different axis (conformance-construct coverage vs. plan-node-
  interaction coverage).
