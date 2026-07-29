## ADDED Requirements

### Requirement: Structured analysis identity is a self-contained value
An analysis identity SHALL consist of ordered stable morpheme keys, the root-morpheme index, and the
stable category key, carried in the artifact as values. It SHALL NOT be a reference resolved against
a compiled model, and SHALL NOT contain compiler-assigned dense ordinals. `guessed` SHALL be
serialized on the analysis record and SHALL NOT participate in `identityDigest`. Gloss, surface
shape, properties, duplicate counts, discovery order, traces, timing, and serialization formatting
SHALL remain outside identity.

#### Scenario: A morpheme is deleted from the candidate grammar
- **WHEN** an analysis in the baseline references a morpheme the candidate no longer defines
- **THEN** that analysis is `removed` and the case is comparable, because identity carries the key
  rather than resolving it

#### Scenario: Two morphemes share a source key within one model
- **WHEN** a compiled model yields two distinct morphemes with the same stable key
- **THEN** that is an integrity error, not a comparison outcome

#### Scenario: A part of speech is added in FieldWorks
- **WHEN** a new part of speech shifts every later symbol's dense index
- **THEN** identities are unaffected, because category is carried as its stable symbol id

### Requirement: An identity profile cannot strand a caller's adjudicated corpus
The v1 identity profile SHALL be `pangloss.machine-word-analysis/v1`, declared by the suite and
recorded in every report. A later profile SHALL ship with either a total mechanical mapping from its
predecessor or an explicit statement of why none exists. `golden-diff` SHALL refuse a profile
mismatch rather than evaluating expectations written in another profile's encoding.

#### Scenario: A key-synthesis rule changes
- **WHEN** a new profile re-encodes existing identities without changing which analyses are distinct
- **THEN** it ships a total mapping, and a caller's adjudicated expectations survive the upgrade

#### Scenario: A new profile splits one identity into two
- **WHEN** no total mapping exists because the change is genuinely semantic
- **THEN** that is stated explicitly, and affected expectations require caller re-adjudication rather
  than being silently reinterpreted

### Requirement: An assessment reports atomic per-case outcomes
Every case SHALL be `complete`, `incomplete`, or `not_attempted`. An authoritative analysis set SHALL
appear only for a complete case. An incomplete case MAY carry diagnostic partial candidates, clearly
separated and never serialized as the authoritative set. Completed earlier cases SHALL remain valid
when a cumulative budget prevents later cases.

#### Scenario: A batch budget is exhausted midway
- **WHEN** cases 1-30 complete and the batch budget stops the run
- **THEN** cases 1-30 remain complete and valid, and cases 31 onward are `not_attempted`

#### Scenario: Import fails before any case runs
- **WHEN** the suite validated but compilation failed safely
- **THEN** a failed assessment artifact is emitted with every case
  `not_attempted/assessment_setup_failed`, and no per-word empty set is fabricated

### Requirement: Only deterministic budgets decide a digest-bearing outcome
Logical work budgets SHALL be the only mechanism permitted to decide a case's outcome kind in a
reproducible assessment. No default caps SHALL be invented; budgets SHALL be unbounded unless the
caller names a resource envelope, and the effective envelope SHALL be recorded. An outer safety net
that stops a case SHALL type it `wall_clock_timeout` and SHALL set `reproducible: false` on the
report.

#### Scenario: The same suite runs on two machines
- **WHEN** no wall-clock stop occurred in either run
- **THEN** both reports carry the same `outcomeDigest` and the same `semanticDigest`

#### Scenario: A watchdog stops one case
- **WHEN** an outer safety net decides a case rather than a logical budget
- **THEN** the case is `incomplete/wall_clock_timeout` and the report records `reproducible: false`
  rather than presenting a machine-dependent outcome as reproducible

### Requirement: Three digests answer three distinct questions
`reportId` SHALL cover the whole canonical artifact. `semanticDigest` SHALL cover the run, including
duplicate counts, effective budgets, pipeline, importer and compiler versions, model fingerprint,
and source hash. `outcomeDigest` SHALL cover behavior only: suite digest, per-case outcome kind, and
deduplicated identity sets. Each digest's preimage SHALL include its projection name and version.
Digests SHALL be computed over expanded, deduplicated, `identityDigest`-sorted analyses.

#### Scenario: PanGloss is upgraded and no analysis changes
- **WHEN** the same suite is reassessed against the same grammar with a newer compiler
- **THEN** `outcomeDigest` is unchanged while `semanticDigest` and `reportId` differ

#### Scenario: An FST change removes redundant proposal paths
- **WHEN** duplicate counts fall but every analysis identity is retained
- **THEN** `outcomeDigest` is unchanged, `semanticDigest` differs, and the delta reports a
  duplicate-count flag rather than any addition or removal

#### Scenario: Two reports serialize the same analyses differently
- **WHEN** analysis order or key-table order differs but the identity sets are equal
- **THEN** both digests are equal

#### Scenario: The same grammar is assessed on Windows and on Linux
- **WHEN** checkout line endings differ but the compiled model is identical
- **THEN** `sourceSha256` and `reportId` differ while `semanticDigest` and `outcomeDigest` are equal,
  because run identity is carried by `modelFingerprint` rather than by bytes on disk

### Requirement: Comparison is exact evidence, never a quality judgment
`compare` SHALL match cases by exact `caseId`, following declared `supersedes` links. It SHALL
categorize each case as `unchanged`, `added_only`, `removed_only`, `mixed`, `annotation_changed`,
`completeness_changed`, `baseline_only`, `candidate_only`, or `not_comparable`. It SHALL NOT label an
addition an improvement or a removal a regression, SHALL NOT emit a quality score or a `better`
verdict, and SHALL NOT assert that a grammar edit caused a change merely because it participated.
Every `not_comparable` SHALL carry a typed reason.

#### Scenario: Analyses are both added and removed
- **WHEN** the candidate gains one analysis and loses another for the same case
- **THEN** the category is `mixed`, never collapsed into a gain or a loss

#### Scenario: A root falls out of the lexicon
- **WHEN** a retained identity's `guessed` changes from false to true
- **THEN** the category is `annotation_changed` and the case counts as changed

#### Scenario: Identity profiles are incompatible
- **WHEN** baseline and candidate declare different identity profiles
- **THEN** a valid delta artifact is produced with every case
  `not_comparable/identity_profile_changed` and the command exits `0`

### Requirement: Expectations are evaluated only where the caller adjudicated them
`golden-diff` SHALL evaluate `required`, `forbidden`, `allowed`, and `closedWorld` only for complete
outcomes and only for `adjudicated` expectations. Incomplete and not-attempted outcomes SHALL be
`not_evaluable` with their typed execution outcome. Missing, `unresolved`, and `out_of_scope`
expectations SHALL be `not_adjudicated`. Aggregates SHALL retain denominators. The suite SHALL never
be modified.

#### Scenario: A form is declared ungrammatical
- **WHEN** an expectation sets `closedWorld` with empty required and allowed sets
- **THEN** agreement requires a complete empty analysis set

#### Scenario: A case timed out
- **WHEN** a case with a closed-world empty expectation is incomplete
- **THEN** it is `not_evaluable` and never counted as agreeing with the empty set

#### Scenario: An unknown analysis appears under open-world policy
- **WHEN** an observed identity is mentioned by neither required nor allowed and `closedWorld` is
  false
- **THEN** it is not `unexpected` and agreement is unaffected

### Requirement: Investigation supplies binding and attribution, never a root cause
`investigate` SHALL verify that the report, model fingerprint, case, input, pipeline, and options
agree before emitting evidence. It SHALL label evidence `retained`, `regenerated`, or `unavailable`
and record which engine and pipeline produced it. It SHALL emit stable source IDs where they exist
and mark compiler-assigned references as such. It SHALL NOT claim a root cause or prescribe a
grammar edit.

#### Scenario: An analysis is missing because the proposer never offered it
- **WHEN** HermitCrab alone produces an analysis that `foma-confirm` did not
- **THEN** the handoff attributes the absence to a proposer recall gap rather than to the grammar

#### Scenario: The assessment ran on the foma pipeline
- **WHEN** evidence is regenerated on a different pipeline than the one assessed
- **THEN** the handoff records the producing pipeline and does not present the evidence as captured
  during the original assessment

#### Scenario: A morphological rule participates in a changed analysis
- **WHEN** the handoff references that rule
- **THEN** it is marked `compilerAssigned` and explicitly not a FieldWorks source identity

### Requirement: The caller owns storage and expectation lifecycle
PanGloss SHALL write artifacts to stdout unless a path is named, and SHALL overwrite a named path
freely. It SHALL NOT write to `.fwdata`, update an input expectation file, bless current output,
create or transition an expectation status, interpret or reanchor caller `sourceReferences`, or
contact a network service.

#### Scenario: A caller reruns an assessment over an existing artifact
- **WHEN** the destination already exists
- **THEN** it is overwritten, because guarding a baseline against the caller's own scripts is the
  caller's responsibility

#### Scenario: A caller supplies redacted source references
- **WHEN** references are omitted or redacted before input
- **THEN** the report records that they were redacted and carries the remainder exactly as supplied
