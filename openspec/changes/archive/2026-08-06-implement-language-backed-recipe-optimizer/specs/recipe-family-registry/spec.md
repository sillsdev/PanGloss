## ADDED Requirements

### Requirement: Versioned extensible recipe registry
The system SHALL load a schema-versioned registry whose recipe families declare stable identifiers,
parameters, applicability predicates, ordering constraints, materialization strategy, and evidence
provenance. Adding a family SHALL NOT require changing the optimizer's search or ranking core.

#### Scenario: Add a compatible family
- **WHEN** a valid family definition and materializer are registered
- **THEN** the optimizer includes applicable instances of that family without modifying its core algorithm

#### Scenario: Reject an invalid family
- **WHEN** a family has an unknown schema version, dangling parameter reference, or no materializer
- **THEN** registry loading fails with a typed diagnostic before search starts

### Requirement: Linguistic evidence is a prior, not a correctness claim
The registry SHALL distinguish attested-construction provenance from grammar-derived hard
constraints. Actual-language documentation MUST NOT by itself admit a recipe, exclude a
grammar-supported recipe, or certify conformance.

#### Scenario: Research prior conflicts with grammar facts
- **WHEN** a family is well attested but violates the input grammar's hard dependencies
- **THEN** the candidate is pruned and the report attributes the pruning to grammar constraints

### Requirement: Seed realizable family coverage
The initial registry SHALL represent ordered morphophonology, class/exception-partitioned cascades,
complete-template alternatives, specialized morphology branches, hybrid bounded/unbounded-copy
branches, bounded metathesis cascades, and layered morphology.

#### Scenario: Inspect the initial registry
- **WHEN** the registry is serialized for diagnostics
- **THEN** all seven seeded family categories and their provenance are present

