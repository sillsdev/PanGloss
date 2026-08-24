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

Those four are the reported regressions, and they cannot be observed here, so no fix to them can be
verified here either.

The mechanism they concern is the short-circuit in `backend_runtime.rs`: a candidate whose adapter
`interprets_plan()` and whose plan carries unbuildable markers returns
`Certification::Unsupported` with zero measured operations, before any build or confirm. The
contract governing that is `out_of_scope_marker_subtrees_are_attributed_not_blamed_on_the_grammar`,
which requires a whole-grammar strategy to return a real measurement and a `PlanComposed` candidate
with unbuildable markers to be refused. **That contract test runs and passes.** Whatever the four
corpus cases disagree about is therefore specific to the real grammar and invisible without it, so
changing the short-circuit on the strength of the passing synthetic contract alone would risk
breaking the one guarantee currently proven.

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
