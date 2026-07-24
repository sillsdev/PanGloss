## Decisions

Execution order, prerequisites, and exclusive ownership are governed by
`openspec/changes/STAGING.md`; this change is not dispatchable outside that graph.

- **The interaction surface is the reified plan DAG, not a flat knob space.** Coverage is computed
  over composition-node-kind tuples (per `reify-compilation-plans` D5, nodes are individually
  addressable by content-address and kind), restricted to capability-legal co-occurrences — a
  constrained combinatorial-interaction / software-product-line sampling problem, not free CIT over
  raw grammar knobs.
- **Orthogonality prunes before it tests.** Node-kind pairs proven orthogonal (parallel-independence
  / critical-pair non-overlap; feeding/bleeding disjointness — see `add-capability-characteristics-
  check` D4) are *retired* and recorded as evidence, never fuzzed. This is the convergence mechanism
  that keeps interaction coverage finite.
- **Fuzz the node/subtree, not the node's declared envelope.** A test exercises the actual composed
  behavior of a non-orthogonal node and its connected subtree, which is what attacks the emergent-
  n-way envelope-lossiness gap (a node's declared capability envelope can be lossier than its real
  composed behavior).
- **Fixtures map onto the tree.** Every conformance/fuzz fixture is tagged with the plan node/subtree
  it exercises; t-wise coverage over composition-types is a CI report, flagging node-kind
  interactions with zero covering fixtures.
- Covering arrays are primary evidence; seeded random fuzzing is a secondary discovery tool. Failures
  minimize to named recipes under the same resource envelope.
- Reports include required, covered, uncovered, orthogonality-retired, and contains-unsupported
  node-kind tuples; `interaction_coverage=complete` requires zero uncovered required tuples.

## Dependencies

Requires the reified plan's node-addressability (`reify-compilation-plans` D5) and the orthogonality
predicates (`add-capability-characteristics-check` D4). Runs against a pinned post-Stage-2 plan/ledger
revision after multi-table, RTL, Simultaneous, and every dispatched remaining-construct subsection has
merged. Unsupported tuples remain visible exclusions, not passes. Feeds proven-orthogonal and
declared-limitation results back into the capability registry (ADR 0001).
