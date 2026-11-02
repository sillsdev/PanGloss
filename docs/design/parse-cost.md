# Parse cost

Parse cost is a deterministic estimate of a word's instructions:

```
cost = work_steps × 676 + bytes allocated × 2 + registers copied × 268
```

`work_steps` counts budget reservations. Bytes are requested from the allocator, and registers are
the FST registers copied in `pg_fst::traverse`. The weights are constants in `pg-cost`.

## What each measure is for

| Measure | Use | Moved by load? |
|---|---|---|
| Instructions | The verdict on any algorithm change | No, within 0.3% |
| Cost | Hosts and quick checks: `batch --cost-column`, then `cost-compare` | No; totals moved at most 0.0001% across thread counts and word order |
| `work_steps` | Caps | No |
| Wall time | Never a comparison on a shared machine | Yes: +5% to +121% measured |

**Every algorithm improvement gets an instruction comparison**: the same word list on both builds,
per grammar, with a no-change control as the noise floor. Cost predicts direction; it does not
replace that run, because a code change can change what a unit costs.

## Why the weights are fixed

The weights follow the code more than the grammar, so a stored per-project weight earns nothing. A
code change can move them, which is what `cost-calibrate` is for.

Evidence for 676 / 2 / 268, from 6 historic change pairs across 5 grammars (30 rows, 6113 words kept
because their status was `ok` on both sides, 0 work-count mismatches against the stored runs):

- When cost moved by 2% or more, its direction matched instructions 17 of 17 times; at 5% or more,
  10 of 10.
- Pearson r 0.978, median error 0.46 points, worst 9.51 points. `work_steps` alone: direction 16 of
  17, r 0.908, worst 16.67 points.
- These weights came from the same grammars they are scored on, so this is in-sample.
  Leave-one-grammar-out fits ranged work 311-700, bytes 2.09-2.55, registers 259-385, with scale
  0.98-1.14 against the built-in weights. 676 / 2 / 268 tied 500 / 2 / 330 on direction and had the
  lower median and worst error.

Limits:

- Magnitude is rough. A real -45% read about -55%; a real -9% read -3%.
- Changes under 2% are not resolved: a real +1.15% read -0.71%.
- Cost only sees changes in how much counted work happens. A constant-factor change, such as a
  cheaper hash per register copied, leaves the counts identical and reads as no change. Instruction
  comparison (`rust/tools/instruction-compare.ps1`) remains the required measure for algorithm
  changes; cost is the quick grammar-author signal.
## Cost is a report

No cap, deadline or parse decision reads cost, so it cannot change a parse. Measurement stays off
until a host asks for it, and asking on a build without `pg_cost::CountingAllocator` is an error
rather than a cost that silently omits allocation. `batch` refuses `--cost-column` with `--stats`,
because the collector's own allocations would count as parse cost.

Cost is compared only within one PanGloss version, on one word list, with no deadline. It never
compares two languages.

A capped cost is a lower bound for that side, so a word that newly caps would shrink the total and
read as an improvement. `cost-compare` therefore totals only words that are `ok` on both sides, and
reports how many words it left out (`excludedWords` in `--json`), including words capped on both
sides. If any word's status differs between the runs (`statusChanges`), the verdict is
"inconclusive" whatever the percent. A different signature on an `ok` word is reported beside the
verdict as `parseChanges`. A row whose status field is not `ok`, `CAP`, `TIMEOUT` or `SKIPPED`, as
happens when a word contains a tab, is refused as an unexpected shape.

## Tuning the weights

`pangloss cost-calibrate` exists only with the `cost-calibration` feature, and no released build has
it. It measures about 40 evenly spaced words on one thread and prints suggested weights, leaving the
constants to a code change.

- With `--targets`, it fits instruction counts, which load cannot disturb.
- Without it, it fits wall time and refuses a run where the thread held the CPU under 95% of wall
  time. Measured: idle 0.98-0.99, 20 spinning processes 0.48-0.72, 20 copies 0.50-0.57.
- Machine speed is removed first, and one run can move a weight only ×0.75-1.5.

## Retuning after a parser change

After a change to the parser gets a GO from `rust/tools/instruction-compare.ps1`, `rust/tools/retune-parser-work.ps1`
tests whether the weights still fit; the steps, the skip rule and what an owner decides are in
`.claude/skills/retune-parser-work/SKILL.md`. It refits on the fit grammars from per-word Valgrind
instructions (`cost-calibrate --parse-only`, `--targets`, `--counts-out`, `--fit-counts`) and scores old and
new weights on grammars held out of the fit. The rule it prints:

- **ACCEPT** only if the fit settled and, on the held-out set, direction is no worse, median error is lower
  and worst error is no worse. A real pair whose counts did not change has no direction rows and can still
  ACCEPT on level error; the reason then says "direction unmeasured (0 rows)".
- **KEEP** otherwise, including a fit that did not settle and any `-NoChangeCheck` run (a proof run of the
  tool, which never exits 0).
- **REFUSED** when a measurement cannot be trusted: a failed no-change control, a status change, a missing
  grammar, or fewer than 8 scored words in any held-out grammar.

One fit moves a weight at most ×0.75-1.5, so the tool continues it from its own result until it settles; the
counts do not depend on the weights. Beside the level error it prints a scale-free error (the median
predicted/measured ratio divided out), report-only, so an ACCEPT shows whether the shape improved or the
weights only rescaled. It warns when a weight moves by more than a quarter, and a result from 40-word samples
is not evidence to adopt.

It never edits a weight and never touches `work_steps`. Memory, simulated cache misses and CPU time print
beside the result and stay out of the fit.

## Deployed grammars

WASM keeps its default cap. `PanGlossGrammar.withoutWorkLimits` removes the attempt and work limits
for a host that trusts its grammar. The counters remain, but no cap can stop a word: counting costs
under 1% of a parse, and the root search cache admits entries by the work counted. On wasm32 `usize::MAX` is 2^32-1, so the limit is still about 4.29e9 units and a word can report
capped there. A word with no limit can run for minutes, blocks its thread, and can exhaust linear
memory and trap, so it belongs off the UI thread. WASM installs no
counting allocator, so it cannot report cost.

The evidence and open questions are in `openspec/changes/accurate-work-steps/design.md`.
