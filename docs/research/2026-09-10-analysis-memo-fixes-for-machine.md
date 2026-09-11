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
