# Sweep commands for land-an-optimization

Reached from step 3 and step 4 of `SKILL.md`. Paths below are from this machine; `<scratch>` is the session scratchpad, never the repo.

## One core, one run (default)

The sweep takes one core. One grammar at a time, `--threads=1`, one process, never beside another sweep run:

```
nice -n 19 ionice -c3 /usr/bin/time -v <bin> batch --threads=1 --work-columns --analyses=<abs>/out.analyses \
  --word-timeout-ms 20000 <grammar> <words> <abs>/out.tsv 2> <abs>/time.txt
```

`<abs>/out.tsv` is the positional third argument; stdout carries nothing the sweep keeps, so redirecting it leaves `out.tsv` unwritten.
Defaults when the flags are omitted: `--work-cap` 5,000,000,000 per word, `--word-mem-cap-mib` 100. Both are real bounds
(Mbugwe `maanja` hits the work cap under them), so "default caps" in `SKILL.md` means these two.
Before any tree or build, `df -h /home`: each tree's release target is large and the sweep data adds more. Below 30 GB free, run
the `cleaning-up-worktrees` skill first; the v0.35.0 sweep started at 96% full.

`stile admit --jobs 1` before every build, release build with `-Jobs 1` (see Build). Batch runs are one process by construction, so they need no admit.
`stile admit` prints `waiting: load ...` until the box has room; wait for it (about 10 minutes under load in the v0.35.0 sweep). A slow walk is the same loop over grammars, sequential,
smallest first, Before then After per grammar, reps interleaved. `stile reap` refuses PIDs not started from the caller's shell unless
given `--pid-file` inside the lane root; start sweep jobs with a pid file there.

## Build

Make both trees with `stile worktree new`, which takes a SHA (verified here; detached worktrees work):

```
cd /home/johnm/work/PanGloss
git fetch origin main refs/pull/<N>/head          # FETCH_HEAD is the PR head
stile worktree new PanGloss --detach <origin/main sha> --path /home/johnm/work/PanGloss.worktrees/sweep-before-<N>
stile worktree new PanGloss --detach <PR head sha>     --path /home/johnm/work/PanGloss.worktrees/sweep-after-<N>
```

For each tree, one at a time: `stile admit --jobs 1`, then
`cd <tree> && nice -n 19 pwsh -NoProfile -File rust/tools/pg.ps1 -Mode build -Package pg-cli -Jobs 1`: 18 to 19 minutes on Linux, the lint
gate first, plus the admit wait. Copy `<tree>/rust/target/release/pangloss` (the target directory is in the tree unless `CARGO_TARGET_DIR`, `PANGLOSS_SSD_CACHE_ROOT`, `PANGLOSS_TARGET_ROOT` or `PANGLOSS_CARGO_CACHE_ROOT` is set; then the build output names it) to
`<scratch>/bin/pangloss-before` and `-after`, and run those copies.
Run `parse_compare.py` from the stable checkout (`/home/johnm/work/PanGloss`) or a copy in `<scratch>`: the sweep trees can be removed
mid-run by an external cleanup, and the script's path goes with them.
Remove both trees with `stile worktree remove <path>` after the sweep.

## Run and measure

- Under `pwsh -File`, `pg.ps1 -Mode run` rejects `--` (pass args bare, in `--threads=1` and `--analyses=path` form), refuses outside a git
  checkout, and needs absolute paths. Its pwsh wrapper adds 1.3 to 6 s user CPU and 130 to 155 MiB RSS.
- So for CPU and peak RSS run the bare `pangloss` under `/usr/bin/time -v` (command above): "User time" and "Maximum resident set size".
  Copy each binary out of its target directory first. Load is noisy (swahili After 221 s against 370 s on one box); work counts are the reliable column.
- Report the SKIPPED count per grammar from the TSV; SKIPPED words are IDENTICAL trivially and compare nothing.

## Settling a status difference

TIMEOUT is wall clock. For every STATUS_DIFF word, write them to a list and rerun both sides at `--word-timeout-ms 300000`
(then 600000 for what remains), keeping the default caps. Treat MEMCAP like CAP. For a CAP word, rerun at `--work-cap` raised
(50000000 settled aweti). Mbugwe `maanja` is settled for v0.35.0: Before CAPs at the default 5.26G work; given `--work-cap 200000000000` and
`--word-timeout-ms 14400000` (4 h) it ends MEMCAP at 16.39G work after 66 minutes (30G is not enough); After ends MEMCAP at 0.93G work, 17.6x fewer steps.
So the Before and After statuses agree once the cap order is set aside; rerun it only for a change that moves memory accounting. Compare again with `parse_compare.py --multiset`.
Expected residue: Before CAP at the work cap, After MEMCAP at the 100 MiB word-memory ceiling (fewer steps reach it first). Raised reruns on
heavy words take 10 to 25 minutes a side per set.

Lowered caps exercise CAP, which default caps never do: aweti `--work-cap 1000000`, cal-work-cap `--work-cap 60000`; list newly finishing words.

## Calibration grammars

They are not in PanGloss: `rust/tools/calibration/manifest.json` holds expected counts only. Grammars live in the conformance suite's
`calibration/<set>/` (`grammar.xml`, `words.txt`), which exists in `conformance/` only after `pg.ps1 -Mode conformance-test -Scope all`
has cloned it at `rust/tools/calibration/conformance.commit`. This machine also has a clone at `/home/johnm/work/fieldworks-parser-conformance`.
`cal-lexsize-50k` ships `grammar.sha256` and no `grammar.xml`; regenerate (checked, hash matches):

```
cd <conformance clone> && python3 -I -c "import sys; sys.path.insert(0,'scripts'); import calib_a_gen as g; \
open('<scratch>/cal-lexsize-50k.xml','w',encoding='utf-8',newline='\n').write(g.fam_lexsize(50000)['xml'])"
sha256sum <scratch>/cal-lexsize-50k.xml   # equals calibration/cal-lexsize-50k/grammar.sha256
```

Other grammars and word lists: `/home/johnm/work/fst-production-20261103/lanes/motif/optimization-workflow/grammar-inventory.md`;
the five main grammars are under `/home/johnm/work/fst-production-20261103/inputs/main5/`. Copy data to scratch.

The inventory lists no Tamil `base`, `p145` or `p147` grammars. They are `base.fwdata`, `p145.fwdata` and `p147.fwdata` in
`/home/johnm/work/fst-production-20261103/lanes/motif/issue-477/`, and they are the largest movers (instructions -13% to -85% for #481),
so include them. The inventory's own `tam.fwdata` is light (54M work in total) and, for #481, read +0.78% [+0.70, +0.86], a small real regression: its control's
noise floor is about 0.45% (see "Verdicts").

The inventory leaves three choices open; the v0.35.0 sweep took these, and the PR names whichever it takes:
- Tamil: the 23,682-word `tam9/0220b-lexical-core.words.txt` list against each Tamil grammar, plus a 24-word `cap24` list (the words
  that hit the cap) against base, p145 and p147.
- Swahili: `inputs/swh/swh.fwdata` with the full 280,099-type corpus.
- Cebuano: the 44,801-word `ceb-rig-20261116/words.txt` list; of the three candidate `ceb.fwdata` copies, record the md5 of the one used.

## Capped words and per-grammar summaries

- `parse_compare.py` reports a word whose side is CAP as CAPPED and does not compare the partial signatures, so a change that alters what a
  capped word reached passes silently. For each CAPPED word, compare the two `signature` columns (the partial result at the cap) yourself, and
  say in the PR whether they agree. v0.35.0 used a scratch `capcheck.py` for this; `parse_compare.py` itself does not.
- No tool summarises work, CPU and RSS per grammar. Take work from the `--work-columns` TSV (sum per grammar), "User time" and "Maximum
  resident set size" from the `/usr/bin/time -v` file, and tabulate Before against After in the PR. v0.35.0 used a scratch `summarize.py`.
  A script that does this belongs in `rust/tools`; until then write it in `<scratch>` and say so.

## Verdicts

- NO-GO (regression) on a grammar the change does not reach (work identical or nearly so): compare it with its own Before-against-Before
  control, run at the same repeats. A regression inside the control's noise floor is noise: record both numbers in the PR; it does not block.
  A regression above the floor (tam at +0.78% against a floor of about 0.45%) blocks until the sweep record explains it in writing, naming the
  cause and the size (for example "fixed per-word table overhead on words with no repeated matches") and the lead signs off there.
- Calibration sets of 5 to 7 words and about 10k work are noise-dominated: report their verdicts, do not gate on them.
- A tiny control (retune's 60-word samples) reads INCONCLUSIVE on swh and ceb. That is the expected reading, not a failure.


## Windows (instruction-compare)

- Run `stile admit --placement windows --jobs 1` first. `herdr-x run win` runs PowerShell, not bash (no `tail`, `grep`).
- No push needed: in the Windows PanGloss clone run `git fetch origin main refs/pull/<N>/head`, then make both trees with
  `stile worktree new <windows clone> --detach <sha> --path H:\repos\PanGloss.worktrees\sweep-before-<N>` (origin/main sha) and
  `... sweep-after-<N>` (PR head sha); run through `herdr-x run win` (`~/.claude/skills/herdr/wsl/herdr-x`, not on PATH). `--detach` and `--path` are in `stile worktree new --help`.
- Build each: `herdr-x run win --cwd <worktree> --wait -- 'pwsh -NoProfile -File rust/tools/pg.ps1 -Mode build -Package pg-cli -Jobs 1'`.
  The exe lands at `C:\cargo-targets\<worktree name>\release\pangloss.exe` (11 to 15 min each).
- Make every Windows tree with `stile worktree new`, as above; `git worktree add` skips the registration `stile` keeps and the cleanup it enables.
- `herdr-x run win` hands the command to PowerShell, so write arrays in PowerShell syntax (`-Grammars a.fwdata,b.fwdata` or `@(...)`), not
  as a bash list and not through a nested `pwsh -File`. `herdr-x run win` also consumes stdin, so never pipe it into or feed it from a loop; run one command per call.
- `rust/tools/instruction-compare.ps1 -Before <exe> -After <exe> -Grammars <g> -Words <w>` with Windows paths; data on a Windows drive
  (WSL `/mnt/h/...` paths work). Run the control (Before against Before) once per session.
- The PMC lock is machine-wide: one grammar per run, one run at a time. Each takes 1 to 6 minutes at 5 repeats.

## Expected wall per grammar (one side, 1 thread, loaded box)

| grammar | wall |
|---|---|
| indonesian, amharic, 16 calibration sets | under 3 s |
| aweti, sena | 7 to 22 s |
| tamil cap24 (base, p145, p147) | 5 to 70 s |
| swahili full corpus (280,099 types) | 4 to 8 min |
| cebuano 44,801 words | 4 to 8 min (grammar load, 417 MiB RSS) |
| tamil core 23,682 words | 10 to 24 min |
| mbugwe, `--word-timeout-ms 20000` | 21 to 34 min |
| mbugwe reruns at 300 / 600 / 3600 s | 10 to 25 min per word set |
| release build (fat LTO) | 18 to 19 min each, plus the admit wait |
| retune cost-calibration build | about 17 min each, two of them |
| retune (60 words, results reused) | about 26 min |
| retune callgrind verify | over 45 min |

Default `--word-timeout-ms` is 20000 for Mbugwe and Tamil; leave the others unset. A one-core sweep of all grammars, both sides, is
about 4 to 5 hours: schedule it overnight.

## Retune on Windows runs

- The retune pair and control (`-PairResult`, `-ControlResult`) need Windows instruction-compare runs on retune's own samples: 60 words per
  grammar, word `i` taken at index `floor(i*len/60)` of the grammar's ok-both list. A saved run log parses directly.
- Retune builds twice with `--features cost-calibration`, Before then After, about 17 minutes each. Copy the binary out of the target
  directory after the first build, or the second overwrites it.
