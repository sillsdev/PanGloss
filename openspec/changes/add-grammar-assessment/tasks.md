# Tasks

Six merge units. Ordering is forced: identity gates everything, `assess` gates `compare` and
`golden-diff`, `investigate` needs the report format. Each schema ships with the code that produces
it, so no schema is designed against imagined output.

## 1. Identity, canonical JSON, and digests

Exclusive ownership: new identity crate, `pg-parse` analysis projection. Amends
`define-grammar-coverage-contract/specs/grammar-coverage-contract/spec.md`.

- [x] 1.1 Add a structured analysis identity value: ordered stable morpheme keys, root-morpheme
      index, stable category key. No compiler-assigned ordinals in the value (ADR 0006)
- [x] 1.2 Project `pg_parse::WordAnalysis` to that identity via `MorphemeInfo.xml_key` and the
      part-of-speech symbol id; do not widen or reuse `WordAnalysis`'s derived `Eq`, which spans
      `syn_fs`/`mpr`/`provenance` and stays load-bearing for `pg_lexicon`'s `push_unique`
- [x] 1.3 Pin synthesized keys explicitly: variant entries (`{variant_guid}#{guid}`,
      `compile/lexicon.rs:668`) and null affixes (`null-affix#{guid}`, `compile/templates.rs:263`)
- [x] 1.4 Serialize `guessed` on the analysis record and exclude it from `identityDigest`
- [x] 1.5 Implement RFC 8785 JCS with numeric edge-case conformance fixtures, not only happy-path
      tests; reject duplicate object keys
- [x] 1.6 Define and version two named projections, `pangloss.assessment-semantic/v1` and
      `pangloss.assessment-outcome/v1`, with the projection name inside each digest preimage
- [x] 1.7 Compute digests over expanded, deduplicated, `identityDigest`-sorted analyses so
      serialization order, duplicate multiplicity, and key-table order cannot affect them
- [x] 1.8 Add `sourceSha256` over exact file bytes; do not reuse
      `pg_lexicon::grammar_source_fingerprint` (`runtime.rs:804-806`), which normalizes CRLF
- [x] 1.9 Add an in-memory `modelFingerprint` distinct from `sourceSha256` and from
      `PackManifest.package_fingerprint`; formatting-only source differences may move one and not
      the other. `semanticDigest` rests entirely on it, so gate it: it SHALL move for every
      analysis-relevant model change and SHALL NOT move for serialization-only differences, proven
      by paired fixtures in both directions
- [x] 1.9a Exclude `sourceSha256` from the semantic projection; keep it in `reportId`, the report
      body, and `contextDifferences` (D3a)
- [x] 1.10 Amend the coverage contract: scope its missing-source-key `not_comparable` rule to engine
      parity; state that for grammar delta an absent key is `added`/`removed`, and that key
      collision within one model remains an integrity error
- [x] 1.11 Name the v1 identity profile `pangloss.machine-word-analysis/v1`, declared by the suite
      and recorded in every report; document the rule that a later profile ships either a total
      mechanical mapping from its predecessor or a stated reason none exists
- [ ] 1.12 Verify duplicate-count determinism under parallel batch; if nondeterministic, move
      duplicate counts out of the semantic projection and record the finding
- [ ] Gate: `cargo test -p pg-parse -p pg-grammar`; JCS conformance fixtures pass; identical inputs
      reproduce `semanticDigest` and `outcomeDigest` across runs and platforms

## 2. Assessment suite schema and validator

- [x] 2.1 Add `pangloss.assessment-suite/v1` with `suiteId`, `suiteRevision`, opaque `caseId`,
      authoritative case order, and declared identity profile
- [x] 2.2 Accept duplicate surface forms as distinct cases; reject duplicate `caseId`
- [x] 2.3 Add the expectation algebra: `required`, `forbidden`, `allowed`, `closedWorld`; reject
      overlapping sets; `closedWorld` with empty required and allowed means a complete empty
      analysis set
- [x] 2.4 Record expectation status `adjudicated|unresolved|out_of_scope|invalid` without creating
      or transitioning it
- [x] 2.5 Add optional `supersedes: [caseId]` case lineage; PanGloss follows a declared link and
      never infers one
- [x] 2.6 Add the namespaced `extensions` object on suite and case; excluded from both projections,
      included in `reportId`
- [x] 2.7 Treat `sourceReferences` as opaque; validate shape and size only, carry or omit exactly as
      supplied
- [x] 2.8 Reject an unsupported `schemaVersion` as a typed validation failure, never best-effort
- [x] 2.9 Compute the suite semantic digest over the full canonical suite, unknown caller metadata
      included
- [ ] Gate: `cargo test -p pg-cli suite_`; positive and negative fixtures for every rule above

## 3. `assess` and the assessment report

Exclusive ownership: `pg-foma/src/composite.rs` for this unit only (STAGING merge hotspot,
terminal-outcome routing owner).

- [ ] 3.1 Add a budgeted production entry point on `FomaAnalyzer` returning a typed incomplete,
      reusing `ApplyBudget` and the existing `ProfiledFomaApplyOutcome::Incomplete`
      (`composite.rs:169-177,309`) without imposing profiling overhead on every word
- [ ] 3.2 Add `not_attempted` as a real outcome, completing `CONTEXT.md:203`'s contract in code
- [ ] 3.3 Add `--pipeline foma-confirm|hermitcrab` defaulting to `foma-confirm`, replacing
      `--engine default|foma` (`pg-cli/src/main.rs:102-116`); an unavailable pipeline returns
      `unsupported_capability` with no silent fallback
- [ ] 3.4 Leave logical budgets unbounded unless a resource envelope is named; record the effective
      envelope in the report
- [ ] 3.5 Type an outer-safety-net stop as `wall_clock_timeout` and set `reproducible: false` on the
      report
- [ ] 3.6 Add `pangloss.assessment-report/v1` with the interned key table, per-case outcomes,
      `reportId`, `semanticDigest`, `outcomeDigest`, and top-level `status: complete|partial|failed`
- [ ] 3.7 Emit an authoritative analysis set only for a complete case; keep partial candidates
      clearly separated and never in `analyses`
- [ ] 3.8 Retain importer and compiler diagnostics inline. Give each importer and snapshot-validation
      warning a stable short code alongside its existing prose, replacing bare `Vec<String>`
      (`pg-fwdata/src/lib.rs:52-55`, helper at `extract/mod.rs:55`, ~70 sites across `pg-fwdata` and
      `pg-snapshot`). No warning taxonomy is designed up front; `compare` diffs by code and count so
      rewording prose is never a context difference
- [ ] 3.9 Write to stdout by default; `--report` writes to a path and overwrites freely
- [ ] 3.10 Add exit codes: `0` artifact produced, `2` invalid input/schema, `3` unsupported
      capability or incompatible profile, `4` containment prevented the artifact, `70` internal
- [ ] 3.11 Emit a failed assessment artifact with `not_attempted/assessment_setup_failed` cases when
      suite validation passed but import or compile failed safely
- [ ] 3.12 Accept a bare word list as well as a suite; synthesize deterministic case IDs from
      position and surface form so a caller need not author a suite for a quick run
- [ ] 3.13 Retire `pg_cli::diagnostics::AssessmentReport` (`diagnostics.rs:167-176`); `diagnose`
      emits `pangloss.assessment-report/v1` and keeps its own `build.json`. One assessment artifact
      exists in the repo
- [ ] 3.14 Delete `assess_words`' second compiled foma network (`diagnostics.rs:178-192`): it exists
      only because `FomaAnalyzer` exposed no budgeted entry point, which task 3.1 adds. `diagnose`
      compiles the grammar once
- [ ] 3.15 Update `certify-language-readiness` and `run-synthetic-conformance-matrix` for the report
      shape they consume
- [ ] Gate: `cargo test -p pg-cli assess_`; a repeated run reproduces both digests; a timestamp or
      path change moves `reportId` only

## 4. `compare` and the grammar delta

- [ ] 4.1 Add `pangloss.grammar-delta/v1`; match cases by exact `caseId`, following declared
      `supersedes` links
- [ ] 4.2 Implement the category set: `unchanged`, `added_only`, `removed_only`, `mixed`,
      `annotation_changed`, `completeness_changed`, `baseline_only`, `candidate_only`,
      `not_comparable`
- [ ] 4.3 Report `annotation_changed` as a changed case when a retained identity's `guessed` flipped
- [ ] 4.4 Report duplicate-count and context differences as flags that do not make a case changed
- [ ] 4.5 Add a typed `not_comparable` reason enum covering `case_definition_changed`,
      `identity_profile_changed`, both-incomplete, both-not-attempted, and key collision
- [ ] 4.6 Treat a stable key absent from the other side as `added`/`removed`, never
      `not_comparable`
- [ ] 4.7 Order output as baseline report order followed by candidate-only cases in candidate order
- [ ] 4.8 Report context differences without refusing comparison; never label an addition an
      improvement or a removal a regression
- [ ] 4.9 Produce a valid artifact with every case `not_comparable/identity_profile_changed` and
      exit `0` when profiles are incompatible
- [ ] Gate: `cargo test -p pg-cli compare_`; a grammar edit deleting a morpheme yields `removed_only`,
      not `not_comparable`; engine discovery order does not change any category

## 5. `golden-diff`

- [ ] 5.1 Add `pangloss.golden-set-diff/v1` with `matchingRequired`, `missingRequired`,
      `matchingAllowed`, `observedForbidden`, `unexpected`, and structured identities, not counts
      alone
- [ ] 5.2 Evaluate expectations only for complete outcomes; report `not_evaluable` with the typed
      execution outcome otherwise
- [ ] 5.3 Evaluate agreement only for `adjudicated`; missing, `unresolved`, and `out_of_scope`
      produce `not_adjudicated`
- [ ] 5.4 Require the exact suite id, revision, semantic digest, and identity profile recorded in
      the assessment; never re-evaluate an old run against revised policy
- [ ] 5.5 Retain denominators in every aggregate: total, complete, incomplete, not attempted,
      adjudicated and evaluable, agrees, disagrees, unresolved, out of scope, invalid
- [ ] 5.6 Never update the suite
- [ ] Gate: `cargo test -p pg-cli golden_`; an incomplete case never satisfies an empty closed-world
      expectation

## 6. `investigate` and the failure narrative

- [ ] 6.1 Add `pangloss.investigation-handoff/v1`; verify report, model fingerprint, case, input,
      pipeline, and options agree before emitting anything
- [ ] 6.2 Emit lexical-entry source GUIDs from `LexEntryDef.authored_id`; mark rule, stratum, and
      template references `compilerAssigned` and explicitly not source identities
- [ ] 6.3 Label evidence `retained`, `regenerated`, or `unavailable`, and record which engine and
      pipeline produced it; never present regenerated evidence as originally captured
- [ ] 6.4 Attribute a missing analysis to HermitCrab rejection or proposer recall gap by running the
      case on both pipelines
- [ ] 6.5 Emit the pruned failure narrative from the existing `FailureReason` taxonomy
      (`pg-rules/src/trace.rs:106-123`): candidates attempted, where each died, and why
- [ ] 6.6 State that FieldWorks' C# HermitCrab traces a different engine, so a divergence there is
      not necessarily a grammar defect
- [ ] 6.7 Make no root-cause claim and prescribe no grammar edit in any artifact
- [ ] Gate: `cargo test -p pg-cli investigate_`; a synthetic proposer recall gap is attributed to the
      proposer, not to the grammar

## 7. End-to-end fixture

- [ ] 7.1 Build a synthetic `.fwdata` fixture demonstrating: two cases sharing a surface form with
      distinct case IDs; a required analysis appearing; a forbidden analysis appearing; an allowed
      alternative; an analysis removed while another is added; a complete empty analysis set; a
      logical-budget incomplete; an importer warning; and an on-demand handoff
- [ ] 7.2 Run the fixture through both pipelines and compare full structured analysis sets for cases
      complete in both; an incomplete case fails the fixture rather than comparing as empty
- [ ] 7.3 Confirm Windows and Linux produce identical `semanticDigest` and `outcomeDigest` for the
      same grammar despite differing line endings on checkout (`core.autocrlf = true`, no
      `.gitattributes`), and that `sourceSha256` and `reportId` correctly differ. Do not "fix" a
      digest mismatch here by reintroducing CRLF-normalized source hashing (D12)
- [ ] Gate: `cargo test -p pg-conformance-fixtures assessment_e2e`; synthetic data only
