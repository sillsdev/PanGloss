## ADDED Requirements

### Requirement: Metathesis preserves HermitCrab switch semantics
Proven metathesis variants SHALL compile as an oracle-equivalent swap relation preserving switch
identity, direction, environments, boundaries, feature classes, and character-table ownership.

#### Scenario: A boundary moves through the switched region
- **WHEN** HermitCrab places a boundary within the resulting surface shape
- **THEN** the proposer relation preserves that placement and confirmation returns the oracle analysis

### Requirement: Metathesis is not ordinary iterative replacement
The compiler SHALL NOT reuse an iterative ordinary-replacement algorithm where it changes the
original metathesis switch set.

#### Scenario: A newly swapped substring would match again
- **WHEN** iterative replacement would rescan output created by the swap
- **THEN** the compiled relation uses only the oracle's original switch semantics
