## ADDED Requirements

### Requirement: Simultaneous rewriting uses one input snapshot
All matches for a simultaneous rewrite SHALL be selected against the same input representation and SHALL produce the full-HC-equivalent combined output.

#### Scenario: First replacement would affect second match iteratively
- **WHEN** an iterative implementation would create or destroy a later match
- **THEN** simultaneous compilation preserves the oracle's original match set
