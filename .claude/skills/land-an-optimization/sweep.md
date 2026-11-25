# Sweep commands for land-an-optimization

Reached from step 3 and step 4 of `SKILL.md`. Paths below are from this machine; `<scratch>` is the session scratchpad, never the repo.

## One core, one run (default)

The sweep takes one core. One grammar at a time, `--threads=1`, one process, never beside another sweep run:

```
nice -n 19 ionice -c3 /usr/bin/time -v <bin> batch --threads=1 --work-columns --analyses=<abs>/out.analyses \
  --word-timeout-ms 20000 <grammar> <words> > <abs>/out.tsv 2> <abs>/time.txt
```

`stile admit --jobs 1` before every build, release build with `-Jobs 1` (see Build). Batch runs are one process by construction, so they need no admit.
`stile admit` prints `waiting: load ...` until the box has room; wait for it. A slow walk is the same loop over grammars, sequential,
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
`cd <tree> && nice -n 19 pwsh -NoProfile -File rust/tools/pg.ps1 -Mode build -Package pg-cli -Jobs 1` (7 min). Copy
the `release/pangloss` that the build output names (the target directory is derived per tree) to `<scratch>/bin/pangloss-before` and `-after`, and run those copies.
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
(50000000 settled aweti; Mbugwe `maanja` needs `--work-cap 30000000000`). Compare again with `parse_compare.py --multiset`.
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

## Windows (instruction-compare)

- Run `stile admit --placement windows --jobs 1` first. `herdr-x run win` runs PowerShell, not bash (no `tail`, `grep`).
- No push needed: in the Windows PanGloss clone run `git fetch origin main refs/pull/<N>/head`, then make both trees with
  `stile worktree new <windows clone> --detach <sha> --path H:\repos\PanGloss.worktrees\sweep-before-<N>` (origin/main sha) and
  `... sweep-after-<N>` (PR head sha); run through `herdr-x run win` (`~/.claude/skills/herdr/wsl/herdr-x`, not on PATH). `--detach` and `--path` are in `stile worktree new --help`.
- Build each: `herdr-x run win --cwd <worktree> --wait -- 'pwsh -NoProfile -File rust/tools/pg.ps1 -Mode build -Package pg-cli -Jobs 1'`.
  The exe lands at `C:\cargo-targets\<worktree name>\release\pangloss.exe` (11 to 15 min each).
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
| release build (fat LTO) | 7 min each |
| retune callgrind verify | over 45 min |

Default `--word-timeout-ms` is 20000 for Mbugwe and Tamil; leave the others unset. A one-core sweep of all grammars, both sides, is
about 4 to 5 hours: schedule it overnight.
