# Analysis-memo fixes: measurement plan (skeleton)

Skeleton only -- a later agent fills in the Results section after running the instruments below
against `fix/analysis-memo-bounds` (the `memo-fixes` worktree). See
`docs/research/analysis-memo-explosion.md` for the bug this is measuring.

## What this measures

Whether the analysis-memo fix changes what `pg_parse::Morpher` finds (it must not) and by how much
it changes wall time and peak memory (the point of the fix).

## How to reproduce the measurements

1. **In-repo parity gate** (synthetic, always runs, must be green before and after the fix):
   `rust/tools/pg.ps1 -Mode test -Package pg-parse -TestTarget memo_parity_gate`
   Every conformance-fixture word, memo on vs off: same deduplicated analysis-identity set, same
   `capped` flag.

2. **Corpus parity + survival gate** (needs `samples/data/`, or `PANGLOSS_CORPUS_ROOT` pointed at a
   checkout that has it):
   `rust/tools/pg.ps1 -Mode corpus-test -Package pg-foma -TestTarget memo_corpus_gate`
   - Aweti: first 44 words, step cap 200,000
   - Sena: first 300 words, default step cap (50,000,000)
   - Mbugwe: first 60 words, step cap 2,000,000
   - Asserts identical analysis-identity sets for every word that completes on both sides, and that
     a capped word is capped on both sides (a capped word's own partial set is not compared --
     step order legitimately differs, see `analysis-memo-explosion.md`).

3. **Wall time / peak memory**: build once, then measure each mode:
   ```
   rust\tools\pg.ps1 -Mode build -Bin pangloss -DebugProfile
   rust\tools\memo-measure.ps1 -Grammar <grammar> -Words <words.txt> `
     -Exe <build output>\pangloss.exe -StepCap <N> -MemoModes on,off
   ```
   Add `-Env @{ HC_STEP_STATS = '1' }` for a total-step column.

## Results

TBD. Report per instrument: (1) fixture/word/analysis counts and pass/fail; (2) corpus-gate
checked-word and capped-word counts per corpus, pass/fail; (3) wall seconds, peak WS MB, CAP/TIMEOUT
counts, TSV SHA-256, and total steps, one row per grammar x memo mode.

## Byte-budget margin

`pg_memo::AnalysisScope`'s per-table byte budget (`DEFAULT_MEMO_BYTE_BUDGET`) is a second guard
alongside the entry and word caps. This section records the sweep that set its value; it does not
speak to the rest of this doc's (still-TBD) Results section.

Debug build, `--threads 1`, `-RunMemoryGB 6`, `--step-cap unbounded`, budget swept via `HC_MEMO_BYTES`
(a developer-diagnostic env var read in `pg-cli`, mirroring `HC_STEP_STATS`/`HC_MEMO_STATS`).

**Aweti** (`Ajkululape`, `ekozokotu` in one word file; `oteʼikateʼika` run separately since it
aborts before either budget matters -- see below):

| Budget | Ajkululape steps (x unbounded) | ekozokotu steps (x unbounded) | Peak WS (both words) |
|---|---|---|---|
| 16 MiB | 2,135,886 (7.53x) | 13,613,118 (6.70x) | 2249.7 MB |
| 64 MiB | 1,777,964 (6.27x) | 9,622,166 (4.74x) | 2331.7 MB |
| 256 MiB | 283,572 (1.00x) | 7,877,597 (3.88x) | 2578.2 MB |
| 1 GiB | 283,572 (1.00x) | 2,032,063 (1.00x) | 3262.8 MB |
| none (word cap only) | 283,572 (1.00x, unbounded baseline) | 2,032,063 (1.00x, unbounded baseline) | 3259.4 MB |

TSV SHA-256 (ms-stripped) is `866c5a3e872c61990ee1aefd73cc53128d0b83d2274a5db139565dfa1336e707` at
every budget above -- byte-identical output regardless of the byte budget.

`oteʼikateʼika` aborts with a Rust allocation failure at both tested extremes -- 16 MiB (peak
~4999 MB reached before the abort) and none (~5430 MB) -- so its failure is independent of the
memo byte budget. This matches the upstream port note that this word still aborts at 8 GB; it is a
pre-existing limitation this constant cannot fix (the memo bounds its own tables, not the
unmemoized cascade's own live working set).

**Sena** (300 words, default step cap) and **Mbugwe** (60 words, step cap 2,000,000): both
insensitive to the budget -- peak WS stays within a few MB across {64 MiB, 256 MiB, none} (Sena:
42.5-52.3 MB; Mbugwe: 411.7-419.4 MB, CAP=4 throughout), and the ms-stripped TSV SHA is identical
at every budget for each corpus. Neither corpus's words are large enough to exercise the byte
budget.

**Decision rule**: the smallest budget at which no word's step count exceeds 1.5x its
unbounded-memo count and every word stays under 2 GiB peak. No tested budget satisfies both --
even 256 MiB leaves `ekozokotu` at 3.88x steps and the pair's peak at 2578 MB. Falling back to
"prefer bounded memory and say what it costs": **256 MiB** is the smallest tested point where one
word (`Ajkululape`) matches its unbounded step count exactly and the other's ratio is the best
among the meaningfully-bounding options (3.88x vs. 64 MiB's 4.74x and 16 MiB's 6.70x), while still
cutting peak memory ~21% versus no budget on this pair (2578 vs. 3259 MB). Going smaller (64 MiB,
16 MiB) buys only marginal further memory reduction (2331.7 MB, 2249.7 MB) at steadily worse step
costs, and 1 GiB/none give no bounding effect at all for this pair. Cost of the choice: a
pathological word can still need several times its unbounded step count, and peak memory for a
genuinely adversarial word is not guaranteed under 2 GiB -- the byte budget bounds the memo
tables, not the process's total working set.
