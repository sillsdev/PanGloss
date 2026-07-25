## ADDED Requirements

### Requirement: Diagnose one grammar through supervised production pipelines
`pangloss diagnose` SHALL accept one parser-supported grammar and word list, run the default and
production foma pipelines under the shared resource supervisor, and emit separate schema-versioned
immutable build and assessment reports. Compilation SHALL be usable in memory without writing a
Language Pack; package serialization SHALL be an optional later operation on the successful build.

#### Scenario: Aweti is diagnosed
- **WHEN** an Aweti grammar is selected
- **THEN** all potentially unsafe compile and parse work uses the safety change's watchdog and effective resource policy, never an uncapped diagnostic Morpher

### Requirement: Build and assessment artifacts remain separate
The build report SHALL contain compilation inputs, effective compile budgets, construction outcome,
compiled-model fingerprint, FST measurements, and compiler-health findings but no word-test results.
It SHALL remain immutable if the caller later serializes that model; the Language Pack carries its
own identity and the write operation returns its path/hash separately. The assessment report SHALL
contain one compiled-model context, effective apply budgets, and atomic caller-word outcomes but no
compiler admission verdict. Build-report comparison and assessment-report comparison SHALL be
separate operations. A convenience command MAY produce both in one invocation.

#### Scenario: Caller compiles and assesses without releasing
- **WHEN** compilation succeeds, the caller supplies words, and no package output is requested
- **THEN** PanGloss assesses the in-memory production model and returns both reports without writing
  a `.pgpack`

### Requirement: Diagnostics consume coverage semantics
The report SHALL use the evidence levels, analysis identity, denominator, and Complete/Truncated
types defined by `define-grammar-coverage-contract` and SHALL NOT infer certification.

#### Scenario: Enumeration truncates
- **WHEN** a cap or timeout stops an enumeration
- **THEN** the result is diagnostic and incomplete and no exact corpus-recall claim is emitted

### Requirement: Named timing boundaries are distinct
The report SHALL distinguish load, FST traversal, decode/dedup, confirm-group construction,
restricted HC parse, result routing, total confirmation, and full-oracle time.

#### Scenario: Oracle generation dominates
- **WHEN** full-oracle generation is slow but production confirmation is fast
- **THEN** oracle time is reported separately and is not attributed to production confirm

### Requirement: Rust gloss output preserves analysis multiplicity and shape
Every completed Rust analysis SHALL produce a stable gloss-chain and surface-shape entry derived
from its structured analysis. Duplicate entries SHALL retain their counts; missing glosses SHALL use
a documented collision-free token carrying the morpheme ID.

Gloss values SHALL use the shared reference-parity encoding: RFC 8785 canonical JSON strings tagged
as `g:` for literal gloss, `m:` for missing-gloss morpheme ID, and `s:` for surface shape, without
Unicode normalization.

#### Scenario: Duplicate analyses share a gloss
- **WHEN** two analyses render the same gloss chain and shape
- **THEN** `glosses.tsv` contains two equivalent entries rather than a deduplicated set

#### Scenario: Duplicate discovery differs but semantic sets match
- **WHEN** combined analysis discovers 24 identical structured analyses and HermitCrab-only discovers one
- **THEN** semantic parity passes while the report preserves duplicate counts and contributing
  proposal/rule provenance when available for developer or AI diagnosis

### Requirement: Diagnostic instrumentation preserves behavior
With diagnostic sinks disabled, production networks and parse results SHALL remain structurally and
semantically equivalent under the named existing gates.

#### Scenario: Normal batch runs after instrumentation lands
- **WHEN** diagnostics are not requested
- **THEN** the authoritative Indonesian, Amharic, and pinned Aweti gates remain unchanged

### Requirement: Diagnostics feed but do not duplicate certification
The report schema SHALL be consumable by `run-synthetic-conformance-matrix`; this change SHALL NOT run
or publish an independent four-language certification matrix.

#### Scenario: Diagnostic implementation completes
- **WHEN** this change is verified
- **THEN** verification produces a representative single-grammar report, not a new four-language result table

### Requirement: Native diagnostics validate caller-supplied word sets across Rust pipelines
The native CLI and PowerShell wrapper SHALL accept a caller-supplied word set and run both the
combined and Rust-HermitCrab-only named pipelines under their effective budgets. For completed words
it SHALL compare structured analysis collections semantically rather than comparing output bytes.
Incomplete and not-attempted words SHALL retain those outcomes and SHALL not produce a parity verdict.
The comparison SHALL consume the coverage contract's versioned structured-analysis identity rather
than inventing a diagnostics-specific projection.

#### Scenario: A validation batch contains easy and difficult words
- **WHEN** easy words complete in both pipelines and a difficult word exhausts one budget
- **THEN** easy words receive semantic parity verdicts and the difficult word is `not_comparable`

### Requirement: Native diagnostics support intentional grammar deltas
The native comparison operation SHALL accept two canonical assessment reports even when their
fingerprints, stem data, options, engines, or budgets differ. It SHALL report each context and
per-word added, removed, unchanged, incomplete, and not-attempted semantic analyses without loading
or compiling hidden engine state and without treating context difference as a reason not to run.

#### Scenario: A developer considers a grammar edit
- **WHEN** before and after runs complete for a word
- **THEN** the report identifies exactly which semantic analyses were gained, lost, or unchanged and
  separately reports duplicate and compiler-health changes
- **AND** compiler-health differences, when requested, come from a separate build-report comparison

### Requirement: Diagnostics can diff caller-supplied golden semantic sets
The native comparison utility SHALL accept optional caller-supplied golden structured-analysis sets
and report exact per-word matching, missing, unexpected, incomplete, and not-attempted outcomes using
the shared identity schema. It SHALL keep this diff separate from compiler health and SHALL emit no
linguistic-quality verdict or aggregate closeness score.

#### Scenario: Golden and observed sets partially overlap
- **WHEN** both are complete
- **THEN** the report returns their exact intersection and each directional difference

### Requirement: Proposed goldens are separate review artifacts
The native utility MAY emit a proposed golden only to a distinct caller-selected output path. It
SHALL include identity schema, grammar/package, stems, options, pipeline, engine, completeness, and
budget context plus the exact current-to-proposed diff. It SHALL never modify the current golden.

#### Scenario: An AI evaluates a grammar change
- **WHEN** it requests a proposed golden
- **THEN** the proposal and diff are available for review without accepting the changed analyses

### Requirement: Diagnostics expose factual delta breadcrumbs
For gained, lost, duplicate, and rejected analyses, diagnostics SHALL attach available compiler-
owned stable rule/construct IDs, named stages, proposal paths, confirmation outcomes, and trace
completeness. They SHALL use participation/association language and SHALL leave causal interpretation
to the consuming developer or AI tool.

#### Scenario: Twenty-four duplicate paths share two rules
- **WHEN** provenance is available
- **THEN** the report exposes their shared and differing breadcrumbs without inventing which rule should be changed
