## ADDED Requirements

### Requirement: Reference full mode is HC-XML-only
Reference HermitCrab parity SHALL accept only HC XML grammars and SHALL reject `.json` and `.fwdata`
before starting the C# tool.

#### Scenario: Full mode receives fwdata
- **WHEN** `-Full` is requested for a `.fwdata` grammar
- **THEN** the command fails clearly as unsupported and does not present Rust-only output as full evidence

### Requirement: C# execution uses its real wrapper shape
The system SHALL invoke HermitCrab.Tool by loading the XML with `-i` and executing a temporary
script with `-s`; the script SHALL contain the `gloss-batch` command.

#### Scenario: Reference batch runs
- **WHEN** full mode is requested for XML
- **THEN** execution has the form `dotnet hc.dll -i grammar.xml -s script.txt`

### Requirement: C# analysis-batch emits structured machine-delta evidence
The C# tool SHALL provide `analysis-batch` over a caller-supplied word list. It SHALL call the public
`Morpher.AnalyzeWord` surface and emit the coverage contract's canonical semantic identities using
stable XML `id` keys, root position, and category/POS. Per word it SHALL distinguish complete,
incomplete, skipped, and failed outcomes; sort and deduplicate semantic identities; and retain
pre-dedup discovery counts separately.

#### Scenario: One semantic analysis is discovered 24 times
- **WHEN** C# completes analysis of the word
- **THEN** `analysis-batch` emits one canonical identity plus duplicate count 24

#### Scenario: Stable-key mapping is unavailable
- **WHEN** the loader cannot map a returned morpheme object to one unique XML `id`
- **THEN** that comparison is `not_comparable` with a typed mapping diagnostic

### Requirement: Structured and explanatory batches remain distinct
`analysis-batch` SHALL be the authoritative machine-delta evidence. `gloss-batch` SHALL remain
duplicate-sensitive explanatory evidence carrying gloss chain and shape. A gloss match SHALL NOT
substitute for structured semantic equality.

#### Scenario: Glosses match but root positions differ
- **WHEN** `gloss-batch` matches and `analysis-batch` differs
- **THEN** structured parity reports the semantic mismatch and retains the gloss match as diagnostics

### Requirement: Gloss signatures preserve shape and multiplicity
Each analysis SHALL contribute a tagged gloss chain paired with its boundary-inclusive surface
shape. A literal gloss SHALL be `g:<canonical-json-string>`, a missing gloss SHALL be
`m:<owning-morpheme-id-as-canonical-json-string>`, and shape SHALL be
`s:<canonical-json-string>`, where strings use RFC 8785 serialization without Unicode
normalization. `+`, `|`, and `;` SHALL act as separators only outside JSON strings. Analysis entries
SHALL sort by unsigned canonical UTF-8 bytes and compare as multisets preserving duplicate counts.

#### Scenario: Duplicate count differs
- **WHEN** Rust returns two identical gloss-chain/shape entries and C# returns one
- **THEN** parity fails even though their mathematical sets are equal

#### Scenario: Literal gloss contains signature delimiters
- **WHEN** a literal gloss contains `+`, `|`, `;`, tab, CR, LF, quote, or backslash
- **THEN** its canonical JSON string round-trips exactly and none of its contents split the signature

#### Scenario: Missing and literal values look alike
- **WHEN** one morpheme lacks gloss with ID `42` and another has the literal gloss `42`
- **THEN** their components are respectively `m:"42"` and `g:"42"` and never compare equal

#### Scenario: Unicode has multiple canonically equivalent sequences
- **WHEN** two gloss strings differ only by Unicode normalization form
- **THEN** their original code-point sequences remain distinct because the signature performs no normalization

#### Scenario: Word has no analyses or is skipped
- **WHEN** a word produces zero analyses or has `SKIPPED` status
- **THEN** its signature is the existing literal `-`

### Requirement: Timing uses the canonical TSV column
Reference output SHALL use the five-column adapter row and its `ms` field; no separate timing
sidecar SHALL be required.

#### Scenario: A word completes
- **WHEN** C# finishes parsing it
- **THEN** its TSV row contains idx, word, elapsed milliseconds, status, and signature

### Requirement: Reference parity remains diagnostic evidence
Gloss multiset parity SHALL NOT by itself establish analysis identity, corpus recall, construct
support, or supported-language certification.

#### Scenario: Gloss signatures match
- **WHEN** all Rust and C# gloss signatures match
- **THEN** the report records reference gloss parity but does not certify the language

#### Scenario: Only duplicate gloss-entry counts differ
- **WHEN** the duplicate-sensitive C# evidence fails but structured Rust semantic sets are equal
- **THEN** the report highlights redundant discovery behavior without claiming a semantic parity failure

### Requirement: C# reference validation is native comparison infrastructure
For an HC-XML source grammar, the native CLI/PowerShell cross-engine validation utility SHALL accept
a caller-supplied word set and MAY invoke C# HermitCrab alongside the combined and Rust-HermitCrab-
only pipelines. The report SHALL name every pipeline and distinguish semantic Rust parity from C#
reference evidence. No C# executable, invocation, or oracle interface SHALL be linked or exported in WASM.
PanGloss SHALL report evidence and availability statuses but SHALL NOT decide whether an application
publishes, rejects, or requires the C# comparison.

#### Scenario: A caller requests full HC validation
- **WHEN** the source is HC XML and the C# prerequisites are available
- **THEN** the native utility runs the word set through all requested pipelines and records C# evidence

#### Scenario: A publishing application consumes the report
- **WHEN** C# evidence is match, mismatch, incomplete, or `not_run`
- **THEN** PanGloss reports that state and its evidence without converting it into a publish/deny decision

#### Scenario: WASM analyzes the same package
- **WHEN** the package is deployed
- **THEN** WASM can run combined or Rust-HermitCrab-only analysis but exposes no C# validation path

### Requirement: C# evidence can participate in before/after grammar deltas
The native comparison infrastructure SHALL permit distinct before and after HC XML grammars and
SHALL record each source fingerprint and engine context. A fingerprint difference SHALL NOT prevent
execution; results SHALL be aligned by input word and stable structured-analysis keys.

#### Scenario: One XML grammar adds a rule
- **WHEN** both C# runs complete over the requested words
- **THEN** the report exposes analyses added, removed, or unchanged by that grammar delta

### Requirement: Trace reruns are explicit and diagnostic
The comparison utility SHALL perform no implicit retry. When the caller explicitly requests
`--rerun-deltas-with-tracing`, it SHALL first compare without tracing and then trace every unique
grammar/engine/word side participating in either a grammar delta or an engine disagreement. It SHALL
deduplicate identical runs. Trace equality SHALL remain non-authoritative diagnostic evidence.

#### Scenario: Caller requests delta tracing
- **WHEN** the initial pass finds a grammar or engine delta and the caller supplied
  `--rerun-deltas-with-tracing`
- **THEN** all participating sides are rerun once with bounded tracing and their artifacts are
  attached without changing the authoritative analysis-set comparison

#### Scenario: Trace reaches its diagnostic limit
- **WHEN** trace collection truncates but the analysis itself completes
- **THEN** the analysis remains complete and comparable while the trace records its independent
  truncation state and effective limits

### Requirement: Reports support caller-owned investigation UIs
For each semantic delta, the assessment-delta or trace-diagnostic report SHALL provide a FieldWorks investigation handoff with
the word key, baseline and candidate fingerprints, exact added/removed identities, associated stable
source-object IDs, suggested morpheme/rule trace filters, trace artifact references, and breadcrumb
completeness. Association SHALL NOT be labeled causation. PanGloss SHALL NOT launch, control, or
require FieldWorks.

#### Scenario: FieldWorks consumes comparison evidence
- **WHEN** FieldWorks receives assessment-delta or trace-diagnostic handoff records after rebuilding and testing an updated grammar
- **THEN** it has structured inputs for its own delta-review or Try-a-Word UI without PanGloss
  invoking any FieldWorks process or window
