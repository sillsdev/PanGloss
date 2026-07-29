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

**D3 — Three digests over two named, independently versioned projections.** `reportId` covers the
whole canonical artifact. `semanticDigest` covers the run: outcomes, analyses, duplicate counts,
effective budgets, pipeline, importer and compiler versions, model fingerprint, and source hash.
`outcomeDigest` covers behavior only: suite digest, per-case outcome kind, and deduplicated identity
sets. Reading which digest moved localizes what changed without diffing anything. The projection
name and version are part of each digest's preimage, so a future projection can never be silently
confused with this one.

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
