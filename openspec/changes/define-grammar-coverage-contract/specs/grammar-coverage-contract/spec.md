## ADDED Requirements

### Requirement: Coverage ledger is exhaustive over the grammar model
The system SHALL maintain a versioned, one-time-audited ledger covering every HermitCrab semantic
variant represented by the frozen `pg-grammar/src/model.rs`, with a compiler disposition and
evidence owner for each row. Permanent source-reflection machinery is not required.

#### Scenario: A future change attempts to extend the model
- **WHEN** a change proposes a new grammar-model construct or behavior-bearing field
- **THEN** it is treated as outside the frozen-model assumption and must explicitly reopen and revise the coverage contract before merge

### Requirement: Corpus recall is analysis-level containment
Corpus recall SHALL mean every complete-oracle analysis is present after proposer-to-confirm processing, including root position and multiplicity where distinct analyses are exposed.
Exact duplicate copies of the same complete structured analysis identity SHALL be deduplicated for
semantic equality; their counts and provenance SHALL remain diagnostic evidence.

#### Scenario: Word remains parsed but loses an analysis
- **WHEN** one of a word's oracle analyses disappears while another remains
- **THEN** the corpus-recall gate fails

### Requirement: Propose broadly and confirm exactly
The FST proposer MAY return analyses that the matched Rust HermitCrab runtime rejects. The combined
pipeline SHALL retain every valid HermitCrab analysis and SHALL expose only confirmed analyses as
final results. Extra proposals are a resource-health concern rather than a correctness failure when
confirmation rejects them correctly.

#### Scenario: A safe overapproximation proposes false positives
- **WHEN** the FST proposes valid and invalid candidates for a word
- **THEN** HermitCrab removes the invalid candidates, every valid analysis remains, and proposal and
  confirmation work are recorded for health evaluation

### Requirement: Completed combined and Rust HermitCrab results are semantically equal
For the same analysis package, stems, word, and options, completed FST-propose-plus-HermitCrab-
confirm and Rust-HermitCrab-only pipelines SHALL return equal structured analysis collections.
Comparison SHALL ignore ordering, timing, traces, and serialization bytes. If either result is
incomplete, parity SHALL be `not_comparable` rather than pass or fail. It SHALL compare deduplicated
sets keyed by complete structured analysis identity and SHALL report duplicate-count differences separately.

Structured analysis identity SHALL be the versioned canonical projection of Machine
`WordAnalysis.Equals`: ordered stable morpheme identities, root-morpheme position, and category/POS.
For HC XML, comparison morpheme keys SHALL be the XML `id`/Rust `xml_key`; for LCM-derived packages,
they SHALL be the retained normalized source GUID. Category SHALL likewise resolve to its stable
source symbol ID/GUID. Optional `<MorphemeId>`, dense ordinals, HVOs, glosses, and forms SHALL NOT be
cross-engine identity keys. Missing or colliding source keys SHALL make parity `not_comparable`.
Rust's `guessed` flag SHALL be compared and reported separately for Rust-to-Rust results. Gloss,
surface shape, morpheme properties, duplicate counts, discovery order, engine-internal paths/traces,
timing, counters, prose, and serialization formatting SHALL remain outside core identity.

#### Scenario: Serialization differs but analyses are the same
- **WHEN** both completed pipelines return the same structured analyses in different orders or encodings
- **THEN** semantic parity passes

#### Scenario: One pipeline discovers the same analysis 24 times
- **WHEN** the other pipeline discovers that identical structured analysis once
- **THEN** semantic parity is unaffected and diagnostics report the duplicate count and available provenance

#### Scenario: Analyses have equal glosses but different roots or categories
- **WHEN** their ordered morphemes match but root position or category/POS differs
- **THEN** they have different semantic identities even if rendered gloss text is identical

#### Scenario: Analyses use different internal paths
- **WHEN** morphemes, root position, and category/POS are equal
- **THEN** they have the same semantic identity and path differences remain diagnostic provenance

#### Scenario: A gloss translation changes
- **WHEN** morphemes, root position, and category/POS remain equal
- **THEN** semantic identity remains equal and only gloss evidence changes

#### Scenario: Every public MorphemeId is empty
- **WHEN** distinct HC XML morphemes have unique XML `id` values
- **THEN** structured identity uses those XML keys and does not collapse the analyses

#### Scenario: A comparison source key collides
- **WHEN** two distinct morphemes resolve to the same key within one declared identity authority
- **THEN** parity is `not_comparable` with a typed collision diagnostic rather than a merged set

### Requirement: Key semantic decisions review Machine and LibLCM precedent
Before defining or changing a key grammar, analysis, generation, identity, feature, or interop
behavior, the change SHALL inspect and cite the relevant Machine and, when applicable, LibLCM source
contract. PanGloss SHALL preserve that precedent unless the change records why it is unsuitable,
the compatibility impact of divergence, and focused evidence for the replacement behavior.

#### Scenario: A change proposes removing root position from identity
- **WHEN** design review finds Machine equality, generation, and transfer consume root position
- **THEN** the proposal preserves it or documents a reviewed incompatibility with regression tests

#### Scenario: Relevant LibLCM source is unavailable
- **WHEN** the decision could affect LibLCM interop but the source cannot be inspected
- **THEN** evidence records `not_run` and the decision cannot claim LibLCM equivalence

### Requirement: Comparison context is reported rather than used as an execution gate
Every comparison side SHALL report its grammar/package fingerprint, identity authority/schema,
stem-data fingerprint, analysis options, named pipeline, effective budgets, completeness, and engine
version when available. Differing or missing context SHALL NOT prevent the requested runs. Strict
engine parity SHALL identify unexpected context differences; intentional grammar-delta mode SHALL
compare the differing sides and report those differences as part of the experiment.

#### Scenario: A caller tests an edited grammar
- **WHEN** before and after grammar fingerprints differ intentionally
- **THEN** both run and the report shows added, removed, and unchanged semantic analyses plus both contexts

#### Scenario: A context field is unavailable
- **WHEN** one engine cannot report that field
- **THEN** the field is `unknown` or `not_available` and the requested comparison still runs

### Requirement: Semantic comparison is exact evidence, not grammar-quality scoring
PanGloss SHALL report exact observed-versus-golden and before-versus-after semantic set differences:
matching, missing, unexpected, incomplete, and not-attempted. It SHALL NOT label an analysis addition
or removal linguistically better or worse and SHALL NOT collapse the diff into an unlabeled quality
or closeness score. Compiler/runtime health remains a separate objective assessment.

#### Scenario: An edited grammar adds an analysis
- **WHEN** no caller-supplied golden expectation classifies it
- **THEN** the report lists the added identity without calling the grammar better or worse

#### Scenario: Observed output differs from a golden set
- **WHEN** both are complete
- **THEN** the report lists exact missing, unexpected, and matching identities rather than inferring quality

### Requirement: Validation never mutates golden evidence
Comparison and validation SHALL treat every input golden as immutable. PanGloss MAY generate a
separate proposed golden containing completed observed sets, its full available context, and a diff
from the current golden. Adoption, replacement, or deletion of an authoritative golden SHALL require
an explicit caller action outside the validation run.

#### Scenario: Current output differs from the golden
- **WHEN** proposed-golden generation is requested
- **THEN** PanGloss writes a separate proposal and diff and leaves the input golden byte-for-byte unchanged

#### Scenario: Every observed result matches
- **WHEN** validation completes cleanly
- **THEN** no golden file is implicitly rewritten, reformatted, or re-versioned

### Requirement: Delta evidence owns breadcrumbs but not causal verdicts
When instrumentation provides provenance, comparison reports SHALL retain stable rule/construct
identities, named stages, proposal/confirmation path relationships, outcomes, and completeness or
truncation. Reports MAY state that a changed rule participated in or is associated with an added,
removed, duplicate, or rejected analysis. They SHALL NOT state that the edit caused the delta unless
an independently defined proof establishes that claim.

#### Scenario: A gained analysis traverses an edited rule
- **WHEN** its complete trace contains that rule
- **THEN** the report links the rule as participating evidence without declaring it the sole cause

#### Scenario: Breadcrumb collection is incomplete
- **WHEN** trace limits or unavailable instrumentation omit provenance
- **THEN** the report marks the breadcrumb set incomplete and does not infer missing steps

#### Scenario: One pipeline exhausts its budget
- **WHEN** the other pipeline completes
- **THEN** semantic parity is `not_comparable` and the incomplete outcome remains visible

### Requirement: Unsupported is not complete
A detected but uncompiled semantic variant SHALL be reported as honest unsupported and SHALL block supported-language certification when exercised.

#### Scenario: Honest skip is exercised
- **WHEN** a corpus requires an honestly skipped construct
- **THEN** its language status remains uncertified
