## ADDED Requirements

### Requirement: Bounded regular quantifiers compile exactly
Optional and explicitly bounded regular quantifier variants SHALL compile with oracle-equivalent
matching and environment behavior under the effective logical build budget.

#### Scenario: Optional singleton is exercised
- **WHEN** the oracle permits zero or one occurrence
- **THEN** proposer-to-confirm results include both valid cases and exclude a second occurrence

### Requirement: Unbounded semantics are not replaced by a cutoff
An unbounded or non-regular quantifier SHALL remain honestly unsupported unless an exact regular
construction is separately proven; a finite enumeration cutoff SHALL NOT be labeled equivalent.

#### Scenario: Expansion would exceed its preflight budget
- **WHEN** a bounded combination crosses the effective work limit
- **THEN** compilation returns a typed budget outcome before materializing the expansion
