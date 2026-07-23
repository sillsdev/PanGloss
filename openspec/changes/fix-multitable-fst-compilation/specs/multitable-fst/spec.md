## ADDED Requirements

### Requirement: Rewrite compilation respects character-table ownership
Each rewrite rule and alpha-variable expansion SHALL resolve segments and features against its declared CharacterDefinitionTable.

#### Scenario: Same symbol differs between tables
- **WHEN** two strata use tables in which the same surface symbol maps to different definitions
- **THEN** each compiled rule uses its own table and proposer-to-confirm results match the oracle
