## 1. Node/subtree coverage model

- [ ] 1.1 Enumerate legal composition-node-kind combinations from the reified plans (per-grammar,
      restricted to capability-legal co-occurrences)
- [ ] 1.2 Prune by orthogonality proofs (parallel-independence / critical-pair; feeding/bleeding
      disjointness) — record retired pairs as evidence, not as tested cases
- [ ] 1.3 Generate deterministic covering arrays over composition-types with stable case identifiers
- [ ] 1.4 Tag every conformance/fuzz fixture with the plan node/subtree it exercises; emit a t-wise
      coverage report (required, covered, uncovered, retired, contains-unsupported) and fail on
      uncovered required node-kind tuples

## 2. Gates

- [ ] 2.1 Generate full-HC oracle witnesses per fuzzed node/subtree and require complete analysis
      containment
- [ ] 2.2 Assert every enabled node/subtree interaction changes or constrains at least one witness
- [ ] 2.3 Separate unsupported, resource-breached, truncated, and semantic-failure outcomes

## 3. Fuzz and minimize

- [ ] 3.1 Add seeded subtree fuzzing under hard resource supervision (secondary discovery tool)
- [ ] 3.2 Minimize each new failure to a stable named recipe and regression test

## 4. Feedback into the registry

- [ ] 4.1 Feed proven-orthogonal results and declared pairwise-only limitations back into the
      capability registry (ADR 0001)
