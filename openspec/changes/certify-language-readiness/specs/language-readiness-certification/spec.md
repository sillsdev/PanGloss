## ADDED Requirements

### Requirement: The conformance suite measures per-word timing in either engine mode
The synthetic-language conformance suite SHALL time every word it runs, SHALL be runnable in both the
complete-engine and compiled-proposer modes, and SHALL emit machine-readable per-word results plus a
rendered table.

#### Scenario: Both modes are compared over one fixture set
- **WHEN** the suite runs in both engine modes over the same fixtures
- **THEN** the output reports per-fixture latency for each mode and the speedup between them

#### Scenario: A fixture the compiled path refuses
- **WHEN** a fixture's grammar is refused under capability enforcement in the compiled mode
- **THEN** that fixture records a refusal outcome naming the refusing predicate, and is neither
  reported as a zero time nor omitted from the table

#### Scenario: Results are grouped by typology
- **WHEN** the table is rendered
- **THEN** speedup is attributable per construct/typology group, not only as a single aggregate

### Requirement: Sub-millisecond results are never reported as zero
Where the measurement path's resolution cannot distinguish a duration from zero, the report SHALL say
so rather than emit `0`.

#### Scenario: A fast fixture is timed
- **WHEN** a word completes below the measurement floor
- **THEN** the report shows a below-floor indicator and states the floor, never `0`

### Requirement: Certification produces a tiered verdict with named failures
Certification SHALL evaluate declared thresholds for pack size, lexicon scale, token analysis rate, and
p50/p90/p99 latency, and SHALL produce a tiered verdict. Every failed check SHALL be reported with its
measured value and its threshold.

#### Scenario: A grammar misses a latency threshold
- **WHEN** measured p99 exceeds the declared p99 threshold
- **THEN** the verdict is the not-yet tier, and the report names the check, the measured value, and the
  threshold

#### Scenario: A grammar contains a refused construct
- **WHEN** the grammar contains a construct the capability gate refuses
- **THEN** the verdict is the not-supported tier, and the report names the refusing predicate and the
  construct, sourced from the real capability evaluation

#### Scenario: A threshold policy version is recorded
- **WHEN** any verdict is produced
- **THEN** it records the threshold policy version that produced it

### Requirement: An override-trusted artifact is never certifiable
An artifact carrying a degraded-trust stamp from the capability override SHALL NOT receive a passing
certification under any configuration, and the report SHALL state the override as the reason.

#### Scenario: A force-compiled pack is submitted for certification
- **WHEN** certification runs against a pack stamped unproven
- **THEN** certification refuses, naming the override, and no threshold result is presented as passing

### Requirement: Held-out corpus status is an attestation, never a measurement
The certificate SHALL record held-out status as an attestation carrying an attestor and a date, and
SHALL state that it is unverified.

#### Scenario: A corpus is supplied with an attestation
- **WHEN** a coverage corpus is supplied with an attestation
- **THEN** the certificate records the attestor and date and marks the property unverified

#### Scenario: No corpus is supplied
- **WHEN** no coverage corpus is available for the language
- **THEN** the coverage check reports not-assessed, and not-assessed is never presented as passed

### Requirement: Coverage is reported as an analysis rate, not as accuracy
The coverage figure SHALL be described as the fraction of tokens receiving at least one analysis, and
SHALL NOT be worded as accuracy or correctness.

#### Scenario: A coverage figure is rendered
- **WHEN** the coverage result appears in a report
- **THEN** its wording identifies it as an analysis rate, and the report states that a token may
  receive an incorrect analysis and still count

### Requirement: Latency thresholds name their target device class
Latency thresholds and results SHALL name the device class they were measured against.

#### Scenario: Percentiles are reported
- **WHEN** latency percentiles appear in a certificate
- **THEN** the device class is named alongside them

### Requirement: A per-language report composes the evidence and states what was not tested
A single command SHALL produce a markdown report containing build time, artifact size, latency
percentiles, the compilation-plan diagram, and the conformance verdict — and SHALL state which checks
it did not perform and the pinned revisions of its inputs.

#### Scenario: A fully passing language is reported
- **WHEN** every conformance check passes and every threshold is met
- **THEN** the report states that plainly, alongside what it did not test

#### Scenario: A partially failing language is reported
- **WHEN** any conformance check or threshold fails
- **THEN** the report names each failing point specifically, never only a summary verdict

#### Scenario: The report is re-derivable
- **WHEN** a report is produced
- **THEN** it records the pinned revisions of grammar, pack, corpus, and submodule sufficient to
  re-derive it
