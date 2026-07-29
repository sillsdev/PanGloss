# PanGloss grammar-assessment handoff specification

Status: implementation handoff for the PanGloss repository.

Date: 2026-07-28.

## 1. Purpose

PanGloss must provide the deterministic execution and evidence layer for a grammar-change workflow:

1. import and compile an explicitly identified baseline grammar;
2. import and compile an explicitly identified candidate grammar;
3. run the same caller-supplied assessment suite against each;
4. emit immutable, machine-readable assessment reports;
5. compare those reports by structured analysis identity;
6. compare either report with caller-supplied expected-analysis policy;
7. produce trace and FieldWorks-investigation artifacts on demand.

PanGloss reports facts. It does not decide whether a grammar is linguistically better, approve a
proposal, mutate FieldWorks, persist review workflow, call an AI model, or impersonate a native
speaker.

This specification supersedes `PanGloss/docs/verify-cli-plan.md` where that plan classifies every
added analysis as a gain and every removed analysis as a regression. More analyses are not
necessarily better, and fewer analyses are not necessarily worse. Quality requires comparison with
explicit required, forbidden, allowed, exact, or unresolved expectations.

## 2. Existing PanGloss capabilities to reuse

Do not build a second path around existing PanGloss behavior. Reuse:

- `.fwdata` streaming import through `pg-fwdata`;
- versioned `pg-snapshot` JSON;
- grammar compilation through `pg-grammar`;
- the FST-propose plus Rust-HermitCrab-confirm pipeline;
- the Rust-HermitCrab-only pipeline;
- structured analysis identity already defined in `CONTEXT.md`;
- atomic complete/incomplete/not-attempted word outcomes;
- import and compiler-health diagnostics;
- tracing, diagnosis, breadcrumbs, and stable FieldWorks source IDs;
- current resource envelopes, logical budgets, watchdog behavior, and capability reporting;
- existing CLI and SDK packaging.

The new work composes these capabilities into stable assessment artifacts and comparison operations.

## 3. Normative boundaries

### 3.1 PanGloss owns

- grammar-source import and import warnings;
- compiled-model identity;
- parser execution;
- structured analysis identity;
- atomic per-case execution outcomes;
- immutable build and assessment reports;
- exact report-to-report semantic deltas;
- exact observed-to-expected deltas;
- parser/compiler diagnostics, traces, and factual breadcrumbs;
- content fingerprints for all PanGloss-owned inputs and outputs.

### 3.2 The caller owns

- which grammar revisions are baseline and candidate;
- the assessment suite and its lifecycle;
- case enrollment, retirement, tags, and release policy;
- whether an expectation is linguistically correct;
- review, approval, rejection, and supersession;
- human, AI, and native-speaker identity;
- storage and synchronization of review history;
- applying accepted changes to FieldWorks;
- the final judgment “better,” “worse,” or “accept.”

### 3.3 PanGloss must never

- write to `.fwdata`;
- update an input expectation file;
- silently “bless” current output;
- collapse incomplete execution into an empty analysis set;
- compare analyses by gloss strings, display text, serialization order, or trace text;
- emit a scalar grammar-quality score;
- emit a `better: true|false` verdict;
- assert that a rule or grammar edit caused a changed analysis merely because it participated;
- treat parser output as gold policy;
- launch or control FieldWorks;
- call an AI provider.

## 4. Terminology

Use the terminology already established in `PanGloss/CONTEXT.md`.

| Term | Meaning in this specification |
|---|---|
| Grammar source | `.fwdata`, `pg-snapshot` JSON, or another source accepted by PanGloss build tooling |
| Compiled model | Immutable in-memory model or `.pgpack` identified by a PanGloss model fingerprint |
| Assessment suite | Caller-supplied, versioned cases and optional executable expectations |
| Assessment case | Stable caller-issued case ID, input form, metadata, and optional expected-analysis policy |
| Assessment report | Immutable result of one suite against one compiled model and named pipeline |
| Grammar delta | Exact relationship between two assessment reports |
| Golden-set diff | Exact relationship between a completed observed set and caller-supplied expectations |
| Structured analysis identity | PanGloss's versioned canonical semantic identity, not display output |
| Complete outcome | The complete confirmed analysis set under the selected pipeline and budgets |
| Incomplete outcome | Analysis began but a typed limit or failure prevented a complete set |
| Not-attempted outcome | The batch stopped before this case began |
| Context difference | A recorded difference in model, engine, importer, options, budgets, or suite inputs |
| Investigation handoff | Machine-readable factual evidence for FieldWorks or another diagnostic client |

## 5. Required public operations

Exact command spelling may follow existing `pg-cli` conventions, but the following public
capabilities are required and must also be available through an internal Rust API suitable for the
PanGloss SDK.

### 5.1 Assess one model

Illustrative CLI:

```text
pangloss assess <grammar-source> <suite.json> --report <assessment.json>
  [--engine foma|hermitcrab]
  [--resource-envelope <name>]
  [--word-timeout-ms <n>]
  [--batch-budget <...>]
```

Behavior:

1. Hash the exact grammar-source bytes.
2. Import and compile using the requested pipeline and options.
3. Retain all importer and compiler diagnostics.
4. Validate the suite before running any case.
5. Execute cases in declared order.
6. Emit one atomic outcome per case.
7. Write an immutable assessment report.
8. Never modify the grammar source or suite.

### 5.2 Compare two assessment reports

Illustrative CLI:

```text
pangloss compare <baseline-assessment.json> <candidate-assessment.json>
  --report <delta.json>
```

Behavior:

- Match cases only by exact `caseId`.
- Preserve suite order from the caller-declared comparison basis.
- Report cases present on only one side.
- Compare complete outcomes by structured analysis identity.
- Report completeness transitions separately from semantic deltas.
- Report context differences without refusing comparison.
- Never label additions as improvements or removals as regressions.

### 5.3 Compare an assessment with expectations

Illustrative CLI:

```text
pangloss golden-diff <assessment.json> <suite.json>
  --report <golden-diff.json>
```

Behavior:

- Match by exact `caseId`.
- Evaluate expectations only for complete outcomes.
- For incomplete or not-attempted outcomes, report `not_evaluable` plus the typed execution outcome.
- Produce missing-required, observed-forbidden, unexpected, matching, and allowed identities.
- Never update the suite.

This operation may be performed automatically by `assess` when the suite contains expectations, but
the assessment report and golden-set-diff report remain logically distinct immutable artifacts.

### 5.4 Generate a trace or investigation handoff

Illustrative CLI:

```text
pangloss investigate <assessment.json> --case <case-id>
  [--analysis <identity-digest>]
  --report <handoff.json>
  [--trace <trace.json>]
```

Behavior:

- Verify that the assessment report, model fingerprint, case, input, pipeline, and options agree.
- Re-run only when necessary and state whether evidence was retained or regenerated.
- Emit stable source IDs, analysis identities, relevant breadcrumbs, trace references, completeness,
  and truncation.
- Suggest trace filters only as factual navigation aids.
- Never claim a root cause or prescribe a grammar edit.

## 6. Assessment-suite schema

Use versioned JSON. Field names below are normative; representation details may be refined before
implementation as long as the semantics and compatibility tests remain.

```json
{
  "schema": "pangloss.assessment-suite",
  "schemaVersion": 1,
  "suiteId": "sena-regression",
  "suiteRevision": "01J...",
  "analysisIdentityProfile": "machine-word-analysis-v1",
  "metadata": {
    "title": "Sena curated morphology regression suite"
  },
  "cases": [
    {
      "caseId": "01JCASE...",
      "input": "word",
      "languageTag": "seh",
      "tags": ["regression", "noun"],
      "sourceReferences": [
        {
          "kind": "fieldworks-occurrence",
          "value": {
            "projectId": "opaque",
            "textGuid": "opaque",
            "paragraphGuid": "opaque",
            "segmentGuid": "opaque",
            "analysisIndex": 3
          }
        }
      ],
      "expectation": {
        "status": "adjudicated",
        "closedWorld": false,
        "required": [],
        "forbidden": [],
        "allowed": []
      }
    }
  ]
}
```

### 6.1 Schema rules

- `suiteId`, `suiteRevision`, and every `caseId` are opaque caller-owned strings.
- `caseId` is unique within a suite.
- Input order is authoritative and retained in reports.
- Duplicate surface forms are allowed and remain distinct cases.
- PanGloss treats `sourceReferences` as opaque metadata except for validating JSON shape and size.
- Unknown optional metadata is preserved when practical but never affects analysis identity.
- An unsupported `schemaVersion` is a typed validation failure, not best-effort execution.
- The suite semantic digest is SHA-256 over a documented canonical JSON representation.
- File path, filesystem timestamp, and JSON object-key order do not affect the semantic digest.

### 6.2 Expectation status

`expectation.status` is one of:

- `adjudicated` — executable policy is authoritative for this suite revision;
- `unresolved` — disagreement exists; do not evaluate pass/fail policy;
- `out_of_scope` — retained for reporting but intentionally excluded from expectation evaluation;
- `invalid` — caller knows the case is unusable; do not execute unless explicitly requested.

PanGloss records these values but does not create or transition them.

### 6.3 Expectation semantics

For a complete observed analysis set `O`:

- `R` = required identities;
- `F` = forbidden identities;
- `A` = allowed alternative identities.

Report:

```text
missingRequired  = R - O
observedForbidden = O ∩ F
matchingRequired = O ∩ R
matchingAllowed  = O ∩ A
unexpected       = if closedWorld then O - (R ∪ A) else ∅
```

An adjudicated case agrees with policy exactly when:

```text
missingRequired is empty
and observedForbidden is empty
and unexpected is empty
```

Rules:

- `required`, `forbidden`, and `allowed` must be pairwise disjoint.
- With `closedWorld: true`, `required: []`, and `allowed: []`, the expected result is a complete empty
  analysis set. This is how an explicitly ungrammatical form is represented.
- With `closedWorld: false`, analyses not mentioned by policy are neither accepted nor rejected.
- Preferred analysis and reviewer preference are outside PanGloss's executable correctness policy.
- Incomplete execution never agrees with an empty expected set.

### 6.4 Analysis identity encoding

The suite must use PanGloss's versioned structured analysis identity, including:

- ordered stable morpheme identities;
- root-morpheme position;
- category/POS;
- required separately reported annotations such as `guessed`, where the selected profile requires
  them.

Every serialized identity also has a canonical `identityDigest` used for indexing and CLI selection.
The digest is not a substitute for retaining the structured identity.

Gloss, natural-language realization, display form, timing, paths, traces, discovery order, and
duplicate count are not part of semantic identity.

## 7. Assessment-report schema

Top-level fields:

```json
{
  "schema": "pangloss.assessment-report",
  "schemaVersion": 1,
  "reportId": "sha256:...",
  "semanticDigest": "sha256:...",
  "generatedAt": "2026-07-28T00:00:00Z",
  "tool": {
    "name": "PanGloss",
    "version": "...",
    "revision": "...",
    "target": "...",
    "capabilityProfile": "native-build"
  },
  "grammar": {
    "sourceKind": "fwdata",
    "sourceSha256": "...",
    "modelFingerprint": "...",
    "importerVersion": "...",
    "compilerOptions": {},
    "importDiagnostics": [],
    "buildReportRef": {}
  },
  "suite": {
    "suiteId": "...",
    "suiteRevision": "...",
    "semanticDigest": "...",
    "analysisIdentityProfile": "..."
  },
  "execution": {
    "pipeline": "foma-confirm",
    "resourceEnvelope": {},
    "options": {},
    "startedAt": "...",
    "completedAt": "...",
    "batchOutcome": "complete"
  },
  "cases": []
}
```

### 7.1 Case result

Each case result contains:

- `caseId`;
- input and relevant caller metadata;
- `outcome`: `complete`, `incomplete`, or `not_attempted`;
- typed reason for incomplete/not-attempted;
- complete, deduplicated structured analysis set when and only when complete;
- duplicate-analysis evidence;
- parser/resource counters and timings;
- warnings and factual breadcrumbs;
- expectation status copied from the suite;
- optional reference to a separately emitted golden-set diff.

### 7.2 Atomicity and completeness

- A `complete` result contains the entire confirmed analysis set.
- An `incomplete` result may contain diagnostic partial candidates, but they are clearly separated
  and never serialized in the authoritative `analyses` field.
- A `not_attempted` result contains no analyses.
- Import failure before case execution produces a build/import report and a typed failed-assessment
  result; it does not fabricate per-word empty sets.
- Completed earlier cases remain valid when a cumulative batch budget prevents later cases.

### 7.3 Report identity

Define two hashes:

- `semanticDigest` — deterministic over semantic inputs and outcomes, excluding timestamps,
  filesystem paths, hostnames, and nonsemantic timing noise;
- `reportId` — content digest of the complete canonical report artifact.

The documentation must state the canonicalization and hash algorithm. Use SHA-256. A repeated run
with identical semantic inputs and outputs must have the same `semanticDigest`; `reportId` may differ
only when explicitly nonsemantic recorded evidence differs.

## 8. Grammar-delta schema

Top-level fields include:

- baseline and candidate assessment report IDs and semantic digests;
- suite/case matching summary;
- context differences;
- aggregate counts by delta and completeness category;
- ordered per-case deltas;
- evidence completeness.

For two complete result sets `B` and `C`:

```text
retained = B ∩ C
removed  = B - C
added    = C - B
```

Per-case delta category is one of:

- `unchanged`;
- `added_only`;
- `removed_only`;
- `mixed`;
- `completeness_changed`;
- `baseline_only`;
- `candidate_only`;
- `not_comparable`.

`mixed` is required when both `added` and `removed` are nonempty. Do not hide it under gain/loss
terminology.

Completeness transitions are explicit, for example:

- `complete_to_incomplete`;
- `incomplete_to_complete`;
- `complete_to_not_attempted`;
- `incomplete_reason_changed`.

Different importer warnings, pipelines, options, budgets, compiler versions, source hashes, or
identity profiles are listed in `contextDifferences`. Comparison still runs when identities are
compatible. Incompatible identity profiles produce `not_comparable`, not an inferred conversion.

## 9. Golden-set-diff schema

For every case:

- execution outcome;
- expectation status;
- `matchingRequired`;
- `missingRequired`;
- `matchingAllowed`;
- `observedForbidden`;
- `unexpected`;
- `agreement`: `agrees`, `disagrees`, `not_evaluable`, or `not_adjudicated`;
- structured identities, not only counts;
- evidence completeness.

Aggregate counts must retain denominators:

- total cases;
- executed complete;
- incomplete;
- not attempted;
- adjudicated and evaluable;
- agrees;
- disagrees;
- unresolved;
- out of scope;
- invalid.

Never report “97% passed” without the numerator, denominator, exclusions, and incomplete count.

## 10. Build and import diagnostics

An assessment must retain:

- every `.fwdata` importer warning;
- skipped or unsupported construct IDs;
- dangling/stale-reference diagnostics;
- compiler-health findings;
- capability profile and overrides;
- model fingerprint;
- resource envelope;
- whether a `.pgpack` was loaded or compilation occurred;
- exact PanGloss version/revision.

A caller must be able to distinguish:

1. the grammar really changed parser behavior;
2. the importer dropped or skipped different data;
3. execution became incomplete;
4. the identity profile changed;
5. only resource/timing evidence changed.

## 11. Exit behavior

PanGloss process exit codes describe whether the requested evidence operation completed, not whether
the grammar is good.

Minimum categories:

- success: requested artifact was validly produced, even if it contains disagreements or incomplete
  case outcomes;
- invalid input/schema;
- unsupported capability or incompatible identity profile;
- resource containment prevented producing the requested top-level artifact;
- internal error.

If CI wants to gate on missing/forbidden analyses, it must interpret the golden-set-diff artifact or
use an explicit opt-in convenience flag whose behavior is exactly documented. The default comparison
command does not make publication policy.

## 12. Determinism and limits

- Case order is caller-authoritative.
- Analysis ordering in serialized output is deterministic but semantically irrelevant.
- Comparisons use sets of structured identities, never discovery order.
- Duplicate evidence is retained separately.
- All string normalization follows the existing PanGloss snapshot/import contracts.
- Every operation uses existing logical budgets and absolute resource ceilings.
- No automatic retry with larger budgets.
- Explicit retry creates a new report with the new resource envelope and references the earlier
  attempt; it never mutates the earlier report.
- Output, metadata, trace, and source-reference sizes have documented hard limits.

## 13. Security and privacy

- Treat `.fwdata`, suite metadata, surface forms, source references, and traces as untrusted input.
- Do not execute content from language packs or suite metadata.
- Reject path traversal in requested output/artifact paths.
- Do not place raw language data in logs unless explicitly requested.
- Permit callers to omit or redact `sourceReferences`.
- Record whether source references or context were redacted.
- Do not contact a network service.

## 14. Compatibility

- Every schema has a name and integer major `schemaVersion`.
- Readers reject unsupported major versions.
- Writers may add optional fields only when older readers can safely ignore them.
- Unknown enum values cannot silently map to a known value.
- Analysis-identity-profile version is independent of report schema version.
- Golden expectations state their identity profile explicitly.
- A profile migration is caller-owned and produces a new suite revision.

## 15. Required tests

### 15.1 Suite validation

- duplicate `caseId` rejected;
- duplicate surface forms with distinct case IDs accepted;
- overlapping required/forbidden/allowed identities rejected;
- unsupported suite version rejected;
- unresolved/out-of-scope cases retained but not adjudicated;
- closed-world empty expectation means complete empty output;
- incomplete result does not satisfy an empty expectation.

### 15.2 Identity and comparison

- engine discovery order does not change semantic comparison;
- gloss-only differences do not change structured identity;
- changed morpheme, root position, or category/POS changes identity;
- baseline removed and candidate added simultaneously reports `mixed`;
- case present on only one side is not silently dropped;
- incompatible identity profiles report `not_comparable`.

### 15.3 Completeness

- complete empty differs from incomplete;
- complete-to-incomplete is a completeness transition, not `removed_only`;
- cumulative batch exhaustion preserves completed earlier cases and marks later cases not attempted;
- explicit retry produces a new immutable report.

### 15.4 Import and provenance

- importer warnings appear in the report;
- differing import warnings appear in context differences;
- source hash and model fingerprint are distinct;
- same semantic run produces the same semantic digest;
- timestamps and paths do not change semantic digest;
- canonical report mutation changes `reportId`.

### 15.5 Golden policy

- required present agrees;
- required missing disagrees;
- forbidden absent agrees;
- forbidden observed disagrees;
- allowed observed agrees;
- unknown observed under open-world policy is not unexpected;
- unknown observed under closed-world policy is unexpected;
- unresolved case is `not_adjudicated`;
- incomplete case is `not_evaluable`.

### 15.6 Investigation handoff

- handoff links exact case, report, model, pipeline, and identity;
- stable FieldWorks source IDs survive into the handoff;
- regenerated trace is labeled regenerated;
- truncated trace is labeled incomplete;
- handoff contains no root-cause or automatic-repair claim.

### 15.7 End-to-end fixtures

At least one synthetic `.fwdata` fixture must demonstrate:

- two cases with the same surface form but different case IDs/source references;
- a required analysis newly appearing;
- a forbidden analysis newly appearing;
- a legitimate allowed alternative;
- an analysis removed while another is added;
- a complete empty analysis set;
- a word timeout;
- an importer warning;
- an on-demand trace and FieldWorks investigation handoff.

Run the fixture through both `foma-confirm` and Rust-HermitCrab-only pipelines and compare their
complete structured analysis sets under the existing parity contract.

## 16. Delivery slices

### Slice 1: immutable single-model assessment

- assessment-suite v1 parser and validator;
- `assess`;
- assessment-report v1;
- structured identity serialization;
- semantic digest;
- complete/incomplete/not-attempted tests.

### Slice 2: report comparison

- `compare`;
- grammar-delta v1;
- context differences;
- mixed and completeness transitions;
- deterministic comparison tests.

### Slice 3: executable expectations

- required/forbidden/allowed/closed-world semantics;
- `golden-diff`;
- golden-set-diff v1;
- denominator-aware aggregate reporting.

### Slice 4: diagnostic handoff

- `investigate`;
- trace artifact references;
- FieldWorks investigation-handoff v1;
- retained versus regenerated evidence.

### Slice 5: SDK stabilization

- Rust APIs for all four operations;
- CLI and SDK schema fixtures;
- compatibility documentation;
- representative Windows and Linux conformance;
- performance and output-size measurements on real projects.

## 17. Normative v1 resolutions

This section closes representation and execution choices that the preceding semantic sections leave
illustrative. If wording above is ambiguous, this section governs v1.

### 17.1 Public pipelines

Use `--pipeline`, not `--engine`:

- `foma-confirm` — FST proposal followed by Rust-HermitCrab confirmation;
- `hermitcrab` — Rust-HermitCrab-only analysis.

The default is `foma-confirm`. An unavailable pipeline returns `unsupported_capability`; there is no
silent fallback. The illustrative `--engine foma|hermitcrab` spelling above is replaced by
`--pipeline foma-confirm|hermitcrab`.

### 17.2 Canonical JSON and artifact IDs

All v1 JSON digests use SHA-256 over RFC 8785 JSON Canonicalization Scheme bytes encoded as UTF-8.
Parsers reject duplicate object keys. Artifact canonicalization performs no additional Unicode
normalization.

- `reportId` is calculated with the `reportId` field omitted from its preimage.
- `semanticDigest` is calculated from a documented semantic projection with both `reportId` and
  `semanticDigest` omitted.
- The semantic projection excludes timestamps, elapsed timings, paths, hostnames, and diagnostic
  prose. It includes source hash, model fingerprint, importer/compiler versions and semantic options,
  suite digest/profile, pipeline, effective logical budgets, outcomes, and structured analyses.
- A suite digest covers the entire canonical suite. Unknown caller metadata is preserved exactly and
  therefore participates in the suite digest.
- Unknown metadata copied into a report is excluded from the report semantic projection but included
  in `reportId`.

`generatedAt` is nonsemantic evidence. It changes `reportId`, not `semanticDigest`.

### 17.3 Closed schema deliverables

Before implementation is complete, PanGloss checks in JSON Schemas plus canonical positive and
negative fixtures for:

- `pangloss.assessment-suite/v1`;
- `pangloss.assessment-report/v1`;
- `pangloss.grammar-delta/v1`;
- `pangloss.golden-set-diff/v1`;
- `pangloss.investigation-handoff/v1`;
- typed failures, diagnostics references, trace references, resource envelopes, batch outcomes, and
  per-case outcomes.

The prose in this handoff defines semantics; those schemas close required fields, optional fields,
enums, nullability, size bounds, and representation.

### 17.4 Case execution and expectation status

- `adjudicated`, `unresolved`, `out_of_scope`, and cases with no expectation execute normally.
- `invalid` cases produce `not_attempted/case_status_invalid` by default.
- `assess --include-invalid` explicitly executes invalid cases.
- Only `adjudicated` expectations are evaluated for agreement.
- Missing expectations, `unresolved`, and `out_of_scope` produce `not_adjudicated` in golden diff.
- `invalid` cases not explicitly executed produce `not_evaluable` with their not-attempted reason.

### 17.5 Golden-diff attribution

V1 `golden-diff` requires the exact suite ID, revision, semantic digest, and identity profile recorded
in the assessment report. It does not reevaluate an old run against revised policy. A policy change
creates a new suite revision and assessment. This keeps every agreement result attributable to the
expectations present when the run was created.

### 17.6 Comparison order and comparability

Comparison output order is baseline report order followed by candidate-only cases in candidate report
order.

For one `caseId`:

1. missing on one side → `baseline_only` or `candidate_only`;
2. different input text, language tag, or identity profile → `not_comparable` with
   `case_definition_changed` or `identity_profile_changed`;
3. any outcome-kind transition → `completeness_changed`;
4. both complete → `unchanged`, `added_only`, `removed_only`, or `mixed`;
5. both incomplete or both not attempted → `not_comparable`, with reason equality/difference reported
   separately.

Changed tags, source references, or display metadata are context differences but do not prevent an
analysis-set comparison.

### 17.7 Top-level assessment failure

An assessment report has top-level `status: complete|partial|failed` and nullable typed `failure`.
When suite validation succeeded but import/compile/setup failed safely, PanGloss emits a failed
assessment artifact whose cases are `not_attempted/assessment_setup_failed`, alongside all available
build/import evidence. If containment or an internal crash prevents a trustworthy assessment
artifact, PanGloss emits only the available failure/build artifact and returns the applicable nonzero
exit code.

“Atomic case result” means an authoritative analysis set appears only for a complete case. Report
files are written to a sibling temporary file, flushed, and atomically renamed; a crash leaves either
no destination or one complete valid JSON artifact.

### 17.8 Exit codes

- `0` — requested artifact was validly produced, even when it reports disagreement or incomplete
  cases;
- `2` — invalid input or schema;
- `3` — unsupported capability or incompatible identity profile;
- `4` — resource containment prevented production of the requested top-level artifact;
- `70` — internal error.

### 17.9 Grammar-source and identity rules

V1 accepts file sources only: `.fwdata`, `pg-snapshot` `.json`, and `.pgpack`. `sourceSha256` hashes
exact file bytes. Formatting-only differences may change `sourceSha256` without changing the compiled
`modelFingerprint`; both are retained.

The v1 identity profile is `machine-word-analysis-v1`, normatively defined by the Structured analysis
identity entry in `PanGloss/CONTEXT.md` and the checked-in v1 schema. The suite-level declaration
applies to every expectation. `identityDigest` is an index only: equality is confirmed with the full
canonical structured value. Unequal structured identities with the same digest are an integrity
error.

### 17.10 Diagnostics, limits, and redaction

Diagnostics are never silently truncated. Inline diagnostics have documented byte/count limits. If
those are exceeded, PanGloss writes the complete set as a content-addressed artifact and records its
SHA-256, media type, byte length, item count, availability, and explicit `externalized`, `redacted`,
and `complete` flags in the report.

Caller-supplied `sourceReferences` remain opaque and are carried or omitted exactly as supplied;
PanGloss does not interpret or reanchor them. Stable FieldWorks grammar-object IDs emitted by the
importer/compiler are separate PanGloss breadcrumbs. Redaction occurs before input to PanGloss or is
represented as an explicit caller-supplied redacted reference.

### 17.11 Investigation availability

Full traces need not be retained in the assessment report. When regeneration is required,
`investigate` accepts `--grammar-source` or an accessible `.pgpack`, verifies its source hash/model
fingerprint, and reruns the exact case, pipeline, semantic options, and resource envelope. If source
or capability is unavailable, the handoff records `evidenceAvailability: unavailable` and contains no
invented trace. Retained and regenerated evidence have distinct artifact IDs, and regenerated evidence
is never represented as captured during the original assessment.

### 17.12 Parity fixture scope

The end-to-end fixture compares full structured analysis sets only for cases complete in both
`foma-confirm` and `hermitcrab`. Any incomplete case fails that conformance fixture rather than being
compared as an empty set. This is the existing PanGloss propose-and-confirm parity invariant, not a
new cross-product quality policy.
### 17.13 Final operation rules

- Comparing reports with incompatible identity profiles still produces a valid grammar-delta artifact:
  every affected case is `not_comparable/identity_profile_changed`, and the command exits `0`.
  Exit `3` is reserved for a requested capability/profile that PanGloss cannot load or execute, where
  the requested top-level artifact cannot be validly produced.
- Assessment `status` is `complete` when every runnable case completed; policy-skipped invalid cases
  do not make it partial. It is `partial` when at least one case completed but another runnable case
  is incomplete or not attempted because execution/batch limits intervened. It is `failed` when
  import/compile/setup prevented all runnable cases from completing. Complete and partial reports are
  valid comparison inputs and normally exit `0`.
- `--report` never overwrites an existing path. An existing destination is invalid input (exit `2`),
  even if its bytes match. Callers choose a new path or verify/reuse the existing artifact themselves.
- The CLI artifact sink defaults to a sibling directory `<report>.artifacts/sha256/<hex>` and uses
  atomic create-without-overwrite. The Rust API accepts an `ArtifactSink` abstraction returning a
  content-addressed artifact reference. Reports include the sink-relative locator as nonsemantic
  metadata plus SHA-256, media type, byte length, and item count; retrieval always verifies SHA-256.
- An `invalid` case executed with `--include-invalid` remains `not_adjudicated`; execution provides
  diagnostic evidence but does not promote invalid policy to an expectation.
- For the definition of done, a “changed case” has category `added_only`, `removed_only`, `mixed`,
  `completeness_changed`, `baseline_only`, `candidate_only`, or `not_comparable`. Context-only changes
  attached to an otherwise `unchanged` case do not make it a changed case. Investigation is required
  for any changed case that exists on the selected baseline or candidate side; it may also be invoked
  for unchanged cases.
## 18. Definition of done

PanGloss's part is complete when an external caller can:

1. provide baseline and candidate `.fwdata` files plus one versioned suite;
2. obtain two immutable assessment reports;
3. obtain an exact structural grammar delta;
4. obtain an exact expected-versus-observed diff;
5. identify every incomplete, skipped, unsupported, or importer-affected case;
6. request a trace/handoff for any changed case;
7. verify artifact identity and provenance;
8. make its own human or AI review decision without reading PanGloss console prose;
9. do all of the above without PanGloss modifying FieldWorks data, expectations, or review state.

## 19. Explicitly deferred

- automatic grammar repair;
- AI-generated explanations or judgments;
- native-speaker workflow;
- Harmony storage and synchronization;
- FieldWorks Avalonia UI;
- applying accepted grammar or occurrence-analysis changes;
- occurrence reanchoring;
- corpus governance and release approval;
- cloud artifact storage;
- a universal grammar-quality score.
