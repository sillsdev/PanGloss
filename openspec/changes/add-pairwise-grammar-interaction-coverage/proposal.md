## Why

Single-construct fixtures do not cover emergent semantic interactions, and unconstrained fuzzing
repeatedly rediscovers known cliffs while giving weak coverage accounting. Pairwise covering arrays
over raw grammar "knobs" are **structure-blind**: the real interaction surface is the reified
compilation plan (ADR 0002 / `reify-compilation-plans`), whose composition nodes are exactly where
constructs meet and where emergent hazards (feeding/bleeding, order-dependence) actually arise. This
change is reframed accordingly — from pairwise-over-knobs to **tree-structured node/subtree
interaction fuzzing** over the plan DAG. (Rename target: `add-plan-interaction-coverage`; the dir
rename is batched into the repo-wide delanguaging/rename pass. Interaction coverage is a
test-coverage *evidence method* feeding the binary proven/unproven gate, ADR 0001 — never a trust
level.)

## What Changes

- Enumerate the legal composition-node-kind combinations from the reified plans (`reify-compilation-
  plans` D5: nodes are individually addressable). Prune by orthogonality proofs (parallel-
  independence / critical-pair analysis; feeding/bleeding disjointness) — proven-orthogonal pairs are
  retired, not tested.
- Fuzz each non-orthogonal composition node and its connected subtree, so a test exercises the
  actual composed behavior rather than a node's declared envelope (this is what attacks the
  emergent-n-way envelope-lossiness gap).
- Index every conformance/fuzz fixture by the plan node/subtree it exercises (fixtures map onto the
  tree). Apply covering-array minimization over **composition-types** (a constrained CIT / software-
  product-line sampling problem restricted to capability-legal co-occurrences), not over raw knobs,
  to cover legal co-occurrences absent from the authored corpus.
- Report required, covered, uncovered, orthogonality-retired, and contains-unsupported node-kind
  tuples. Seeded random fuzzing remains a secondary discovery tool; failures minimize to named
  recipes.

## Impact

Adds interaction evidence without changing production parsing. Runs against a pinned post-Stage-2
plan/ledger revision, and feeds proven-orthogonal and declared-limitation results back into the
capability registry (ADR 0001). Requires the reified plan's node-addressability (`reify-compilation-
plans`).
