## ADDED Requirements

### Requirement: Certification consumes staged diagnostic evidence
The matrix SHALL consume versioned reports produced by the diagnostic, compile-profile, and
resource-safety changes and SHALL NOT independently recalculate their timing, completeness, gloss,
or resource fields.

#### Scenario: A matrix runner computes its own percentile
- **WHEN** the consumed diagnostic report already contains the versioned timing distribution
- **THEN** the matrix uses that field and rejects the competing locally derived value

### Requirement: Language certification uses common acceptance criteria
A language SHALL be certified only when its declared corpus has complete analysis-level recall, remains inside its calibrated resource envelope, and exercises no honest-unsupported construct.

#### Scenario: All words parse but one oracle analysis is missing
- **WHEN** word-level recall is complete but analysis containment fails
- **THEN** the language is not certified

### Requirement: Matrix workloads remain explicit and non-comparable by default
Each language report SHALL state its denominator, exclusions, timeout semantics, correctness unit, and pipeline, and SHALL NOT rank heterogeneous recall percentages as equivalent benchmarks.

#### Scenario: Two rows use different denominators
- **WHEN** readers view the matrix
- **THEN** the differing workload definitions appear with the results and no cross-language recall ranking is emitted
