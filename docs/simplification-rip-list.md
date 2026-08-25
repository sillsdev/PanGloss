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

Status key: **DONE** (implemented, not necessarily committed) · **AUTHORIZED** (decision made; rip
it out) · **REJECTED** (do not build/restore it) · **SPLIT** (execute only the authorized portion) ·
**DEFERRED NEXT** (first follow-on after cleanup) · **OPEN** · **BLOCKED** (needs a decision) ·
**VERIFY** (needs source evidence or measurement). Only the tally of reviewed commits counts as
landed work.

---

## A. Refusal gates (the "grinding to a halt" class)

| # | Item | Evidence | Status |
|---|---|---|---|
| A1 | `ResourceBudgetReached` / `ProvenBoundExceedsBudget` classed as machine-health, so they excluded a backend | `health.rs` class map | **DONE** — moved to the labelling class; `HostContainmentFired` is now the only machine-health code |
| A2 | Pack write gate refused on a severity number | `pack.rs` `validate_health_readiness` | **DONE** — routes on category; oversized artifacts publish with a label |
| A3 | Apply-phase + severity used as a proxy for category | `pack.rs` | **DONE** — tests category directly |
| A4 | `evaluate_via_tuned_emit_mode` rejected on mere *presence* of a finding, before construction | `backend_runtime.rs` | **DONE** |
| A5 | `realize_accuracy_proposer` — same presence-based rejection, second site | `backend_runtime.rs` | **DONE**; dead helper `tuned_surface_resource_refusal` deleted |
| A6 | Marker-bearing candidates banked `Unsupported` with zero work measured | `backend_runtime.rs` | **DONE** — was a revert of `76cf8416`, reinstated by `87320bff` |
| A7 | `--watchdog` structurally could never produce an artifact | `pack.rs` + `worker.rs` | **DONE** |
| A8 | 16 MiB wire frame capped artifact size below the 100 MB label threshold | `worker_contract.rs` | **DONE** — payload moved out of band to a file |
| A9 | `finished_net_digests` — same marker pre-refusal, third site | `backend_runtime.rs` ~1750 | **OPEN** — diagnostic-only, but same false premise |
| A10 | Internal construction caps in `compose_budget.rs` can still stop a representable build | 1,334-line file, 165 refs / 27 files | **AUTHORIZED** — retain useful measurements; delete internal representability/size refusals. The supervised worker's three configured execution limits are the only resource stops |

---

## B. Backward compatibility for users who do not exist

**Nobody is using PanGloss yet.** Every mechanism below exists to read data written by an earlier
version, or to keep a wire shape stable for a consumer that has never existed. All of it is pure
carrying cost, and deleting it now costs nothing.

| # | Item | Evidence | Est. lines | Status |
|---|---|---|---|---|
| B1 | `#[serde(alias)]` on every `Severity` variant for pre-schema-3 spellings, plus the test pinning them | `health.rs` | 60–120 | **DONE** — aliases and the compatibility test removed |
| B2 | `health::OverrideRecord`, kept solely to deserialize already-written reports | `health.rs` | 80–150 | **DONE** — type, field, fixtures, projection, and override-only tests removed |
| B3 | Persistent capability-override records in pack manifests/WASM consumers | `pg_pack::trust`, `readiness_verdict.rs` | — | **AUTHORIZED** — delete from publishable artifacts. Local unproven status survives only in build metadata, which pack rejects |
| B4 | `Certification::MultiplicityMismatch` — doc says "no longer produced, kept for deserializing old reports" | `backend_optimizer.rs` | 20–40 | **DONE** — variant and compatibility fixture removed |
| B5 | `Truncated { corpus: Option<..> }` carries live oracle evidence | `backend_optimizer.rs`, `backend_runtime.rs`, `backend_report.rs` | — | **DONE** — audited and retained; live producers and consumers |
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
| D3 | `oracle.rs` duplicated two `build.rs` helpers verbatim, panic text already drifted | | ~40 | **DONE** |
| D4 | Admission-summary rendering implemented three times | `fst_health.rs`, `pack.rs`, `make_report.rs` | ~20 | **DONE** (2 of 3; the third differs in output, left inline) |
| D5 | `ConfirmedBuckets` flattening copy-pasted three times | `composite.rs` 669, 724, 918 | ~60 | **OPEN** |
| D6 | Remedy rendering diverged between two tables | `make_report.rs` | ~30 | **DONE** |
| D7 | `CompileSizeMode` resolution re-inlined twice | `pack.rs`, `make_report.rs` | ~20 | **DONE** (deliberately left; dies with C1) |
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
| F1 | Five dead match arms — `Refused` is only ever built with two of seven reasons | `characterization.rs` | **DONE** |
| F2 | Test asserting an impossible severity+code pairing | `characterization.rs` ~810 | **DONE** |
| F3 | Test fixture manufacturing pairings production cannot produce | `pack.rs` `synthetic_health` | **DONE** (12 call sites) |
| F4 | Write-only `CompositeRec::morpheme` field | `preexpand.rs` | **DONE** |
| F5 | `#[allow(dead_code)]` where `#[cfg(test)]` lets the compiler enforce the claim | `preexpand.rs`, `unordered.rs` | **DONE** |
| F6 | Duplicate adjacent assertions left by `acd313c6` | `health.rs`, `pack.rs` | **DONE** |
| F7 | Two unlinked copies of the 100 MB threshold | `health.rs`, `readiness_policy.rs` | **DONE** |
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
| I1 | ~24 files of uncommitted work in one tree | **OPEN** — commit |
| I2 | Baseline worktree at `.claude/worktrees/baseline-verify` | **OPEN** — remove |
| I3 | Mismatch ledger: B3, B5, and B6 were inaccurate | **DONE** — corrected from source audit |
| I4 | `2026-08-23-developer-fst-controls.md` drifts both ways; obsolete once C1 lands | **AUTHORIZED** — replace or delete with C1 |
| I5 | Docs referencing envelope retry, automatic selection, build-time corpus work, or compatibility guarantees | **AUTHORIZED** — update in the same slice that removes each behavior |

---

## Execution order: small, intentional commits

Do not preserve the current 18-file dirty diff as a single unit. Partition it by behavior and use
this order unless fresh dependency evidence requires a documented adjustment.

1. **Commit this charter alone.** It is the authority for rewriting tests and rejecting accidental
   restoration during later slices.
2. **Define the new worker/build contract.** Add the finite execution-limit configuration, strict
   worker protocol version, typed outcomes, atomic success artifact, and failed-intermediate cleanup.
   Tests first describe 1 GB/10 GB/10-minute defaults without allocating those quantities.
3. **Remove named-envelope and capped/uncapped behavior.** Delete `CompileSizeMode`,
   `ResourceEnvelopeId`, retry authorization, `--remove-size-limits`, old remedies, persisted fields,
   and tests. Replace them only with the three worker execution limits. Do not stage unrelated
   emitter or selection changes here.
4. **Remove the duplicate precheck traversal.** Delete production-emitter-and-discard APIs and the
   separate recursive synthesize/probe characterization walker. Build retains one real
   pre-expansion traversal. Analysis retains only cheap grammar-derived facts.
5. **Separate build, test, and package.** Build emits a bound artifact; corpus testing consumes it;
   pack verifies and bundles it. Move proposal/confirmation/duplicate metrics out of build. Delete
   all pack-time compilation.
6. **Replace automatic backend selection.** Require configured or explicit backend input; remove
   preferred/top-N/fallback/retry selection and winner/Pareto output. Preserve per-backend
   representability reports and raw corpus comparison metrics.
7. **Freeze within-backend tuning.** Do not redesign recipe search, plan transformations, precision
   modes, or automatic tuning in this cleanup. Make only narrow compile-preserving adaptations to
   the explicit-backend and separated-stage contracts. Bank the completed switch audit as the first
   task immediately after cleanup.
8. **Handle local unproven artifacts.** Keep developer-only generation and corpus consumption, mark
   build metadata unproven, and make pack rejection unconditional. Delete persistent override
   records from pack/WASM schemas.
9. **Break old schemas deliberately.** Bump health/report/pack/worker versions as applicable and
   delete all backward-reading shims and compatibility-only tests in the same commits.
10. **Land emitter consolidation separately.** First repair the unconditional templated-path
    `MorphotacticIndex` construction. Inspect the complete diff and require semantic/byte parity plus
    the retained real-language gates. Do not use this refactor to smuggle in policy changes.
11. **Sweep secondary cruft.** Only after the new pipeline is green: stale F9 docs, duplicate
    flattening, dead finding constructors, unreachable variants, and tests with no live producer.
12. **Authoritative verification.** The primary agent personally reviews every delegated diff and
    claim, then runs the focused gates and relevant full suites through `rust/tools/pg.ps1`. Never
    narrow the test set to hide a failure and never re-add rejected behavior to satisfy a stale test.

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

Committed cleanup so far: **386 deletions / 90 additions, net −296 lines** across four reviewed
commits. Uncommitted work is not counted as removed until its exact staged snapshot is reviewed and
committed. Identified remaining deletion opportunity, conservatively:

| Section | Est. lines |
|---|---|
| B — dead backward compatibility | 330–700 |
| C — capped/uncapped apparatus | 1,200–1,800 |
| A10 — cap refusals in `compose_budget.rs` | 400–700 |
| D2 — duplicate capability substrate (blocked) | 2,717 |
| D5 + D8 — remaining duplication | 140 |
| E — work that belongs outside the build | 300–600 |
| F9/F10 — dead machinery and dead-weight tests | unmeasured, likely large |

Reaching five figures depends on D2 (2,717) plus F10 (tests) plus C. The test suite is the least
explored surface and the most likely to hold the remainder: 2,493 tests, written against a design
that has changed twice.
