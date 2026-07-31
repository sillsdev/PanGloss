## ADDED Requirements

### Requirement: Independent Grammar-to-network pipelines are gated against each other
A standing test gate SHALL compare, on pinned synthetic fixtures, the networks produced by the
independent construction pipelines (plan-composed/`build_controllable`, templated token-cascade,
and the tuned surface-probed emitter) using deterministic observables: proposal sets on a fixed
word list, and confirmed analysis sets against the full-HC oracle. Where pipelines legitimately
diverge (different proposal supersets), the gate SHALL assert the *containment* property that
every oracle-confirmed analysis is proposed by each pipeline, at analysis identity and
multiplicity level — never mere non-emptiness.

#### Scenario: Confirmed-set agreement on a shared fixture
- **WHEN** the gate runs a pinned templated fixture through two pipelines and confirms proposals
  against the oracle
- **THEN** both pipelines' confirmed analysis multisets equal the oracle's analysis multiset

#### Scenario: Mis-routing class is caught by a failing test
- **WHEN** a pipeline change causes a grammar shape to lose template-aware structure (the
  measured uflexc-on-Sena class: unbounded affix stacking proposing far beyond the oracle set)
- **THEN** the gate fails loudly with the divergence quantified (proposal-count ratio), rather
  than the divergence surfacing only in out-of-band corpus measurement

### Requirement: The gate is non-vacuous
The gate SHALL assert that it exercised at least one fixture per pipeline pair and that the
compared word lists produced at least one confirmed analysis, so an empty comparison cannot pass
silently.

#### Scenario: Empty comparison fails
- **WHEN** a fixture regression causes zero analyses to be compared
- **THEN** the gate fails with a distinct non-vacuity error
