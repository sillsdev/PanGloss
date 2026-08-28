# Simplification cleanup plan and rip list

Living implementation plan. Git history is the recovery mechanism; PanGloss is pre-alpha and has no
external compatibility obligation. Old behavior must not be restored merely because a test expects
it. Update or delete that test first when it pins a contract explicitly rejected below.

## Rip-first execution order (2026-08-27)

No one depends on the current pre-alpha implementation. Finish demolition before building its
replacement. Temporary compile holes are expected evidence that an old route was actually removed;
they are recorded, not repaired, during this phase.

1. Freeze destination wiring, adapters, replacement APIs, positive replacement tests, and compile
   repairs.
2. For one rejected contract at a time, delete or rewrite the tests that require it.
3. Delete that contract's source route, data plumbing, flags, advice, fixtures, and documentation.
4. Stage and commit the narrow deletion tranche; record its exact additions/deletions and any
   intentional compile holes.
5. Repeat until a fresh symbol/call-site audit finds no authorized removal inventory.
6. Only then define the smallest coherent explicit-backend/completed-artifact surface, repair the
   remaining compile holes, and add tests for that final surface.

During steps 1-5, do not run Cargo or interpret compilation as an acceptance gate. Use structural
checks, protected-file hashes, diff inspection, and residue searches. A test failure cannot justify
restoring rejected behavior. Safety-sensitive hunks remain deferred until their explicitly recorded
approval is obtained; that deferral does not block unrelated demolition.

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
| A1 | `ResourceBudgetReached` / `ProvenBoundExceedsBudget` classed as machine-health, so they exclude a backend | `health.rs:468-487`, `backend_selection.rs:227-252` | **LANDED UNVERIFIED** — `0e57b994`/`98d4d423`/`870d84e6`; retain them as measurements/labels, never cross-backend selection input |
| A2 | Pack write gate refuses on a severity number | `pack.rs` | **LANDED UNVERIFIED** — `0e001bdc` removed the stale test and `0e3240c2` removed `validate_health_readiness` and its one call site, with the consequence explicitly accepted by the user. The gate never blocked publication: nothing writes a pack, and its only caller was `make-report --pack`, which reads one. Publication follows capability proof, not size/readiness severity, and any future publication route must reject unproven and unready output at the point of publication |
| A3 | Apply-phase + severity used as a proxy for category | `pack.rs`, oversized-pack test | **LANDED UNVERIFIED** — removed with A2 by `0e3240c2`; the stale test went first in `0e001bdc`, as required |
| A4 | `evaluate_via_tuned_emit_mode` rejected on mere *presence* of a finding, before construction | historical `backend_runtime.rs` | **LANDED UNVERIFIED** — removed by `516821e0`; the live function now builds directly |
| A5 | `realize_accuracy_proposer` / `tuned_surface_resource_refusal` repeated the pre-refusal | historical `backend_runtime.rs` | **LANDED UNVERIFIED** — removed by `516821e0`; the live function now realizes directly |
| A6 | Marker-bearing candidates banked `Unsupported` with zero work measured; `finished_net_digests` marker rejection | `backend_runtime.rs::unbuildable_marker_reason` | **RETAINED** — semantic representability, not a resource estimate. Marker-bearing `PlanComposed` candidates are rejected before partial-network measurement; the PlanComposed-network guard remains incomplete for those marked subtrees |
| A7 | `--watchdog` structurally cannot produce a real artifact | historical CLI pack producer/watchdog path | **LANDED UNVERIFIED** — `54508605`/`8889877b`/`c7fe5aaf`/`f1b46d49` plus residue cleanup `0f8ac724`/`dfeeb7ad`/`a00ac0ee`; old producer/placeholder claims and deleted-command coverage are removed |
| A8 | 16 MiB result metadata frame must not cap the selected payload | `worker_contract.rs`, `worker.rs` | **VERIFIED** — protocol v9 uses an independently bounded raw frame; filesystem transport and legacy parser/capture residue are deleted; prefix-before-allocation, clean exit, malformed streams, and supervisor-limit authority are proven |
| A10 | Internal construction caps in `compose_budget.rs` can still stop a representable build | 1,334-line file, 165 refs / 27 files | **PARTIAL — RIPPED FIRST** — retain useful measurements; delete internal representability/size refusals. The supervised worker's three configured execution limits are the only resource stops. `318c9f7d`/`11fff5e4`/`77226079` removed the dead uflexc/gate/`build_controllable` budget-forwarding chain, and `b3c8d14d`/`7eb17a3b`/`86fb56fb` removed the three higher-level parameters it left unread |

---

## B. Backward compatibility for users who do not exist

**Nobody is using PanGloss yet.** Every mechanism below exists to read data written by an earlier
version, or to keep a wire shape stable for a consumer that has never existed. All of it is pure
carrying cost, and deleting it now costs nothing.

| # | Item | Evidence | Est. lines | Status |
|---|---|---|---|---|
| B1 | `#[serde(alias)]` on every `Severity` variant for pre-schema-3 spellings, plus the test pinning them | `health.rs` | 60–120 | **LANDED UNVERIFIED** — aliases and the compatibility test removed; full completion gate is not recorded here |
| B2 | `health::OverrideRecord`, kept solely to deserialize already-written reports | `health.rs` | 80–150 | **LANDED UNVERIFIED** — type, field, fixtures, projection, and override-only tests removed; full completion gate is not recorded here |
| B3 | Persistent capability-override records in pack manifests/WASM consumers | `pg_pack::trust`, `readiness_verdict.rs` | — | **LANDED UNVERIFIED** — `67c661cc`/`05ba71b8` removed persistent override representation and acceptance; `1cad7f2c` removed the stale make-report pack-trust projection. Any future publication route MUST reject unproven output |
| B4 | `Certification::MultiplicityMismatch` — doc says "no longer produced, kept for deserializing old reports" | `backend_optimizer.rs` | 20–40 | **LANDED UNVERIFIED** — variant and compatibility fixture removed; full completion gate is not recorded here |
| B5 | `Truncated { corpus: Option<..> }` carries live oracle evidence | `backend_optimizer.rs`, `backend_runtime.rs`, `backend_report.rs` | — | **PROTECTED** — audited and retained; live producers and consumers |
| B6 | `HEALTH_SCHEMA_VERSION` stamps and validates stored health artifacts | `health.rs`, `fst_health.rs` | — | **LANDED UNVERIFIED** — strict current v7 validation and stale-version rejection are recorded in `49163cb8`/`3d1750f1`/`12d3d2bb`; current optional fields are retained as live schema fields, not compatibility defaults |
| B7 | `ResourceEnvelopeId` versioned identity (`ManagedV1`, `TunedSurfaceWork10kV1`) | named-envelope identity and persisted provenance | see C1 | **LANDED UNVERIFIED** — `aff28856`/`22884062`/`1346cea7` removed the obsolete identity, provenance, and worker plumbing; `2a1138ef` removed stale manifest-field coverage |

---

## C. Replace "capped vs uncapped" with explicit worker limits

The old named-envelope and size-mode system has been removed. The retained build path uses three
externally enforced, configurable execution limits owned by `pg-worker-containment::ExecutionLimits`:
1 GiB serialized payload, 10 GiB committed process-tree memory, and 10 minutes. Logical and
apply/candidate budgets remain separate.

| # | Item | Est. lines | Status |
|---|---|---|---|
| C1 | Delete the two-envelope / size-mode system; retain finite external limits | 1,200–1,800 | **LANDED UNVERIFIED** — `aff28856`/`22884062` removed obsolete worker-control tests/source. `pg-worker-containment::ExecutionLimits` and its 1 GiB/10 GiB/10-minute defaults remain, with the containment contract retained |
| C2 | Delete `RetryAuthorization`, automatic backend retry, and "increase envelope" remedies/tests | included above | **LANDED UNVERIFIED** — `c3b8aeaa` removed compile-retry advice and `22884062` removed obsolete retry/control plumbing; logical/apply budgets remain |
| C3 | Delete `resource_envelope.rs` named-profile/digest machinery | included above | **LANDED UNVERIFIED** — `1346cea7` deleted the named-profile/digest module and `22884062` removed remaining worker-control plumbing |
| C4 | Delete `--remove-size-limits`; keep finite execution limits and local developer-only controls | included above | **LANDED UNVERIFIED** — `aff28856`/`22884062`/`1346cea7` removed the obsolete flags and source controls; strict removed-flag and old-envelope-field rejection tests remain |

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
| D7 | `CompileSizeMode` resolution re-inlined twice | `pack.rs`, `make_report.rs` | ~20 | **LANDED UNVERIFIED** — `22884062`/`1346cea7` removed the old size-mode/envelope resolution; protected-file historical compile-hole residue remains outside this tranche |
| D8 | Three modules hand-assemble a 10-field `HealthFinding` literal; no shared builder | `health_evaluator.rs`, `characterization.rs`, `fst_health.rs` | ~80 | **OPEN** |

---

## E. Work that belongs outside the build

| # | Item | Evidence | Est. lines | Status |
|---|---|---|---|---|
| E1 | `ProposalVolume`, `ConfirmationWork`, `DuplicateAnalysisOverlap` computed during the build | `fst_health.rs` | 200–400 | **PARTIAL — RIPPED FIRST** — `070de6c6`/`3fd3afdf` removed the word-corpus tests, compiler/apply path, findings, and CLI word argument; `fst-health` is grammar characterization only. A replacement post-build corpus operation is intentionally deferred until all ripping is exhausted |
| E2 | The discarded double traversal: the pre-check runs or reproduces the production emitter's closure walk, then the real compile runs it again | `characterization.rs` + `preexpand.rs` + `backend_runtime.rs` | 100–200 | **LANDED UNVERIFIED** — `ab0ed0ed`/`516821e0` plus follow-up wrapper deletions `c744cc4f`/`2ab00e08` removed dry-run tests, duplicate walkers, runtime/selection prechecks, and dead wrappers. Cheap grammar analysis, production trace/evidence, and the one required real build traversal remain |

---

## F. Dead, vacuous, and misleading code

| # | Item | Evidence | Status |
|---|---|---|---|
| F1 | Five dead match arms — `Refused` is only ever built with two of seven reasons | `characterization.rs` | **LANDED UNVERIFIED** |
| F2 | Test asserting an impossible severity+code pairing | `characterization.rs` ~810; a second live instance in `health.rs::admission_is_unchanged_by_the_per_class_view` | **LANDED UNVERIFIED, then reopened and closed** — the row was marked landed while a live instance remained: `BackendCoverageIncomplete` (Representability) at `LargeMultiplier` (Readiness), and `BackendCompilationFailed` (Process) at `MachineLimit` (Containment). Both were invisible until `HealthFinding::new`'s class/severity assertion caught them. Fixed by giving the test codes whose class matches the severity it wants; the assertion it pins reads severities only |
| F3 | Test fixture manufacturing pairings production cannot produce | `pack.rs` `synthetic_health` | **LANDED UNVERIFIED** (12 call sites) |
| F4 | Write-only `CompositeRec::morpheme` field | `preexpand.rs` | **LANDED UNVERIFIED** |
| F5 | `#[allow(dead_code)]` where `#[cfg(test)]` lets the compiler enforce the claim | `preexpand.rs`, `unordered.rs` | **LANDED UNVERIFIED** |
| F6 | Duplicate adjacent assertions left by `acd313c6` | `health.rs`, `pack.rs` | **LANDED UNVERIFIED** |
| F7 | Two unlinked copies of the 100 MB threshold | `health.rs`, `readiness_policy.rs` | **LANDED UNVERIFIED** |
| F8 | `Certification::StaticRejected` was unreachable | `backend_runtime.rs`, `backend_optimizer.rs` | **LANDED UNVERIFIED** — `476d5f5e` removed its tests first, `516821e0` removed both producers and their helper, and `50848d3c` removed the enum variant; current source has no residue |
| F9 | Stale `FailClosed` / `RefusalWitness` docs/tests — source machinery is already absent | `capability.rs` and ledgers/docs | **PARTIAL — RIPPED FIRST** — `1e835455`, `1fbda466`, and `5cfa5ecf` removed active ledger/conformance/staging claims and corrected the four-witness and WASM boundaries. Archived historical records remain history; do not recreate source behavior |
| F10 | Dead-weight tests: 2,493 tests, some pinning behaviour being deliberately removed, some vacuous | whole suite | **OPEN** — the second-pass review's main target |

---

## G. Correctness risks (not tidiness — do these regardless)

| # | Item | Evidence | Status |
|---|---|---|---|
| G1 | The default shipping backend may lack a tag-reachability check the other backend has | `emit.rs` `verify_tags_reachable`, on for templated only | **OPEN** — possible silent wrong output; highest-priority open item |
| G2 | Grammar-derived regex rejection `panic!`s inside functions that already return `Result` | `replace.rs` 875, 896, 1514 | **OPEN** — needs `ComposeError::RegexRejected`; 12 files reference `ComposeError::` |
| G3 | `panic!` if `compounding_max_depth` misses a `Compounding` id, in the production walk | `capability.rs` ~1348 | **OPEN** |
| G4 | Two diagnostics surfaced by `eprintln!` because `Certification` has no field to carry them | `backend_runtime.rs` | **OPEN** |
| G5 | `pg-foma` test failures, counted from a run that actually finished | was 18; now **1**, classified in the 2026-08-28 attribution below | **OPEN** — the row previously read "one pre-existing test failure", which was an artifact of measurement: every earlier run stopped at the first failure, so 838+ tests never executed. `--no-fail-fast` at `ff29935b` reports 1,051 run, 1,033 passed, **18 failed**. `morphotactics_boundary_cleanup_slice` is the one the row used to name |
| G6 | Four `pg-cli` optimizer tests fail — **not cleanup damage; they fail on `main`** | `four_grammar_recipe_evidence` (FIXED); `recipe_optimize_continuation` x3 (still open) | **PARTIAL.** Earlier reading — "three of the four assert candidate abandonment by a resource bound, the envelope machinery this cleanup deleted" — was WRONG, and the correction matters: `Certification::ResourceBreach` still exists, and the three continuation test files are byte-identical between `main` and this branch. The real cause is `87320bff feat(foma): enforce complete FST proposals` (2026-08-22, an ancestor of `main`), which refuses marker-bearing plan-composed candidates. `four_grammar_recipe_evidence` expected 5 feasible for `mpr-gated-exception`, an expectation written 2026-07-30, three weeks before that feature; the measured value is 2 and 2 is correct, since the fixture's baseline plan is marker-bearing. Re-pinned to 2 with the reason in the test. The three continuation tests remain OPEN and are blocked on a capability decision, not a test fix — see `docs/research/cleanup-churn-log.md` items 5-6: their fixture `backend-strata-generic` now confirms ZERO candidates because both whole-grammar emitters report `Partial { uncovered: 1 }` while `fst-health` reports `representability=WithinLimits` for the same grammar |
| G7 | A worker request's `ComposeBudget` never reaches the compile it is documented to bound | `worker.rs:6` says the child "compiles the named grammar under the request's `ComposeBudget`"; `worker.rs:315` builds one from the request and passes it to `FomaProposer::new_with_budget_and_profile`, whose core (`analyzer.rs::new_with_budget_and_profile_policy`) ignores it -- the parameter is unread, and `emit_with_budget_profiled` takes no budget at all | **OPEN, PRE-EXISTING** -- byte-identical at `ff29935b`, so no tranche in this file caused it. Both real consumers read the environment themselves rather than accepting a threaded budget: the per-word peel cap at `composite.rs:618` and `:1176` (`ComposeBudget::from_env()`), and the compound unroll cap at `emit.rs:2101`. So a worker request carrying a non-default chain-depth cap is silently ignored, and only a process-wide `HC_COMPOSE_*` variable has any effect. **Needs a decision, not a cleanup:** either wire the request's budget through to both consumers, or delete `WorkerRequest::compose_budget` and correct the module doc to say budgets are process-scoped. Deleting only the dead parameter would silence the compiler warning that is currently the sole evidence of the gap, so it is deliberately left in place |

---

## H. Structural (deferred past alpha by review, recorded so it is not lost)

| # | Item | Evidence | Status |
|---|---|---|---|
| H1 | `plan.rs` is documented as an IR but only one controllable adapter interprets it; whole-grammar backends ignore it | `enumerate.rs`, `lowering_adapter.rs` | **DEFERRED NEXT** — decide in the immediate post-cleanup switch round; do not redesign it now |
| H2 | `capability.rs` — 3,942 non-test lines, 15 predicates, one file | | **OPEN** — split, do not rip |
| H3 | Adding a backend is a shotgun edit: 162 references across 15 files | | **OPEN** — simplify after D2 removes chooser/ranking coupling |
| H4 | `PlanComposed` / `uflexc` is the weakest backend with known whole-construct holes | `strategy_coverage.rs` 142 | **VERIFY** — it may remain an explicitly selectable backend if capability analysis reports those holes honestly; it gets no fallback/preference role |
| H5 | Old uncalibrated constants | 3,000 / 100 MB / 100 / 512 | **PARTIAL — RIPPED FIRST** — delete non-semantic truncation/refusal constants. `1749195f` removed `MAX_RENDER_VARIANTS` and its silent finite-variant truncation. `8b8277bf`/`f23ad388` removed the arbitrary `MAX_QUANTIFIER_BOUND = 512` finite-quantifier ceiling; large finite quantifiers lower natively and unsupported pattern shapes are still refused on semantic grounds. Replacement execution defaults are 1 GB payload / 10 GB committed RAM / 10 minutes and are configurable, finite, and non-semantic. Closure/iteration/chain caps with live termination roles remain protected or deferred |
| H6 | Concurrent "kill the right one" scheduler | | **REJECTED** — explicitly selected builds run sequentially, so no cross-build resource arbitration machinery is needed |

---

## I. Housekeeping

| # | Item | Status |
|---|---|---|
| I1 | Historical large dirty-tree churn | **RETAINED DISCIPLINE** — every slice is separately committed; final snapshot must be clean |
| I2 | Baseline worktree at `.claude/worktrees/baseline-verify` | **VERIFIED** — path is absent |
| I3 | Mismatch ledger accuracy | **LANDED UNVERIFIED** — B3 publication-override persistence is resolved by `67c661cc`/`05ba71b8`/`1cad7f2c`; B6 strict versioning remains tracked above |
| I4 | `2026-08-23-developer-fst-controls.md` drifts both ways; obsolete once C1 lands | **LANDED UNVERIFIED** — the document was deleted with the named closure controls in `1c8e1773`; nothing in the tree references it |
| I5 | Docs referencing envelope retry, automatic selection, build-time corpus work, or compatibility guarantees | **RETAINED DISCIPLINE** — a rule about how each slice works, not an item to delete: every tranche above updates the prose in the same slice that removes the behaviour, prose-last. Residue is found by the per-tranche residue searches, not by a standing backlog row |

---

## Execution order: small, intentional commits

Use the stages below unless fresh dependency evidence is written into this file. Each stage is one
or more bounded Luna slices with disjoint file ownership. `AUTHORIZED` permits deletion only inside
that stage's allowed scope. `VERIFY`, `OPEN`, `DEFERRED NEXT`, and protected scope are never deletion
permission.

**2026-08-26 rip-first sequencing decision:** PanGloss is pre-alpha and has no compatibility users.
Delete every authorized obsolete test/contract first, then delete its implementation. Deliberate
compile holes are expected until the removal inventory is exhausted; do not restore rejected
behavior merely to make a stale test or intermediate build pass. Build the smaller explicit path
only after the rip stages below. During the rip, use `git diff --check` and targeted residue searches;
defer Cargo and hosted-platform proof until the replacement stage unless a deletion can be checked
without pulling replacement design forward.

**Marching order for every remaining tranche:** (1) research and classify the candidate as obsolete
resource policy, required semantic termination/correctness, or unresolved; (2) delete tests and
fixtures that require the obsolete contract; (3) delete its source/API/schema/plumbing in a separate
commit; (4) inspect the complete diff and run residue/protected-symbol checks; (5) record intentional
compile holes and move on. Do not add a replacement abstraction, compatibility adapter, convenience
API, or new positive test during steps 1-5. Replacement work begins only when the authorized removal
inventory is exhausted. A safety finding narrows or defers that hunk; it never authorizes restoring
already rejected behavior.

1. **Finish raw selected-payload transport (A8).** Delete the rejected filesystem transport, legacy
   aggregate-result parser tests/helper, and stdout-only overflow residue. Add subprocess proof for
   missing, truncated, trailing, malformed, and stalled payloads. Gate: protocol 8 rejection; exact
   length/SHA/fingerprint/EOF; no selected-artifact paths, files, hard links, cleanup, or ownership
   code. Status: **VERIFIED**.
2. **Remove the old supervisor; retain the external-containment destination (C1).** The destination
   enforces the configurable 1 GB final payload, 10 GB
   committed process-tree RAM, and 10-minute wall limit on Windows and Linux. Every production build
   must use it. Gate: descendants die with the worker; memory/time/crash/partial output produce no
   completed artifact and structured provenance. Protected: sequential independent P/Q attempts.
   Status: **RIPPED FIRST.** `5c4b27de` deleted the stale source-shape test and `95ac164d` deleted the
   direct-`Command` supervisor. The external adapters and hidden worker child remain as destination
   substrate. The hosted Linux correction exists in `032e0076`/`1c7cc837`; rerun it only after the
   minimal explicit route is assembled.
3. **Cross-backend automatic choice and watchdog/placeholder paths (D2/A7).** Preference, top-N,
   fallback, retry, winner, and Pareto selection paths are removed; the worker receives an explicit
   backend and validates the result matches it. Protected: independent per-backend analysis reports,
   registry/mechanism capability facts, grammar-required correctness routing, and deferred
   within-backend tuning.
   Status: **WATCHDOG/PLACEHOLDER DELETION LANDED; ROUTE-WIRING REMAINS DEFERRED.** `e10ab3ca`/`64323d45` removed chooser assertions and APIs;
   `54508605`/`8889877b`/`9396c7b3`/`f1b46d49` removed legacy Pack compile tests, build machinery,
   implicit report builds, and callers. `a148d2e6`/`c3b8aeaa` removed compile-retry assertions and
   advice. `b3521c0e`/`7982e6ee`/`1c2fc84b` removed the last direct-Foma parse/batch test, fixed
   backend capability gate, engine flags, in-process compiler branches, and dead help. The resulting
   shared-helper holes were followed forward: `c143c532`/`40891313`/`e03e83e9` deleted the live
   Foma stats tests, helper, and fixture instead of restoring them. `7d987667`/`45633e83` removed
   cross-backend substitution advice. `d8dda4f7`/`af32379a` removed WASM tests and source for
   automatic Foma compilation/fallback; `99f478b2` removed its severity-based publication boolean;
   `a00ac0ee` removed the remaining stale producer/placeholder documentation.
   Protected `make_report.rs` still references the deleted `foma_invalid_shape`, `GATED_BACKEND`,
   and `gated_backend_decision`; these are intentional forward compile holes whose consumers must be
   deleted in the report/corpus tranche. Explicit destination wiring and completed-artifact
   ingestion are intentionally not added yet. The hidden worker child remains required.
4. **Delete internal compile refusal caps (A1-A5/A9/A10/C2-C4/H5).** Rewrite cap/refusal/retry tests,
   then remove state/arc/tuple/group/line/compound/order
   representability stops, named-envelope remedies, and old constants while preserving measurements.
   Protected: `ApplyBudget`/`ApplyOutcome`, apply path/candidate budgets, reduplication peel safety,
   the real build pre-expansion, and semantic correctness predicates. Ordering multiplicity and
   chain depth must be classified by call site before deletion; uncertainty blocks that hunk only.
   Status: **RIP IN PROGRESS.** `79d1b058`/`0cf2da0a` removed the compound-pair test and the entire
   `HC_COMPOUND_PAIR_BUDGET` refusal while retaining compound emission, licensing, and chain-depth
   safety. `1aac38b7`/`566606ec` removed the post-operation state/arc net-size refusal while retaining
   telemetry. `4e1339f8`/`0087d5f1` removed the emitted-line refusal; `d52587f3`/`731e8fd2`
   removed the abandoned-thread compose timeout; `fecfab9d`/`104e3971`/`2e0a8180` then removed
   their serialized no-op fields and fixture arguments. `656f4f49`/`4e6fdf55` removed tuple/group
   refusal fixtures and implementation while preserving real alpha enumeration, agreement, gate
   partitioning, and compilation. `39ccffbe`/`08bf3560`/`139868d3` removed the uncalibrated
   100-rule unordered hard stop from tests, compile configuration, worker protocol, health,
   characterization, and capability routing while preserving `Cascade::combination` and exact
   unordered rule-count facts. The external 1 GiB payload, 10 GiB worker-tree RAM, and 10-minute
   limits remain. Chain-depth safety remains protected pending its separate call-site audit. Worker
   protocol v10 is required in the later schema/add phase. `69efc9dc`/`03745a5e` removed the eager
   enumeration budget and worker/health refusal plumbing. `ae67f086`/`f878435e`/`f782c2ff`/
   `1c8552c9` removed profile threshold tests, reference-band decisions, dead health projection, and
   the single-value profile label while retaining raw compile measurements. `4e11c3d5`/`27f0cafe`/
   `da9d2a00` removed dead health metrics/findings and broke their stale schema fixtures. `81375995`/
   `ff2d9ae6` removed no-op compose-wrapper tests and source/API error plumbing while preserving the
   real chain-depth checks. The TunedSurface closure work/depth caps remain explicitly deferred after
   a safety audit found a cross-rule nontermination case. `8b8277bf`/`f23ad388` then removed the
   arbitrary `MAX_QUANTIFIER_BOUND = 512` finite-quantifier ceiling test-first and source-second,
   with its documentation residue swept in `9bf811f6`/`78e0d319`/`07d46165`. `318c9f7d`/`11fff5e4`/
   `77226079` then removed the dead `ComposeBudget` forwarding chain through `uflexc`/`gate`/
   `build_controllable` while retaining every chain-depth, apply, compound-chain, and closure guard.
5. **Delete duplicate analysis traversal (E2).** Remove production-emitter-and-discard and separate
   closure characterization walkers from `characterization`, `preexpand`, `emit`, runtime, and
   selection. Gate: analysis performs no production compile/traversal; a selected build performs its
   required pre-expansion exactly once. Protected: cheap grammar facts and real build traversal.
   Status: **LANDED UNVERIFIED.** `ab0ed0ed`/`516821e0` plus `c744cc4f`/`2ab00e08` removed dry-run tests,
   duplicate walkers, emitter-and-discard wrappers, selection/runtime prechecks, and dead wrappers:
   1,017 deleted, 4 structural lines added. Production trace/evidence and the one required real build
   traversal remain. The callerless closure-advice residue was removed test-first in `e4e35359` and
   source-second in `5528ae4f`; no replacement advice path was added.
6. **Build the smaller explicit path and separate Analyze, Test, and Package (E1/A2/A3).** Only after
   authorized ripping is complete, move proposal/confirmation/duplicate metrics
   to a post-build corpus operation. `pack` consumes one explicitly named completed artifact and
   never compiles or substitutes payloads. Gate: analysis runs independently; corpus work is absent
   from build-only paths; package rejects missing/stale/mismatched artifacts.
7. **Publication overrides (B3) are removed.** Persistent `CapabilityOverrideRecord` data and pack/WASM
   acceptance of unproven output are deleted. Local unproven generation/testing metadata remains; any
   future publication route MUST reject unproven output.
   Status: **LANDED UNVERIFIED.** `67c661cc` deleted publication-acceptance tests and `05ba71b8` deleted
   persistent override/trust types, the manifest field, and WASM override signal APIs. Local
   parse/batch readiness metadata remains. `1cad7f2c` then removed `make-report`'s stale persistent
   pack-trust projection. Manifest schema v6 and strict v5 rejection belong to the later schema/add
   phase.
8. **Break schemas and sweep stale contracts (B1-B7/F9/I3-I5).** For each schema owner, bump and
   strictly validate the current version, delete aliases/defaults/shims/old fixtures, and update or
   supersede docs/OpenSpec that promise envelopes, retries, preference, build-time corpus work, or
   publication overrides. Historical documents receive a superseded marker rather than fabricated
   retroactive history. Dependency exception already advanced into Stage 2: the truthful
   worker-tree peak-memory metric bumps health and pack manifests to v5 and adds stale standalone/
   embedded-health rejection. `6576c05e`/`959f5b01` removed old-pack compatibility promises and
   missing-current-pack-field defaults. `346b0737`/`6e03444e` removed legacy optimizer-report
   fixtures, the custom candidate compatibility deserializer, provenance scaffolding, and defaults
   for required current score/report/pilot fields. Semantic optionals and strict current version
   checks remain. Schema/protocol bumps and unknown-field rejection stay in the later add phase.
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

- **Resolved 2026-08-27:** `parse` and `batch` remain full-engine runtime operations, with their
  direct Foma branches deleted. `diagnose` is deleted. The grammar/corpus `assess` producer and
  `investigate --grammar` rerun attribution are deleted; `compare`, `golden-diff`, and report-only
  `investigate` remain as artifact consumers. No replacement completed-artifact route is added
  during demolition.
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

### 2026-08-27 CLI assessment-producer deletion tranche

Commits `f6852b18` and `84c3267d` complete the deletion-first tranche: **1,672 deletions / 5
additions** across the tests and CLI producer surface. The grammar/corpus `assess` producer and
`investigate --grammar` rerun attribution are removed; `compare`, `golden-diff`, and report-only
`investigate` remain. CLI acceptance coverage for those retained consumers, including strict
rejection of removed flags, is deferred until the post-demolition replacement/repair phase. Old
producer-coupled tests must not be restored.

### 2026-08-27 direct make-report compile route deletion

`make-report` built a `FomaProposer` in the CLI process to time itself compiling, time per-word
analysis, and run a corpus — a compile with no memory or time ceiling, and one of the five
uncontained paths the inventory below records. Removed with the user's explicit approval,
test-first (`a862d7b3`), source-second (`b8685a0c`), prose-last (`e00474da`):
**352 deletions / 23 additions, net -329 lines**.

Deleted because nothing else called them: `build_report_analyzer`, `measure_timer_floor_ns`,
`latency_measurement`, `render_latency_measurement`, `percentile_ns`, `default_word_list`,
`measure_latency_percentiles_ns`, and `measure_coverage_rate` — the last of which also called the
already-deleted `crate::foma_invalid_shape`, closing one forward compile hole. The flags whose only
consumer was that route went with it: `--words`, `--corpus`, `--attestor`, `--attested-on`,
`--repeats`, plus the corpus read, the all-three-together validation, and both usage strings.

`make-report` now reports on the artifact it is given. `measurements` is `None`, and build time,
latency, and coverage say they were not measured rather than reporting a number the command has no
honest way to produce. The verdict path is unchanged: `certify_with_semantics` already accepts a
`None` measurements — that is exactly what the refused-grammar branch has always passed it.
`Measurements`/`LatencyMeasurement`/`CoverageAssessment` stay in `pg-foma::readiness_verdict`, and
the golden-render tests that build one by hand still exercise the renderer.

Two pre-existing holes in `run_make_report` were neither repaired nor widened: the `pack_path`
match has no `None` arm, and `trust` is never assigned on the `else` branch. Both belong to the
producer deletion that preceded this one, and both are for the replacement phase — which will have
to decide whether `make-report` requires an explicitly named artifact, the direction the ratified
Package contract points.

Status: **LANDED UNVERIFIED.** Structural acceptance only: caller counts for every deleted helper,
a residue sweep for each removed flag across both `make_report.rs` and `main.rs`, an unused-import
check, `git diff --check`, per-commit `--numstat`, and full diff inspection. No Cargo was run.


### 2026-08-27 REP_VARIANT_CAP containment inventory — verdict: still NO-GO

The read-only production-call inventory the handoff required, run against `86fb56fb`. It asks
whether every caller of `surface_variants`, `surface_variants_concat`,
`surface_insert_action_variants`, `pattern_variants`, and `stripped_variants` reaches them only
inside the supervised worker's process tree, under finite memory and time limits.

**It does not, and the answer is not close.** Those five functions run during lexc emission and
pre-expansion, so the question reduces to which entry points compile a proposer. Every
`FomaProposer::new*` construction outside a test:

| Entry point | Contained? |
|---|---|
| `pg-foma::worker.rs:313` (`new_with_budget_and_profile`) | **yes** — this is the supervised worker |
| `pg-cli::make_report.rs` (`new_unproven_with_profile` / `new_with_profile`) | **removed** — `a862d7b3`/`b8685a0c`/`e00474da` deleted this route outright; four in-process paths remain, so the verdict below is unchanged |
| `pg-ffi::grammar.rs:59` (`new`) | no — in-process inside an embedding host application |
| `pg-foma::backend_runtime.rs:1415,2014` (`new`) | no — in-process on the evaluate/assess corpus paths |
| `pg-foma::composite.rs:611` (`new`) | no — in-process on the runtime analyzer path `parse`/`batch` use |
| `pg-foma::witnessed_coverage.rs:115` (`new`) | no — in-process |

Five live in-process production paths, and the handoff's own rule is that one is enough:
**the cap-removal patch stays NO-GO.** Removing a recall-losing overflow bound is only safe where an
external limit catches the blow-up instead, and on the FFI path in particular there is no such limit
at all — the blow-up lands in the host application's address space.

**Two further findings, independent of containment.** The uncommitted `emit.rs` patch changes
`surface_variants`/`surface_variants_concat`/`stripped_variants`/`pattern_variants` from
`(Vec<String>, bool)` to a bare `Vec<String>`, but six call sites outside `emit.rs` still consume the
tuple — `precision.rs:441,501` and `preexpand.rs:326,581,663,666`. The patch as it stands does not
build.

More importantly, `precision.rs` does not merely *carry* the overflow flag, it **decides on it**:
both sites match `Some((variants, false))` and fall through to `Unsupported` / `None` otherwise,
with the comment "Overflow or unsegmentable: can't cheaply prove no overlap either." Dropping the
flag turns "the variant set overflowed, so I cannot prove non-overlap" into "I proved
non-overlap" — a silent precision regression, not a plumbing simplification. Any future
cap-removal has to give `precision.rs` a different way to answer that question first.

Do not stage the protected `emit.rs` diff on the strength of this entry. It records why the answer
is no; reversing it needs the containment work, not another read of the same code.


### 2026-08-27 dead ComposeBudget forwarding tranche

`uflexc::emit_underlying_filtered_with_budget` accepted a `&ComposeBudget` and never read it;
`gate::compile_gated_grammar_with_budget` and `build::build_controllable` existed only to forward it
there. Removed test-first (`318c9f7d`), source-second (`11fff5e4`), prose-last (`77226079`):
**295 deletions / 93 additions, net -202 lines** across 24 files. Each pair collapses onto its honest
name; `build_controllable` loses the parameter outright, with its direct call sites in
`backend_runtime`, `oracle`, `selection`, and `witnessed_coverage` updated. `uflexc`'s
`emit_budget_tests` module went with it: once the argument was gone, its one test asserted only that
a 20-entry fixture emits 20 root entries.

Retained, as the tranche's stated protected boundary: `ComposeBudget` itself,
`CHAIN_DEPTH_ABSOLUTE_CEILING`/`check_chain_depth` and the peel/F6 depth contracts, `ApplyBudget`
and the apply path/candidate limits, compound-chain and closure work/depth guards, real
pre-expansion and probe termination, and marker representability. `with_caps` was NOT restored.

**Follow-on slice, landed.** `oracle::differential_oracle`, `selection::select_plan`, and
`backend_runtime::build_candidate` were each left holding a `&ComposeBudget` whose only remaining
reader was `build_controllable`. `b3c8d14d`/`7eb17a3b`/`86fb56fb` removed all three:
**78 deletions / 17 additions, net -61 lines** across 8 files, again test-first, source-second,
prose-last. The cascade terminated inside `pg-foma` exactly as audited --
`oracle::minimize_disagreement` (which only forwarded to `differential_oracle`),
`backend_runtime::realize_plan_composed` and `realize_accuracy_proposer` (which only forwarded to
`build_candidate`), backend_runtime's three `ComposeBudget::from_env` roots, and
`plan_interaction_coverage`'s one `chain_depth_cap: None` literal. Nothing above those roots carried
a budget, so no caller was left holding one it cannot use. `peel_budget` is untouched throughout:
`peel_candidates` reads its chain-depth cap, and that contract is protected.

Four of the six `with_caps` holes closed with it. The two that remain are the `peel_candidates`
callers, `orthogonal_basis_group_b` and `f6_reduplication_peel_chain_depth`, where the budget
carries a real chain-depth contract and the test genuinely needs a never-tripping base value.
`with_caps` must not come back for them; the repair, when the replacement phase reaches compile
holes, is either `ComposeBudget::from_env().with_chain_depth_cap(n)` (already deterministic wherever
a cap is set, since `chain_depth_cap` is the type's only field) or promoting the existing
`ComposeBudget::unbounded()` from `#[cfg(test)] pub(crate)` to `pub` so an integration-test crate can
see it. That is a decision for the repair phase, not a demolition edit.

**Pre-existing compile holes neither tranche widened or hid.** `build.rs` used `ComposeError`
without importing it at the handoff commit; its `compose_budget` import now names that symbol, which
closed that hole as a side effect of the import becoming honest. The `with_caps` holes are covered by
the follow-on slice above.

Status: **LANDED UNVERIFIED.** Structural acceptance only: call-site and definition residue searches
for both `_with_budget` names, a per-file classification of every surviving `ComposeBudget`
reference, `git diff --check`, per-commit `--numstat`, and full diff inspection. No Cargo was run.

### 2026-08-27 finite quantifier ceiling tranche

The arbitrary `MAX_QUANTIFIER_BOUND = 512` policy is removed: **187 deletions / 125 additions, net
-62 lines** across `8b8277bf` (the stale finite-cap fixture/test first), `f23ad388` (the constant and
both refusal checks second), `9bf811f6` and `78e0d319` (stale source/staging/status prose), and
`07d46165` (the remaining fixture, benchmark, and typology verdict contracts). Large finite
quantifiers now lower natively. Still rejected, and deliberately so on semantic grounds: inverted
finite ranges, empty children, alpha-nested quantifiers, disagree-polarity alpha variables, and other
unsupported pattern constructs. The positive unbounded-large-min test remains.

Status: **LANDED UNVERIFIED.** Structurally accepted under the demolition discipline: negative
residue searches for the deleted constant and for the deleted
`right_to_left_predicate_refuses_quantifier_shaped_rule` citation, symbol-existence checks for every
name the corrected prose cites, XML well-formedness of the edited fixture, `git diff --check`, and
per-commit `--numstat`. No Cargo was run; the suite gate belongs to the final verification stage.

### Audited Stage 2 kill ledger (`b330892f` anchors)

Historical anchors retained so the deletion can be audited. The 2026-08-26 rip-first decision
superseded the former proof-first gate; items 1-4 were removed by `5c4b27de`/`95ac164d`. Items 5-6
remain protected.

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

### Deferred closure-cap removal checkpoint (2026-08-26)

Do not delete the TunedSurface closure work/depth caps in the current pure-removal pass. A
read-only independent safety review found that the existing semantic preflight rejects only an
active `RealizationalRule` with empty `real_fs`. Two active realizational rules with conflicting
non-empty values for the same feature can overwrite and unblock one another indefinitely, while
`MorphRuleDef::max_apps` supplies each a practical-no-op `u16::MAX` bound. The current 3,000-work/
64-depth closure boundary is therefore still a termination barrier for that case, not merely a
machine-size policy.

Resume this removal only after a separately reviewed semantic cycle guard covers reachable
cross-realizational overwrite cycles. That follow-up must preserve actual pre-expansion,
`ClosureTrace`/evidence, semantic `UnboundedTransition`, external `ExecutionLimits`, apply/candidate
budgets, `HC_COMPOSE_CHAIN_DEPTH_BUDGET`, and the independently staged compound-chain decision.
Do not interpret this defer as approval to restore any enumeration, ordering, chooser, envelope,
or compatibility machinery already removed.

---

## Tally

Committed rebased branch range `1225f25a..e00474da`:
**21,294 deletions / 11,029 additions, net -10,265 lines** across 242 files. The dedicated rip-first
range `1c7cc837..e00474da` removed **18,237 lines**, added 2,046 structural/fixture/documentation
lines, and is net **-16,191 lines** across 200 files. 306 of those rip-first additions are the
2026-08-27 continuation handoff plan itself (`24c8171a`); it is process scaffolding, not a removal
win. This is a branch-wide mechanical line tally, not a claim that every commit is
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
The final envelope-constructor RED/GREEN pair removed 9 and added 4 lines at commit scope (net -5).
The rip-first commits then removed the old supervisor, chooser, legacy Pack/report build path,
compile-retry advice, compound-pair and ordering-multiplicity refusals, legacy optimizer-report
compatibility, the old `HealthReport.findings` serde default, and the eager enumeration budget.
The enumeration tranche was staged test-first (`69efc9dc`) and source-second (`03745a5e`): it
removed `EnumerationBudget`, `EnumMeasure`, `HC_ENUM_ENTRY_BUDGET`, `HC_ENUM_PROBE_BUDGET`, the
worker's `BudgetTripped` outcome, and their refusal/health plumbing. It deliberately retained the
real pre-expansion traversal, `ClosureTrace`/terminal evidence, semantic `UnboundedTransition`,
apply/candidate budgets, `HC_COMPOSE_CHAIN_DEPTH_BUDGET`, compound-chain refusal, the hidden worker,
and external `ExecutionLimits`. Their intermediate compile holes are intentional; no stale test
result authorizes restoring rejected paths.

The health-publication readiness deletion is now complete: `0e001bdc` removed the stale test and
`0e3240c2` removed `validate_health_readiness` plus its one call site, after the user explicitly
accepted the consequence (`make-report --pack` now reports on an unhealthy pack instead of erroring;
see A2). `pack.rs` is a test-only module afterwards, and its `evaluate_health` call drops the fifth
argument the live 4-arity signature had stopped accepting. Only `emit.rs` remains as an
uncommitted protected diff, and the containment inventory above says it stays that way.

The remaining-cap audit classified `PATTERN_ITER_CAP`, compound/absolute chain-depth limits, the
structural closure depth, and apply path/candidate limits as live termination or safety boundaries;
they are not deletion authority. `STRUCTURAL_FS_REACHABILITY_STATE_CAP` fails open and needs a
separate termination design. `REP_VARIANT_CAP` was authorized in principle as a larger
recall-losing overflow-plumbing tranche, but the 2026-08-27 containment inventory above returns
**NO-GO**: five live in-process compile paths sit outside the supervised worker, and
`precision.rs` decides on the overflow flag rather than merely carrying it. Historical documents
that show the removed
`fst-health <grammar> <words>` workflow remain queued for the final stale-contract documentation
sweep; they do not authorize restoring that command path.
Remaining deletion opportunity is tracked by the stages above; estimates below are directional
only:

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

---

## 2026-08-28 test-failure attribution — the count was a measurement artifact

Every failure figure in this file before today came from a run that **stopped at the first
failure**. `pg-foma` alone reports 1,053 tests; the runs behind `G5` and `G6` reached 171 and 215.
The ledger was not wrong about the failures it named — it was wrong about how many there were,
because nothing had ever looked.

First complete run, `--no-fail-fast`, both sides:

| tree | run | passed | failed |
|---|---:|---:|---:|
| `ff29935b` (before the architecture work) | 1,051 | 1,033 | **18** |
| candidates 1-3 and Grill 1, first run | 1,053 | 1,033 | 20 |
| the same, after fixing both | 1,053 | 1,035 | **18** |

The failure lists are identical across all three, so **the architecture work broke nothing**. The
middle row's two extra failures were both the new `HealthFinding` seam gate working: one a bug in
the gate (it flagged the literal examples inside its own self-test), one a real illegal pairing it
was built to catch — see `F2`. Both are fixed, and the count is back to the pre-existing 18 over
two more tests than the baseline ran.

The 18 pre-existing failures cluster, which is the useful part:

- **4** `witnessed_strategy_coverage_gate`
- **4** `strategy_aware_capability_gate`
- **3** `advice_catalog_contract`
- **3** `backend_selection_contract`
- **1** each: `coverage_ledger_golden_json` (a trailing newline), `coverage_citation_liveness` (cites
  `unbounded_unordered_stratum_deterministically_refuses_to_compile`, a test the unordered-hard-stop
  removal deleted), `backend_capability_cards_contract`, `morphotactics_boundary_cleanup_slice`
  (the one `G5` already named).

That shape said these were demolition residue rather than scattered rot, and it was right: the
clustering was the symptom of a SINGLE root cause.

### Classification, and the root cause of 15 of the 18

**15 of the 18 were one missing line.** `12deffdb` ("remove retry backend advice") deleted the
remedy `retry-backend-build`, which was the ONLY remedy on the advice catalog's
`backend-build-unavailable` entry. `validate_catalog` requires every entry to carry at least one, so
`builtin_catalog()` began returning `Err`, and the two `.expect(...)` call sites in
`backend_selection.rs` turned that into a panic. Everything downstream of backend selection died
with it: `advice_catalog_contract` (3), `backend_selection_contract` (3),
`backend_capability_cards_contract` (1), `strategy_aware_capability_gate` (4), and — measured, not
inferred — four more that a provisional remedy also cleared. One remedy restored the count from 18
to 3.

**Classified: not a regression in the demolition's intent, but a defect in its execution.** The
tranche meant to remove retry ADVICE and did; it did not notice it had emptied an entry the schema
requires to be non-empty.

**Resolved by removing the entry, not by supplying a remedy.** Every remedy that entry could offer —
retry, "increase the envelope", cross-backend substitution — is on this file's own Rejected list
below. An entry whose entire remedy space has been refused does not have a gap in it; it does not
belong. The advice catalog recommends GRAMMAR changes, and no grammar change starts a compiler, so
a build failure now carries its typed finding and no advice at all. See
`docs/adr/0007-advice-recommends-grammar-changes-only.md`. The nine remaining entries are all
grammar constructs, which is the check that this was the right cut: `backend-build-unavailable` was
the only non-grammar entry in the book.

**The other three, each classified before being touched:**

| test | classification | action |
|---|---|---|
| `coverage_citation_liveness` | **Obsolete citation.** The ledger cited `unbounded_unordered_stratum_deterministically_refuses_to_compile`, a test the unordered-hard-stop removal deliberately deleted. | Citation dropped. This FOLLOWS a rejected-behavior deletion; it does not restore anything. |
| `coverage_ledger_golden_json` | **Stale artifact, no behavior involved.** The committed golden carried a trailing newline `to_json()` never emits, so it did not match what its own `regenerate_coverage_ledger_golden_json` helper produces. Its sibling `readiness_verdict_golden.json` also ends without one. | Regenerated through that helper. |
| `morphotactics_boundary_cleanup_slice::templated_query_accepts_a_surface_with_an_explicit_boundary` | **Resolved: the test bypassed the capability envelope.** Its fixture `backend-ordered-generic` genuinely contains `mrInfixUm`, `mrRedupCV` and `mrRedupFull`, which `TemplatedUnderlyingTokens` cannot represent, so the emitter returns `Partial` and `compile_templated_morphotactics` rejects it. Refusing is CORRECT under the overgeneration invariant below; the defect was that the test called `compile_templated_morphotactics(&grammar)` DIRECTLY and so never reached the envelope that refuses this pairing. | **Deleted.** Rewriting it to assert the refusal would duplicate `strategy_aware_capability_gate::templated_selector_refuses_each_known_unsupported_shape_with_per_allomorph_diagnostics`, which already pins that exact refusal on this exact fixture. See the coverage gap recorded immediately below: the positive claim has no representable host today. |

**Coverage gap opened by that deletion, recorded rather than absorbed.** No FST backend now has any
test that its query encoder accepts a surface containing an explicit boundary character. The oracle
path still covers it -- `cleanup_exercise_boundary_consumed_before_cleanup` pins `mu+i` to exactly
one analysis through `Morpher` -- but the oracle is not a backend. Closing this needs a fixture that
simultaneously (a) declares a `CharDefKind::Boundary` symbol, (b) commits a surface containing that
symbol, and (c) is representable by the backend under test. A sweep of every staged and upstream
`words.yaml` found exactly three fixtures satisfying (a) and (b): `backend-ordered-generic` and
`metathesis-phase-isolation` both carry infix and reduplication rules and so fail (c), and
`loader-isactive-breadth` satisfies all three but lives under `machine/conformance`, so a test
unwrapping it fails rather than skips under `-Scope local`. Authoring a staged fixture is therefore
the honest fix, and it is not done.

**Nothing in this file's failure accounting should be trusted from a run that stopped early.** Use
`--no-fail-fast`.


## The overgeneration invariant (backend support) -- this is ADR-0001, not a new rule

**An FST may only ever overgenerate.** A backend may propose candidates that turn out to be wrong;
confirmation filters those. It may never MISS one. So a backend that can fail to generate a
candidate a grammar licenses -- no infixing, no reduplication, whatever the construct -- must **fail
hard at the capability-envelope step with a clear explanation naming the construct**, never accept
the grammar and quietly emit a network that under-generates.

**This is already ratified.** `docs/adr/0001-honest-capability-boundary.md` decides exactly this: a
grammar is matched against the composed capability envelope and either compiles or is "**hard-failed
at compile time** with a typed diagnostic naming what cannot be done faithfully", where faithful
means "recall-preserving (the propose-and-confirm invariant: never omit a valid HermitCrab
analysis)", and "silent overapproximation-that-loses is never acceptable". It is restated here
because a failure was very nearly misread as a semantics question open for decision, when it is a
violation of a decision this repository already made.

ADR-0001 also gives the reason a partial network is not a lesser good: it is indistinguishable from
a complete one at query time, since both return analyses and the incomplete one merely returns
fewer. Silent under-generation therefore surfaces as a recall problem in the LANGUAGE rather than a
declined capability in the COMPILER -- the most expensive place for the error to appear, with a
linguist debugging their grammar for a gap PanGloss already knew about.

**Measured, not hypothetical -- four live violations.** `faithfulness_coverage_gate` sweeps all 61
fixtures against the full-HC oracle and prints 19 (construct, backend) containment failures, every
one `proposal set offered 0`: the backend missed an analysis the oracle found. They reduce to four
(fixture, word) causes -- `morphotactic-attribute-breadth`/`kuldede`,
`feature-system-breadth`/`isk`, `loader-isactive-breadth`/`mo+kul`,
`mpr-overwrite-order-dependence`/`daboyuxa` -- and `plan-composed` is clean on all four. The gate
asserts non-vacuity only by design, so these are reported and not enforced; its own doc names the
condition for tightening. Full inventory and the two neighbouring findings (a staged fixture no
backend can compile while `fst-health` calls it representable, and a `BuildFailed` reason that
drops the construct name `EmitReport.uncovered` already carries) are in
`docs/research/conformance-containment-inventory.md`.

Two consequences, both load-bearing:

- **The refusal belongs to the envelope, not the emitter.** An emitter that returns `Partial` and a
  caller that rejects it does produce the right outcome, but it produces it too late and in the
  wrong vocabulary: the diagnosis arrives as a compile artifact rather than as a typed capability
  refusal a selector can read and a report can explain. Any construct a backend cannot represent
  should be a capability predicate.
- **Anything that reaches a compiler directly bypasses the guarantee.** A test (or a caller) that
  invokes a backend's compile entry point without going through selection has stepped around the
  envelope, and can therefore observe a failure the envelope exists to have prevented. That is what
  the last remaining `pg-foma` failure actually is -- see the classification table above.

### Queued: infix support for `TemplatedUnderlyingTokens`

Not a bug, a gap. `TemplatedUnderlyingTokens` classifies a standalone rule whose primary allomorph
is `Role::Infix` (and likewise `Role::Reduplication`) as "not representable (v1)" in
`emit.rs`. Under the invariant above that is a legitimate state for a backend to be in, provided it
is declared at the envelope. Implementing it is separate, optional work: it widens what the backend
covers rather than fixing anything that is currently wrong. It should be picked up only as a
deliberate capability decision, and it must land with fixtures that prove recall parity against the
oracle, not merely a compile that stops refusing.


## Completion gate: NOT met as of 2026-08-28

Demolition is not exhausted. The gate this file sets — no `AUTHORIZED`, `PARTIAL`, `OPEN`, `VERIFY`,
or unreviewed tranche left standing — is unmet, and the reason is not a backlog of unstaged
deletions. It is that what remains is not deletion work:

- **`F10` is the one genuinely large removal surface left, and it is unmeasured.** Two mechanical
  probes over the test tree found nothing: no integration test imports a name that no longer
  exists, and no test calls an associated function defined nowhere in the workspace. Whatever
  dead weight is in those 2,493 tests is semantic — a test that still compiles and still passes
  while pinning nothing anyone wants — so finding it needs judgment per test, not a sweep. Do not
  issue it as one task.
- **`G1`-`G5` are correctness defects, not cruft.** This file already forbids mixing them into a
  mechanical deletion slice, and that has been respected. Each needs its own evidence.
- **`H2`/`H3` are refactors** (split `capability.rs`; reduce the 15-file shotgun edit for adding a
  backend). Neither removes behaviour.
- **`D1`/`H4` are `VERIFY`** — they need a measurement or a decision, not a deletion.
- **`D5`/`D8` are consolidation**, ~140 lines of duplication, worth doing but not cruft removal.
- **`REP_VARIANT_CAP` is refused on evidence**, not pending. See the containment inventory above.

**Both compile holes this section used to name are now closed.** `run_make_report`'s
`match &pack_path` has its `None` arm (`make_report.rs`, the no-artifact case: trust stays
`Proven` because no override was exercised), and `trust` is assigned on both sides of the
`else`. The workspace compiles; `pg.ps1 -Mode test` reaches test execution rather than stopping at
a build error.

What that does NOT settle is the question the holes stood in for: whether `make-report` should
require an explicitly named artifact, the direction the ratified Package contract points. The code
now answers "no" by default, which is a real answer arrived at by repair rather than by decision.
It is recorded here so the replacement phase revisits it deliberately.

The honest next step is therefore **not another rip**. It is the replacement stage this file's
execution order already names: define the smallest coherent explicit-backend/completed-artifact
surface, settle the `--pack` question above against it, add tests for that final surface only, and
then run authoritative verification through `rust/tools/pg.ps1`.

Ahead of that sits the 18-failure list in the attribution section above, which is now the shortest
path to a meaningful gate. Those failures are concentrated in four gates rather than scattered, and
until they are classified — obsolete contract, or real regression — no suite-wide claim about this
work can be made. `F10` is best done after that, when a passing suite makes "this test pins
nothing" a checkable claim rather than a guess.
