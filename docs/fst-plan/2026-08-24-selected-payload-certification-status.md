# Selected-payload certification status (2026-08-24)

## Decision

**No route is certified: 0 of 3.** The selected-payload trust boundary — ranked capability reports
choose a route, `select_completed_build` refuses anything that does not match the shipped envelope
and grammar identity, and the exact returned payload bytes reconstruct the analyzer — is built and
runs. It is certified over **no real grammar**.

Earlier "working FST" results for these languages were compile-and-parity evidence. Parity of a
compiled network is not certification of a packaged payload, and this document exists so the two
are not conflated again.

Each blocker below was measured, not assumed, and none is closed by writing more tests:

1. **Indonesian's payload cannot be constructed.** Its gate now runs against the real grammar and
   fails inside the worker, before any of the 120 cases is compared.
2. **Amharic and Aweti admit no backend at all** under the default envelope. There is no route to
   certify, so a gate for either would be unrunnable *and* unpassable.

## What the corpus actually holds

`samples/data/` contents are gitignored (`.gitignore` lines 4-8), so absence is expected in a fresh
worktree and is not a repository defect. Measured against `rust/tools/corpus-manifest.json`'s
`required: true` entries:

Every required word list is present in `samples/data`. The grammars are the gap — but only one of
them is truly gone. Three are simply somewhere else on this machine, left behind in other
worktrees' own `samples/data`:

| Language | Required grammar | In `samples/data` | Recoverable on this machine |
|---|---|---|---|
| indonesian | `indonesian-hc.xml` | no | not under this repo; a lock-verified copy exists outside it |
| sena | `sena-hc.xml` | no | yes, `.claude/worktrees/hc-stats/samples/data` |
| amharic | `amharic-hc.xml` | no | yes, `.claude/worktrees/hc-stats/samples/data` |
| aweti | `aweti.json` | no | yes, an agent worktree's `samples/data` |
| mbugwe | `mbugwe.fwdata` | yes | — |

None of the four is genuinely lost. Gathering the recoverable grammars alongside the word lists into
one directory and pointing `PANGLOSS_CORPUS_ROOT` at it satisfies every manifest-`required` entry,
which is how the runs below were performed.

The `.fwdata` files are present for all five but are **not** substitutes for
`indonesian-hc.xml`: the case-set lock in `rust/tools/three-language-case-sets.json` pins
`grammarSha256` for the declared grammar source and the gates assert those bytes before use, so
repointing a gate at a different source silently changes the denominator it was locked against.

`indonesian-hc.xml` is absent from `samples/data` and from everywhere under this repository. A
byte-identical copy was found **outside** the repo, in an unrelated worktree, and verified against
the lock before use — see the Indonesian section below. With that copy in place all ten required
inputs are present, so `-Mode corpus-test` runs rather than refusing.

`pg.ps1 -Mode corpus-test` remains the correct way to run any of this: it validates every required
path before Cargo starts and fails a run that records zero executed corpus cases. Populate
`samples/data/`, or point `PANGLOSS_CORPUS_ROOT` at a populated root.

## Indonesian — written, believed correct, never run here

`rust/crates/pg-foma/tests/indonesian_worker_selected_payload_gate.rs` implements the full boundary:
`run_selected_compile_worker` spawns the killable worker child, and
`SelectedBackendBuild::into_analyzer` reconstructs the runtime analyzer from the exact bytes the
worker returned, then compares `AnalysisIdentity` sets against the `Morpher` oracle for each of 120
locked cases. It reaches the preferred tuned route through the named
`ResourceEnvelopeId::TunedSurfaceWork10kV1` retry rather than the default envelope.

**It has now run, for the first time, and it fails.** Both Indonesian inputs were recovered from
`C:\Users\johnm\Documents\repos\.worktrees\phase2-w6\samples\data` and verified against the lock
before use — `indonesian-hc.xml` hashes to the pinned `grammarSha256` (`e450110e…`) and
`indonesian-words.txt` to the pinned `sourceSha256` (`004d6aa3…`), both exactly. With all ten
manifest-`required` files present, `-Mode corpus-test` proceeded and the gate reported:

```
indonesian_worker_selected_payload_gate.rs:99
  the contained worker must return the exact selected completed payload:
  Compiler("completed-build closure is incomplete:
            terminal=Incomplete(EnumerationBudgetReached), rule_pairs_visited=1016,
            pending_successor_count=0, pending_rule_ordinals=[], worklist_empty=false")
```

Read that carefully, because the failing dimension is not the one the gate compensates for. The gate
deliberately uses the named `TunedSurfaceWork10kV1` retry, which raises the **closure-work** cap to
10,000; the run stopped at 1,016 visited rule pairs, nowhere near it. The terminal reason is
`EnumerationBudgetReached` — a *different* budget, which that retry envelope does not raise. So the
named retry lifts the wrong limit for this grammar, and the completed build never closes.

The failure is at the `run_selected_compile_worker` call, before any of the 120 cases is compared,
so this says nothing yet about analysis parity — the payload cannot be constructed at all.

**Indonesian is therefore not certified, but the reason is now measured rather than assumed.** The
blocker is a resource-envelope dimension mismatch in the selected-payload route, not missing data
and not a corpus problem.

And there is no envelope that would fix it. `ResourceEnvelopeId` declares exactly two variants, and
they differ in one dimension only:

| Envelope | `tuned_surface_closure_work_cap` | enumeration caps |
|---|---|---|
| `ManagedV1` | 3,000 | `DEFAULT_ENTRY_BUDGET` / `DEFAULT_PROBE_BUDGET` |
| `TunedSurfaceWork10kV1` | 10,000 | the same |

Both carry identical enumeration caps, so the gate is not naming the wrong envelope — no envelope
raising the dimension that actually bound exists. Certifying Indonesian therefore requires a
**decision**, not a fix. Under the current model
(`docs/superpowers/specs/2026-08-23-stress-grammar-construction-and-production-admission.md`), the
remedy for an artificial internal cap is not a bigger fixed named envelope — a bigger number is
still a guess about whether the real cost sits inside or outside it, and finding out only after
picking one is exactly the outcome the model rejects. The remedy is to give this stop reason a
retry path into internal-caps-removed mode, bounded only by machine containment, which resolves to
exactly two outcomes: the closure fits, or the attempt hits `MachineLimit`. Making the
completed-build closure incremental, so it need not finish inside one budget at all, remains a
separate, still-open alternative. Each changes what ships. An envelope id is serialized into
completion evidence, the worker wire protocol, and the envelope digest, so adding a new envelope
variant (if that path were chosen instead) would still be a compatibility-bearing change and not a
local edit.

That is the whole remaining distance between this branch and 1-of-3 certified.

**And the envelope ladder has no rung for the dimension that bound.**
`RetryAuthorization::from_terminal_failure` (`resource_envelope.rs:464-472`) lists
`EnumerationBudgetReached` among the retryable stop reasons, alongside `WorkBudgetReached`,
`DepthBudgetReached`, and `ResourceBudgetReached`, so a compile that dies this way is handed a retry
authorization. `CompileEnvelopeRequest::retry_from` then refuses a same-envelope retry — pinned by
`closure_terminal_parity_gate.rs:230`, which asserts retrying into `ManagedV1` from a `ManagedV1`
failure is an error — and requires escalating to a different envelope id. That design is coherent:
authorize, then escalate.

The gap is that for this stop reason the only escalation target, `TunedSurfaceWork10kV1`, raises
`tuned_surface_closure_work_cap` and nothing else. Escalation is therefore permitted but futile: a
different envelope carrying the same enumeration caps produces the same terminal.

Two qualifications, so this is not read as more alarming than it is. Nothing in production reads
`retry_authorization()` today — the only callers are in `closure_terminal_parity_gate.rs` — so no
shipped code path currently performs a futile retry; the gap is latent. And the authorization itself
is correct evidence: the failure genuinely *is* the retryable kind. What is missing is a retry path
this stop reason could actually use.

The trip site names the same asymmetry from the other side. `emit.rs:4413` reaches
`enum_budget.trip_reason()` and reports "grammar exceeds the foma-engine's eager-enumeration budget
… a floor, not a total (limit {limit}; Aweti's measured uncapped total is ~15x this cap)". A cap
routinely exceeded by an order of magnitude, declared retryable, with no escalation path that would
actually help.

Two coherent resolutions, not the "add a bigger envelope" framing this document used before the
current model superseded it — a fixed, larger enumeration cap is still an arbitrary guessed number,
not a decision about whether the real cost is bounded at all:

1. Give this stop reason a retry path into internal-caps-removed mode — the same mechanism
   `--remove-size-limits` exposes to developers — bounded only by machine containment, rather than
   into another fixed enumeration ceiling. That resolves to exactly two outcomes, fits or
   `MachineLimit`, and either answer is informative in a way a bigger guessed number is not. This is
   the path that could certify Indonesian.
2. Remove `EnumerationBudgetReached` from the retryable set, so the condition reports as terminal
   and no caller is invited to escalate into a fixed envelope that cannot help.

Resolution 1 is the one that unblocks certification; resolution 2 only removes a misleading
affordance without fixing the underlying cap. Whichever is chosen should be decided before any
caller starts acting on `retry_authorization()`, because today none does and the choice is still
free.

**A trap worth naming:** the repo's own `samples/data/indonesian-words.txt` (750 bytes) does **not**
match the lock's `sourceSha256`; the recovered copy (1,105 bytes) does. Populating `samples/data`
from the in-repo copy will fail the lock assertion, which reads like a gate bug and is not one.

**Gap fixed here, independently of the corpus:** this gate, and three of the four corpus tests in
`backend_runtime_net_is_queryable_gate`, were not listed in the manifest's
`indonesian.requiring_tests`; they now are. That list is how `-Mode corpus-test` knows which tests
break when a corpus goes missing, and the manifest's own comment records a previous instance of that
contract naming a gate that could never run. A gate that needs `indonesian-hc.xml` but does not
declare it is the mirror-image defect: it declares nothing and so is silently skipped rather than
loudly refused.

## The Amharic and Aweti verdicts are measured, not inferred

With the recoverable grammars assembled under `PANGLOSS_CORPUS_ROOT` and ignored tests enabled,
`five_language_backend_reports_gate` runs against the real grammars rather than a reading of
`capability.rs`:

```
PASS  amharic_backend_reports_are_complete   (1.047s)   real amharic-hc.xml
PASS  aweti_backend_reports_are_complete     (1.490s)   real aweti.json
FAIL  indonesian_backend_reports_are_complete           corpus.rs:91, input absent
```

Both passing gates assert `assert_default_resource_no_path` — `is_no_path()`, `preferred() == None`,
`selected().is_empty()`. Passing against real data is therefore positive evidence that **Amharic and
Aweti have no admitted backend at all** under the default envelope. The sections below explain why;
this is the measurement that grounds them.

The Indonesian failure is the fail-closed contract behaving correctly. `corpus::require` refuses
rather than skipping, with the reason "This test was requested explicitly, so it fails rather than
reporting a pass it did not earn." That is the outcome to want from a missing input.

This was a targeted run, not `-Mode corpus-test`, and it does not claim that suite's guarantees.

## Amharic — blocked by a missing emission mechanism

`capability.rs`'s `templated_shape_floor` refuses any grammar carrying a `Role::Infix` allomorph.
The reason string is explicit: "Role::Infix is handled only by the emitter's uncovered-role branch;
the templated proposer has no Copy-Insert-Copy/infix entry". Amharic's productive morphology is
root-and-pattern interdigitation, so this is a categorical `CannotRepresent` refusal, not a budget
that a larger envelope could raise.

`five_language_backend_reports_gate.rs::amharic_backend_reports_are_complete` calls
`assert_default_resource_no_path`, which asserts `is_no_path()`, `preferred() == None`, and
`selected().is_empty()`. Under the default envelope Amharic has no admitted backend, and unlike
Indonesian there is no known bounded retry: the recorded TunedSurface characterization exceeded
3,000,000 visited rule pairs and was still growing at depth 16.

Closing this is a compiler feature — infix/Copy-Insert-Copy emission in the templated backend, then
selection wiring — not a test-authoring task.

## Aweti — blocked by selection wiring, with the eager route provably dead

`docs/fst-plan/2026-08-20-aweti-enum-budget-census.md` records the enumeration route producing
3,093,412 composite entries against a 200,000 cap, 15.5x over, with every filter and depth-cap
hypothesis either already applied or numerically insufficient. The eager route is not a tuning
problem.

The templated/cascade construction is much closer: it compiles all 18 phonological rules in under
three seconds and recalls 100 of 106 oracle-bearing words, with six named residual misses. Two
things still block certification: those six misses against this project's 100%-recall bar, and the
fact that `backend_selection` does not admit the templated route for an Aweti-shaped grammar at all
— `aweti_backend_reports_are_complete` likewise asserts no backend is admitted (`is_no_path()`).

## The four net-queryable regressions are the same blocker

`rust/crates/pg-foma/tests/backend_runtime_net_is_queryable_gate.rs` holds six tests. Four are
`#[ignore]`d on "needs the private corpus at samples/data/indonesian-hc.xml":
`corpus_indonesian_first_word_runtime_phases_complete`,
`corpus_indonesian_registry_candidates_are_named_before_build`,
`corpus_indonesian_plan_composed_baseline_completes`, and
`corpus_indonesian_confirms_after_the_finish_step`. Measured on this machine:

```
Summary [0.788s] 2 tests run: 2 passed, 4 skipped
```

These four are blocked on the same missing grammar as everything above. They are **not** the
recipe-optimizer regressions recorded elsewhere as outstanding; an earlier draft of this document
said they were, on nothing better than both sets numbering four. See the next section for the real
ones.

The contract governing the short-circuit these cases exercise is
`out_of_scope_marker_subtrees_are_attributed_not_blamed_on_the_grammar`, which requires a
whole-grammar strategy to return a real measurement and a `PlanComposed` candidate with unbuildable
markers to be refused. That contract test runs and passes.

## Real-language evidence that the marker refusal, not fixture choice, is the defect

With the corpus complete, the four previously-unrunnable corpus tests in
`backend_runtime_net_is_queryable_gate` ran for the first time. Three pass:

```
PASS  corpus_indonesian_first_word_runtime_phases_complete
PASS  corpus_indonesian_plan_composed_baseline_completes
PASS  corpus_indonesian_registry_candidates_are_named_before_build
FAIL  corpus_indonesian_confirms_after_the_finish_step
[pg] corpus-test executed 9 corpus case(s) across 3 label(s)
```

The one failure matters more than the three passes. On the **real Indonesian grammar**, no candidate
reaches `FullHcConfirmed`, and the seven certifications say exactly why:

- **five** `Unsupported { "plan requires subtrees build_controllable does not build
  (CompositeEmissionMarker, StructuralCompositeMarker); use a whole-grammar backend" }`
- one `StaticRejected` — TunedSurface needs more than 3,000 reachable root/chain-state × rule pairs
  and stopped at 3,001: "the current TunedSurface operational envelope is too small"
- one `BuildFailed` — templated emission `Partial { uncovered: 3 }`

That is the *same* short-circuit at `backend_runtime.rs:1830-1838` that produces the four `pg-cli`
recipe failures below, reproducing here on real-language data rather than a staged fixture. It
settles a question that was open: the hypothesis that those failures were a
**fixture-selection defect** — fixable by repointing the tests at some other grammar — does not
survive. The refusal fires on the reference grammar too, so no choice of fixture avoids it.

The test's own message records that it once passed: "Pre-fix this read 0 of 3 confirmed with a
`merasa` multiplicity mismatch." So the marker-bearing candidates were measurable at some point and
are not now.

## The recipe-optimizer regressions are four runnable `pg-cli` failures

`four_grammar_recipe_evidence::four_promoted_grammars_have_truthful_recipe_evidence` and three tests
in `recipe_optimize_continuation` fail today, need no corpus, and are reproducible on any machine:

| Test | Assertion |
|---|---|
| `a_final_candidate_that_overruns_an_aggregate_bound_still_writes_a_report` | needs measurable confirmation work; got `[0, 0, 0, 0, 0, 0]` |
| `a_failing_candidate_neither_stops_the_run_nor_vanishes_from_progress` | "the fixture must also confirm at least one candidate" |
| `a_candidate_abandoned_by_a_resource_bound_is_banked_with_its_own_verdict` | got `"unsupported"`, wanted `"resource-breach"` |
| `four_promoted_grammars_have_truthful_recipe_evidence` | feasible count 2, wanted 5 |

They are **pre-existing**, with byte-identical panic messages, line numbers and values at the
pre-session baseline and at every commit since.

Root cause for the three `recipe_optimize_continuation` failures, measured by running
`pangloss recipe-optimize` against `conformance-staging/edge-cases/backend-strata-generic/grammar.xml`
and reading `progress.jsonl`: all six candidates refuse before measurement. Four short-circuit to
`Certification::Unsupported` with an all-zero score at `backend_runtime.rs:1830-1838`, where
`unbuildable_marker_reason` flags a `CompositeEmissionMarker` leaf before `build_candidate` is ever
called. The other two are whole-grammar adapters, which skip that check and instead fail on a
genuine build failure at one uncovered construct. Since only `FullHcConfirmed` is selectable, nothing
can confirm.

`recipe_optimize_continuation.rs:154` says "Non-vacuity FIRST: without both kinds present this test
asserts nothing", which invites the theory that the fixture simply became unsuitable. **The
real-language evidence above rules that out**: the same refusal fires on the reference Indonesian
grammar, so no fixture avoids it. Repointing these tests would trade a specific pinned claim — that
the optimizer measures marker-bearing grammars — for a vague one, and leave the defect in place.

The fix therefore reaches into backend selection: replacing the coarse
`ProcessMorphology => CannotRepresent` row in `strategy_coverage.rs` with a predicate-backed
disposition resolved through the classifier. That is scheduled work in the
`cover-circumfix-cross-product-and-infix-drop` change, not this branch's, and it should not be
attempted piecemeal — the synthetic contract test
(`out_of_scope_marker_subtrees_are_attributed_not_blamed_on_the_grammar`) is currently the only
proven guarantee about the short-circuit's behavior.

`four_grammar_recipe_evidence` is a different fixture (`edge-cases/mpr-gated-exception`) whose
feasible count fell to 2 against an expected 5. That fixture has previously produced a genuine
winner, so a stale expectation and a real feasibility regression are both live hypotheses, and the
count alone does not distinguish them.

## Why neither problem was visible

`pg-foma`'s test suite does not compile. `tests/templated_morphology_marker_gate.rs` is a deliberate
TDD red gate — `openspec/changes/cover-circumfix-cross-product-and-infix-drop/tasks.md` records task
1.3 ("verify RED") as done and tasks 4.1/4.2/3.2/3.5/3.6 as not — but it fails to *compile* rather
than to *fail*, referencing nine fields that have never existed on `TemplatedCompileProfile`
(`templated_compile.rs:20`) plus a `marked_input` signature that became fallible after the gate was
written. A non-compiling test target takes the whole package's test build with it.

The consequence is the part worth remembering: every green result on this branch came from narrow
`-TestTarget` runs, which compile only the named target. Such a run never builds the broken gate and
never runs the failing `pg-cli` tests, so it reports green while two real defects sit next to it. A
targeted run is a claim about one target, never about a package — and this is the same shape as the
`-Scope` rule for conformance runs and the `developer-tools` two-pass rule: a narrower run that
reports identically to a broader one is the hazard, not the narrowness itself.

## Full-suite baseline, measured

With the non-compiling gate removed, `pg-foma` builds and runs as a package for the first time.
Measured on this branch, identically with and without `developer-tools`, and `pg-cli` run with
`--no-fail-fast` so nothing hides behind an earlier failure:

| Package | Features | Result |
|---|---|---|
| `pg-foma` | default and `developer-tools` alike | 1154 tests, **1 failed**, 73 skipped |
| `pg-cli` | default | 123 tests, **4 failed**, 11 skipped |
| `pg-cli` | `developer-tools` | 141 tests, **6 failed**, 11 skipped |

The two extra `pg-cli` failures are `#[cfg(feature = "developer-tools")]`, so a production build sees
four, not six. Both of those, and the four recipe failures, need `--no-fail-fast` to be seen at all:
nextest cancels the run after the first, and the `bin/pangloss` unit tests sort after the integration
targets, so a default run stops before reaching them.

All seven distinct failures reproduce byte-identically at the pre-session baseline, so none is a
regression from this branch:

- the four recipe-optimizer failures above;
- `morphotactics_boundary_cleanup_slice::templated_query_accepts_a_surface_with_an_explicit_boundary`
  — templated compile refuses a grammar carrying infix and reduplication rules as "not representable
  (v1)" (a `CannotRepresent` refusal), the same categorical gap that blocks Amharic;
- `pack::tests::pack_redup_grammar_declares_reduplication_peel_runtime_feature` — the pack-level
  policy that correctness overrides may not admit a `NotProductionReady`/`CannotRepresent`
  readiness finding;
- `capability_gate_tests::run_batch_foma_engine_no_enforce_capability_proceeds_for_permanently_refused`
  — this one is a latent inconsistency between two tests rather than a compiler gap.
  `capability_gate`'s `!enforce` branch returns `overridden: false` (and
  `capability_gate_no_flags_never_blocks_either_grammar` asserts that it must), so
  `--no-enforce-capability` relaxes the capability gate but never grants unproven-*tier* admission,
  which is `--allow-unproven`'s job. The failing test expects one flag to do both.

Two of these sit on files byte-identical to `main`, and the recipe-optimizer pair does too, so
`main`'s own `pg-cli` suite is red today. That is worth fixing at the source rather than here.

## What would change this document

1. Recover `indonesian-hc.xml` — the one genuinely missing input, and the only thing standing
   between Indonesian and certification. Copy the three recoverable grammars named above into
   `samples/data` alongside it, then run `pg.ps1 -Mode corpus-test -Package pg-foma`. That single
   file also unblocks the four skipped net-queryable tests and the 120-case selected-payload gate.
   Note the grammar must be the exact bytes the case lock's `grammarSha256` pins; another copy of
   "Indonesian" will not do.
2. Settle whether the recipe-optimizer refusal above is a fixture-selection defect or a real
   backend-selection gap. It needs no corpus, and it is main's defect rather than this branch's.
3. Aweti: close the six named recall misses, then admit the templated route in
   `backend_selection`/`capability`, then write the payload gate against the existing 106-case lock.
4. Amharic: implement templated infix emission, then selection wiring, then the payload gate against
   the existing 200-case lock.

Until 1 completes, the honest count stays **0 of 3**, and any report claiming otherwise is
describing compile parity rather than payload certification.
