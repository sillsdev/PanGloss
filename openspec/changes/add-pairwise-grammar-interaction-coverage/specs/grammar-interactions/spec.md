## ADDED Requirements

### Requirement: Legal plan-node-kind interactions receive coverage

The generator SHALL enumerate the capability-legal composition-node-kind tuples for the pinned
plans, prune those proven orthogonal (recording them as retired evidence), and produce a
deterministic covering array over the remaining composition-types. It SHALL publish required,
covered, uncovered, retired, and contains-unsupported tuples, and SHALL fail non-vacuously on any
uncovered required tuple.

#### Scenario: Required node-kind tuple is absent

- **WHEN** a capability-legal, non-orthogonal node-kind tuple has no fixture tagged as exercising it
- **THEN** the interaction gate fails non-vacuously and names the missing tuple

#### Scenario: Orthogonal pair is retired, not tested

- **WHEN** two branches are proven orthogonal (parallel-independence / feeding-bleeding disjoint)
- **THEN** their pair is recorded as retired evidence and no fuzz case is generated for it

### Requirement: Fixtures are indexed by the plan subtree they exercise

Every conformance and fuzz fixture SHALL be tagged with the plan node/subtree it exercises so that
t-wise coverage over composition-types can be computed and reported.

#### Scenario: Coverage report over composition-types

- **WHEN** the coverage report is generated
- **THEN** it accounts for each fixture by the node/subtree it exercises and lists node-kind tuples
  with zero covering fixtures

### Requirement: Fuzz failures become reproducible recipes

Any seeded subtree-fuzz failure SHALL be minimized and persisted with its seed, envelope, oracle
evidence, and typed outcome before it is considered triaged.

#### Scenario: Fuzzed subtree loses an analysis

- **WHEN** subtree fuzzing finds corpus-recall loss
- **THEN** a deterministic minimized recipe is added before the failure is considered triaged
