---
name: retune-parser-work
description: >-
  Use after a change to the parser gets a GO from instruction-compare, to test whether the parser-work
  weights in pg-cost (work, bytes allocated, allocator calls) still describe the code and, if a refit
  is better on held-out grammars, to propose it. Trigger on "retune the cost weights", "recalibrate parser
  work", "did this speedup move the weights", "200 / 1.25 / 0 / 150". Skip it for any change that does not reach
  the parser, including pangloss facts. It never retunes work_steps, which is the cap.
---

# Retune the parser-work weights

Parser work (named cost in the commands and `pg-cost`) = a x work_steps + b x bytes allocated + c x FST registers copied + d x allocator calls, in
instructions, with a, b, c, d fixed at 200, 1.25, 0 and 150 in `rust/crates/pg-cost/src/lib.rs` (held in quarters of an instruction).
Registers are counted and weighted 0: with allocator calls in the fit they add nothing. A parser change can move what one unit costs,
and then the weights drift. It is report-only: no cap, deadline or parse decision reads it, so changing
its weights cannot change a parse. `work_steps` is the cap and is never retuned here: retuning it could
change a parse, and that is a separate, output-affecting change.

## When to use

After a parser change got **GO** from `rust/tools/instruction-compare.ps1` (a 95% interval below zero and a
change of at least the 0.5% minimum effect, with a no-change control that reads neither GO nor regression). Run it before the change lands if the weights should move with it,
or in the next change if not.

## When to skip

Skip when the change does not reach the parser. Decide with the diff, not a guess:

```
git diff --name-only origin/main...HEAD | grep -E '^rust/(crates/(pg-parse|pg-rules|pg-fst|pg-shape|pg-featstruct|pg-grammar-model|pg-snapshot|pg-cost|pg-grammar)/(src/|Cargo\.toml$)|crates/pg-cli/src/main\.rs$|Cargo\.toml$|Cargo\.lock$)' | grep -vE '(^|/)tests?(\.rs|/)'
```

The crates are `pg-parse` and its normal dependencies in the workspace, plus `pg-grammar`, which builds the
grammar the parser reads. No output: skip. Output: read it. A `Cargo.lock` or profile change counts only if
it moves a dependency of those crates or the release profile. The global allocator in
`rust/crates/pg-cli/src/main.rs` also counts, because it prices every allocated byte; read that file's hunk. Changes to
`pg-facts`, `pg-assess`, the rest of `pg-cli`, tests, tooling and docs do not reach the parser, so a
`pangloss facts` speedup never triggers this skill. The rule held on #123 (FST traversal hash: reaches),
#122 (pruning: reaches), and #127 and #125 (facts: skip).

A change that only makes each counted unit cheaper at a constant factor leaves the counts identical, and
cost cannot see it; the weights then move only if the refit says so.

## Procedure

1. Confirm the trigger: the GO, and the path rule above.
2. Build **Before** and **After** release binaries with `--features cost-calibration`, both from trees that
   contain `cost-calibrate --counts-out` (set `PANGLOSS_EXTRA_ARGS='--features cost-calibration'`, then
   `pwsh -NoProfile -File rust/tools/pg.ps1 -Mode build -Package pg-cli`). Copy each `pangloss` out of the
   target directory. An older build lacks the command and the tool refuses it by name.
3. Write a manifest in local scratch, never in the repo (format in the header of `rust/tools/retune-parser-work.ps1`).
   Default split: indonesian, aweti, amharic, mbugwe and sena fit; Swahili and Cebuano held out. Use scratch
   copies of the grammars and word lists, with every word status ok (the tool refuses SKIPPED, CAP and
   TIMEOUT words, and prints counts, not words).
4. Run it, one managed job at a time, with `-SaveDir <scratch>` so a rerun reuses the saved counts and
   instruction-compare output:
   `pwsh -NoProfile -File rust/tools/retune-parser-work.ps1 -Manifest <manifest> -Before <old> -After <new> -SaveDir <scratch>`.
   It verifies with instruction-compare and a no-change control, refits on the fit set from per-word
   Valgrind instructions, scores old and new weights on the held-out set, and decides. Plan about an hour
   for the default seven grammars (Cebuano alone is about 13 minutes per cache-simulation run). It is slow:
   Valgrind runs at 30 to 200 times native speed and a large grammar's start-up dominates, so keep each list
   to fast words (under about 0.3 s native). Options:
   - `-Words N` (8 to 400, default 40): words per grammar. Start-up dominates the run time, so more words add little time and more evidence.
   - `-SkipSideReports`: skip the memory and cache part, the last third of the run.
   - `-NoChangeCheck`: proves the tool on a same-build pair; it is capped at KEEP and never exits 0.
   - `-PairResult` / `-ControlResult`: read saved instruction-compare output; only grammar file names are checked against the manifest.
5. Read the exit code. 0 is ACCEPT: direction no worse, the predicted change of grammar totals no worse, and a
   better shape on the held-out set. Its reason names the evidence: "direction no worse" with 8 or more rows,
   "direction weak" with fewer, "direction unmeasured (0 rows)" and "change of grammar totals unmeasured" when
   the change left the counts equal, so only the shape discriminated. 10 is KEEP: stop, and record the numbers
   in the PR; it also covers a fit whose words do not carry the byte term and every `-NoChangeCheck` run. 3 is
   REFUSED: a failed control, a status change, a missing file or a held-out grammar with under 8 scored words;
   fix the measurement, never the rule. 2 means it could not start.
6. The fit is exact and solved once. Read its lines: the free fit (work, byte, register, allocation) and how much worse the fit is with each
   term left out. The candidate is the free fit rounded to a quarter. A register weight the fit wants above 0 means the words carry a
   register term beside allocator calls: look before keeping it at 0. A weight at 0.25 (the lowest nonzero) means the fit wants
   it lower than a quarter can say.
7. The stored calibration set prints beside the result: per-family error and linearity of the current and candidate weights over the
   synthetic families (a rung that drifts more than 7% flags), the real-grammar guard (on the scale-free error, not the level) and each change
   pair: the change of every family's total the weights predict against the change measured (`docs/design/parse-cost.md`, "The standing
   calibration set"). It is arithmetic on stored counts. A NON-LINEAR family, a WORSE real-grammar guard or a WORSE change pair is a
   finding for the owner (`calibration-set.ps1 -Score` exits 1 on them). Re-measure the set with
   `rust/tools/calibration-set.ps1 -Measure` only when a code change alters allocation shape, and store a pair
   (`-MeasurePair`, two builds that both write the allocations column) for a code change worth checking a prediction against.
8. The level line is report-only. Cost is read only as a ratio of two totals, so one factor on all the
   weights changes no verdict, and the level differs by 1.5 to 2 times between callgrind and the Windows
   counters (`docs/design/parse-cost.md`). Do not retune to fix the level alone.
9. Memory, simulated cache misses and CPU time print beside the result. They are never in the fit or the
   decision; read them to see whether the change traded memory for instructions.

## Applying accepted weights

The tool never edits a weight. After the owner agrees:

1. Edit `WORK_WEIGHT`, `BYTE_WEIGHT`, `REGISTER_WEIGHT` and `ALLOCATION_WEIGHT` in `rust/crates/pg-cost/src/lib.rs`, and the
   numbers in the formula and evidence block of `docs/design/parse-cost.md` (pair count, grammars, rows,
   direction, r, median and worst error, from this run).
2. Add a line to `CHANGELOG.md` naming the old and new weights; the `batch --cost-column` line quotes them.
3. Run `pwsh -NoProfile -File rust/tools/pg.ps1 -Mode check -Jobs 4`, then `-Mode preland -Jobs 4`.
4. Land by squash PR, from a branch rebased on main. Do not push or merge unasked.

Costs from different PanGloss versions are never compared, so a weight change needs no migration.

## What an owner must decide

- Whether to retune at all when the change moved only constant factors: the fit can find a drift that the
  change did not cause.
- Whether a candidate that moves a weight by more than a quarter is credible, given the integer resolution
  of the byte weight (a quarter of an instruction). The tool prints a warning for each such move. A result from
  40-word samples is not adoptable evidence.
- The held-out set. Changing it changes what ACCEPT means.
- Whether the change of grammar totals is evidence enough. One pair gives one result per grammar, and a pair
  that only cuts allocation says nothing about work against allocator calls. A switch in one binary that moves
  them apart (`HC_RSL=off`) is the pair that does; a bytes-moving pair checks the byte weight.
- Whether to change the level. The candidate carries the level of the callgrind fit; nothing reads it.
- Whether a KEEP with a large gap in the candidate warrants a second look at the grammars.

## Limits

Direction uses words whose instructions moved at least 2%; with a no-change pair or a constant-factor change
there are none, no grammar total moves either, and only the shape can move the decision (a no-change pair is
capped at KEEP). The weights are fitted to callgrind in WSL, which the design calls provisional; the Windows
counters read 1.5 to 2 times as many instructions for the same parse, and no per-word Windows count exists to
fit against. Valgrind counts are compared only against Valgrind counts, with
`std` hash keys pinned. The tool runs on Linux and WSL only; `perf stat` does not work under WSL.
