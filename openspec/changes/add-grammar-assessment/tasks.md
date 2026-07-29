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
- [x] 1.12 Verify duplicate-count determinism under parallel batch; if nondeterministic, move
      duplicate counts out of the semantic projection and record the finding.
      **Verdict: deterministic — duplicate counts stay in the semantic projection, unchanged.**
      `analyze_words_with_threads` parallelizes only *across* words
      (`composite.rs`'s `confirm_proposed_words_in_pool`, one word per rayon task); propose runs
      sequentially on a single `&mut self.proposer` handle, and each word's own multiplicity is
      decided inside one single-threaded `confirm_batch` call that is a pure function of
      `(g, owners, morpher, candidates, word)`. `Morpher`'s only interior-mutable state is created
      fresh per call, never shared.
      Proven, not just argued, in `pg-assess/tests/duplicate_count_determinism.rs`: thread counts
      1/2/4/8 x 15 repetitions, comparing per-word duplicate-count vectors, per-word identity
      digests, and the run's `semanticDigest` against a baseline.
      Two things make that test non-vacuous, which matters more than the result:
      `dup_root_fixture_genuinely_produces_a_triple_duplicate` pins a synthetic fixture with three
      identical-shape allomorphs of one entry, giving a real `duplicate_count == 3` before threading
      enters at all — the obvious candidate fixture (`deep-optional-affix-nesting`) turns out to
      produce all-distinct identities, so it would have tested nothing, and
      `sanity_deep_optional_affix_nesting_produces_no_identity_duplicates` records that.
      `confirm_across_words_genuinely_overlaps_at_thread_count_above_one` arms a concurrency guard
      that holds for 20ms and tracks a high-water mark, so it fails unless tasks were genuinely
      inside confirm simultaneously — it establishes observed overlap, not merely a requested thread
      count.
      `pg-assess` gained a `[dev-dependencies]` entry on `pg-foma` for this; the crate's real
      dependency graph stays engine-agnostic
- [x] Gate: `rust/tools/test.ps1 -Package pg-assess` — **154/154**. The identity/JCS/digest work
      lives in `pg-assess`, not `pg-parse`/`pg-grammar`, so that is where the gate actually runs.
      JCS numeric and ordering edge cases pass (`jcs::tests`, incl. UTF-16 key ordering and float
      refusal); identical inputs reproduce both digests
      (`report::tests::a_timestamp_moves_only_the_report_id`,
      `duplicate_count_determinism::duplicate_counts_and_semantic_digest_are_thread_count_invariant`).
      Cross-*platform* reproduction is asserted by `assessment_e2e` task 7.3's test but has only been
      *run* on Windows here — the Linux half is unverified by me and stated as such

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
- [x] Gate: `rust/tools/test.ps1 -Package pg-assess` — the suite validator and its 16
      positive/negative tests are `pg-assess`'s `suite::tests::*`, not `pg-cli`'s, and the original
      `-Filter suite_` would have matched nothing. Every rule above has both a positive and a
      refusal test (duplicate case IDs, overlapping expectation sets, unsupported schemaVersion,
      foreign identity profile, oversized source references)

## 3. `assess` and the assessment report

Exclusive ownership: `pg-foma/src/composite.rs` for this unit only (STAGING merge hotspot,
terminal-outcome routing owner).

- [x] 3.1 Add a budgeted production entry point on `FomaAnalyzer` returning a typed incomplete,
      reusing `ApplyBudget` and the existing `ProfiledFomaApplyOutcome::Incomplete`
      (`composite.rs:169-177,309`) without imposing profiling overhead on every word
      (`FomaAnalyzer::analyze_word_budgeted` + `FomaProposer::propose_budgeted_counted`: the
      counters the decode loop already kept, no clock reads; gated against the diagnostic path)
- [x] 3.2 Add `not_attempted` as a real outcome, completing `CONTEXT.md:203`'s contract in code
      (`pg-assess/src/outcome.rs`)
- [x] 3.3 Add `--pipeline foma-confirm|hermitcrab` defaulting to `foma-confirm`, replacing
      `--engine default|foma` (`pg-cli/src/main.rs:102-116`); an unavailable pipeline returns
      `unsupported_capability` with no silent fallback
  (`pg-cli/src/assess.rs`: `--pipeline foma-confirm|hermitcrab`, default foma-confirm; a
      grammar the pipeline cannot run is exit 3 (capability) or exit 4 (containment), never a
      silent fallback to the other engine)
- [x] 3.4 Leave logical budgets unbounded unless a resource envelope is named; record the effective
      envelope in the report
- [x] 3.5 Type an outer-safety-net stop as `wall_clock_timeout` and set `reproducible: false` on the
      report
  (`IncompleteReason::WallClockTimeout` sets `reproducible: false`; no wall-clock net is
      armed by default, so no assessment is unreproducible unless one is asked for)
- [x] 3.6 Add `pangloss.assessment-report/v1` with the interned key table, per-case outcomes,
      `reportId`, `semanticDigest`, `outcomeDigest`, and top-level `status: complete|partial|failed`
      (`pg-assess/src/report.rs`; digests over the expanded form, so key-table order cannot move
      one. `parse_report` reads from the artifact's own table and recomputes rather than trusting)
- [x] 3.7 Emit an authoritative analysis set only for a complete case; keep partial candidates
      clearly separated and never in `analyses`
- [x] 3.8 Retain importer and compiler diagnostics inline. Give each importer and
      snapshot-validation warning a stable short code alongside its existing prose, replacing bare
      `Vec<String>`. `compare` diffs by code and count so rewording prose is never a context
      difference.
      17 codes, dotted and grouped by what actually went wrong, declared in
      `pg-fwdata/src/extract/codes.rs` (13, e.g. `fwdata.dangling-reference`,
      `fwdata.unsupported-morph-type`, `fwdata.only-first-used`) and `pg-snapshot/src/validate.rs`
      (4, e.g. `snapshot.feature-structure-unresolved`). No taxonomy was designed up front, as the
      task required — each code names its own emission site's meaning, and identical situations at
      different sites share one.
      Guarded by `pg-fwdata/tests/fixture_tests.rs`:
      `import_warning_prose_is_unchanged` (this was additive; no message was reworded),
      `structurally_different_warnings_get_different_codes`, and
      `unknown_morph_type_warning_carries_its_stable_code`
- [x] 3.8a Carry those codes all the way into the artifact. Coding the emission sites is necessary
      but not sufficient: `load_grammar` returns `Vec<String>` and flattened every code away, so the
      report still tagged everything `importer.warning` and `compare` still saw one bucket — the
      task's stated purpose ("`compare` diffs by code and count") was only half-achieved.
      `load_grammar_coded` keeps `pg_snapshot::Warning`, and the two commands that build assessment
      reports use it. `load_grammar` itself is unchanged, so no other caller was touched.
      `pg_grammar::compile_project`'s warnings are still uncoded and are tagged `compiler.warning` —
      honestly one bucket, because that is all the granularity that exists on that side today,
      rather than a finer-looking code invented for appearance.
      Gated by `diagnostics::tests::importer_warning_codes_reach_the_report_rather_than_one_bucket`,
      which asserts both the codes and their per-code multiplicity survive, and that
      `importer.warning` is gone
- [x] 3.9 Write to stdout by default; `--report` writes to a path and overwrites freely
- [x] 3.10 Add exit codes: `0` artifact produced, `2` invalid input/schema, `3` unsupported
      capability or incompatible profile, `4` containment prevented the artifact, `70` internal
  (verified against the real binary in `pg-cli/tests/assessment_e2e.rs`)
- [x] 3.11 Emit a failed assessment artifact with `not_attempted/assessment_setup_failed` cases when
      suite validation passed but import or compile failed safely
  (a broken grammar yields `status: failed` with every case
      `not_attempted/assessmentSetupFailed` and exit 0 — evidence, not an error exit)
- [x] 3.12 Accept a bare word list as well as a suite; synthesize deterministic case IDs from
      position and surface form so a caller need not author a suite for a quick run
- [x] 3.13 Retire `pg_cli::diagnostics::AssessmentReport` (`diagnostics.rs:167-176`); `diagnose`
      emits `pangloss.assessment-report/v1` and keeps its own `build.json`. One assessment artifact
      exists in the repo.
      Per-word diagnostics the canonical report has no field for — propose-side over-generation and
      the gloss signature — moved to the report's namespaced `extensions` (outside both semantic
      projections, inside `reportId`), so consolidating cost no evidence. `WordApplyStatus`,
      `WordAssessmentEntry` and `AssessmentSummary` are gone
- [x] 3.14 Delete `assess_words`' second compiled foma network (`diagnostics.rs:178-192`): it exists
      only because `FomaAnalyzer` exposed no budgeted entry point, which task 3.1 adds. `diagnose`
      compiles the grammar once.
      This also removed a correctness hazard, not just a cost: the budget used to be measured on a
      *different* network from the one that produced the analyses, so the recorded apply status
      described a traversal that could in principle disagree with the reported result. One run now
      decides both
- [~] 3.15 Update `certify-language-readiness` and `run-synthetic-conformance-matrix` for the report
      shape they consume. **No-op: the premise does not hold.** Checked at the time of 3.13:
      `certify-language-readiness` has no change directory in `openspec/changes/`, and
      `run-synthetic-conformance-matrix/{proposal,design}.md` never mention an assessment report,
      `diagnose`, or a report shape. They were named as declared consumers during planning, but
      neither actually references the retired type, so there was nothing to update. Recorded rather
      than ticked, so a later reader does not assume a migration happened that never did
- [x] Gate: `rust/tools/test.ps1 -Package pg-cli` — **85/86**, the one failure being the
      documented CRLF golden (`make_report_golden_md`, a module this change never touches; it passes
      in a checkout whose files are LF). A repeated run reproduces both digests and a timestamp moves
      `reportId` only: `assessment_e2e::an_assessment_is_reproducible_and_only_its_report_id_moves`,
      driving the real binary twice over a real grammar

## 4. `compare` and the grammar delta

- [x] 4.1 Add `pangloss.grammar-delta/v1`; match cases by exact `caseId`, following declared
      `supersedes` links
- [x] 4.2 Implement the category set: `unchanged`, `added_only`, `removed_only`, `mixed`,
      `annotation_changed`, `completeness_changed`, `baseline_only`, `candidate_only`,
      `not_comparable`
- [x] 4.3 Report `annotation_changed` as a changed case when a retained identity's `guessed` flipped
- [x] 4.4 Report duplicate-count and context differences as flags that do not make a case changed
- [x] 4.5 Add a typed `not_comparable` reason enum covering `case_definition_changed`,
      `identity_profile_changed`, both-incomplete, both-not-attempted, and key collision
- [x] 4.6 Treat a stable key absent from the other side as `added`/`removed`, never
      `not_comparable`
- [x] 4.7 Order output as baseline report order followed by candidate-only cases in candidate order
- [x] 4.8 Report context differences without refusing comparison; never label an addition an
      improvement or a removal a regression
- [x] 4.9 Produce a valid artifact with every case `not_comparable/identity_profile_changed` and
      exit `0` when profiles are incompatible
- [x] Gate: `rust/tools/test.ps1 -Package pg-assess` — `compare` lives in `pg-assess::delta`.
      A deleted morpheme yields `removed_only`
      (`delta::tests::a_deleted_morpheme_is_removed_evidence_not_a_refusal`) and discovery order
      changes no category (`delta::tests::discovery_order_does_not_change_any_category`). The
      FieldWorks count-subtraction defect is pinned as
      `two_analyses_replaced_by_two_others_is_mixed_not_no_change`

## 5. `golden-diff`

- [x] 5.1 Add `pangloss.golden-set-diff/v1` with `matchingRequired`, `missingRequired`,
      `matchingAllowed`, `observedForbidden`, `unexpected`, and structured identities, not counts
      alone
- [x] 5.2 Evaluate expectations only for complete outcomes; report `not_evaluable` with the typed
      execution outcome otherwise
- [x] 5.3 Evaluate agreement only for `adjudicated`; missing, `unresolved`, and `out_of_scope`
      produce `not_adjudicated`
- [x] 5.4 Require the exact suite id, revision, semantic digest, and identity profile recorded in
      the assessment; never re-evaluate an old run against revised policy
- [x] 5.5 Retain denominators in every aggregate: total, complete, incomplete, not attempted,
      adjudicated and evaluable, agrees, disagrees, unresolved, out of scope, invalid
- [x] 5.6 Never update the suite
- [x] Gate: `rust/tools/test.ps1 -Package pg-assess` — `golden-diff` lives in
      `pg-assess::golden`. The load-bearing refusal is
      `golden::tests::an_incomplete_case_never_satisfies_an_empty_closed_world_expectation`, plus
      `every_aggregate_carries_its_denominator_and_no_rate` and
      `evaluation_never_writes_to_the_suite`

## 6. `investigate` and the failure narrative

- [x] 6.1 Add `pangloss.investigation-handoff/v1`; verify report, model fingerprint, case, input,
      pipeline, and options agree before emitting anything
- [x] 6.2 Emit lexical-entry source GUIDs from `LexEntryDef.authored_id`; mark rule, stratum, and
      template references `compilerAssigned` and explicitly not source identities
- [x] 6.3 Label evidence `retained`, `regenerated`, or `unavailable`, and record which engine and
      pipeline produced it; never present regenerated evidence as originally captured
- [x] 6.4 Attribute a missing analysis to HermitCrab rejection or proposer recall gap by running the
      case on both pipelines. `run_investigate` with `--grammar` runs HermitCrab (real
      `TreeTraceSink`) and foma-confirm (real `FomaAnalyzer`) and classifies via `attribute_causes`:
      produced by HC but not foma -> `ProposerRecallGap`; produced by neither, with a trace node
      whose candidate matches -> `HermitcrabRejected`; produced by neither without that evidence ->
      `NeitherPipelineProduces`; foma unavailable, or foma produced it -> `Undetermined`.
      The bias is deliberate: an identity it cannot place confidently stays
      `NeitherPipelineProduces` rather than an asserted rejection, and a pipeline it could not run
      leaves the whole set `Undetermined`. It never guesses a cause.
      **Two limits, recorded rather than papered over:**
      (a) The gate test `a_synthetic_proposer_recall_gap_is_attributed_to_the_proposer_not_the_grammar`
      uses real unstubbed engines on a real grammar, but the gap it exercises is a *capability*
      difference — HermitCrab's guesser fabricates a root that the foma path has no guess facility to
      offer at all. That is a genuine disagreement, and it proves the attribution logic; it is not an
      ordinary-rule-application recall gap, which is what the propose-and-confirm invariant actually
      targets. An attempt to build one (a two-rule chained-reduplication grammar, on the theory that
      nested reduplication is unproven) found **no gap** — foma recovered the same analysis — and the
      negative finding is recorded in a comment rather than deleted silently. Not finding an
      ordinary-path gap on demand is mildly reassuring, not evidence the code is untested.
      (b) `HermitcrabRejected` and `NeitherPipelineProduces` are covered by unit tests over
      hand-built inputs only; no real grammar drives those two branches end to end.
      Also: the rejection match compares morpheme sequences alone
      (`HermitcrabFailure::candidate_morphemes`), not root index or category, so a rejected candidate
      of the same shape but a different root position matches. The error direction is bounded — the
      trace really did show HermitCrab rejecting that morpheme chain — but it is a precision limit,
      not exactness
- [x] 6.5 Emit the pruned failure narrative from the existing `FailureReason` taxonomy
      (`pg-rules/src/trace.rs`): candidates attempted, where each died, and why.
      `collect_hermitcrab_failures` walks a real `TreeTraceSink` and keeps only nodes carrying a
      `FailureReason` — that is the whole pruning step, and it is what keeps a thousand-node trace
      from reaching an AI consumer as a dump. Reason names are carried verbatim so a Rust narrative
      and a C# trace name the same thing.
      A real two-step narrative for `"sagd"`, pinned by
      `the_pruned_narrative_for_a_real_word_shows_where_and_why_a_candidate_died`:
      `NonPartialRuleProhibitedAfterFinalTemplate` at a `compilerAssigned` morphological rule
      (`ed_suffix`), then `PartialParse` at lexical entry `e32` as a `sourceId`. Rules, strata and
      templates are always `compilerAssigned`; only lexical entries, which genuinely retain an
      authored id, are `sourceId` (ADR 0001). `no_artifact_field_prescribes_a_grammar_edit` still
      passes, so nothing in the artifact diagnoses or prescribes
- [x] 6.6 State that FieldWorks' C# HermitCrab traces a different engine, so a divergence there is
      not necessarily a grammar defect
- [x] 6.7 Make no root-cause claim and prescribe no grammar edit in any artifact
- [x] Gate: `rust/tools/test.ps1 -Package pg-cli` — 85/86 as above.
      `assess::tests::a_synthetic_proposer_recall_gap_is_attributed_to_the_proposer_not_the_grammar`
      drives both real engines and asserts `ProposerRecallGap`. See 6.4(a) for exactly which kind of
      gap that is and which kind remains unexercised — the tick means the attribution logic is
      proven against a real disagreement, not that every gap shape has been seen

## 7. End-to-end fixture

- [x] 7.1 Build a synthetic `.fwdata` fixture demonstrating: two cases sharing a surface form with
      distinct case IDs; a required analysis appearing; a forbidden analysis appearing; an allowed
      alternative; an analysis removed while another is added; a complete empty analysis set; a
      logical-budget incomplete; an importer warning; and an on-demand handoff
- [x] 7.2 Run the fixture through both pipelines and compare full structured analysis sets for cases
      complete in both; an incomplete case fails the fixture rather than comparing as empty
- [x] 7.3 Confirm Windows and Linux produce identical `semanticDigest` and `outcomeDigest` for the
      same grammar despite differing line endings on checkout (`core.autocrlf = true`, no
      `.gitattributes`), and that `sourceSha256` and `reportId` correctly differ. Do not "fix" a
      digest mismatch here by reintroducing CRLF-normalized source hashing (D12)
- [x] Gate: the end-to-end suite lives in `pg-cli/tests/assessment_e2e.rs` (it drives the real
      binary, so it belongs beside it rather than in `pg-conformance-fixtures`). Synthetic data
      only: `machine/conformance/edge-cases/deep-optional-affix-nesting`, whose all-optional
      12-slot chain guarantees `C(12,k)` analyses by construction — 1/12/66, asserted, so a
      projection bug cannot hide behind "whatever the parser said".
      Run with `rust/tools/test.ps1 -Package pg-cli`. 7 e2e tests + 12 CLI unit tests pass.

## Closed schema deliverables (handoff spec §17.3)

- [x] S.1 Check in JSON Schemas for all five artifacts plus these shared definitions §17.3 names:
      typed failures (`assessmentFailure`), diagnostics, per-case outcomes, batch outcomes, and
      resource envelopes — `pg-assess/schemas/`
- [x] S.1a **Trace references** — §17.3's sixth shared item is now a *recorded* non-goal rather than
      an unexplained gap: design.md **D15**, plus a matching section in `schemas/README.md`. The
      reasoning is that D9/D10 already draw the boundary (investigate binds evidence and supplies a
      pruned narrative; it does not compete with FieldWorks on trace presentation), and a `traceRef`
      field would have nothing backing it — tracing exists on the HermitCrab-confirm side but not on
      the FST-propose stage of `foma-confirm`, the default pipeline. D15 states what a future change
      closing it would need: a `--trace` output path, trace support on FST-propose, and a staleness
      story distinct from `EvidenceAvailability`
- [x] S.2 Canonical positive fixtures: every schema is validated against an artifact the real
      emitter produced, not a hand-written sample, so schema/emitter drift fails either way
- [x] S.3 Negative fixtures: each must be rejected AND rejected at the field at fault, so a
      negative fixture cannot pass for the wrong reason. Fixtures are constructed in
      `tests/schema_conformance.rs` from real emitter output rather than checked in as static
      `.json` files — a static positive fixture proves only that the file matches the schema, never
      that the emitter still does, so it would stay green through exactly the drift these guard
      against (`schemas/README.md` says so)
- [x] S.4 The validator covers a declared JSON Schema subset and treats an unsupported keyword as a
      hard error, so a schema cannot grow a construct nothing checks. A general validator would mean
      a new dependency this repo has not taken; claiming to be one would be worse than declaring the
      subset
- [x] S.5 `docs/grammar-assessment-schemas.md` — consumer reference for generating client types:
      where the schemas live, one line per artifact, the two-axis versioning contract
      (`schemaVersion` vs the independently versioned identity profile), every named digest
      projection, the closed enums a client may switch on, and what `additionalProperties: false`
      obliges a client to do with unknown fields.
      It also documents the one thing most likely to break a generator: `$ref` here is **not**
      standard cross-file `$ref`. Every reference is a bare `#/$defs/<name>` resolved only after
      `common.defs.json`'s `$defs` are merged under the artifact file's own. The doc gives two
      concrete ways to handle that
- Gate: `rust/tools/test.ps1 -Package pg-assess` — 20 schema-conformance tests

## Known environment limitation (not a defect in this change)

`core.autocrlf=true` with no `.gitattributes` means a **freshly created worktree** checks out CRLF
source files, while the main checkout's files are LF. Ten tests whose golden is a string constant
embedded in a Rust source therefore fail in a fresh worktree only — the literal picks up `\r\n`
while the renderer emits `\n`. Nine are in `pg-foma` (`plan_diagram`, `coverage_ledger`,
`readiness_verdict`, `preflight`, `selection`, `plan_interaction_coverage`) and one is
`pg-cli make_report::tests::make_report_golden_md`. None is in a module this change touches, and all
pass in the main checkout.

Deliberately **not** fixed here by adding `.gitattributes` or changing `core.autocrlf`: worktrees
share `.git/config` with main, and the absence of `.gitattributes` is load-bearing for D3a and for
task 7.3, which exist precisely to prove digests survive differing line endings. Rewriting the
worktree's files to LF was tried and rejected — git then reports 846 files modified, which would
pollute the merge. The real fix belongs in a separate change that decides the repo-wide policy.
