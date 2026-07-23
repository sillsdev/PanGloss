## ADDED Requirements

### Requirement: Supported semantic variants receive pairwise interaction coverage
The generator SHALL produce a deterministic covering array covering every legal required pair of supported semantic variants and SHALL publish the uncovered/skipped pairs.

#### Scenario: Required pair is absent
- **WHEN** a supported table/alpha pair has no generated case
- **THEN** the interaction gate fails non-vacuously

### Requirement: Fuzz failures become reproducible recipes
Any seeded fuzz failure SHALL be minimized and persisted with its seed, envelope, oracle evidence, and typed outcome.

#### Scenario: Random case loses an analysis
- **WHEN** fuzzing finds corpus-recall loss
- **THEN** a deterministic minimized recipe is added before the failure is considered triaged
