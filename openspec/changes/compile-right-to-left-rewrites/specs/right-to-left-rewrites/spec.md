## ADDED Requirements

### Requirement: RightToLeft rewrite order matches HermitCrab
The FST compiler SHALL implement RightToLeft rule application with the same match ordering, environments, and boundary behavior as the full-HC oracle.

#### Scenario: Direction changes the result
- **WHEN** an iterative rule has distinct valid leftmost and rightmost applications
- **THEN** the compiled RTL relation recalls exactly the oracle-required analyses and is not compiled as LTR
