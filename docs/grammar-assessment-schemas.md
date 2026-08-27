# Grammar-assessment wire schemas: a consumer reference

Audience: a downstream tool (for example LCAtom) that needs to generate client types from
PanGloss's grammar-assessment artifacts without reading the Rust crate that emits them. This
document describes the checked-in JSON Schemas exactly as written; where the schemas and
`docs/grammar-assessment-handoff-spec.md` disagree, that is called out rather than papered over
(see [Known inconsistencies](#known-inconsistencies-with-the-prose-spec)).

Status (2026-08-27): the grammar/corpus `pangloss assess` producer and grammar-backed
`investigate` rerun/attribution path are removed from the CLI. These schemas remain the wire-format
reference for existing artifacts and retained `compare`, `golden-diff`, and report-only
`investigate` consumers. The pipeline and missing-cause enum values below are retained only for
v1 artifact compatibility; they are not CLI selection or rerun behavior.

Normative semantics live in `docs/grammar-assessment-handoff-spec.md`. Decision rationale lives in
`openspec/changes/add-grammar-assessment/design.md` (D1-D15). This document is a map of the wire
format for people writing a code generator, not a restatement of either.

## Where the schemas live

All five artifact schemas and their shared definitions are checked into
`rust/crates/pg-assess/schemas/`:

| File | `$id` |
|---|---|
| `assessment-suite.schema.json` | `pangloss.assessment-suite/v1` |
| `assessment-report.schema.json` | `pangloss.assessment-report/v1` |
| `grammar-delta.schema.json` | `pangloss.grammar-delta/v1` |
| `golden-set-diff.schema.json` | `pangloss.golden-set-diff/v1` |
| `investigation-handoff.schema.json` | `pangloss.investigation-handoff/v1` |
| `common.defs.json` | `pangloss/common.defs/v1` (defs only, not an artifact) |

Each artifact file's root has a top-level `"schema"` string constant matching its `$id` without the
version suffix (e.g. `"pangloss.assessment-report"`) and a top-level integer `"schemaVersion"`.
Every emitted document carries both, so a consumer can dispatch on `schema` + `schemaVersion`
before deserializing further.

These are hand-written, not generated from the Rust types, specifically so they can disagree with
the emitter — `rust/crates/pg-assess/tests/schema_conformance.rs` validates every schema against
artifacts the real code produces (not hand-written samples) and asserts every negative fixture is
rejected at the correct field. See `rust/crates/pg-assess/schemas/README.md` for that test's
declared JSON Schema subset.

## What each artifact is for

- **`assessment-suite`** — the caller's input: a versioned, ordered list of cases (each an opaque
  `caseId`, an input string, and an optional required/forbidden/allowed expectation). It remains
  caller-owned and is consumed by artifact producers; it is never authored or transitioned by the
  retained CLI.
- **`assessment-report`** — an immutable result artifact from an assessment run: a per-case outcome
  (`complete` / `incomplete` / `not_attempted`), the authoritative analysis set for complete cases
  only, provenance, execution metadata, and three identity digests.
- **`grammar-delta`** — the exact structural difference between a baseline and a candidate
  `assessment-report`, matched by `caseId` (following declared `supersedes` links), categorized
  into a closed set of change kinds. It never labels an addition an improvement or a removal a
  regression.
- **`golden-set-diff`** — the exact difference between one assessment's observed output and the
  same suite's declared expectations: `missingRequired`, `observedForbidden`, `unexpected`, etc.,
  as structured identities, never bare counts, with denominators on every aggregate.
- **`investigation-handoff`** — a handoff for one case bound to an exact report and its recorded
  execution metadata. The v1 wire shape retains observed/missing analyses, construct references,
  narrative, and caveat fields for existing artifacts; the retained CLI supplies report-only
  handoffs and does not regenerate evidence or compute cause attribution. No field makes a
  root-cause claim or prescribes a grammar edit.

## `common.defs.json` and how `$ref` resolves against it

`common.defs.json` holds the definitions §17.3 calls out besides the five artifacts themselves:
`digest`, `identityProfile`, `stableSourceKey`, `analysisIdentity`, `budgetDimension`,
`incompleteReason`, `notAttemptedReason`, `caseOutcomeKind`, `batchOutcome`, `resourceEnvelope`,
`diagnostic`, `extensions`, and `assessmentFailure`. It is not itself a `schema`/`schemaVersion`
artifact — nothing ever names it as `"schema"`.

**This is not standard cross-file `$ref`.** Every `$ref` in every schema file — including
`common.defs.json`'s own entries and each artifact file's local `$defs` — is spelled as a bare
`"#/$defs/<name>"`, with no filename prefix and no `$id`-based resolution. Taken at face value by a
generic JSON Schema tool, a `$ref` to `"#/$defs/digest"` inside, say, `assessment-report.schema.json`
would fail to resolve, because that file has no `digest` entry of its own — it lives only in
`common.defs.json`.

The repository's own validator (`rust/crates/pg-assess/tests/schema_conformance.rs`, `Validator::new`)
resolves this by **merging `$defs` dictionaries before resolving any reference**: it starts from
`common.defs.json`'s `$defs`, then overlays the artifact schema's own local `$defs` on top (a local
name would win over a common one of the same name; in practice no such collision exists today — each
artifact's local `$defs`, e.g. `investigation-handoff.schema.json`'s `missingAnalysisCause`,
`constructRef`, `narrativeStep`, use names disjoint from `common.defs.json`'s). Only after that merge
does `"#/$defs/<name>"` resolve.

A client code generator must reproduce that merge, not treat each `.schema.json` file as
self-contained. Two practical ways to do it:

1. Preprocess: for each of the five artifact files, build a synthetic document whose `$defs` is
   `common.defs.json`'s `$defs` overlaid by the file's own `$defs`, then run a standard generator
   against that synthetic document (which now is fully self-contained under plain `#/$defs/*`
   resolution).
2. Or: load `common.defs.json` and the target artifact file into the same in-memory schema store
   under one shared `$defs` namespace before resolving any reference, exactly as the Rust validator
   does.

Every keyword actually used across all six files (`type` including `["...", "null"]`, `required`,
`properties`, `additionalProperties`, `enum`, `const`, `items`, `oneOf`, `minimum`, `minLength`,
`maxLength`, `minItems`, `maxItems`, and one literal `pattern`) is standard JSON Schema 2020-12, so
once the `$defs` merge above is handled, an off-the-shelf generator should otherwise need no
special-casing.

## The digest and versioning contract

Two independent version axes exist; do not conflate them:

- **`schemaVersion`** (integer, `const` per file — currently `1` on all five) is the wire-format
  major version of the artifact shape itself. An unsupported major version is a typed validation
  failure; a reader must reject it rather than best-effort parsing it.
- **The analysis identity profile** (`analysisIdentityProfile`, currently the fixed string
  `"pangloss.machine-word-analysis/v1"`, declared via `common.defs.json`'s `identityProfile`
  `const`) versions the *semantics and encoding* of an `analysisIdentity` value — ordered stable
  morpheme keys, root-morpheme index, stable category key (ADR 0006) — independently of
  `schemaVersion`. A suite declares it once (`assessment-suite.analysisIdentityProfile`); every
  report and comparison that consumes that suite echoes it back
  (`assessment-report.suite.analysisIdentityProfile`), and an incompatible profile on a `compare`
  produces a valid `not_comparable/identity_profile_changed` delta rather than an error. A future
  identity-profile version is a separate compatibility question from a future `schemaVersion` bump.

All digests are lowercase-hex SHA-256, algorithm-prefixed (`^sha256:[0-9a-f]{64}$`, the shared
`digest` def, `maxLength` 71) computed over [RFC 8785 JSON Canonicalization Scheme](https://www.rfc-editor.org/rfc/rfc8785)
bytes, per handoff spec §17.2. The canonicalizer used to produce them rejects duplicate JSON object
keys; a consumer that recomputes a digest independently to verify it must reject them too.

An `assessment-report` carries three separately meaningful hashes, each over a **named,
independently versioned projection** — the projection's name is folded into the digest preimage
(`{"projection": name, "value": ...}`) precisely so that two different projections over otherwise
identical JSON can never collide:

- **`reportId`** — projection `pangloss.assessment-report/v1` (the schema name plus version),
  computed over the whole serialized artifact with the `reportId` field itself absent — the only
  value it cannot contain is its own. Drops nothing else. "Are these the same bytes?" Moves on
  *any* change, including `generatedAt`, file paths, or reworded diagnostic prose.
- **`semanticDigest`** — projection `pangloss.assessment-semantic/v1`. Drops timestamps,
  paths, timings, and `sourceSha256`. "Was this the same run?" Rests entirely on `modelFingerprint`
  (see below), plus recorded execution metadata, importer/compiler versions, and outcomes/analyses.
- **`outcomeDigest`** — projection `pangloss.assessment-outcome/v1`. Drops everything
  `semanticDigest` drops, plus tool/importer/compiler versions, budgets, and pipeline. "Did the
  grammar behave the same?" Only the suite digest, per-case outcome kind, and deduplicated identity
  sets survive — `supersedes` lineage is also excluded here, since declaring one case replaces
  another changes how a comparison joins, not what the grammar did. A `grammar-delta`'s top-level
  `outcomeDigestsAgree` is a cheap equality check on this value alone, answerable without reading a
  single case.

`modelFingerprint` itself (below) is computed under its own named projection, `pangloss.model/v1`,
over the grammar source's canonicalized content plus the compiler version — never by walking the
compiled in-memory model field-by-field, since a fingerprint that forgets one of many fields would
silently under-report change.

Separately, each `identifiedAnalysis` value everywhere it appears (`grammar-delta`,
`golden-set-diff`, `assessment-report` analysis records) carries its own `identityDigest`, computed
over the *identity profile itself* as the projection name (currently
`pangloss.machine-word-analysis/v1`). It is an index for lookups and CLI/API selection only —
equality must be confirmed against the full structured `identity` value, and two unequal structured
identities sharing a digest is an integrity error, not a match.

Finally, `assessment-report.provenance` carries two hashes that answer different questions and must
not be assumed equal or substitutable:

- **`sourceSha256`** — the exact bytes of the grammar-source file, no normalization at all. Two
  checkouts of the same grammar under different line-ending conventions (e.g. Windows vs. Linux
  with `core.autocrlf`) hash differently here, correctly, because they *are* different files.
- **`modelFingerprint`** — what was actually analyzed: the source's canonical content plus the
  compiler version that turned it into a model. Formatting-only source differences may move
  `sourceSha256` without moving `modelFingerprint`. `semanticDigest` and `outcomeDigest` rest on
  `modelFingerprint`, never on `sourceSha256`.

## Closed enums a client can switch on

Every enum below is closed (`"enum": [...]`, or a tagged `"oneOf"` on `kind`) — no reference is
labeled "extensible" or documented as open to unknown values. A client can safely generate a
discriminated union / switch statement for each rather than falling back to a string type.

| Field | Values | Defined in |
|---|---|---|
| `execution.pipeline` (report), `binding.pipeline` (handoff) | retained v1 values: `foma-confirm`, `hermitcrab` | artifact files (inline) |
| per-case `outcome` (`caseOutcomeKind`) | `complete`, `incomplete`, `not_attempted` | `common.defs.json` |
| top-level report `status` (`batchOutcome`) | `complete`, `partial`, `failed` | `common.defs.json` |
| `incompleteReason.kind` | `logicalBudget`, `wallClockTimeout` (`oneOf`, each with its own required fields) | `common.defs.json` |
| `budgetDimension` | `decodedPaths`, `candidates`, `hermitcrabSteps` | `common.defs.json` |
| `notAttemptedReason.kind` | `batchBudgetExhausted`, `assessmentSetupFailed`, `caseStatusInvalid` | `common.defs.json` |
| `assessmentFailure.kind` | `assessment_setup_failed`, `unsupported_capability`, `containment_prevented`, `internal_error` | `common.defs.json` |
| `diagnostic.severity` | `warning`, `error` | `common.defs.json` |
| suite `expectation.status` | `adjudicated`, `unresolved`, `out_of_scope`, `invalid` | `assessment-suite.schema.json` |
| `deltaCategory` | `unchanged`, `added_only`, `removed_only`, `mixed`, `annotation_changed`, `completeness_changed`, `baseline_only`, `candidate_only`, `not_comparable` | `grammar-delta.schema.json` |
| `notComparableReason` | `case_definition_changed`, `identity_profile_changed`, `both_incomplete`, `both_not_attempted`, `incomparable_outcomes`, `key_collision` | `grammar-delta.schema.json` |
| golden case `verdict` | `agrees`, `disagrees`, `not_evaluable`, `not_adjudicated` | `golden-set-diff.schema.json` |
| golden `notEvaluableReason` (nullable) | `incomplete`, `not_attempted`, `null` | `golden-set-diff.schema.json` |
| golden `notAdjudicatedReason` (nullable) | `no_expectation`, `unresolved`, `out_of_scope`, `invalid`, `null` | `golden-set-diff.schema.json` |
| `constructRef.kind` | `lexicalEntry`, `morphologicalRule`, `phonologicalRule`, `stratum`, `template` | `investigation-handoff.schema.json` |
| `constructRef.idKind` (`SourceIdKind`) | `sourceId`, `compilerAssigned` | `investigation-handoff.schema.json` |
| `evidence.availability` (`EvidenceAvailability`) | `retained`, `regenerated`, `unavailable` | `investigation-handoff.schema.json` |
| `missingAnalysisCause` (retained v1 field) | `hermitcrabRejected`, `proposerRecallGap`, `neitherPipelineProduces`, `undetermined` | `investigation-handoff.schema.json` |

`analysisIdentityProfile` is a `const` (currently one fixed value, not an enum of alternatives) —
treat it as a version tag to compare for equality, not a set to switch over.

## The `additionalProperties: false` implication

Every object type in every one of the six files — every artifact root and every nested `$defs`
object, in both `common.defs.json` and the five artifact files — declares
`"additionalProperties": false` explicitly. There is exactly one deliberate exception: the
`extensions` def in `common.defs.json` (`{"type": "object"}`, no `properties` and no
`additionalProperties` restriction), which is the one sanctioned place a caller's or a future
producer's unknown namespaced data is allowed to live. It appears on `assessment-suite` (suite
level and per-case), and on `assessment-report`. `sourceReferences` on a suite case is similarly
opaque (`"items": {}`, unconstrained per-item shape, `maxItems: 64`) — validated for shape/size
only, never interpreted.

Everywhere else, closed means closed: a client's deserializer must not silently ignore a field it
does not recognize. If a client generates strict types (Rust `#[serde(deny_unknown_fields)]`,
similarly strict codegen in other languages, or literal JSON Schema validation with
`additionalProperties: false`), a payload carrying a field the client's generated version does not
know about will be **rejected outright**, not silently dropped. That is intentional for artifact
producers — `rust/crates/pg-assess/schemas/README.md` states the reasoning: a schema that tolerated
unknown fields could grow a construct nothing checks and still look green — but it means a client
must track `schemaVersion` bumps and regenerate rather than assuming forward compatibility for any
field outside the two `extensions`/`sourceReferences` escape hatches.

## Known inconsistencies with the prose spec

**Trace references (handoff spec §17.3).** §17.3 lists "trace references" among the shared
definitions to schema. No schema field distinct from `investigation-handoff`'s `evidence` object
represents a stored trace artifact, and no command implements the illustrative `--trace <trace.json>`
flag from §5.4. This is recorded as a deliberate non-goal — see design.md D15 and the note in
`rust/crates/pg-assess/schemas/README.md` — rather than an oversight, but a client should not expect
a `traceRef`-shaped field to appear in a future minor revision without a corresponding
`schemaVersion` bump or new artifact.

**Fixture layout does not match `schemas/README.md`.** The README states positive/negative fixtures
live at `fixtures/valid/*.json` and `fixtures/invalid/*.json` under `rust/crates/pg-assess/schemas/`.
As of this writing both directories exist but are empty; the actual positive and negative fixtures
are constructed programmatically inside `rust/crates/pg-assess/tests/schema_conformance.rs` (e.g.
`full_report()`, `SUITE_JSON`, and the individual `assert_rejected(...)` calls), not stored as
standalone JSON files. A client relying on the README's description to locate example payloads on
disk will not find them there; the closest thing to a canonical example payload today is what those
Rust helpers construct, or the JSON literals embedded directly in the test file's negative-fixture
assertions.
