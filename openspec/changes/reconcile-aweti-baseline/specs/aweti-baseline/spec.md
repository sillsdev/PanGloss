## ADDED Requirements

### Requirement: Aweti evidence uses an honest pinned baseline
Aweti correctness and performance evidence SHALL name the grammar hash, code commit, rule-support manifest, denominator, and exact analysis manifest.

The existing `32/104` value SHALL be labeled a word-level any-analysis-reachability floor until the
exact manifest is established; it SHALL NOT be labeled corpus recall. Historical `68/104` SHALL be
labeled non-comparable because it used different rule support.

#### Scenario: Historic result used with different rule support
- **WHEN** a report references the old 68/104 result after unsupported rules are skipped
- **THEN** validation rejects it as a mismatched baseline

### Requirement: Correctness and timing use the same network
The Aweti gate and trace harness SHALL use one shared constructor and SHALL report the same network fingerprint.

#### Scenario: Harness measures a different network
- **WHEN** fingerprints differ
- **THEN** the timing report is invalid
