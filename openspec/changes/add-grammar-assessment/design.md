## Context

Three documents describe overlapping territory. `define-grammar-coverage-contract` owns the
semantics of neutral identity-based comparison in normative language and has no code.
`add-grammar-diagnostics` owns the comparison work in deferred tasks 2.5-2.11 and has landed
`diagnose` plus its build/assessment report split. `docs/grammar-assessment-handoff-spec.md`
supplies the artifact and CLI contract and is the only one of the three with caller-owned case
identity.

The substrate is in better shape than the handoff spec's own reuse list assumes in some places and
worse in others. Morphemes already resolve to stable source keys through `MorphemeInfo.xml_key` —
MSA GUIDs on the LibLCM path, `id` attributes on HC XML. Parts of speech likewise carry stable
symbol ids. The `TraceManager` port is substantially complete, with a full `FailureReason` taxonomy.
A budgeted foma apply path with a typed incomplete outcome already exists. Against that: importer
warnings are untyped strings, no in-memory model fingerprint exists, no canonical JSON exists,
`not_attempted` exists in the glossary but not in code, and stable source IDs survive import only
for lexical entries.

## Prior art: FieldWorks already ships a diffable parser report

FieldWorks has a Check Parser feature that saves a report and diffs two of them
(`FieldWorks/Src/LexText/ParserCore/ParserReport.cs`). It is the closest existing thing to `assess`
plus `compare`, it is in real use, and reading it settles three decisions below that were otherwise
argued from first principles. It is also thinner than its name suggests, in ways worth stating
plainly.

`ParseReport` (`ParserReport.cs:317-360`) carries, per word: the word, parse time, an error message,
and four **scalar counts** — `NumAnalyses`, `NumUserApprovedAnalysesMissing`,
`NumUserDisapprovedAnalyses`, `NumUserNoOpinionAnalyses`. It does not serialize *which* analyses were
produced. Reports are written to `ProjectReports/<Guid.NewGuid()>.json` (`ParserReport.cs:179-185`),
identified to a human by a `(ProjectName, SourceText, Timestamp, MachineName)` tuple, with no schema
version and no content hash.

Three consequences follow, each of which this change is designed around:

**Comparison by count subtraction can report "no change" when everything changed.**
`DiffParseReport` (`ParserReport.cs:418-442`) computes `NumAnalyses - oldReport.NumAnalyses`. A word
that produced two analyses before an edit and two entirely different analyses after diffs to `0`.
That is a silent wrong answer, which is the worst output an evidence system can produce, and it is
unavoidable once the artifact stores counts instead of sets. It is the concrete justification for D1
(identity as a self-contained value) and for the delta categories in `compare`: the artifact has to
carry identities or the comparison cannot be correct in principle.

**The join key is the surface string.** `ParseReports` is an `IDictionary<string, ParseReport>` keyed
by the vernacular word (`ParserReport.cs:127-129`), and `DiffParserReports` joins on that key
(`ParserReport.cs:212-233`). So two questions about the same form cannot coexist, and nothing can
express "this case replaces that one." This is the real-world instance of the problem caller-owned
`caseId` and `supersedes` exist to solve, and it is why a case is a question rather than a word.

**A truncated parse is recorded as an ordinary count.** On `ReachedMaxAnalyses` or
`ReachedMaxBufferSize`, `XAmpleParser.ProcessParseResults` (`XAmpleParser.cs:183-228`) sets a prose
`errorMessage` and then **still** builds the analysis list from whatever the parser emitted, so
`NumAnalyses` records a truncated count with nothing marking it non-authoritative. A budget stop is
therefore indistinguishable downstream from a genuine result. D5 and the atomic
`complete`/`incomplete`/`not_attempted` outcome are fixing an observed defect here, not guarding a
hypothetical one.

Two things FieldWorks does that we should not mistake for gaps:

- **It ships no quality score.** No percentage, no better/worse verdict, no gain/loss framing — only
  absolute counters, with positive values tinted red in the grid
  (`ParserReportDialog.xaml:56-95`). Declining to compute a score withdraws nothing users have.
- **Its drill-downs re-derive from the live model.** "Show Analyses" and "Reparse"
  (`ParserReportDialog.xaml.cs:60-94`) open the *current* wordform and the *current* grammar, not
  what the report saw — necessarily, since the report never captured it. This is what D9's
  `retained`/`regenerated`/`unavailable` labeling is for.

**The user-opinion tally is the expectation algebra, computed live.** FieldWorks already classifies
each word's analyses against the user's stored `IWfiAnalysis` opinions three ways
(`ParserReport.cs:376-411`, via `ParseResult.MatchesIWfiAnalysis`, `ParseResult.cs:102-133`):
approved but not produced, produced but disapproved, and produced with no opinion. That maps onto
this change's algebra almost exactly — `missingRequired`, `observedForbidden`, and
`not_adjudicated` — which is convergent evidence that the required/forbidden/allowed split and the
`adjudicated`/`unresolved` lifecycle are the right shape, since the two were designed independently.
The difference is that FieldWorks evaluates the opinions against the live cache and keeps only the
counts, so the judgment cannot be replayed, audited, or compared across machines.

That correspondence names a migration nobody owns: a caller holding years of FLEx approved and
disapproved opinions should be able to turn them into an assessment suite. Under §3.2 the suite is
caller-owned, so PanGloss does not author one — but the conversion is now a **named** non-goal
rather than an unmentioned one, because a caller will expect to reuse that corpus and should not
discover its absence by inference.

## Goals

- One caller-facing contract with one owner of the wire format.
- Evidence that stays meaningful after the grammar that produced it has changed or become
  unloadable.
- Deterministic outcomes, so a digest means something.
- Honest artifacts that never overstate what PanGloss knows.

## Non-Goals

- Deciding whether a grammar is better. No score, no verdict, no causal claim.
- Competing with FieldWorks on trace presentation.
- Retaining rule/stratum/template source GUIDs through import (named follow-up).
- Tracing on the foma pipeline (named follow-up).
- Converting a caller's existing FLEx approved/disapproved `IWfiAnalysis` opinions into an
  assessment suite. The suite is caller-owned (§3.2), so PanGloss validates and executes one but
  never authors it — including from FieldWorks' own user-opinion corpus, which is the closest
  existing analog to this change's expectation algebra (see prior art above).

## Decisions

**D1 — Structured analysis identity is a self-contained value.** An identity is an ordered list of
stable morpheme keys, a root-morpheme index, and a stable category key, carried in the report as
strings. It is never a reference resolved against a compiled model. Consequence: a morpheme present
in baseline and absent from candidate yields `removed`, not a comparison failure. See ADR 0006.

**D2 — `guessed` is an annotation, not identity.** It is always serialized on the analysis record
and excluded from `identityDigest`, matching `CONTEXT.md` and the coverage contract against the
handoff spec's §6.4. A retained identity whose `guessed` flipped reports `annotation_changed`,
because `false → true` means the root stopped being found in the lexicon — a real regression that
must not be hidden as an `unchanged` case.

**D3 — Three digests over two named, independently versioned projections.** Each digest canonicalizes
the artifact and drops what is irrelevant *to its own question*; the drop-list is the question.
`reportId` drops nothing: "are these the same bytes?" `semanticDigest` drops timestamps, paths, and
timings, covering outcomes, analyses, duplicate counts, effective budgets, pipeline, importer and
compiler versions, and model fingerprint: "was this the same run?" `outcomeDigest` drops all of that
too, leaving suite digest, per-case outcome kind, and deduplicated identity sets: "did the grammar
behave the same?" Reading which digest moved localizes what changed without diffing anything. Each
projection's name and version are part of its digest preimage, so changing what a projection drops
can never silently change what its digest means.

**D3a — `sourceSha256` is recorded but not hashed into `semanticDigest`.** Run identity is carried by
`modelFingerprint` — what was actually analyzed — not by the bytes on disk. With `core.autocrlf =
true` and no `.gitattributes`, the same grammar has different bytes on Windows and Linux, so
including the source hash would make every cross-platform comparison report a source-hash context
difference forever, for a difference git invented. §17.9 already anticipates this: formatting-only
differences may move `sourceSha256` without moving `modelFingerprint`. The hash stays in `reportId`
and in the report body, and remains visible in `contextDifferences`. The cost is that
`semanticDigest` now rests entirely on `modelFingerprint`, so that fingerprint must move for any
analysis-relevant model change — a named gate in merge unit 1, not an assumption.

**D4 — Digests are computed over the expanded, deduplicated, sorted form.** Analyses are
deduplicated to a set and sorted by `identityDigest`; interned key references are expanded to their
key strings before canonicalization. Serialization order, duplicate multiplicity, and key-table
ordering therefore cannot affect any digest. Duplicate counts remain serialized evidence and
participate in `semanticDigest` only.

**D5 — Only deterministic logical budgets may decide a digest-bearing outcome.** No invented default
caps; unbounded unless the caller names a resource envelope, and the effective envelope is recorded.
A wall-clock word timeout or watchdog may still fire as an outer safety net, but any case it decides
is typed `wall_clock_timeout` and sets `reproducible: false` on the report. This applies
`CONTEXT.md`'s existing logical-budget doctrine to the digest contract: a machine-dependent outcome
kind would make `outcomeDigest` intermittently wrong, which is worse than slow.

**D6 — A key absent on one side is `added` or `removed`, never `not_comparable`.** The coverage
contract's missing-source-key rule is scoped to engine parity, where both sides run the same grammar
and a missing key really is an internal fault. Key *collision within one model* remains an integrity
error. Every `not_comparable` carries a typed reason; prose is not a reason.

**D7 — Reports intern stable keys.** A top-level key table holds each distinct morpheme and category
key once; cases reference them by index. `identityDigest` is derivable and is computed rather than
stored, while remaining accepted on the CLI for analysis selection. This takes a 50,000-case suite
from roughly 60-70MB to roughly 9-12MB, and the key table doubles as a diffable inventory of the
model's morphemes and categories.

**D8 — The caller owns storage.** Artifacts go to stdout unless `--report` names a path, and
`--report` overwrites freely. There is no existence check, no retry flag, and no content-addressed
artifact sink; diagnostics stay inline. PanGloss derives no paths of its own. Guarding a caller's
baseline against its own scripts is the caller's responsibility.

**D9 — `investigate` supplies binding and cause attribution, not trace presentation.** FieldWorks
has its own HermitCrab and its own trace UI; what it cannot do is bind evidence to a specific
PanGloss report, model fingerprint, and case. The handoff carries that binding, lexical-entry source
GUIDs, identities, completeness, and truncation. Rule, stratum, and template references are marked
`compilerAssigned` rather than presented as source identities.

**D10 — The failure narrative is a distinct rendering for AI consumers.** A trace tree is a poor
input for a model. `investigate` additionally emits a pruned prose explanation built from the
existing `FailureReason` taxonomy: which candidate parses were attempted, where each died, and why.

**D11 — `investigate` attributes a missing analysis to a cause class.** A missing analysis under
`foma-confirm` is either a HermitCrab rejection (a grammar fact) or a proposer recall gap (a PanGloss
defect). Since the operation re-runs one case anyway, it runs both pipelines and reports which. A
narrative that conflated these would send a reviewer to edit a correct grammar.

**D12 — `sourceSha256` hashes exact file bytes.** It must not reuse
`pg_lexicon::grammar_source_fingerprint`, which normalizes CRLF before hashing and would silently
make a Windows-authored source and its Linux CI copy hash alike. `modelFingerprint` is separate and
covers the compiled model.

**D14 — One assessment artifact exists in the repo.** `pg_cli::diagnostics::AssessmentReport` is
retired and `diagnose` emits `pangloss.assessment-report/v1`, keeping its own `build.json`. Two
artifacts describing word outcomes against a compiled model would be the second path §2 forbids, and
they would drift. To keep diagnose's ergonomics, `assess` accepts a bare word list and synthesizes
deterministic case IDs from position and surface form; authoring a suite is required only when the
caller wants stable identity across runs.

Landing this also deletes a workaround. `assess_words` compiles the grammar to foma twice today —
once inside `FomaAnalyzer` and once as a standalone `FomaProposer` — solely because `FomaAnalyzer`
exposes no budgeted entry point and `composite.rs` was a hotspot the change declined to open
(`diagnostics.rs:178-192`). Merge unit 3 adds that entry point, so the second compiled network goes
away.

**D13 — `--pipeline foma-confirm|hermitcrab`, defaulting to `foma-confirm`.** This replaces today's
`--engine default|foma` and inverts today's default. An unavailable pipeline returns
`unsupported_capability`; there is no silent fallback.

## Dependencies and Ownership

This change exclusively owns the five assessment artifact schemas, the structured analysis identity
type, and the four operations. It amends `define-grammar-coverage-contract`'s missing-key rule and
retires `add-grammar-diagnostics` tasks 2.5-2.11; `diagnose` and the build report stay with that
change. It consumes `harden-foma-resource-safety`'s `ApplyBudget` rather than inventing containment,
and `add-capability-characteristics-check`'s capability vocabulary rather than redefining it.
`add-reference-hermitcrab-parity` retains the C# oracle lane; nothing here executes C#.
`certify-language-readiness` and `run-synthetic-conformance-matrix` consume these artifacts and must
be updated when unit 3 lands. `composite.rs` is held for merge unit 3 only.

## Risks

- Duplicate counts participate in `semanticDigest` and must be verified deterministic under parallel
  batch execution before unit 3 lands; if they are not, they move out of the projection.
- RFC 8785 is greenfield and every digest guarantee rests on it. It needs numeric edge-case
  conformance fixtures, not only happy-path tests.
- Interning makes hand-reading a report require a lookup. Accepted for a roughly sixfold size
  reduction.
- An unbounded default means a pathological grammar can run long. Accepted; the alternative is
  silently capping recall with uncalibrated numbers.
- The failure narrative is the one artifact that interprets rather than reports. It must state
  failure reasons and attribution as observed facts and never prescribe a grammar edit.
