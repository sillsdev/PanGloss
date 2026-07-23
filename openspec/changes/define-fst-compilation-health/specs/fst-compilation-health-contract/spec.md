## ADDED Requirements

### Requirement: FST health findings are stable compiler diagnostics
Every FST health finding SHALL have an immutable `PGFdddd` code, severity, phase, affected
identifiers, measured or predicted value, effective thresholds, explanation, and applicable remedies.

#### Scenario: A rule creates a large alternatives product
- **WHEN** the compiler emits a finding for that product
- **THEN** the finding names the rule and factors, reports their product and thresholds, and lists
  only remedies applicable to those factors

#### Scenario: Several rules contribute to duplicate proposals
- **WHEN** compiler-owned provenance identifies their participation
- **THEN** the finding reports those breadcrumbs and measured overlap without claiming one rule caused the duplication

### Requirement: FST payload size has five admission bands
The FST payload SHALL use decimal-byte bands: Ideal through 10,000,000; Info above 10,000,000 through
20,000,000; Warning above 20,000,000 through 100,000,000; Error above 100,000,000 through
500,000,000; and Critical above 500,000,000.

#### Scenario: FST payload is exactly 100,000,000 bytes
- **WHEN** size severity is calculated
- **THEN** the result is Warning

### Requirement: Overrides are explicit and bounded
Warning and below SHALL permit normal artifact publication. Error SHALL require an explicit override
recorded in the health report and package manifest. Critical SHALL reject compilation/publication and
SHALL NOT be overridable.

#### Scenario: An Error package is explicitly published
- **WHEN** a caller supplies the Error override
- **THEN** publication succeeds and permanently records severity, metric, value, threshold, and override

### Requirement: Size is not the only health dimension
Overall admission SHALL be the worst applicable finding across payload size, estimated and actual
construction work, intermediate networks, compile time, candidates, paths, application time, and
unknown or unbounded work. Unknown cost alone SHALL NOT be Critical when compilation remains
recall-preserving: the compiler SHALL attempt it within the resource envelope and determine the
result from observed budget and size outcomes. Semantic uncertainty that could omit an analysis
SHALL fail closed.

#### Scenario: Small final FST required explosive intermediate work
- **WHEN** its final payload is Ideal but an intermediate dimension is Critical
- **THEN** overall admission is Critical

#### Scenario: Recall-preserving work has unknown growth
- **WHEN** the compiler cannot predict how large a recall-preserving construction will become
- **THEN** it reports the uncertainty, attempts construction inside the resource envelope, and stops
  with a typed resource finding only if an enforced budget is reached

#### Scenario: Unknown growth reaches a resource limit
- **WHEN** a compilation attempt reaches an enforced budget
- **THEN** its finding reports the effective envelope, reached metric, partial measurements, and
  applicable grammar improvements before suggesting an explicit larger-envelope retry

#### Scenario: A trusted bound exceeds the remaining budget
- **WHEN** an exact value or proven lower bound shows an operation cannot fit
- **THEN** compilation stops before that operation and the finding identifies the proof inputs

#### Scenario: A large value is only a heuristic estimate
- **WHEN** the estimate is not a trustworthy rejection bound
- **THEN** it may raise a finding but cannot by itself prevent an attempted budgeted compilation

#### Scenario: A lowering may omit an analysis
- **WHEN** the compiler cannot guarantee recall preservation for a represented construct
- **THEN** it rejects the lowering rather than emitting an incomplete artifact

### Requirement: Proposal and confirmation work are first-class health dimensions
Recall-preserving FST overapproximation SHALL be permitted, but candidate count, path count,
confirmation count, rejection share, and confirmation work SHALL be measured and evaluated
independently of final-result correctness and FST payload size.
Pre-dedup duplicate analysis count, duplicate ratio, and available rule/proposal-path provenance SHALL
also be first-class health evidence without being classified as extra semantic answers.

#### Scenario: A compact FST proposes excessive candidates
- **WHEN** HermitCrab produces the correct final analyses but must reject a very large proposal set
- **THEN** health findings identify proposal and confirmation work as the cause even though payload
  size and semantic correctness are acceptable

#### Scenario: Overlapping rules produce many identical analyses
- **WHEN** one word yields 24 copies of the same structured analysis before deduplication
- **THEN** the final semantic set contains one analysis and a health finding identifies the duplicate
  count, ratio, and contributing rule or proposal-path pattern when available

### Requirement: Compilation health is not grammar quality
Findings SHALL describe FST/compilation consequences and SHALL NOT score linguistic quality.

#### Scenario: Reordering might reduce an intermediate product
- **WHEN** ordering is suggested
- **THEN** the remedy states that it applies only if the orders are linguistically equivalent

#### Scenario: A grammar delta improves compiler health but changes analyses
- **WHEN** the new FST is smaller but the semantic diff contains additions or removals
- **THEN** health reports the computational improvement and the semantic diff reports the changes
  without claiming the grammar is linguistically better

### Requirement: Compiler remedies do not silently change grammar meaning
The compiler MAY automatically apply an internal transformation only when it has a compiler-owned
correctness argument that preserves the complete HermitCrab analysis set. Reordering rules, adding
constraints, or otherwise editing a grammar without that guarantee SHALL remain advice requiring an
external grammar change and a new compilation.

#### Scenario: Reordering could shrink an intermediate network
- **WHEN** equivalence of the two orders is not guaranteed
- **THEN** the compiler reports the opportunity and semantic caveat but does not reorder the grammar

#### Scenario: An internal representation is exactly equivalent
- **WHEN** the lowering has the required semantics-preservation argument and verification
- **THEN** the compiler may apply it automatically and records the transformation in profile evidence
