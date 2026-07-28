## ADDED Requirements

### Requirement: Report successive recipe-space bounds
For an input grammar, the system SHALL deterministically report `N_syntactic`, `N_attested`,
`N_static`, and either an exact or explicitly estimated `N_feasible`, with definitions and
overflow-safe representations.

#### Scenario: Static pruning reduces a large space
- **WHEN** grammar dependencies, capability predicates, canonicalization, and family applicability eliminate candidates
- **THEN** the report includes counts before and after each pruning class and identifies the dominant reductions

#### Scenario: Exact feasible count is unavailable
- **WHEN** the budget expires before feasible-space enumeration completes
- **THEN** `N_feasible` is labeled as an estimate or bound with method, sample size, and uncertainty

### Requirement: Hard constraints come from executable grammar facts
The characterizer SHALL derive admissibility constraints from the parsed HC grammar, Plan
invariants, capability envelope, resource-safety rules, and recipe materializer contracts.

#### Scenario: Ordering dependency is present
- **WHEN** one operation consumes or repairs output produced by another
- **THEN** candidates that reverse the required order are statically inadmissible

### Requirement: Pilot measurement informs algorithm choice
The system SHALL measure pruning yield and evaluation-cost distributions on a bounded deterministic
pilot before selecting a search strategy.

#### Scenario: Pilot completes
- **WHEN** the optimizer receives a new grammar
- **THEN** it records pilot size, seed, elapsed cost, pruning ratio, and measured evaluation quantiles

