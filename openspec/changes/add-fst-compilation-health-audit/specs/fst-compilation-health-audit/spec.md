## ADDED Requirements

### Requirement: Rust produces one canonical FST health audit
Rust SHALL produce the canonical FST health findings and admission result consumed by CLI,
FieldWorks, AI tooling, diagnostics, and artifact publication. No second implementation SHALL
recalculate those measurements or severities.

#### Scenario: AI tooling requests compiler health
- **WHEN** it invokes or reads the audit
- **THEN** it receives the same canonical codes, evidence, and admission used by package publication

### Requirement: Preflight covers the frozen grammar model
Preflight SHALL visit every represented construct variant and SHALL report its disposition, cost
inputs, bounded interactions, and unknown/unbounded work before calling foma.

#### Scenario: A represented variant lacks a cost model
- **WHEN** preflight reaches that variant
- **THEN** it emits a typed cost-uncertainty finding, does not assume zero cost, and permits a
  recall-preserving compilation attempt inside the resource envelope

#### Scenario: A represented variant lacks a recall-preserving disposition
- **WHEN** preflight cannot guarantee that compilation and confirmation retain every analysis
- **THEN** it rejects compilation with a typed semantic finding

### Requirement: Observed audit reuses owned measurements
Observed health SHALL consume values from budget and compile-profile events and SHALL NOT independently
remeasure or derive competing values for the same metric.

#### Scenario: Profile reports final FST bytes
- **WHEN** health evaluates size severity
- **THEN** it uses that exact schema-versioned field

#### Scenario: HermitCrab filters an overapproximated proposal set
- **WHEN** final analyses are correct after many proposed candidates are rejected
- **THEN** the audit preserves proposal count, confirmation count, rejection share, and confirmation
  work as first-class evidence rather than treating the run as healthy solely because it is correct

#### Scenario: One word exhausts its confirmation budget
- **WHEN** other words in the same batch complete
- **THEN** the audit records the word as incomplete with partial diagnostic measurements, preserves
  the completed words, and never classifies partial analyses as final results

#### Scenario: A batch stops before later words begin
- **WHEN** its cumulative application budget is exhausted
- **THEN** the audit distinguishes complete and incomplete words from not-attempted words and reports
  the effective per-word and batch limits

### Requirement: Health audits provide actionable compiler remedies
Each remediable finding SHALL list deterministic, ranked actions tied to its measured cause and SHALL
state semantic caveats for actions such as rule reordering or constraint changes.

#### Scenario: Alternatives multiply across two rules
- **WHEN** the product crosses a warning threshold
- **THEN** the audit reports both factors and suggests only applicable constraints, decomposition, or
  linguistically equivalent ordering remedies

### Requirement: Artifact publication consumes admission
Warning and below SHALL publish normally; Error SHALL publish only with its explicit recorded
override; Critical, incomplete, truncated, and watchdog-terminated results SHALL not publish.

#### Scenario: Error override publishes a package
- **WHEN** the caller explicitly overrides an Error result
- **THEN** the package embeds the original finding and override record unchanged
