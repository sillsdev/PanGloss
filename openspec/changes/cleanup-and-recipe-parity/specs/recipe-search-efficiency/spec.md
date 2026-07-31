## ADDED Requirements

### Requirement: Provably-tying plan-rewrite families are declared, not searched
Plan-rewrite families whose transforms are proven (by the registry's own recorded evidence) to
produce minimization-identical networks on compositional topologies SHALL remain declared in the
registry with their evidence, but SHALL NOT be materialized into evaluated candidates by default.
An explicit opt-in (flag or registry override) SHALL re-enable searching them.

#### Scenario: Compositional fixture skips permutation families
- **WHEN** the optimizer runs on a fixture whose topology is compositional
- **THEN** gate-permutation, union-permutation, and partition-refinement candidates are reported
  as declared-not-searched (with the count), and only the baseline plan-composed candidate plus
  whole-grammar strategy candidates are evaluated

#### Scenario: Opt-in restores the old behavior
- **WHEN** the optimizer runs with the explicit search-all-families opt-in
- **THEN** the permutation families are materialized and evaluated as before

### Requirement: Grammar-invariant work is computed once per run
Work that depends only on the (grammar, corpus) pair — full-HC oracle ground truth and the
whole-grammar emission report — SHALL be computed at most once per optimizer run and shared
across candidate evaluations. No candidate SHALL trigger a second computation of the same
emission report within one run.

#### Scenario: Oracle parses once
- **WHEN** an optimizer run evaluates N > 1 candidates over the same corpus
- **THEN** each corpus word's oracle ground truth is computed exactly once, and every candidate's
  certification compares against that single shared result (including the corpus-wide
  capped/timed-out exclusion latch, which is decided once)

#### Scenario: Surface-probe candidate pays emission once
- **WHEN** the `surface-probe-morphology` whole-grammar candidate is evaluated
- **THEN** the tuned emitter runs exactly once for that candidate (no unconditional
  whole-grammar emission precedes the strategy dispatch)

### Requirement: Budget exhaustion banks completed results
When the elapsed budget expires mid-run, the optimizer SHALL persist the fully-evaluated
candidates measured so far (scores, certification outcomes, realized strategies) in the partial
report, and the partial report SHALL remain explicitly non-certifying with reason
`budget-exhausted`. A timeout is inconclusive evidence, never an empty analysis set and never a
certified winner.

#### Scenario: Partial report carries data
- **WHEN** a run evaluates 3 of 7 candidates before the deadline trips
- **THEN** the partial report lists those 3 candidates with their measured scores, names the 4
  unevaluated candidates, declares no winner, and exits non-zero

### Requirement: Efficiency changes do not alter measured scores
Hoisting and caching SHALL be observationally pure: for any candidate evaluated both before and
after this change on the same fixture and corpus, the deterministic score fields (states, arcs,
proposals, confirmation calls, confirmation steps) SHALL be identical.

#### Scenario: Score invariance on a fixed fixture
- **WHEN** the same candidate is evaluated with and without the shared-work path on a pinned
  fixture
- **THEN** all deterministic score fields match exactly
