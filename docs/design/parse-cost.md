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

The two tools do not count the same thing, so a count is compared only with a count from the same tool.
Measured 2026-11-11 on the first machine, with the same words and work cap and two builds from before #232
(the Windows one five commits older, none of them in the parser):

| Grammar | Windows counters | Callgrind in WSL | Ratio |
|---|---:|---:|---:|
| Sena, 80 words | 2,020M to 2,032M | 1,085M | 1.87 |
| Swahili, 77 words | 854M to 856M | 433M | 1.97 |
| Indonesian, 70 words | 2,047M to 2,094M | 1,204M | 1.72 |
| Amharic, 100 words | 8,774M | 5,831M | 1.50 |

`work_steps` was identical on the two systems and the cost total differed by at most 8 parts per million, so the
parser did the same counted work. The Windows counters themselves are right: a loop of two instructions
per iteration read 4,001.3M instructions for 4,000M known ones between two lengths, plus 16M to 20M for each
process. Callgrind counts user-mode instructions of a Linux build, and the Windows counters also include
what the kernel runs for the process. So "instructions" has no single level across the two, and a cost
weight fitted to callgrind is in callgrind's units.

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

With no switch it reports what is missing and exits 1 until the machine is ready. `-Verify` traces a
`dec`/`jnz` loop at two lengths and refuses unless the counted difference is within 1% of the known one. A ready
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
On 2026-11-12 the known-loop `-Verify` passed on that machine with memory integrity (HVCI) on.

## Why the weights are fixed

The weights follow the code more than the grammar, so a stored per-project weight earns nothing. A
code change can move them, which is what `cost-calibrate` is for.

Cost is an index. It is read only as a ratio of two totals, so it is judged on its shape and on how well
it predicts a change, never on its level against instructions.

113 / 2 / 45 (#233) replaced 676 / 2 / 268. Work and registers keep their ratio, scaled by about a sixth; bytes
stay at 2, so bytes carry a larger share of cost than before. Evidence, from two change pairs over seven
grammars (callgrind in WSL; change of each grammar's total, error in percentage points):

| Pair | Weights | Sign right | Mean / worst error | Per-word direction (moved 2% or more) |
|---|---|---|---|---|
| #232: bytes -17% to -44%, registers unchanged | 676 / 2 / 268 | 7 of 7 | 5.35 / 11.37 | 430 of 430 for every set (Swahili and Cebuano only) |
| | 113 / 2 / 45 | 7 of 7 | 5.32 / 11.49 | |
| `HC_RSL=off`: work falls, registers rise | 676 / 2 / 268 | 4 of 4 | 0.78 / 2.67 | 535 of 593 (all seven) |
| | 113 / 2 / 45 | 4 of 4 | 0.28 / 0.63 | 569 of 593 |
| | `work_steps` alone | 2 of 4 | 4.29 / 13.55 | 317 of 593 |

- On #232 the two sets tie: 676 / 2 / 268 predicts too small a cut on all seven grammars, 113 / 2 / 45 too large
  a cut on six. Per-word direction says nothing there, because every word moved the same way.
- `HC_RSL=off` is the pair that separates them, and the only one that splits work from registers. Its best
  ratios are 116 / 2 / 42; the tied fits below give 101 to 111 / 2 / 40 to 44.
- Two pairs is thin, and both are callgrind. A standing set of pairs, including grammar edits, is #240.

Limits:

- Magnitude is rough. On #232 a real -27.7% read -39.2% and a real -17.1% read -14.8%.
- Changes under 2% are not resolved: on `HC_RSL=off` a real +0.42% read +1.04% and a real -0.14% read 0.00%.
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
new weights.

**What decides.** Cost is only ever read as a ratio of two totals (`cost-compare`), so multiplying all three
weights by one factor changes no verdict. The tool therefore decides on what a ratio can see:

- the **shape**: the per-word error on held-out grammars after the median predicted/measured ratio is divided
  out;
- the **change**: for every grammar, the change of its total that the weights predict from the Before and
  After counts, against the change `instruction-compare` measured;
- the per-word **direction** on held-out grammars.

The **level** (predicted against measured instructions) is printed and decides nothing. It also has no single
value: see the table under "Where instructions are measured".

The rule it prints:

- **ACCEPT** only if direction is no worse, the predicted change of grammar totals is no worse (the sign on
  every grammar that moved 0.5% or more, and a mean error within 0.5 points) and the shape is better on the
  held-out set.
- **KEEP** otherwise, including a fit whose words do not carry the byte term and any `-NoChangeCheck` run (a
  proof run of the tool, which never exits 0).
- **REFUSED** when a measurement cannot be trusted: a failed no-change control, a status change, a missing
  grammar, or fewer than 8 scored words in any held-out grammar.

**The fit.** `cost-calibrate --fit-counts` solves the least-squares fit on relative error exactly, with no
weight below zero, and prints how much worse the fit is when work, bytes or registers is left out. The words
carry a term only if leaving it out makes the fit at least a tenth worse. Per-word totals of work and of
registers copied rise and fall together, so real word lists do not split them. Measured on main with #232 and
#234:

| Fit words | Free fit (work / byte / register) | Fit without work | Fit without registers | Work and registers at 676 : 268, which 113 : 45 keeps |
|---|---|---|---|---|
| 199 (40 per grammar) | 58.2 / 2.27 / 60.7 | x1.01 worse | x1.08 worse | 111.1 / 2.25 / 44.0 |
| 779 | 177.0 / 2.22 / 13.6 | x1.06 worse | x1.00 worse | 100.9 / 2.26 / 40.0 |

The free weights differ by a factor of three to four between two samples of the same code, and the fit with the
two held at their current ratio moves by a tenth, at a cost of 0.1 points of fit error. So when the words do
not carry both, the candidate keeps the ratio of work to registers and refits their sum against bytes. Only the
byte weight is settled by per-word totals (2.16 to 2.30 over 1000 resamples of the 779 words).

A change that moves work and registers apart does split them. Turning the required-slot look-ahead off
(`HC_RSL=off`, one binary) lowers work and raises registers copied: on Swahili work -6.4%, registers +11.3%,
instructions +2.6% (callgrind, 323 words). Over the seven grammars the ratios that predict that change best
are work/byte 58 and register/byte 21, which is 116 / 2 / 42. `work_steps` alone has the wrong sign there on
Sena and on Swahili, on callgrind and on the Windows counters.

It warns when a weight moves by more than a quarter, and a result from 40-word samples is not evidence to adopt.

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
