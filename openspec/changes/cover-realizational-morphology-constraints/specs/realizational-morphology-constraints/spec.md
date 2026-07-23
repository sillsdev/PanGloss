## ADDED Requirements

### Requirement: Morphology constraints stay at their correct boundary
Realizational features, stem names/families, blocking, maximum application, and co-occurrence
variants SHALL be individually classified. Constraints requiring full derivational state or feature
unification SHALL remain HermitCrab-confirmed rather than be approximated as exact FST semantics.

#### Scenario: An admission filter cannot prove recall preservation
- **WHEN** filtering could remove an oracle-required analysis
- **THEN** the FST overapproximates and HermitCrab applies the constraint during confirmation

### Requirement: Constraint overapproximation remains bounded
Confirm-only and overapproximated constraint paths SHALL enforce existing candidate, path, output,
and elapsed-work budgets without changing their semantic disposition.

#### Scenario: Max-application expansion exceeds its budget
- **WHEN** proposer enumeration crosses the effective logical limit
- **THEN** it returns a typed budget outcome rather than silently lowering the maximum
