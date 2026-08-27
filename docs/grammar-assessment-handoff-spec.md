# PanGloss grammar-assessment handoff specification

Status: implementation handoff for the PanGloss repository.

Date: 2026-07-28.

## 1. Purpose

PanGloss provides the deterministic evidence-consumption layer for a grammar-change workflow:

1. consume immutable, machine-readable assessment reports;
2. compare those reports by structured analysis identity;
3. compare a report with caller-supplied expected-analysis policy;
4. produce a report-only investigation handoff on demand.

The grammar/corpus report producer is outside this CLI's retained surface. The compare, golden-diff,
and report-only investigate consumers remain supported; no replacement producer route is specified
during demolition.

PanGloss reports facts. It does not decide whether a grammar is linguistically better, approve a
proposal, mutate FieldWorks, persist review workflow, call an AI model, or impersonate a native
speaker.

This specification supersedes `PanGloss/docs/verify-cli-plan.md` where that plan classifies every
added analysis as a gain and every removed analysis as a regression. More analyses are not
necessarily better, and fewer analyses are not necessarily worse. Quality requires comparison with
explicit required, forbidden, allowed, exact, or unresolved expectations.

## 2. Retained PanGloss capabilities

Reuse the existing assessment wire schemas, canonical artifact digests, structured analysis identity,
report parsing, report comparison, golden-diff evaluation, and report-only investigation handoff
APIs. The retained CLI consumes caller-owned artifacts; it does not import or compile a grammar,
execute a corpus, or produce a replacement assessment report route.

## 3. Normative boundaries

### 3.1 PanGloss owns

- assessment artifact schema and identity semantics;
- exact report-to-report semantic deltas;
- exact observed-to-expected deltas;
- report-bound investigation handoffs;
- content fingerprints and diagnostics already present in caller-owned artifacts.

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
| Assessment report | Immutable artifact describing one completed assessment run |
| Grammar delta | Exact relationship between two assessment reports |
| Golden-set diff | Exact relationship between a completed observed set and caller-supplied expectations |
| Structured analysis identity | PanGloss's versioned canonical semantic identity, not display output |
| Complete outcome | A complete authoritative analysis set recorded in a report |
| Incomplete outcome | Analysis began but a typed limit or failure prevented a complete set |
| Not-attempted outcome | The batch stopped before this case began |
| Context difference | A recorded difference in report provenance or execution metadata |
| Investigation handoff | Machine-readable factual evidence for FieldWorks or another diagnostic client |

## 5. Required public operations

Exact command spelling may follow existing `pg-cli` conventions, but the following public
capabilities are required and must also be available through an internal Rust API suitable for the
PanGloss SDK.

### 5.1 Compare two assessment reports

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

### 5.2 Compare an assessment with expectations

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

A producer may create the assessment report separately; the report and golden-set-diff remain
logically distinct immutable artifacts.

### 5.3 Report-only investigation handoff

The retained CLI operation is:

```text
pangloss investigate <assessment.json> --case <case-id> --report <handoff.json>
```

It reads the selected report and emits a handoff bound to that report and case. It does not accept
a grammar path, rerun a case, select a pipeline, regenerate traces, or attribute a missing analysis.
The handoff can carry only evidence already represented by the report and explicit unavailable
status; it never claims a root cause or prescribes a grammar edit.

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

- A `complete` result contains the entire authoritative analysis set recorded in the report.
- An `incomplete` result does not authorize consumers to infer an empty analysis set.
- A `not_attempted` result contains no authoritative analyses.
- Comparison and golden-diff preserve the outcome kind and typed reason rather than collapsing it.

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

Different execution metadata, source hashes, compiler versions, or identity profiles are listed in
`contextDifferences`. Comparison still runs when identities are compatible. Incompatible identity
profiles produce `not_comparable`, not an inferred conversion.

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

## 10. Stored report diagnostics

Reports may contain importer/compiler warnings, skipped or unsupported construct IDs, stale-reference
diagnostics, capability metadata, model fingerprints, resource metadata, and completeness
transitions. Comparison exposes differences in this metadata without changing compatible identity
comparison. The retained CLI does not create new import or compile diagnostics.

## 11. Exit behavior

Retained CLI exit codes describe whether the requested evidence operation completed, not whether
the grammar is good: success, invalid input/schema, unsupported capability or incompatible identity
profile, and internal error. CI must interpret the golden-set-diff artifact when it needs a policy
gate; comparison itself does not make a publication decision.

## 12. Determinism and limits

- Case order is caller-authoritative.
- Analysis ordering in serialized output is deterministic but semantically irrelevant.
- Comparisons use sets of structured identities, never discovery order.
- Duplicate evidence is retained separately.
- All string normalization follows the existing PanGloss snapshot/import contracts.
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

## 15. Required consumer tests

### 15.1 Identity and comparison

- producer discovery order does not change semantic comparison;
- gloss-only differences do not change structured identity;
- changed morpheme, root position, or category/POS changes identity;
- baseline removed and candidate added simultaneously reports `mixed`;
- a case present on only one side is not silently dropped;
- incompatible identity profiles report `not_comparable`.

### 15.2 Golden policy

- required present agrees and required missing disagrees;
- forbidden absent agrees and forbidden observed disagrees;
- allowed observed agrees;
- unknown observed follows open-world/closed-world policy;
- unresolved cases are `not_adjudicated`;
- incomplete cases are `not_evaluable`.

### 15.3 Report-only investigation handoff

- handoff binds the exact report and case;
- stable FieldWorks source IDs survive when present in the report;
- unavailable evidence is labeled unavailable rather than regenerated;
- no field makes a root-cause or automatic-repair claim.

## 16. Retained CLI scope

- `compare` consumes two assessment reports and emits a structured grammar delta.
- `golden-diff` consumes a report and suite expectations and emits a structured policy diff.
- Report-only `investigate` consumes a report and case ID and emits a bound handoff.
- Assessment-suite and assessment-report schemas remain wire-format references for artifact producers
  and consumers.
- CLI acceptance coverage for retained consumers, including strict rejection of removed flags, is
  deferred to the post-demolition replacement/repair phase. Producer-coupled tests are not restored.

## 17. Normative v1 resolutions

This section closes representation and execution choices that the preceding semantic sections leave
illustrative. If wording above is ambiguous, this section governs v1.

### 17.1 Retained operation boundary

The CLI no longer exposes a grammar/corpus assessment producer or a pipeline-selection flag.
`compare`, `golden-diff`, and report-only `investigate` operate on caller-owned artifacts. The
`foma-confirm` and `hermitcrab` values may remain in stored report/handoff schema data for wire
compatibility, but this CLI does not choose between them or rerun either pipeline.

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
- Only `adjudicated` expectations are evaluated for agreement.
- Missing expectations, `unresolved`, and `out_of_scope` produce `not_adjudicated` in golden diff.
- `invalid` cases not explicitly executed produce `not_evaluable` with their not-attempted reason.

### 17.5 Golden-diff policy binding

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

### 17.7 Stored assessment status

Consumers preserve the report's top-level `status` and nullable typed `failure`. They must not
infer a successful empty analysis set from a failed or incomplete report. Producer-side setup,
containment, and publication behavior is outside this retained CLI contract.

### 17.8 Exit codes

For retained evidence operations:

- `0` — requested artifact was validly produced;
- `2` — invalid input or schema;
- `3` — unsupported capability or incompatible identity profile;
- `70` — internal error.

### 17.9 Report identity rules

For existing reports, `sourceSha256`, `modelFingerprint`, and `identityDigest` retain the
distinctions defined by the v1 wire schema. The identity profile remains caller-declared and
comparisons with incompatible profiles are `not_comparable`; this retained CLI does not accept
grammar sources or compile models.

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

The retained `investigate` operation is report-only. It reads the requested report and case and
emits a handoff bound to that report; it does not accept `--grammar` or `--grammar-source`, rerun
a case, regenerate traces, select a pipeline, or perform cause attribution. Evidence unavailable
in the report remains explicitly unavailable.

### 17.13 Final operation rules

- Comparing incompatible identity profiles still produces a valid
  `not_comparable/identity_profile_changed` grammar-delta artifact and normally exits `0`.
- `golden-diff` evaluates expectations represented by the supplied suite; it does not mutate the
  suite or create a new assessment run.
- Report-only `investigate` does not rerun, regenerate, or attribute; it emits only a handoff bound
  to the selected report and case.
- `--report` writes the requested retained-consumer artifact through the shared output path.

## 18. Definition of done

The retained PanGloss surface is complete when an external caller can:

1. compare baseline and candidate assessment reports by structured identity;
2. obtain an exact expected-versus-observed golden diff;
3. identify incomplete, skipped, unsupported, or importer-affected cases from report data;
4. request a report-only handoff for a selected case;
5. verify artifact identity and provenance;
6. make its own human or AI review decision without reading PanGloss console prose.

The grammar/corpus report producer and any replacement/repair acceptance route are intentionally
deferred until after demolition. Old producer-coupled tests must not be restored.

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
