# Simplification cleanup plan and rip list

Living implementation plan. Git history is the recovery mechanism; PanGloss is pre-alpha and has no
external compatibility obligation. Old behavior must not be restored merely because a test expects
it. Update or delete that test first when it pins a contract explicitly rejected below.

## Ratified marching orders (2026-08-25)

These decisions are authoritative. The detailed rip list is subordinate to them.

### Pipeline boundaries

1. **Analyze** — independently and cheaply inspect the grammar. Report each backend's
   representability, warnings, and grammar-derived cost estimates. Analysis must not compile, run
   the production emitter, synthesize recursive closure, apply phonology to generated successors,
   or perform a production traversal whose output is discarded.
2. **Choose** — backend selection is explicit. A normal production build reads one backend from
   project configuration. A local CLI run may explicitly override it or request several backends.
   Missing selection is an error for build/package, but not for analysis. There is no implicit
   preferred backend, top-N chooser, fallback, or retry backend.
3. **Build** — cheaply revalidate representability, then compile the selected backend exactly once
   in a supervised worker. The real pre-expansion traversal remains inside the build because it is
   required for interdigitation and boundary-fusion correctness. Automatic pre-build closure walks
   that duplicate this work must die.
4. **Test** — run a large word corpus against completed artifacts as a separate operation. Report
   raw per-backend measurements; do not select a winner. Humans compare artifact size/build cost
   with analysis speed and record the chosen backend in configuration.
5. **Package** — `pangloss pack` consumes one explicitly named, already-completed artifact. It
   never compiles and never silently substitutes another backend.

### Build and containment contract

- Every build, including local experimental builds, runs in the supervised worker.
- Each explicitly requested backend is an independent attempt. Multiple requests run sequentially;
  a failed P attempt does not prevent an explicitly requested Q attempt.
- There are no named resource envelopes and no “increase the envelope” retry.
- Each attempt has three configurable, finite, positive execution limits:
  - maximum final serialized FST payload: **1 GB** by default;
  - maximum actual committed memory for the entire worker process tree: **10 GB** by default;
  - maximum wall-clock time for construction plus serialization: **10 minutes** by default.
- Users may raise or lower these limits but may not disable them. They are operational containment,
  not representability judgments and not backend-selection inputs.
- Crossing any limit, crashing, producing an empty/partial result, or failing compilation produces
  no artifact. Delete intermediate payload files and retain only structured diagnostics: backend,
  failed phase, elapsed time, peak committed memory, and applicable limit.
- The serialized-size limit is enforced against final FST payload bytes. The RAM limit is enforced
  externally against the complete worker process tree. Timeout is external wall clock over the
  complete attempt.

### Proof, publication, and artifact identity

- A process/build failure is distinct from readiness and representability.
- A completed artifact is publishable only when the selected backend's capability analysis proves
  it can represent the grammar completely.
- `--allow-unproven` is local testing only. It may produce a locally retained artifact for corpus
  testing, clearly marked unproven in build metadata. `pangloss pack` rejects the option and rejects
  every unproven artifact. Corpus success never promotes an unproven artifact to publishable.
- Corpus testing is optional validation evidence, not a universal publication prerequisite.
- A completed artifact is bound to the exact grammar digest, backend identifier, compiler/worker
  protocol version, and effective semantic build configuration. Packaging rejects stale or
  mismatched artifacts.
- Execution limits and observed resource use are recorded as provenance but do not affect model
  identity.
- Packs, health reports, optimizer reports, and worker messages from before this cleanup need not
  remain readable. Bump the applicable schema/protocol version, reject stale data loudly, and delete
  aliases, defaults, migration shims, and compatibility-only tests. Worker protocol remains strict
  lockstep until the normal 1.0 release.

### Backend and tuning policy

- Analysis reports backends independently. It emits no global preference, selected set, Pareto
  winner, or composite preference score.
- Production backend selection is an explicit project configuration value. A CLI override is an
  explicit per-run choice, never a fallback.
- Corpus comparison reports raw metrics such as recall, ambiguity/duplicate analyses, proposal and
  confirmation work, artifact size, build time, peak memory, and analysis speed.
- Lower-level tuning switches *within* the selected backend normally use `auto`, but each real
  performance switch is configurable and controllable. The effective values are recorded in the
  semantic build configuration and therefore in artifact identity.
- Grammar-required correctness routing is not a performance switch and cannot be configured off.
  Configuration may select among capability-proven equivalent implementations, but cannot suppress
  a route required to preserve the grammar's relation.

### Current switch audit

The source currently has four different things called or treated like automatic choices. They must
not survive as one optimizer-shaped abstraction.

1. **Grammar-required correctness routing — retain as automatic facts.**
   - `preexpand::should_run` enables composite emission when phonology or infix morphology requires
     it.
   - `emit::structural_candidate_rules` enables structural composites for shapes the ordinary
     emitter cannot safely decompose.
   - `gate::find_gated_subrules` plus `partition_entries` derives entry groups from actual MPR/POS
     predicates.
   - `MorphotacticIndex::build` derives legal transitions from strata, template slots, ordering,
     optional/vacuous rules, and partial-root state.
   - The chosen lowering adapter fixes its emitter backbone: surface emission uses composite
     closure; templated-underlying emission disables it and verifies tag reachability.
   These choices answer “what is required for correctness?”, not “what seems faster?”
2. **Offline recipe search — computation over candidates and a corpus.**
   - Grammar semantics first determine which candidate families are applicable: gated exceptions,
     templates, reduplication, metathesis, splittable groups, strata, entries, and phonology.
   - The registry materializes plan transformations: gate/union permutation and partition
     bisection/fan-out, plus whole-backend adapters that must be removed from this within-backend
     search.
   - The current pilot actually builds up to eight candidates and evaluates the first eight corpus
     words. It computes p50/p95 stage costs.
   - Search is exhaustive when `candidate count × pilot p95` fits half the remaining time;
     otherwise it uses branch-and-bound for compositional/strong-pruning topology, or a diverse beam
     of width 16.
   - Fully corpus-confirmed candidates are ranked by deterministic work: confirmation steps plus
     raw proposer paths, then confirmation calls, proposals, and network size.
   This is an explicit offline tuning experiment, not cheap grammar analysis and not ordinary
   production compilation.
3. **Real emitter precision choice — keep only implemented choices.**
   - `PrecisionConfig::Strip` is the real default.
   - `AllFlags` is implemented and enabled only for a statically safe constraint shape: singleton,
     required, literal-left, and no right context.
   - `FullCompile` and `Auto(u32)` currently behave like `Strip`; they are stub variants and should
     be deleted unless a second implementation is deliberately funded.
4. **Scaffolding and non-choices — delete or keep private until real alternatives exist.**
   - `ComposeStrategy::Static` is the only compose strategy.
   - `SurfaceEmitStrategy` has only `ReifiedPlan` and `AllRoots`.
   - Plan-shape permutations generally canonicalize to the same network after minimization. The
     code's recorded measurements found identical proposal/confirmation work and often worse build
     time. Do not expose them as user switches without a repeatable corpus-backed win.
   - Branch-and-bound is currently inert because production candidates have no exact incumbent
     objective.

The already-ratified deletion remains unchanged: `backend_selection.rs` severity/finding-count and
fixed-preference ranking is cross-backend automatic selection, not within-backend tuning, and must
die.

**Deferred to the immediate post-cleanup round:** redesigning within-backend `auto`, exposing tuning
configuration, changing recipe/Plan search, deleting experimental plan transformations, and
collapsing precision/strategy scaffolding. This cleanup must leave those internals behaviorally
unchanged except where compilation requires a narrow adaptation to the new explicit-backend or
stage-boundary contracts. When this cleanup finishes, the next work begins directly from the switch
audit above.

### Test and staging discipline

1. Rewrite or delete tests that assert a rejected contract before deleting its implementation.
2. Add the acceptance test for the replacement contract and verify that it fails for the intended
   reason where practical.
3. Delete one coherent behavior cluster at a time; never stage the whole dirty tree.
4. Inspect the exact staged diff, run its focused tests through `rust/tools/pg.ps1`, then run the
   relevant authoritative package suites and `git diff --check`.
5. Keep central emitter work isolated. Require emitted-byte or semantic parity plus real-language
   gates for every behavior intended to remain.
6. Never restore named envelopes, automatic selection, compatibility shims, duplicate prechecks,
   or build-time corpus work just to make an old test green.

Status key: **VERIFIED** (source, focused behavior proof, integration proof, docs, and residue grep
all passed) · **LANDED UNVERIFIED** (committed source exists but the full proof gate has not passed)
· **AUTHORIZED** (decision made; rip it out) · **REJECTED** (do not build/restore it) · **SPLIT**
(execute only the authorized portion) · **PARTIAL** (some, but not all, of the item landed) ·
**RETAINED** · **PROTECTED** · **DEFERRED NEXT** · **OPEN** · **BLOCKED**
(needs a decision) · **VERIFY** (needs source evidence or measurement). `DONE` is no longer used:
it hid live remnants. Only reviewed commits count as landed work; tests and documentation are part
of the completion gate, not evidence for restoring a rejected contract.

---

## A. Refusal gates (the "grinding to a halt" class)

| # | Item | Evidence | Status |
|---|---|---|---|
| A1 | `ResourceBudgetReached` / `ProvenBoundExceedsBudget` classed as machine-health, so they exclude a backend | `health.rs:468-487`, `backend_selection.rs:227-252` | **AUTHORIZED** — live contradiction found; make them measurements/labels, never cross-backend selection input |
| A2 | Pack write gate refuses on a severity number | `pack.rs:202-232` | **AUTHORIZED** — live contradiction found; publication follows capability proof, not size/readiness severity |
| A3 | Apply-phase + severity used as a proxy for category | `pack.rs:202-232`, oversized-pack test | **AUTHORIZED** — rewrite the stale test before deleting the gate |
| A4 | `evaluate_via_tuned_emit_mode` rejects on mere *presence* of a finding, before construction | `backend_runtime.rs:1428-1458` | **AUTHORIZED** — live pre-refusal remains |
| A5 | `realize_accuracy_proposer` / `tuned_surface_resource_refusal` repeats the pre-refusal | `backend_runtime.rs:1428-1458,2082-2113` | **AUTHORIZED** — helper and callers remain |
| A6 | Marker-bearing candidates banked `Unsupported` with zero work measured | `backend_runtime.rs` | **LANDED UNVERIFIED** — re-audit after A4/A5 deletion |
| A7 | `--watchdog` structurally cannot produce a real artifact | `pack.rs:267-325,479-489,562-565,624-642` | **AUTHORIZED** — live watchdog/placeholder production path remains |
| A8 | 16 MiB result metadata frame must not cap the selected payload | `worker_contract.rs`, `worker.rs` | **VERIFIED** — protocol v9 uses an independently bounded raw frame; filesystem transport and legacy parser/capture residue are deleted; prefix-before-allocation, clean exit, malformed streams, and supervisor-limit authority are proven |
| A9 | `finished_net_digests` — same marker pre-refusal, third site | `backend_runtime.rs` ~1750 | **OPEN** — diagnostic-only, but same false premise |
| A10 | Internal construction caps in `compose_budget.rs` can still stop a representable build | 1,334-line file, 165 refs / 27 files | **AUTHORIZED** — retain useful measurements; delete internal representability/size refusals. The supervised worker's three configured execution limits are the only resource stops |

---

## B. Backward compatibility for users who do not exist

**Nobody is using PanGloss yet.** Every mechanism below exists to read data written by an earlier
version, or to keep a wire shape stable for a consumer that has never existed. All of it is pure
carrying cost, and deleting it now costs nothing.

| # | Item | Evidence | Est. lines | Status |
|---|---|---|---|---|
| B1 | `#[serde(alias)]` on every `Severity` variant for pre-schema-3 spellings, plus the test pinning them | `health.rs` | 60–120 | **LANDED UNVERIFIED** — aliases and the compatibility test removed; full completion gate is not recorded here |
| B2 | `health::OverrideRecord`, kept solely to deserialize already-written reports | `health.rs` | 80–150 | **LANDED UNVERIFIED** — type, field, fixtures, projection, and override-only tests removed; full completion gate is not recorded here |
| B3 | Persistent capability-override records in pack manifests/WASM consumers | `pg_pack::trust`, `readiness_verdict.rs` | — | **AUTHORIZED** — delete from publishable artifacts. Local unproven status survives only in build metadata, which pack rejects |
| B4 | `Certification::MultiplicityMismatch` — doc says "no longer produced, kept for deserializing old reports" | `backend_optimizer.rs` | 20–40 | **LANDED UNVERIFIED** — variant and compatibility fixture removed; full completion gate is not recorded here |
| B5 | `Truncated { corpus: Option<..> }` carries live oracle evidence | `backend_optimizer.rs`, `backend_runtime.rs`, `backend_report.rs` | — | **PROTECTED** — audited and retained; live producers and consumers |
| B6 | `HEALTH_SCHEMA_VERSION` stamps and validates stored health artifacts | `health.rs`, `fst_health.rs` | — | **AUTHORIZED** — keep strict versioning, bump for the break, reject old reports, delete compatibility-only defaults/tests |
| B7 | `ResourceEnvelopeId` versioned identity (`ManagedV1`, `TunedSurfaceWork10kV1`) | `resource_envelope.rs`, 47 refs / 9 files | see C1 | **AUTHORIZED** — delete both named envelopes and their persisted provenance |

---

## C. Replace "capped vs uncapped" with explicit worker limits

The old system chooses between internal construction-cap profiles. The replacement is one build
path with three externally enforced, configurable execution limits. Verified old reference counts:

| Symbol | Refs | Files |
|---|---|---|
| `CompileSizeMode` | 53 | 10 |
| `ResourceEnvelope` / `ResourceEnvelopeId` | 119 | 10 |
| `--remove-size-limits` | 36 | 4 |
| `developer-tools` feature gate | 78 | 6 |
| `TunedSurfaceWork10kV1` | 6 | 2 |
| `RetryAuthorization` | 6 | 2 |
| `DeveloperStress` | 10 | 6 |

| # | Item | Est. lines | Status |
|---|---|---|---|
| C1 | Delete the two-envelope / size-mode system; replace it with 1 GB serialized bytes, 10 GB process-tree committed memory, and 10-minute wall-clock defaults | 1,200–1,800 | **AUTHORIZED** |
| C2 | Delete `RetryAuthorization`, automatic backend retry, and "increase envelope" remedies/tests | included above | **AUTHORIZED** |
| C3 | Delete `resource_envelope.rs` named-profile/digest machinery; retain only a small execution-limit configuration type if that is its cleanest owner | included above | **AUTHORIZED** |
| C4 | Delete `--remove-size-limits`; all three execution limits remain finite. Retain `developer-tools` only where still needed for local `--allow-unproven`, then reassess the feature itself | included above | **AUTHORIZED** |

---

## D. Duplicate implementations

| # | Item | Evidence | Est. lines | Status |
|---|---|---|---|---|
| D1 | Two ~900-line emission pipelines differing by one already-parameterized flag | `emit.rs` | −524 | **VERIFY** — promising uncommitted consolidation, but currently constructs `MorphotacticIndex` unconditionally for the templated path. Fix and prove output/parity in its own commit |
| D2 | Duplicate capability-answering substrate and automatic backend preference machinery | `backend_registry.rs` 1,330 + `backend_mechanism.rs` 1,199 + `mechanism_provider.rs` 188 | 2,717 | **SPLIT** — delete cross-backend ranking/preference/selection in this cleanup. Freeze registry/mechanism/Plan/recipe tuning internals; redesign or delete them in the immediate post-cleanup switch round |
| D3 | `oracle.rs` duplicated two `build.rs` helpers verbatim, panic text already drifted | | ~40 | **LANDED UNVERIFIED** |
| D4 | Admission-summary rendering implemented three times | `fst_health.rs`, `pack.rs`, `make_report.rs` | ~20 | **PARTIAL** (2 of 3; the third differs in output, left inline) |
| D5 | `ConfirmedBuckets` flattening copy-pasted three times | `composite.rs` 669, 724, 918 | ~60 | **OPEN** |
| D6 | Remedy rendering diverged between two tables | `make_report.rs` | ~30 | **LANDED UNVERIFIED** |
| D7 | `CompileSizeMode` resolution re-inlined twice | `pack.rs`, `make_report.rs` | ~20 | **AUTHORIZED** (deliberately left; dies with C1) |
| D8 | Three modules hand-assemble a 10-field `HealthFinding` literal; no shared builder | `health_evaluator.rs`, `characterization.rs`, `fst_health.rs` | ~80 | **OPEN** |

---

## E. Work that belongs outside the build

| # | Item | Evidence | Est. lines | Status |
|---|---|---|---|---|
| E1 | `ProposalVolume`, `ConfirmationWork`, `DuplicateAnalysisOverlap` computed during the build | `fst_health.rs` | 200–400 | **AUTHORIZED** — move exclusively to the explicit post-build corpus-test step |
| E2 | The discarded double traversal: the pre-check runs or reproduces the production emitter's closure walk, then the real compile runs it again | `characterization.rs` + `preexpand.rs` + `backend_runtime.rs` | 100–200 | **AUTHORIZED** — delete automatic and explicit dry-run closure traversal. Keep cheap grammar analysis and the build's one required pre-expansion traversal |

---

## F. Dead, vacuous, and misleading code

| # | Item | Evidence | Status |
|---|---|---|---|
| F1 | Five dead match arms — `Refused` is only ever built with two of seven reasons | `characterization.rs` | **LANDED UNVERIFIED** |
| F2 | Test asserting an impossible severity+code pairing | `characterization.rs` ~810 | **LANDED UNVERIFIED** |
| F3 | Test fixture manufacturing pairings production cannot produce | `pack.rs` `synthetic_health` | **LANDED UNVERIFIED** (12 call sites) |
| F4 | Write-only `CompositeRec::morpheme` field | `preexpand.rs` | **LANDED UNVERIFIED** |
| F5 | `#[allow(dead_code)]` where `#[cfg(test)]` lets the compiler enforce the claim | `preexpand.rs`, `unordered.rs` | **LANDED UNVERIFIED** |
| F6 | Duplicate adjacent assertions left by `acd313c6` | `health.rs`, `pack.rs` | **LANDED UNVERIFIED** |
| F7 | Two unlinked copies of the 100 MB threshold | `health.rs`, `readiness_policy.rs` | **LANDED UNVERIFIED** |
| F8 | `Certification::StaticRejected` may now be unreachable | `backend_runtime.rs` | **VERIFY** |
| F9 | Stale `FailClosed` / `RefusalWitness` docs/tests — source machinery is already absent | `capability.rs` and ledgers/docs | **AUTHORIZED** — sweep stale references; do not recreate source behavior |
| F10 | Dead-weight tests: 2,493 tests, some pinning behaviour being deliberately removed, some vacuous | whole suite | **OPEN** — the second-pass review's main target |

---

## G. Correctness risks (not tidiness — do these regardless)

| # | Item | Evidence | Status |
|---|---|---|---|
| G1 | The default shipping backend may lack a tag-reachability check the other backend has | `emit.rs` `verify_tags_reachable`, on for templated only | **OPEN** — possible silent wrong output; highest-priority open item |
| G2 | Grammar-derived regex rejection `panic!`s inside functions that already return `Result` | `replace.rs` 875, 896, 1514 | **OPEN** — needs `ComposeError::RegexRejected`; 12 files reference `ComposeError::` |
| G3 | `panic!` if `compounding_max_depth` misses a `Compounding` id, in the production walk | `capability.rs` ~1348 | **OPEN** |
| G4 | Two diagnostics surfaced by `eprintln!` because `Certification` has no field to carry them | `backend_runtime.rs` | **OPEN** |
| G5 | One pre-existing test failure, proven pre-existing at `acd313c6` | `morphotactics_boundary_cleanup_slice::templated_query_accepts_a_surface_with_an_explicit_boundary` | **OPEN** |

---

## H. Structural (deferred past alpha by review, recorded so it is not lost)

| # | Item | Evidence | Status |
|---|---|---|---|
| H1 | `plan.rs` is documented as an IR but only one controllable adapter interprets it; whole-grammar backends ignore it | `enumerate.rs`, `lowering_adapter.rs` | **DEFERRED NEXT** — decide in the immediate post-cleanup switch round; do not redesign it now |
| H2 | `capability.rs` — 3,942 non-test lines, 15 predicates, one file | | **OPEN** — split, do not rip |
| H3 | Adding a backend is a shotgun edit: 162 references across 15 files | | **OPEN** — simplify after D2 removes chooser/ranking coupling |
| H4 | `PlanComposed` / `uflexc` is the weakest backend with known whole-construct holes | `strategy_coverage.rs` 142 | **VERIFY** — it may remain an explicitly selectable backend if capability analysis reports those holes honestly; it gets no fallback/preference role |
| H5 | Old uncalibrated constants | 3,000 / 100 MB / 100 | **AUTHORIZED** — delete. Replacement execution defaults are 1 GB payload / 10 GB committed RAM / 10 minutes and are configurable, finite, and non-semantic |
| H6 | Concurrent "kill the right one" scheduler | | **REJECTED** — explicitly selected builds run sequentially, so no cross-build resource arbitration machinery is needed |

---

## I. Housekeeping

| # | Item | Status |
|---|---|---|
| I1 | Historical large dirty-tree churn | **RETAINED DISCIPLINE** — every slice is separately committed; final snapshot must be clean |
| I2 | Baseline worktree at `.claude/worktrees/baseline-verify` | **VERIFIED** — path is absent |
| I3 | Mismatch ledger accuracy | **AUTHORIZED** — B3 persistence and B6 stale-version acceptance still contradict the charter; correct them with their behavior slices |
| I4 | `2026-08-23-developer-fst-controls.md` drifts both ways; obsolete once C1 lands | **AUTHORIZED** — replace or delete with C1 |
| I5 | Docs referencing envelope retry, automatic selection, build-time corpus work, or compatibility guarantees | **AUTHORIZED** — update in the same slice that removes each behavior |

---

## Execution order: small, intentional commits

Use the stages below unless fresh dependency evidence is written into this file. Each stage is one
or more bounded Luna slices with disjoint file ownership. `AUTHORIZED` permits deletion only inside
that stage's allowed scope. `VERIFY`, `OPEN`, `DEFERRED NEXT`, and protected scope are never deletion
permission.

1. **Finish raw selected-payload transport (A8).** Delete the rejected filesystem transport, legacy
   aggregate-result parser tests/helper, and stdout-only overflow residue. Add subprocess proof for
   missing, truncated, trailing, malformed, and stalled payloads. Gate: protocol 8 rejection; exact
   length/SHA/fingerprint/EOF; no selected-artifact paths, files, hard links, cleanup, or ownership
   code. Status: **VERIFIED**.
2. **Install real external containment (C1).** Enforce the configurable 1 GB final payload, 10 GB
   committed process-tree RAM, and 10-minute wall limit on Windows and Linux. Every production build
   must use it. Gate: descendants die with the worker; memory/time/crash/partial output produce no
   completed artifact and structured provenance. Protected: sequential independent P/Q attempts.
   Status: **PARTIAL** until the worker adapters pass and Stage 3 migrates every production build
   route; adapter proof alone verifies only the artifact-worker sub-slice and does not unlock Stage 4.
   Committed checkpoints: replacement descendant-failure tests (`40897d45`), typed containment
   outcomes plus the required health/pack v5 break (`b330892f`), and the safe helper API with its
   verified Windows adapter (`9c7330c2`), test-first Linux wrapper contract (`b6894312` through
   `9249b4ee`), the fail-closed Linux wrapper source checkpoint (`694de90f`), and the pinned hosted
   Linux RED/GREEN gate (`42f64571`, `9243cc25`). Still pending, in order: a green execution of that
   exact delegated-host/service-lifecycle job, production routing, then deletion of the shared
   direct-`Command` supervisor loop and its source-shape test.
   Cross-platform fixture success is not Linux runtime proof and does not authorize that deletion.
   A fresh final-tip Windows rerun reported 5 unit and 11 Windows containment tests passing and the
   managed command exited cleanly. The earlier procgov teardown hang did not reproduce with an
   immediate-exit child or the exact cached Cargo target, so no speculative watchdog code was added.
   The Linux CI dependency remains the sole platform gate. The pinned `ubuntu-24.04` job provisions
   its own transient delegated systemd service and controlled service-main-death probe, removing the
   former need for a pre-provisioned self-hosted runner. The job has not yet executed, so workflow-
   only wiring is prerequisite progress, not Linux runtime proof or deletion authority.
3. **Delete cross-backend automatic choice and route explicit builds (D2/A7).** Rewrite preference,
   top-N, fallback, retry, winner, and Pareto tests first. Delete `BACKEND_PREFERENCE`, `preferred`,
   `select_up_to`, rank keys, fallback paths, watchdog/placeholder pack compilation, and production
   in-process build routes. The worker receives an explicit backend and validates the result matches
   it. Protected: independent per-backend analysis reports, registry/mechanism capability facts,
   grammar-required correctness routing, and deferred within-backend tuning.
   Status: **PARTIAL** only for completed-artifact validation. The RED contract `21faa1b5` and GREEN
   implementation `efcaafa6` removed the validator's `BackendSelection`/`preferred()` dependency and
   require one explicit requested strategy. The RED/GREEN pair `8b218093`/`777df49d` then deleted the
   zero-caller `run_selected_compile_worker`, its stale keeper assertion, and 50 lines of private
   deserialization residue: 105 production deletions and no protocol removal. Selector derivation,
   supervisor routing, Pack/CLI migration, and the broader chooser deletion remain untouched until
   Stage 2's Linux gate. The independent RED/GREEN pair `5d428ca7`/`f2f7d69e` also deleted the
   zero-caller, explicitly rejected top-N `select_up_to` method and its ranking-only tests while
   preserving `preferred()` for the later atomic route migration. `ecdbb65e`/`341fb5a4` deleted the
   final zero-caller envelope convenience constructor; the richer production selector path remains.
4. **Delete internal compile refusal caps (A1-A5/A9/A10/C2-C4/H5).** Only after stages 2-3 prove
   containment, rewrite cap/refusal/retry tests, then remove state/arc/tuple/group/line/compound/order
   representability stops, named-envelope remedies, and old constants while preserving measurements.
   Protected: `ApplyBudget`/`ApplyOutcome`, apply path/candidate budgets, reduplication peel safety,
   the real build pre-expansion, and semantic correctness predicates. Ordering multiplicity and
   chain depth must be classified by call site before deletion; uncertainty blocks that hunk only.
5. **Delete duplicate analysis traversal (E2).** Remove production-emitter-and-discard and separate
   closure characterization walkers from `characterization`, `preexpand`, `emit`, runtime, and
   selection. Gate: analysis performs no production compile/traversal; a selected build performs its
   required pre-expansion exactly once. Protected: cheap grammar facts and real build traversal.
6. **Separate Analyze, Test, and Package (E1/A2/A3).** Move proposal/confirmation/duplicate metrics
   to a post-build corpus operation. `pack` consumes one explicitly named completed artifact and
   never compiles or substitutes payloads. Gate: analysis runs independently; corpus work is absent
   from build-only paths; package rejects missing/stale/mismatched artifacts.
7. **Remove publication overrides (B3).** Keep explicitly local unproven generation/testing metadata;
   delete persistent `CapabilityOverrideRecord` data and all pack/WASM acceptance of unproven output.
   `pangloss pack` rejects `--allow-unproven` and every unproven artifact unconditionally.
8. **Break schemas and sweep stale contracts (B1-B7/F9/I3-I5).** For each schema owner, bump and
   strictly validate the current version, delete aliases/defaults/shims/old fixtures, and update or
   supersede docs/OpenSpec that promise envelopes, retries, preference, build-time corpus work, or
   publication overrides. Historical documents receive a superseded marker rather than fabricated
   retroactive history. Dependency exception already advanced into Stage 2: the truthful
   worker-tree peak-memory metric bumps health and pack manifests to v5 and adds stale standalone/
   embedded-health rejection; the remaining schema sweep stays here.
9. **Verify emitter consolidation separately (D1/G1).** Fix tag-reachability correctness first, then
   require byte/semantic parity over representative grammars before deleting either emitter path.
   No selection, containment, or tuning policy changes belong in this slice.
10. **Narrow secondary slices.** Treat D5, D8, F8, F10, G2-G5, H2-H4, and A9 as separate research or
    correctness tasks with exact files and proof gates. Never issue a Luna task to “clean up all
    remaining cruft.”
11. **Authoritative verification.** The primary agent personally inspects every delegated diff and
    claim, runs focused and relevant full suites through `rust/tools/pg.ps1`, performs residue greps,
    and verifies the final tree is clean. Never narrow tests to hide failure or re-add rejected
    behavior to satisfy a stale test.

### Protected and deferred scope

- **Retain:** grammar-required correctness routing; `MorphotacticIndex`; real build pre-expansion;
  apply-time and reduplication safety budgets; sequential independent explicit builds;
  `Certification::Truncated`; capability facts describing grammar properties.
- **Deferred immediately after cleanup:** within-backend `auto` and configuration exposure; recipe
  search and experimental transformations; precision/strategy scaffolding; whole-grammar Plan IR.
- **Rejected:** named execution envelopes or an “increase envelope” remedy; automatic retry,
  fallback, preference, or top-N; concurrent cross-build kill arbitration.

### Open boundaries—no deletion authority until resolved

- Whether batch, parse, diagnose, assessment, and library convenience constructors are production
  build routes that must consume supervised completed artifacts, or explicitly runtime-only APIs.
- Whether a completed artifact must include a HermitCrab runtime payload immediately, or whether a
  completed Foma payload is the only current serializable artifact.
- Whether `backend_runtime`'s PlanComposed-to-tuned path is deferred within-backend tuning or a
  forbidden cross-backend fallback.
- Whether `witnessed_coverage` remains an explicit compile-consuming evidence command outside the
  normal pipeline.
- Whether static backend-card “Envelope” names describe harmless grammar cost facts or are stale
  execution-envelope terminology.

### Luna removal handoff ledger

| Stage | Exact primary source | Rewrite/delete tests first | Minimum focused proof |
|---|---|---|---|
| 1 raw transport | `worker.rs`, `worker_contract.rs`, `worker_test_child.rs` | `worker.rs` legacy `parse_result_frame` cases; `worker_execution_limits_contract.rs` subprocess cases | `selected_` unit gate plus worker execution-limit integration target; protocol/file-residue grep |
| 2 containment | worker supervisor/host-containment module; CLI limit configuration | worker limit defaults stay; replace watchdog-input/source-string tests with descendant-memory/time tests | Windows Job Object and Linux cgroup-v2 aggregate descendant containment; no artifact on termination |
| 3 explicit backend | `backend_selection.rs`, `completed_build.rs`, `worker.rs`, CLI `pack/main/make_report/fst_health`, `witnessed_coverage.rs` | ranking/preference tests in `backend_selection.rs`, `backend_selection_contract.rs`, five-language, trusted-selected-build, strategy-aware, unordered coverage | explicit P builds only P; P+Q independent; P failure does not suppress Q; no fallback/preference symbols |
| 4 compile refusals | `compose_budget.rs`, `morphotactics.rs`, `analyzer.rs`, `emit.rs`, `preexpand.rs`, `replace.rs`, `uflexc.rs`, `unordered.rs`, `gate.rs`, `health_evaluator.rs` | cap/refusal tests in those modules plus compounding, unordered, closure, and phase-C integration tests | representable builds are not internally resource-refused; external typed containment still fires; apply/redup gates unchanged |
| 5 duplicate traversal | `characterization.rs`, `preexpand.rs`, `emit.rs`, `backend_runtime.rs`, `backend_selection.rs` | characterization/closure tests that demand a dry-run | analysis invokes no production emitter; selected build has exactly one real pre-expansion |
| 6 stage separation | `fst_health.rs`, `pack.rs`, `make_report.rs`, `main.rs`, completed-artifact/manifest code | mixed corpus/build tests and grammar-compiling pack fixtures become completed-artifact fixtures | analyze without compile; corpus consumes artifact; pack contains exact artifact bytes and never compiles |
| 7 publication proof | `pg-pack/trust.rs`, format/manifest, CLI pack/report, `pg-wasm/pack.rs` | overridden-manifest/WASM acceptance and allow-unproven publication tests | local unproven generation remains; every publication route rejects it; no persistent override record |
| 8 schemas/docs | schema owners plus cited docs/OpenSpec | delete stale compatibility fixtures before shims | current round-trip passes; stale versions fail loudly; contract grep matches source |

### Audited Stage 2 kill ledger (`b330892f` anchors)

Execute this only after the safe lifecycle seam and both platform adapters pass their gates:

1. In `worker_execution_limits_contract.rs`, retain/rewrite the descendant timeout and crash proofs
   (lines 168–254) and the protocol classification cases (281–323). Delete the source-shape test
   `supervisor_accepts_execution_limits_as_its_only_execution_control_input` (325–361) only after
   its behavioral replacements are red. Keep defaults/configuration, protocol v9, payload-limit
   authority, exact-payload, wire-versus-execution-limit, and build-identity coverage.
2. Replace `worker.rs:1205–1400`; the old direct loop itself is lines 1252–1400. Remove its one
   `Command::spawn`, four direct `kill`/`wait` pairs, `try_wait`, direct pipe extraction, and three
   unbounded reader/writer joins. Preserve request prevalidation and all raw-protocol helpers.
3. The replacement order is fixed: prevalidate; contained launch; start bounded protocol I/O; poll
   child/containment/time/stderr/protocol; terminate the whole tree on non-success; bound tree drain,
   child reap, pipe closure, and reader joins; accept only clean exit + exact EOF + empty tree + final
   clean containment poll.
4. Rewrite the stale standard-library/caller-owned-containment docs in `worker.rs:1–14,49–54,
   1205–1215` and `lib.rs:338–345`, and then remove `Command`/`Stdio` imports that have no remaining
   use. Retain `SpawnFailed`, but describe a contained launch failure.
5. Do not delete the descendant fixture branches or synthetic protocol-output modes. The obsolete
   `PANGLOSS_WORKER_TEST_SLEEP_MS`/`CRASH` branches are already absent and must not return.
6. Defer Pack's `--watchdog`, health-only branch, placeholder, and hidden-child wording to Stage 3;
   the hidden worker entry point itself remains required. Recipe supervision, Git commands, WSL
   oracle processes, test launchers, and batch threads are explicitly outside this deletion.

Expected old-loop target: 196 source lines including documentation, with 149 lines in the direct
implementation region. Replacement size is not counted as deletion until its committed diff exists.

### Audited Stage 3 kill ledger (`b330892f` anchors)

1. Rewrite chooser tests first: `backend_selection.rs:739–1111`,
   `backend_selection_contract.rs:67–189,312–407`, five-language reports (40–162), and trusted
   selected builds (74–239). New assertions name the requested backend; P builds only P; P+Q run
   sequentially and independently; P failure does not suppress Q; mismatch/stale/missing artifacts
   fail without substitution. Preserve independent capability reports and strategy facts.
2. Delete `BACKEND_PREFERENCE`, `preference_index`, rank keys, `selected`, `preferred`, and
   `select_up_to` from `backend_selection.rs`. Refactor the result into keyed, independent reports;
   analysis reports facts and never returns a production route.
3. Replace `completed_build::select_completed_build` and `PreferredBuildMissing` with validation of
   one explicitly requested strategy. Preserve grammar/attempt identity, realized-route, trust,
   payload-presence, and integrity checks.
4. **Done (`8b218093`, `777df49d`):** the zero-caller `run_selected_compile_worker` convenience API
   was deleted rather than assigned a speculative caller. Its source-shape keeper assertion and
   private `from_wire`/label parser residue were deleted with it. Route encoding/validation,
   protocol-error transport, and the real generic supervisor remain.
5. Make Pack consume one named completed artifact. Delete its fixed `GATED_BACKEND`, in-process and
   watchdog compilation branches, `PLACEHOLDER_FOMA_PAYLOAD`, substitution/explanation state, and
   chooser-derived certificate attachment. Rewrite Pack tests as completed-artifact ingestion tests
   before source deletion.
6. Classify CLI parse/batch, report, FST-health, witnessed coverage, WASM, and FFI constructors at
   the recorded open boundary. Any retained compile-consuming operation must take an explicit route
   or completed artifact; none may silently compile fixed Foma, retry, or switch engines. Preserve
   per-word guess-root fallback and other linguistic/correctness fallbacks.
7. Delete cross-backend retry advice in `health_evaluator.rs` and the advice catalog only where it
   recommends backend substitution. Preserve apply-time caller-controlled remedies and all
   grammar-required routing.
8. Residue gate: no production `BACKEND_PREFERENCE`, `preferred`, `select_up_to`, rank key, implicit
   backend fallback/retry, placeholder Pack payload, or chooser-derived worker route. Optimizer
   winner/Pareto terms remain only inside deferred within-backend tuning.

Expected Stage 3 deletion/rewrite opportunity: roughly 800–1,200 lines in selector/build/worker/
CLI/report/health/witnessed source and tests, plus 150–300 lines if adjacent WASM/FFI APIs are placed
in scope. The 2,717-line registry/mechanism substrate is explicitly excluded.

### Per-commit staging checklist

- Name the single contract changed or preserved.
- List exact files intended for the commit before staging.
- Inspect `git diff -- <files>` and exclude unrelated/user-owned changes.
- Rewrite/delete obsolete tests before interpreting their failures as implementation regressions.
- Stage only those paths or hunks; inspect `git diff --cached --stat` and `git diff --cached`.
- Run focused tests, relevant full suites, and `git diff --check` on the exact staged snapshot.
- Record additions, deletions, and net lines from the committed diff—not from the dirty worktree.

---

## Tally

Committed rebased branch range `1225f25a..341fb5a4` (through the unused envelope-constructor deletion):
**4,581 deletions / 10,500 additions, net +5,919 lines** across 81 files. Production Rust accounts
for 2,099 deletions and Rust integration tests for 988 deletions. This is a branch-wide mechanical
line tally, not a claim that every commit is
cleanup: it includes the ratified charter, designs/plans, replacement tests, and the typed contract
needed before the old containment loop can be removed. The completed raw-transport range removed
432 and added 426 lines in `worker.rs` plus `worker_contract.rs` (net −6 production lines), while
deliberately adding 186 lines of subprocess fixture/integration proof. The Stage 2 typed-outcome/
schema checkpoint added 339 and removed 54 lines; count it as replacement scaffolding, not as a
removal win. The Windows containment checkpoint added 1,739 and removed 128 lines, primarily its
native adapter and adversarial process-tree test harness. It is prerequisite containment, not a
removal win; the authorized shared-loop and routing deletions remain downstream. The committed
Linux wrapper contract and source checkpoint added 1,172 lines and removed 27 across four tool
files. Its 623-line test contract came first; its 549-addition/27-deletion production commit is
prerequisite infrastructure, not a removal win or a Linux runtime pass. The explicit completed-route
slice then removed 98 and added 36 lines across its RED test and GREEN production commits (net -62).
The hosted-Linux gate added 132 RED-contract lines and 212 workflow/script lines without deleting
production code; those 344 lines are prerequisite proof infrastructure, not a removal win.
The selected-wrapper RED/GREEN commits removed 116 and added 3 lines at commit scope (net -113),
including 105 production deletions. The branch-range totals are smaller than additive commit totals
where this cleanup deletes lines introduced after the baseline; both numbers are intentional.
The top-N RED/GREEN commits then removed 67 and added 5 lines at commit scope (net -62), including
13 production lines; their focused backend-selection contract passes 11/11.
The final envelope-constructor RED/GREEN pair removed 9 and added 4 lines at commit scope (net -5),
all nine deletions in production; the same 11/11 contract remains green.
Uncommitted work is never counted until its exact staged snapshot is inspected and committed. Remaining deletion
opportunity is tracked by the stages above; estimates below are directional only:

| Section | Est. lines |
|---|---|
| B — dead backward compatibility | 330–700 |
| C — capped/uncapped apparatus | 1,200–1,800 |
| A10 — cap refusals in `compose_budget.rs` | 400–700 |
| D2 — cross-backend chooser/ranking only; registry/mechanism substrate protected | unmeasured |
| D5 + D8 — remaining duplication | 140 |
| E — work that belongs outside the build | 300–600 |
| F9/F10 — dead machinery and dead-weight tests | unmeasured, likely large |

The 2,717-line registry/mechanism substrate named in D2 remains present and protected; it is not
part of the deletion tally and must not be described as removed. Only the cross-backend chooser and
ranking glue is authorized in this cleanup. Any future five-figure estimate must count only actual
deletions, not the protected substrate. The test suite is the least explored surface and the most
likely to hold the remainder: 2,493 tests, written against a design that has changed twice.
