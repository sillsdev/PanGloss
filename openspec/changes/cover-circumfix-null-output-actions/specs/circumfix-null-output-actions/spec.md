## ADDED Requirements

### Requirement: Discontinuous and null morphology has explicit dispositions
Every represented circumfix, null-role/truncating input, and ordered output-action variant SHALL be
compiled, peeled, confirm-only, or honestly unsupported with positive and negative oracle evidence.

#### Scenario: One morpheme emits on both sides of a stem
- **WHEN** a supported circumfix analysis is proposed
- **THEN** its two surface pieces retain one morpheme identity through HermitCrab confirmation

### Requirement: Output actions are not silently simplified
The proposer SHALL NOT discard Copy, InsertSegments, Modify, InsertContext, or input-part identity
from an ordered output-action sequence to obtain a compilable-looking rule.

#### Scenario: An action cannot be represented in the active proposer path
- **WHEN** exact compilation or bounded preexpansion is unavailable
- **THEN** the variant follows its declared non-compiled disposition and cannot satisfy compiled coverage
