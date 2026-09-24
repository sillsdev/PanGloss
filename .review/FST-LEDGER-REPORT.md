# FST ledger reconciliation report

**Status: incomplete; managed verification stopped at the required storage guard.** This report
records the baseline and the edits made before that guard blocked further builds. No Machine
submodule files were changed. Gate 1/2 work is committed locally as `e498a7e7`; no push or merge was
performed.

## Baseline

Before editing, I ran:

```powershell
pwsh -NoProfile -Command '$env:PANGLOSS_MIN_FREE_SSD_GB = "15"; & ".\rust\tools\pg.ps1" -Mode conformance-test -Scope all'
```

The run executed 2,721 tests: 2,717 passed, 4 failed, and 143 were skipped. The four failures were
the two orthogonal-basis premise checks and the two backend ratchets described below.

## Gate findings

### 1. Staging clone — premise changed legitimately (a)

The baseline clone guard failed because
`conformance-staging/edge-cases/backend-ordered-generic` lagged the upstream
`machine:languages/metathesis-phase-isolation` input. I synchronized its `grammar.xml` and
`words.yaml` from Machine commit `34215889c7adf3012f700c9ee1c1d6712c056d15`, changing only the
language name. A static byte comparison after normalizing that name confirms the files match; the
clone test now also compares ordered word inputs. This static check is not a substitute for the
blocked managed test run.

The old 2026-08-31 founding-oracle note applied to the previous staged word list. `STAGING.md` now
limits that claim to those previous rows and records that the newly synchronized upstream rows were
not rechecked against C# in this worktree.

### 2. Bounded-copy census — premise changed legitimately (a)

Machine added `mrRedupHi` (`redupFixedI`), whose repeated input part has finite width 1. I added a
second rule-level exercise on committed word `titula`, retained the existing `mrRedupCV` width-2
exercise, asserted both finite bounds alongside unbounded `redupFull`, and removed the stale
corpus-wide ceiling. The two bounded-copy witnesses remain in one independent source fixture; the
synchronized staging clone is not counted as an independent fixture.

The new word is part of the baseline conformance corpus, whose parser checks passed. The new
rule-level test and the full post-edit suite could not be run because of the storage guard.

### 3. TunedSurfaceProbed scoreboard — correctness/representability gap (b)

The baseline measured TunedSurfaceProbed at 70 oracle-exact fixture cells and 2
compiles-but-misses cells, against the stale 71/1 ratchet. The new Machine miss is
`machine:languages/metathesis-phase-isolation`, word `hasaasa`: the committed oracle requires
identity `morphemes=[8, 16]`, `root_index=1`, and the containment report says the proposal set
offered it zero times.

I synchronized the staging clone after that baseline. It now has the same grammar and ordered word
inputs as the Machine fixture, so it will reproduce the same backend outcome as a second scored
fixture cell. The post-sync scoreboard count was not measured: 69 exact / 3 misses is the direct
projection if that cloned cell has the same deterministic outcome. The scoreboard ratchet is
therefore still unchanged and must be measured before it is updated.

### 4. Proposal-faithfulness coverage — correctness/representability gap (b)

The baseline reported 20 failed `(construct kind, backend)` pairs against the `NoMoreThan { failures:
14 }` ratchet. Six new failures are all TunedSurfaceProbed's missing `hasaasa` identity above:

| Construct kind | Backend | Fixture and word | Required identity |
|---|---|---|---|
| IterativeRewrite | TunedSurfaceProbed | `machine:languages/metathesis-phase-isolation`, `hasaasa` | `[8, 16]`, root index 1 |
| LeftToRightRewrite | TunedSurfaceProbed | same fixture and word | `[8, 16]`, root index 1 |
| Metathesis | TunedSurfaceProbed | same fixture and word | `[8, 16]`, root index 1 |
| SubruleGating | TunedSurfaceProbed | same fixture and word | `[8, 16]`, root index 1 |
| CircumfixOutputAction | TunedSurfaceProbed | same fixture and word | `[8, 16]`, root index 1 |
| Reduplication | TunedSurfaceProbed | same fixture and word | `[8, 16]`, root index 1 |

The containment gate directly measured one required identity offered zero times for each pair. Source
inspection supplies the likely cause: the fixture derives `hasaasa` by copying `hasa` and deleting
the second copy's initial `h` (`prHDel`), leaving surface pieces `hasa` + `asa`. The query-time
`ReduplicationPeeler` in `rust/crates/pg-foma-runtime/src/peel.rs` recognizes repeated chunks only
when the compared character slices are equal and infers prefix/suffix order from the matched
position. On this surface it can match the shorter `asa` suffix, but that places the reduplication
morpheme after the root; the oracle requires the leading-copy identity. This is a source-based
diagnosis of the measured miss, not a post-edit backend experiment.

The TSP card is a static contract and does not claim recovery of phonologically altered copies. A
rule- and phonology-aware inverse alignment is not a small evidence-backed fix within that stated
route, so no backend implementation was changed. The containment ratchet remains unchanged pending
a post-sync measurement; the baseline value 20 is expected to remain the distinct pair count after
the clone sync because the clone duplicates the same kind/backend failures, but that was not
verified.

No new soundness failure was reported by the baseline coverage run: candidate-only surviving
identities were zero. These are recall/correctness findings, not readiness refusals or resource
containment decisions.

## Verification blocker and next action

After the baseline and edits, live counters showed about 30.5 GB available physical memory and
active Cargo/rustc processes, so I did not start another build. A managed `pg.ps1 -Mode doctor`
preflight (with `PANGLOSS_MIN_FREE_SSD_GB=15` set first) reported only 14.4 GB free on C: and chose
`G:\cargo-build-cache`; writing its owner marker there was denied. The brief says to stop if a build
would use G:, so I did not run `-Mode check`, the post-edit `-Mode conformance-test -Scope all`, or
`-Mode test`, and did not try to free space or redirect the cache.

The gate 1/2 edits are in local commit `e498a7e7`; the evidence ledger and this report are in the
follow-up local commit. Ratchets 3/4 are still at their baseline values. Once the permitted C:
target root has at least the required 15 GB free,
the remaining work is to run the managed check, measure and set the post-sync scoreboard and
faithfulness ratchets, rerun both requested suites with the environment set before each `pg.ps1`
call, and commit the verified changes incrementally. No push or merge was performed.
