## ADDED Requirements

### Requirement: Single diagnostic entry point over an incoming folder or a project path
The system SHALL provide one command that produces a grammar diagnostic for either every language
present under `incoming/<lang>/` or a single grammar given by path. A language directory SHALL be
recognized when it contains a grammar file (`grammar.xml`, `grammar.json`, or `grammar.fwdata`) and
a `words.txt`. Grammar loading SHALL reuse the existing extension dispatch (`.xml` legacy HermitCrab
XML, `.json` snapshot, `.fwdata` import), so the diagnostic accepts every input format the parser
accepts. The command SHALL exit non-zero and name the offending directory when a selected language
is missing its grammar or word list.

#### Scenario: Sweep every language in the incoming folder
- **WHEN** the runner is invoked with `-All` and `incoming/` contains `sena/` and `aweti/`, each with a grammar file and `words.txt`
- **THEN** the system produces a diagnostic report for both `sena` and `aweti` and no other directory

#### Scenario: Diagnose a single project by path
- **WHEN** the runner is invoked with `-Project <path-to-grammar>` and an adjacent or supplied word list
- **THEN** the system produces a diagnostic report for exactly that grammar

#### Scenario: Missing inputs fail loudly
- **WHEN** a selected language directory contains a grammar file but no `words.txt`
- **THEN** the command exits non-zero and its message names that directory and the missing file

### Requirement: Per-word timing distribution
The report SHALL include, for each engine measured, the total number of words with a wall-clock
per-word parse time and the p50, p95, p99, worst (max), and mean of those per-word times. Timing
SHALL be wall-clock per word and SHALL exclude one-time grammar compile time (reported separately,
see the compile profile). Words that hit the configured per-word timeout SHALL be reported as
timed-out and counted separately, never folded into the timing percentiles as if they had
completed.

#### Scenario: Distribution reported per engine
- **WHEN** a grammar's word list is diagnosed
- **THEN** the report contains, for both the default engine and the foma propose→confirm engine, the word count and p50/p95/p99/worst/mean per-word parse times

#### Scenario: Timed-out words are segregated
- **WHEN** a word exceeds the configured per-word timeout under an engine
- **THEN** that word is reported as timed-out for that engine and is excluded from that engine's timing percentiles

### Requirement: Per-mechanism compile profile
The report SHALL attribute compilation cost to HC mechanism categories aligned with the construct
matrix in `docs/fst-plan/synthetic-stress-grammar-plan.md`: phonological rewrite rules (subtyped as
plain, α-variable-bound, MPR/POS-gated, and Simultaneous/RightToLeft-or-metathesis-skipped), affix
templates, MPR/POS partition groups, compounding rules, strata, and character-definition tables.
For each category the report SHALL give a count, and — where the compile is performed per mechanism
(each phonological rule's own net, each α-tuple fold, per-continuation-class lexc emit) — the
compile time and an FST size metric (states and arcs, or lexc line count) contributed by that
mechanism. Category counts SHALL be derived from the loaded `Grammar` model.

#### Scenario: Every present mechanism category is counted
- **WHEN** a grammar containing rewrite rules, affix templates, and multiple strata is diagnosed
- **THEN** the compile profile lists each of those categories with its count

#### Scenario: Per-rule compile cost is attributed
- **WHEN** the foma FST is compiled
- **THEN** each phonological rule that compiles to its own net contributes a recorded compile time and own-net state/arc count in the profile

### Requirement: Compile state-explosion curve
The report SHALL record the size (states and arcs) of the composed network after each compose-fold
step of the rule cascade, so a single mechanism that multiplies the composed-net size is visible as
a spike. The per-mechanism/per-rule table SHALL be orderable by size or time contribution so the
largest contributor is identifiable without reading every row.

#### Scenario: A pathological fold shows as a spike
- **WHEN** folding one rule into the cascade increases the composed-net state count by a large multiple relative to the prior step
- **THEN** the state-explosion curve records that step's before/after size and the rule responsible is identifiable in the ordered profile

### Requirement: Word-to-gloss output
The system SHALL emit, for the diagnosed grammar, a record of every input word and its resulting
gloss(es), in a stable, machine-readable form (`glosses.tsv`). A word with multiple analyses SHALL
list each analysis's gloss; a word with no analysis SHALL be recorded as such rather than omitted.

#### Scenario: Every word appears with its gloss
- **WHEN** a grammar and its word list are diagnosed
- **THEN** `glosses.tsv` contains one entry per input word, each carrying that word's gloss(es) or an explicit no-analysis marker

### Requirement: Dual-engine measurement with optional deep propose→confirm debug
The system SHALL measure both the default `Morpher` engine and the `--engine=foma` propose→confirm
engine. When deep debug is requested (`--debug`), the report SHALL additionally record, per word on
the foma path, the number of proposed candidates, the number confirmed, and a cascade-dead-end
signal, so the report identifies the proposer-precision lever described by the `dead-end-census`
skill. Deep debug SHALL be opt-in because it adds per-word measurement overhead.

#### Scenario: Both engines measured by default
- **WHEN** a grammar is diagnosed without `--debug`
- **THEN** the report contains timing for both the default and foma engines and no per-word candidate counts

#### Scenario: Deep debug adds propose/confirm counts
- **WHEN** a grammar is diagnosed with `--debug`
- **THEN** each foma-path word additionally reports its proposed-candidate count, confirmed count, and dead-end signal

### Requirement: Optional reference HermitCrab parity run
When the full run is requested (`--full`), the system SHALL run the C# HermitCrab reference over the
same word list via a harness command in the `machine` submodule's HermitCrab tool, emit comparable
per-word gloss and timing, and report word→gloss **parity** (agreement between the Rust engine and
the C# reference) plus Rust-vs-C# comparative timing. The full run SHALL be off by default, and its
absence SHALL NOT block the Rust-only report. When `dotnet` is unavailable, `--full` SHALL fail with
a clear message rather than silently producing a Rust-only report.

#### Scenario: Full run reports parity
- **WHEN** a grammar is diagnosed with `--full` and `dotnet` is available
- **THEN** the report includes, per word, whether the Rust gloss set matches the C# HermitCrab gloss set, and the comparative per-word timing

#### Scenario: Full run without dotnet fails clearly
- **WHEN** `--full` is requested and `dotnet` is not available
- **THEN** the command fails with a message naming the missing `dotnet` dependency, and does not present a Rust-only report as if it were a full run

### Requirement: Structured and human-readable report artifacts
For each diagnosed grammar the system SHALL write a machine-readable `report.json` (grammar
metadata, word count, per-word rows, compile profile, and the timing/parity summary) and a
human-readable `report.md` rendering the per-language timing table, the compile-profile table with
the state-explosion curve, and (when present) the parity summary. The `glosses.tsv` and, under
`--debug`, a `debug.jsonl` SHALL accompany them.

#### Scenario: Both machine and human reports are produced
- **WHEN** a grammar is diagnosed
- **THEN** a `report.json` and a `report.md` are written for it, and the `report.md` contains the per-word timing distribution and the compile-profile table

### Requirement: CI guard on a synthetic fixture
The pipeline SHALL be exercised in CI against a small committed synthetic grammar produced by
`pg-grammar-gen`, so a change that breaks the diagnostic is caught. CI SHALL NOT depend on the
gitignored real-language corpora, and SHALL NOT require the C# full run.

#### Scenario: CI runs the diagnostic on a committed fixture
- **WHEN** CI runs
- **THEN** the diagnostic is executed on a committed synthetic grammar and fails the build if the diagnostic errors or its report is malformed

#### Scenario: CI needs no gitignored corpora
- **WHEN** CI runs the diagnostic guard
- **THEN** it uses only committed inputs and does not require `samples/data/` or a `--full` C# run

### Requirement: Diagnostic instrumentation preserves production behavior
The diagnostic timers and counters SHALL be confined to the diagnostic path and SHALL NOT alter the
compiled FST, the emitted lexc bytes, or any parsing result of the production engine. Existing
byte-identity and recall gates (Indonesian 97/97, Amharic parity, the Aweti gate) SHALL remain green
after this change.

#### Scenario: Byte-identity and recall gates unaffected
- **WHEN** the diagnostic instrumentation is present and the existing gate suite runs
- **THEN** the Indonesian byte-identity, Amharic parity, and Aweti recall gates pass unchanged
