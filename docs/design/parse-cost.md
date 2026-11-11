# Parse cost

Parse cost is a deterministic estimate of a word's instructions:

```
cost = work_steps × 113 + bytes allocated × 2 + registers copied × 45
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

### Where instructions are measured

Instructions come from the CPU's hardware counters, so `rust/tools/instruction-compare.ps1` runs on
Windows. WSL does not expose the counters. Callgrind in WSL counts simulated instructions; use it only
as a provisional signal, never as the verdict.

The script does not need an elevated shell. It starts and stops a trace through two scheduled tasks
that hold the elevation:

| Task (`\PanGloss\`) | Runs as | Action |
|---|---|---|
| `PanGloss PMC Start` | the measuring user, highest privileges | `wpr.exe -start "C:\ProgramData\PanGlossPmc\pg.wprp!PG" -filemode` |
| `PanGloss PMC Stop` | the measuring user, highest privileges | `wpr.exe -stop "C:\ProgramData\PanGlossPmc\current.etl" -skipPdbGen` |

The profile, `rust/tools/instruction-compare/pg.wprp`, records `InstructionRetired` alone on each
context switch, using a programmable counter. It records one counter because the reader takes the first
value of each event: with `TotalCycles` added, a binary compared with itself read 837M against 1,777M
instructions on the same grammar. It does not use the fixed counters
(`InstructionsRetiredFixed` and the like): something else on the machine can hold them, and on the
first machine set up something did (see below). The profile is `Strict="true"`, so WPR refuses to
start when it cannot claim a counter. Without that, WPR silently records a trace with no counter values.

#### Setting up a machine

`rust/tools/pmc-setup.ps1` installs and checks all of this. Run it from your own unelevated shell. The steps
that need an administrator ask for elevation once and register the tasks for you, not for the
administrator account:

```powershell
pwsh -NoProfile -File rust/tools/pmc-setup.ps1 -Install   # one UAC prompt
pwsh -NoProfile -File rust/tools/pmc-setup.ps1 -Verify    # must print VERIFIED
```

With no switch it reports what is missing and exits 1 until the machine is ready. `-Verify` takes a
real trace of a short busy process and refuses unless the trace counted instructions for it. A ready
report is not proof; only `-Verify` is. Then run a binary against itself with
`instruction-compare.ps1`: that control must read NO-GO, not refuse.

If either script refuses because the PMC Start task failed, or with "trace carries no counter values on
context switches", Windows recorded no counter values, and no instruction verdict is possible until
`-Verify` passes. To see why, run `wpr -start "C:\ProgramData\PanGlossPmc\pg.wprp!PG" -filemode` in
an elevated shell, then `wpr -cancel`. "The requested resource is in use" (0x800700AA) means another
component holds the counters the profile names.

On 2026-11-10 the first machine set up (an i7-12700 running Windows 11, Defender, and the hypervisor
for WSL2) failed this way. It had been recording traces with no counter values because the profile
named the fixed counters and was not strict. A strict profile with the fixed instruction counter
failed with 0x800700AA, while `InstructionRetired` and `TotalCycles` recorded. The holder was not
identified. Microsoft Defender is reported to claim the fixed counters on Intel Core CPUs. Turning
memory integrity (HVCI) off first made no difference, so it was not the cause.
`-DisableMemoryIntegrity` remains for a machine where `-Verify` shows otherwise. Memory integrity set
by group policy cannot be turned off this way, and the script refuses to try.

## Why the weights are fixed

The weights follow the code more than the grammar, so a stored per-project weight earns nothing. A
code change can move them, which is what `cost-calibrate` is for.

Evidence for 113 / 2 / 45, a rescale of the earlier 676 / 2 / 268 shape after #232 and #234. The old weights
over-predicted instructions about 2x (the 2026-11-02 proof run on a pre-#228, pre-#232 build already showed
93% median error; pg-cost was last set 2026-11-01). The work and register terms are collinear in the
samples, so the split is not refitted: one factor, 0.167, scales both, chosen to minimise the median level error
on the 779-word fit set (indonesian, aweti, amharic, mbugwe, sena; words that parse ok and cost under 20M
instructions). The byte weight stays 2 because an integer cannot scale to 0.33. Median / worst level error:

| | fit set | held out (Swahili + Cebuano, 723 words) |
|---|---|---|
| 676 / 2 / 268 | 175.50% / 290.26% | 86.09% / 194.38% |
| 113 / 2 / 45 | 4.45% / 32.04% | 16.14% / 41.30% |

- Free fits are unstable: 40 words per grammar gave 58 / 2 / 61, 779 words gave 177 / 2 / 14 and did not settle in 20
  rounds. Work and register cannot be separated with these words, so the shape is kept.
- Follow-up: refit the split once FST-heavy words decorrelate the work and register terms.
- Earlier evidence (6 change pairs, r 0.978) described the previous parser and no longer applies.

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
