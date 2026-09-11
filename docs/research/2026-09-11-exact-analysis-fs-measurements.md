# Exact analysis-FS mode: five-grammar measurements

Measured 2026-09-11, same method as `2026-09-10-hc-analysis-fs-port-measurements.md` (release builds,
`pangloss batch --threads 1 --word-timeout-ms 60000 --stats` through `pg.ps1 -Mode run` under an 8 GB
job ceiling, the same 30-word stride samples and pinned worst-word lists, side order alternated per
sample). Sides:

- **before** = main at 459e8cb1: PriorityUnion analysis (upstream PR 494), state-keyed merge (PR 493),
  plus the memo-bounds work landed since.
- **exact** = this branch: `ana_syn_fs` is the exact inverse of synthesis (gate on
  `PriorityUnion(required, out)`, strip `out`'s feature paths off the stem, unify with `required`,
  never clear), and the two analysis-side template-battery collapse sites widen the survivor's FS
  with `union` instead of dropping a same-key word.

## Parse sets

`rust/tools/parse_compare.py` on every pair: no analysis-set difference on any word both sides
completed (199 words). Two status changes, both gains:

- Amharic worst `ተማሪዮቹን`: `TIMEOUT` at 60 s before, `ok` in 35.6 s now, two analyses.
- Aweti: the before binary exceeded the 8 GB ceiling at word 19 (`otope`) and never finished the
  sample; the exact binary finished all 30 words.

## Time and deterministic work (words both sides finished with `ok`)

| Sample | ok pairs | before ms | exact ms | time | before attempts | exact attempts | attempts |
|---|---:|---:|---:|---:|---:|---:|---:|
| Amharic worst | 4 | 40,492 | 11,359 | 3.56x | 23,525 | 6,629 | 3.55x |
| Aweti 30 | 18 | 43,201 | 16,911 | 2.55x | n/a (aborted) | 23,670,761 | n/a |
| Mbugwe 30 | 27 (+3 timeouts each) | 74,569 | 51,647 | 1.44x | 39,309,213 | 29,475,214 | 1.33x |
| Amharic 30 | 30 | 41,506 | 30,906 | 1.34x | 18,459 | 11,311 | 1.63x |
| Sena worst | 9 | 275 | 256 | 1.07x | 47,828 | 45,617 | 1.05x |
| Sena 30 | 30 | 127 | 137 | 0.93x | 35,476 | 35,422 | 1.00x |
| Indonesian 30 | 30 | 61 | 78 | 0.78x | 3,151 | 2,431 | 1.30x |

Sena is unchanged: its rules carry required features, so the stronger gate rarely adds a rejection and
there are no `out`-only features to strip. Indonesian does 30% fewer rule attempts; its wall-clock
figure is tens of milliseconds and inside noise. The Mbugwe attempt counts include the three 60 s
timeouts on each side, which dominate them.

## What changed semantically, and the oracle

hc.dll master still folds a rule's required FS with `Add`; upstream PR 494 moves to `PriorityUnion`;
the exact inverse exists upstream only on the owner's research branch (`HC_ANALYSIS_FS_MERGE=Exact`).
So this is a deliberate divergence from the founding oracle in one direction: the exact mode finds
parses hc.dll loses when an outer rule's `out` feature overwrites an inner rule's `out` feature on the
stem (`rust/crates/pg-parse/tests/exact_analysis_fs_recall.rs`), and it admits a strict subset of the
rule attempts, so it cannot lose a parse hc.dll finds. No conformance expectation changed; the full
suite reports only the pre-existing `sena3_imports_with_expected_counts` fwdata drift.

The C# research note lists FS-aware template-battery dedup as the prerequisite for shipping Exact.
Rust's equivalent (`run_template_batch_raw` / `apply_slot_batch` widening) is implemented, but no
existing or new test in this repo could be made to fail with it disabled: the ported
`same_rule_used_in_multiple_templates` and the new two-template test each parse separate surface
words that never collide inside one battery call. That branch is therefore defensively correct and
untested for being load-bearing; a direct `crate::stratum` unit test forcing a same-key collision is
the open follow-on.
