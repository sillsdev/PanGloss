# Selected-payload certification status (2026-08-24)

## Decision

**No route is certified: 0 of 3.** The selected-payload trust boundary — ranked capability reports
choose a route, `select_completed_build` refuses anything that does not match the shipped envelope
and grammar identity, and the exact returned payload bytes reconstruct the analyzer — is built and
green over synthetic fixtures. It is certified over **no real grammar**.

Earlier "working FST" results for these languages were compile-and-parity evidence. Parity of a
compiled network is not certification of a packaged payload, and this document exists so the two
are not conflated again.

The blockers are two independent kinds, and neither is closed by writing more tests:

1. **The private grammar inputs are absent on this machine.** Every route that would be certified
   needs one, and no gate can be run without it.
2. **Amharic and Aweti have no admitted backend at all** under the default envelope. There is no
   route to certify, so a gate for either would be unrunnable *and* unpassable.

## What the corpus actually holds

`samples/data/` contents are gitignored (`.gitignore` lines 4-8), so absence is expected in a fresh
worktree and is not a repository defect. Measured against `rust/tools/corpus-manifest.json`'s
`required: true` entries:

| Language | Required grammar | Present | Required word list | Present |
|---|---|---|---|---|
| indonesian | `indonesian-hc.xml` | **no** | `indonesian-words.txt` | yes |
| sena | `sena-hc.xml` | **no** | `sena-words.txt` | yes |
| amharic | `amharic-hc.xml` | **no** | `amharic-words.txt` | yes |
| aweti | `aweti.json` | **no** | `aweti-words.txt` | yes |
| mbugwe | `mbugwe.fwdata` | yes | `mbugwe-words.txt` | yes |

Only mbugwe's required pair is complete, and mbugwe is not in the acceptance slice. The `.fwdata`
files for the other four are present but are **not** substitutes: the case-set lock in
`rust/tools/three-language-case-sets.json` pins `grammarSha256` for the declared grammar source and
the gates assert those bytes before use, so repointing a gate at a different source silently
changes the denominator it was locked against.

`pg.ps1 -Mode corpus-test` is the correct way to run any of this: it refuses before Cargo starts
when a required input is missing, and fails a run that records zero executed corpus cases. Populate
`samples/data/`, or point `PANGLOSS_CORPUS_ROOT` at a populated root.

## Indonesian — written, believed correct, never run here

`rust/crates/pg-foma/tests/indonesian_worker_selected_payload_gate.rs` implements the full boundary:
`run_selected_compile_worker` spawns the killable worker child, and
`SelectedBackendBuild::into_analyzer` reconstructs the runtime analyzer from the exact bytes the
worker returned, then compares `AnalysisIdentity` sets against the `Morpher` oracle for each of 120
locked cases. It reaches the preferred tuned route through the named
`ResourceEnvelopeId::TunedSurfaceWork10kV1` retry rather than the default envelope.

It is `#[ignore]`d with the reason "needs local private Indonesian grammar/corpus; run through
pg.ps1 corpus-test", and `indonesian-hc.xml` is absent, so it has not executed here.

**Gap worth fixing independently of the corpus:** this gate is *not* listed in the manifest's
`indonesian.requiring_tests`. That list is how `-Mode corpus-test` knows which tests break when a
corpus goes missing, and the manifest's own comment records a previous instance of that contract
naming a gate that could never run. A gate that needs `indonesian-hc.xml` but does not declare it is
the mirror-image defect: it declares nothing and so is silently skipped rather than loudly refused.

## Amharic — blocked by a missing emission mechanism

`capability.rs`'s `templated_shape_floor` refuses any grammar carrying a `Role::Infix` allomorph.
The reason string is explicit: "Role::Infix is handled only by the emitter's uncovered-role branch;
the templated proposer has no Copy-Insert-Copy/infix entry". Amharic's productive morphology is
root-and-pattern interdigitation, so this is a categorical refusal, not a budget that a larger
envelope could raise.

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
— `aweti_backend_reports_are_complete` likewise asserts no path.

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

Note what `recipe_optimize_continuation.rs:154` says about itself: "Non-vacuity FIRST: without both
kinds present this test asserts nothing." These tests require a fixture producing both a confirmed
and a non-confirmed candidate, and the chosen fixture produces only the latter — so the defect may
be fixture selection rather than the compiler. Which of those it is decides whether the fix is small
or reaches into unchecked backend-selection work, and it should be settled before anyone edits the
short-circuit: the synthetic contract test above is the only guarantee currently proven about it.

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

## What would change this document

1. Populate `samples/data/` (or set `PANGLOSS_CORPUS_ROOT`) and run
   `pg.ps1 -Mode corpus-test -Package pg-foma`. That alone can move Indonesian from written to
   certified, and turns the four skipped net-queryable tests into evidence rather than a gap.
2. Add the Indonesian selected-payload gate to the manifest's `requiring_tests` so its inputs are
   declared and its absence is refused rather than skipped. This is doable now, with no corpus.
3. Aweti: close the six named recall misses, then admit the templated route in
   `backend_selection`/`capability`, then write the payload gate against the existing 106-case lock.
4. Amharic: implement templated infix emission, then selection wiring, then the payload gate against
   the existing 200-case lock.

Until 1 completes, the honest count stays **0 of 3**, and any report claiming otherwise is
describing compile parity rather than payload certification.
