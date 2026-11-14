# Parse cost

Parse cost is a deterministic estimate of a word's instructions:

```
cost = work_steps × 200 + bytes allocated × 1.25 + allocator calls × 150
```

`work_steps` counts budget reservations. Bytes are requested from the allocator, except that a trail step is charged as the 40 bytes it cost before the per-thread slab (#284) and the slab's own growth is unmetered, so cost does not depend on the thread count. Allocator calls are
`alloc`, `realloc` and `alloc_zeroed` on the counting allocator; a trail step's synthetic byte charge is not a call. The FST
registers copied in `pg_fst::traverse` are still counted, at weight 0: once allocator calls are in, they add nothing.
The weights are constants in `pg-cost`, held in quarters of an instruction (`WEIGHT_SCALE`).

## What each measure is for

| Measure | Use | Moved by load? |
|---|---|---|
| Instructions | The verdict on any algorithm change | No, within 0.3% |
| Cost | Hosts and quick checks: `batch --cost-column`, then `cost-compare` | No; totals moved at most 0.0001% across thread counts and word order |
| `work_steps` | Caps | No |
| Wall time | Never a comparison on a shared machine | Yes: +5% to +121% measured |

**Every algorithm improvement gets an instruction comparison**: the same word list on both builds,
per grammar. Cost predicts direction; it does not replace that run, because a code change can change
what a unit costs.

### How a comparison decides

Each side runs `-Repeats` rounds (default 5 on the Windows counters, 2 on callgrind), alternating
which side goes first. Each round gives one parse count per side, counted directly (see "How the parse
is counted apart from the load"), or, for a PMC binary without `--phase-marks`, as the full run minus
that round's empty-list run, so the start-up noise stays in the spread. Welch's t gives a 95% two-sided interval for the change of
the mean, in percent of Before, printed on every comparison. With a minimum effect of 0.5%:

| Verdict | When | Exit code |
|---|---|---|
| GO | the interval lies below zero and the change is at least 0.5% smaller | 0 when every grammar is GO |
| NO-GO (regression) | the interval lies above zero and the change is at least 0.5% larger | 10 |
| NO-GO (no change) | the whole interval lies within ±0.5% | 10 |
| INCONCLUSIVE | anything else: too wide to claim a change or its absence | 11, over 10 |

A refused grammar is 3 and a refused run is 2, over everything else. INCONCLUSIVE asks for more
repeats; it is not a quiet GO. Why Welch's t, and what was rejected:
`docs/adr/0015-instruction-verdict-statistics.md`.

The **instrument check** is a binary compared with itself. It must read neither GO nor
NO-GO (regression); NO-GO (no change) and INCONCLUSIVE both pass. Run it once per machine session and
whenever a verdict looks odd. It checks the instrument; it is not part of every comparison and sets no
threshold.

**How the parse is counted apart from the load.** Thread attribution cannot do it: at `--threads 1`,
which the comparison uses, `pangloss batch` takes its sequential path (the `if threads == 1` branch in
`rust/crates/pg-cli/src/main.rs`) and parses each word inline on the thread that loaded the grammar
(measured on a synthetic stress grammar, 65 words, one pinned CPU: that thread read 344M with no words,
573M with the list and 790M with the list twice; at `--threads 2` and `4` the pool threads' total for
the same 130 words differed by 2.3%). Subtracting an empty-list run leaves the load's noise in the
figure: on Swahili (load 2.55G, parse 0.51G) a build against itself read +4.3% and on Cebuano (load
27.6G, parse 3.2G) it read -15% or refused, and #272 read +3.88% on Cebuano while the instructions
inside `parse_word_opts` moved +0.30% (#277, #278). So each backend counts the parse phase directly:

- **callgrind** collects only while `Morpher::parse_word_opts` is on the stack (`--collect-atstart=no
  --toggle-collect=*parse_word_opts*`), callees included. It needs no change to the binary.
- **PMC** needs the binary to mark the phase. `batch --phase-marks` (with `--threads 1`) starts and
  joins one thread that spins about 60M instructions before the first word and one after the last. The
  trace carries thread starts and a context switch with a counter snapshot at each join. A process also
  starts loader threads (8M to 34M instructions each in the nine grammars) and others, so
  `instruction_compare_etl.py` takes as the markers the two threads, other than the busiest, whose own
  total is 51M to 69M (the spin measured 60.2M to 60.5M), and totals what the process's other threads run between the first marker's first
  switch-out and the second marker's first switch-in (`parse_instructions`). It refuses a trace that
  does not yield exactly two markers. The marks cost a thread start each and are equal on both sides.

**The backends measure different scopes.** Callgrind counts only inside `parse_word_opts`. The PMC
parse phase counts everything the parse thread runs between the two markers: the STARTED write,
status/signature and TSV formatting, buffer writes, kernel time, and any path that bypasses
`parse_word_opts`. Known caveat (#294): on cebuano.json the `HC_RSL=off` pair reads +5.59% on PMC
(CI +-0.27%) and +0.14% on callgrind, while the other eight grammars agree in direction. Do not read a
PMC NO-GO on Cebuano as a parser regression without the callgrind cross-check.

When a binary lacks `--phase-marks` the PMC backend falls back to the per-round subtraction and refuses
a grammar whose load is more than `-MaxLoadRatio` (default 4) times its parse, with the reason, instead of
reporting load noise as a change.

**Why run files have neutral names of equal length.** Each run's output files are named
`g<grammar>-r<round>-sideA-<kind>` or `...-sideB-...`. They used to carry `before` and `after`. With
those names, a binary compared with itself read NO-GO (regression) at +0.96% and +1.12% on fusion.xml
(65 words). A per-thread dump put the whole gap, about 1.8M instructions, on the parsing thread of
the run named `after`, in every round, whichever side went first. Swapping the names moved the bias
to the other side. With `sideA`/`sideB` three runs read +0.08%, +0.16% and -0.02%, all INCONCLUSIVE.
The likely cause is that the `--threads 1` writer flushes the output file twice per word, and that
kernel write cost depends on the file; this was not confirmed with kernel stacks.

**Why rows are buffered.** When both binaries list `batch --no-flush` in `--describe`, the comparison
passes it, so a run writes its rows once at exit instead of flushing after every word. Per-word
flushes put file-system work on the parsing thread in bursts: on fusion.xml, 12 of 76 flushed full
runs came out 3M to 21M instructions above their side's median, and 0 of 128 buffered runs on one CPU
did. In one trace of 12 rounds, the same binary read a per-round SD of 5.9M flushed and 0.75M
buffered (#260). A binary without the flag is run as before, on both sides, and the comparison says so.

**Why hash keys are not pinned.** std `HashMap` keys are drawn afresh every run on Windows, so the
spread averages over hash layouts. Pinning one seed in both builds was tried (#260) and not adopted:
two builds of one binary that differ only in the pinned seed read +0.55%, 95% CI [+0.13%, +0.97%], a
NO-GO (regression). A pinned seed is one draw of the layout, and a code change that creates one more
map before another draws a different one, so a pinned comparison carries an offset as large as the
minimum effect that no instrument check can see. Callgrind's pinned keys are the same kind of single
draw: four seeds of the shim gave parse counts 1.5% apart on fusion.xml.

**Expect a wide interval on short lists.** With five rounds and about 65 words, the Windows counters
gave intervals from ±0.5% to ±1% for a binary against itself. Buffering removes the outlier rounds
that widened some intervals much further, but not the ordinary spread: buffered with fresh hash keys,
five rounds gave ±0.50% and twelve gave ±0.74%. A change near the 0.5% minimum effect needs more
`-Repeats` or a longer word list to leave INCONCLUSIVE. Two CPUs instead of one, or an E-core instead
of CPU 4, did not narrow it.

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
report is not proof; only `-Verify` is. Then run the instrument check, a binary against itself with
`instruction-compare.ps1`: it must read neither GO nor NO-GO (regression), and must not refuse.

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

200 / 1.25 / 0 / 150 (work / byte / register / allocation) replaced 113 / 2 / 45 / 0 (#233), which replaced 676 / 2 / 268.
The old model had no term for allocator calls, and what it charged instead (bytes and registers copied) tracks calls
differently in each grammar. Allocator functions are 27% to 38% of all instructions in the main five, Swahili and
cebuano.json, and 50% in ceb.fwdata, at 140 to 250 instructions per call. ceb.fwdata makes fewer, larger allocations (257 B per call)
and copies many registers per call, so it was over-predicted; cebuano.json makes smaller ones (123 B per call) and was
under-predicted. A register copy is one allocation plus a short memcpy (arrays of 6 to 100 slots), and the old weights
charged it about 1,200 against a real few hundred. Least squares on all 390 measured words with allocator calls in
leaves registers at exactly 0. Fitted on the main five only (indonesian, aweti, amharic, mbugwe, sena), with Swahili,
cebuano.json and ceb.fwdata held out; `retune-parser-work.ps1` refits 195.6 / 1.31 / 0 / 149.3 over 206 words, which
rounds within a quarter-instruction of the weights in use. Callgrind in WSL, error is predicted against measured instructions of the
same words (sum / median absolute, percent):

| Grammar | Words | 113 / 2 / 45 | 200 / 1.25 / 0 / 150 |
|---|---|---|---|
| Indonesian | 38 | +7.3 / 8.8 | -1.6 / 3.3 |
| Aweti | 29 | +2.5 / 6.3 | -1.5 / 2.4 |
| Amharic | 50 | +10.8 / 8.1 | -5.2 / 6.4 |
| Mbugwe | 50 | -12.5 / 7.5 | -16.7 / 4.0 |
| Sena | 40 | 0.0 / 3.2 | +2.9 / 2.9 |
| Swahili (held out) | 40 | -18.5 / 21.8 | -7.9 / 9.1 |
| swh.fwdata (held out) | 50 | -17.5 / 18.1 | -5.4 / 6.1 |
| cebuano.json (held out) | 40 | -32.0 / 34.1 | -18.9 / 20.0 |
| ceb.fwdata (held out), 60 words | 60 | +27.3 / 7.6 | +1.0 / 3.9 |
| ceb.fwdata heavy, 20M or more | 28 | +27.6 / 7.9 | +1.0 / 4.6 |

Mbugwe's sum worsens: two of its heavy words carry an extra cost no counted term explains (the trail-step count fits
them better, and is a separate lead). Cebuano's heavy words, the case behind this change, go from +28% to +1%.

Change check, callgrind: `HC_RSL=off` against on moves work and registers apart. The change of each grammar's total is
predicted with mean / worst error 0.47 / 1.30 points by 113 / 2 / 45 and 0.46 / 1.25 by 200 / 1.25 / 0 / 150, and the sign is right
on 6 of 6 grammars that moved.

**Checked against the Windows counters, 2026-11-13.** The same pair and the same samples (`rslbefore` is the `after` build with
look-ahead off, read from the executable name), `instruction-compare.ps1` on the PMC counters, 5 rounds. Raw output, kept beside the
callgrind runs in scratch: `save/win-pmc-pair.txt` (and `.raw`), `save/win-pmc-control.txt` (and `.raw`), `save/win-rslbefore-patch.diff`,
and the predictions from `save/callgrind-rsl-retune.out`. The control (a build against itself) read no change or INCONCLUSIVE
on every grammar except Swahili json, where it read NO-GO (regression), so that row is not evidence. cebuano.json was refused: its
load dwarfs its parse. An earlier run of the same pair read within PMC noise of this one (Mbugwe -12.45%, Sena -0.97%, Swahili
-4.10%) but its output was not kept, so only this run is quoted. Change of each grammar's total, measured on Windows against the
change the weights predict from the counts (error in points):

| Grammar | Windows | 113 / 2 / 45 | 200 / 1.25 / 0 / 150 |
|---|---|---|---|
| Indonesian | +0.23% N | 0.00% (0.23) | 0.00% (0.23) |
| Aweti | -0.55% I | -1.02% (0.47) | -1.02% (0.47) |
| Amharic | -1.52% GO | -1.43% (0.09) | -1.43% (0.09) |
| Mbugwe | -12.55% GO | -10.73% (1.82) | -10.77% (1.78) |
| Sena | -2.37% GO | -1.13% (1.24) | -1.15% (1.22) |
| Swahili json (held out, control failed) | -1.96% I | -1.97% (0.01) | -2.02% (0.06) |
| swh.fwdata (held out) | -0.98% I | -0.40% (0.58) | -0.42% (0.56) |
| ceb.fwdata (held out) | +0.12% I | -0.06% (0.18) | -0.06% (0.18) |

Mean / worst error 0.58 / 1.82 points with the old weights, 0.57 / 1.78 with the new: no difference. The Windows counters do not
disagree with the callgrind fit: the ratio of Windows instructions to predicted cost over the words, per grammar, is more even with
the new weights. Over the seven grammars with a count (not swh.fwdata or cebuano.json) the ratios are 1.26 to 2.28 (coefficient of
variation 19%) with 113 / 2 / 45 and 1.59 to 2.09 (9%) with the new weights, and ceb.fwdata moves from 31% below the other
grammars' mean to 15% below. The level is about 1.8 times callgrind's, as before. No per-word Windows count exists, so the weights
are not fitted on Windows.

The weights before this change, with their evidence:

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
- `--counts-out` rows are `idx, work, bytes, registers, allocations, instructions`.

## Retuning after a parser change

After a change to the parser gets a GO from `rust/tools/instruction-compare.ps1`, `rust/tools/retune-parser-work.ps1`
tests whether the weights still fit; the steps, the skip rule and what an owner decides are in
`.claude/skills/retune-parser-work/SKILL.md`. It refits on the fit grammars from per-word Valgrind
instructions (`cost-calibrate --parse-only`, `--targets`, `--counts-out`, `--fit-counts`) and scores old and
new weights.

**What decides.** Cost is only ever read as a ratio of two totals (`cost-compare`), so multiplying all the
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
- **REFUSED** when a measurement cannot be trusted: a no-change control that reads GO or regression, a status change, a missing
  grammar, or fewer than 8 scored words in any held-out grammar.

**The fit.** `cost-calibrate --fit-counts` solves the least-squares fit on relative error exactly, with no
weight below zero, and prints how much worse the fit is when work, bytes, registers or allocator calls is left out.
The candidate is the free fit, rounded to a quarter; since allocator calls joined the fit, registers fit to 0 and there is no
work-to-register ratio to tie. The two paragraphs below are the fit as it was with three terms (113 / 2 / 45, #233 and #234). The words
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

**Checked against the Windows counters, 2026-11-12 (a2a35131).** Pair: look-ahead off (Before) against main
(After), one build with the switch read from the executable name. The comparison and its control came from
the Windows counters (`-PairResult`, `-ControlResult`, 5 rounds), on every word of each list that parses `ok`
at the default cap with a cost under 500M. Cebuano was left out: its load is about nine times its parse,
and both of its runs refused. That leaves Swahili as the only held-out grammar, which is not the standard set.

| | 113 / 2 / 45 (current) | 105 / 2 / 42 (candidate) |
|---|---|---|
| Change of grammar totals, sign | 5 of 5 | 5 of 5 |
| Change, mean / worst error (points) | 0.93 / 2.24 | 0.93 / 2.20 |
| Held-out direction (Swahili, 77 words) | 41 of 42 | 41 of 42 |
| Held-out shape, median / worst | 3.38% / 15.27% | 3.22% / 14.49% |

The tool read ACCEPT. The weights were kept: the candidate moves work and registers by 7%, inside the
101 to 111 / 2 / 40 to 44 that samples of one code already give (above), the change prediction is the same,
and the shape gain is 0.16 points on one grammar. The fit (400 words, five grammars) again could not split work
from registers (x1.01 without registers) and carried bytes (x4.05 without them). Measured against the Windows
counters, the change of each grammar's total was -7.35% (Aweti), -6.75% (Mbugwe), -3.10% (Amharic), -2.84%
(Sena), +0.18% (Indonesian, no change) and -0.63% (Swahili, inconclusive); 113 / 2 / 45 predicted -5.11%,
-5.52%, -3.09%, -2.34%, 0.00% and -2.06%.

## The standing calibration set

Each family's cost ladder answers one question: does parser cost track instructions linearly as one driver grows? The set
is the synthetic `calibration/` folder of `sillsdev/fieldworks-parser-conformance` (16 `cal-*` grammars: six scale families, the
`cal-lexsize-1k` to `-50k` lexicon ladder and six structural families), pinned in `rust/tools/calibration/conformance.commit`
(`3ba5db1`, the merge of #43 and #44, content identical to `b8a65d1` on which the counts were measured). Its words are calibration only: they are
slow on purpose, `pg-conformance-fixtures` never enumerates them (`calibration_leaks` and its tests), and no normal parse or test
run reads them.

`rust/tools/calibration-set.ps1 -Measure` runs every family through `cost-calibrate --counts-out` and per-word callgrind
instructions (collected inside the per-word parse, the bracket `retune-parser-work.ps1` uses) and stores word-free
`<set>.counts.tsv` rows plus `manifest.json` (both commits, the binary hash, flags, a rung for every word index) in
`rust/tools/calibration/`. `cal-work-cap` runs at `--work-cap 200000` with its cap words kept (`--keep-capped`: a capped word stops at a
fixed step count, so its counts are repeatable); `cal-prefilter-scale` runs with `--prefilter on`. Measure from a build with
`--features cost-calibration,foma-tools`.

`calibration-set.ps1 -Score [-Weights w,b,r,a]` scores weights on the stored counts, with no measurement, and
`retune-parser-work.ps1` prints the same report for the current and candidate weights (`-SkipStoredSet` omits it). It gives:

- per family, the mean and RMS of the per-word relative error against instructions (words under 1000 work units left out, as in a fit),
  and a shape error with the set's scale divided out;
- per rung, the cost-to-instruction ratio against the family's mean rung ratio. A family is **NON-LINEAR** when a ladder rung
  is more than 7% off. Why 7%: callgrind repeats agree within 0.005%, so the limit is about the model, not noise, and what the ladder
  is for is predicting the change a landed edit makes to a total. Those edits move totals by 2% to 7% (#232 up to 44% on bytes alone,
  `HC_RSL=off` 0% to 13%), so a rung that drifts more than about that cannot predict one. An earlier 15% was the spread between
  grammars' levels (max/min 1.26), a different quantity from drift along one driver. The largest drift measured at
  200 / 1.25 / 0 / 150 is 6.8% (`cal-prefilter-scale`, 64x) and 6.6% (`cal-allomorph-explosion`, 1x, one 4-word rung): inside the limit,
  deliberately near it. Rungs of 4 to 6 words are single-word ratios (clitic-chain scores 5 of 6 words past the 1000-work floor,
  large-lexicon 15 of 25), so a flag on one is a lead, not a verdict. A drift is measured from the family's own mean rung ratio, so a family
  that is uniformly off shows only in the level columns. `wasted` and `band` groups are printed (marked `*`) but never flag: a ladder
  drift asks whether cost follows a driver in proportion, and these groups mix drivers (`wasted` is words with no analysis, all search;
  `band` is a spread of sizes), so they differ from the ladder mean by their composition. They are the largest drifts in the report
  (`wasted` +9.6% on `templatic-nonconcat`, `band` -9.8% on `prefilter-scale`); they would flag three families at 7%;
- the **real-grammar guard**: per-grammar totals of the stored real counts (`real-grammar-totals.json`: totals only, no words and no per-word rows)
  scored the same way. It gates on the **scale-free** error, the per-grammar error left after the set's own scale (the geometric mean of
  the grammars' cost/instructions) is divided out: a candidate fails it if its mean scale-free error is more than 1.0 point above the
  current weights'. Level is not gated, because cost is read as a ratio and a pure rescale of every weight (113 / 2 / 45 against
  200 / 1.25 / 0 / 150, or any candidate that fits the instruction level better) predicts every change the same. The file holds nine
  grammars of seven languages: Cebuano and Swahili each appear twice, as a json grammar (`cebuano`, `swahili`) and as an fwdata
  project (`ceb`, `swh`) with its own word list. They are different grammar files, so both are kept, and those two languages count
  twice in the means; the 1.26 max/min spread is over all nine. `cebuano` scores 14 of its 40 words. The totals come from the
  #286 build's counts (`parse-cost-alloc/save`, the only stored files with the allocations column; the older `cost-review-rsl`
  files have none);
- the **change pairs** (below), which are what #240 was for. A candidate fails a pair when its mean change error is more than 0.5
  point above the current weights', or it loses a sign the current weights get right; the weights themselves must keep each pair's
  mean change error within 1 point (the sign of each family that moved is reported, and a candidate may not lose one). `calibration-set.ps1 -Score` exits 1 on any of: a NON-LINEAR family, a
  WORSE real-grammar guard, a failed pair gate.

`cal-prefilter-scale` is scored and shown but is not in the means or in the set's scale: its instructions include the admission and tiled-filter
walks, which cost does not count, so its error (20.8%, scale 0.789) is a modelling gap that no weight closes (`excludeFromMean` and
`modellingGap` in the manifest).

### Change pairs

A level error says how far cost sits from instructions; a retune is used on the change between two builds. A pair measures the same
words through a before and an after and stores, per family, the totals of both sides (work, bytes, registers, allocations and the
instructions inside the parse; no words) in `rust/tools/calibration/pairs/<name>.json`, listed in the manifest's `pairs`. `-Score`
predicts each family's change from the counters and compares it with the measured change, in percentage points, with the sign of every
family that moved 1% or more. A word counts if both sides parsed it and it reaches the 1000-work floor on either side; `cal-prefilter-scale`
is left out of pairs. Both sides must write the allocations column (`-MeasurePair` refuses a binary that does not). A future pair (#299's
NFA change) is added with `calibration-set.ps1 -MeasurePair -PairName ... -BeforeExe ... -AfterExe ...` (or `-BeforeEnv/-AfterEnv NAME=value`
for one binary), then committed with the pair file and the manifest line.

The three stored pairs, scored at 200 / 1.25 / 0 / 150 (change of each family's total, predicted from the counters against measured
in callgrind instructions; error in points; the per-family rows print with `-Score`):

| Pair | Builds | Families | Mean / worst error | Sign (moved 1% or more) | Gate |
|---|---|---|---|---|---|
| `hc-rsl-off`: look-ahead off, work and registers move apart | one binary, `HC_RSL=off` | 12 | 0.25 / 0.83 | 2 of 2 | yes |
| `stratum-diff-275`: #275, a stratum's output words stored as differences | `b98e75ac` and `2708ab80`, each with #286's counting cherry-picked | 11 | 0.39 / 2.18 | 0 of 1 | yes |
| `trail-slab-284`: #284, trail steps in a refcounted slab | `0d0cb92e` and `77f4aaa8`, each with #286's counting cherry-picked | 11 | 3.17 / 8.06 | 1 of 9 | report-only |

- The synthetic words barely move under `HC_RSL=off`: ten of twelve families stay inside 0.5%, so that pair reads two families
  (`templatic-nonconcat` -2.4%, `deep-template-16` +1.4%). It still splits work from registers where they move, which the real-grammar totals
  cannot do (the pair on real grammars is in the table above, 0.46 / 1.25 points).
- `stratum-diff-275` is a count-changing edit and cost follows it (every family within 0.7 points but `allomorph-explosion`, where
  instructions rose 2.2% and cost fell 0.02%: a constant-factor cost of the change, not a counted one).
- `trail-slab-284` is the case the "Limits" list names: the slab charges each step as its former 40-byte allocation, so every counter
  stays put while instructions rose 1.9% to 7.6% on seven of eleven families (the refcounts and chunk bookkeeping). Cost reads that as no change, and the sign
  and size are both missed. No weight can fix that, so it is stored and printed but gates nothing. It is the pair that shows what cost does
  not cover, and a retune is not judged on it.
- The older builds have no `--work-cap`, so `cal-work-cap` is not in `stratum-diff-275` or `trail-slab-284`.

Not covered: Windows PMC columns for the set are deferred (a run needs a Windows binary of the same commit and
17 grammars of traces); #299's NFA change joins the pairs once it lands; grammar-edit pairs (#240) are not stored. Instructions of a `--prefilter on` word include the filter's table walks, which cost does not count.

## Mechanism trial (machine#575), rerun 2026-11-12

Each mechanism was switched off in one build of a2a35131 and compared with that build switched on, per
grammar, with `instruction-compare.ps1` (5 rounds, rows buffered). Sena, Swahili, Cebuano and Indonesian ran at
`--work-cap 100000`, Mbugwe, Aweti and Amharic at 20,000,000, as in the first trial (2026-11-10, builds of
b663c8cf, 2 rounds, the old max-min threshold). The instrument check (a build against itself) read no change or
INCONCLUSIVE on every grammar. Change in instructions, 95% CI, verdict:

| Off | Sena | Swahili | Indonesian | Mbugwe | Aweti | Amharic |
|---|---|---|---|---|---|---|
| Look-ahead | +2.46% [2.25, 2.68] R ¹ | +3.68% [0.96, 6.41] R ³ | +0.01% [-0.10, 0.12] N | +6.02% [5.48, 6.56] R ² | +17.29% [17.23, 17.34] R | +3.24% [2.94, 3.54] R |
| Next-slot overlap | +0.16% [0.04, 0.28] N | +2.62% [2.01, 3.23] R ³ | -0.02% [-0.16, 0.13] N | -0.11% [-0.23, 0.01] N | +9.55% [9.49, 9.61] R | +1.63% [1.49, 1.78] R |
| Allomorph prefilter | +0.13% [0.00, 0.27] N | +2.30% [1.48, 3.13] R | +0.13% [0.02, 0.24] N | -4.89% [-5.10, -4.68] GO ² | +45.65% [45.57, 45.73] R | +9.49% [9.33, 9.64] R |
| Both edge checks | refused | +128.05% R | +6.00% R | -2.24% [-2.48, -2.00] GO ² | +57.21% R | +9.66% R |
| Edge index | +11.64% [11.29, 12.00] R ¹ | +23.86% R | +1.22% [1.05, 1.39] R | +0.93% [0.72, 1.14] R ² | +1.44% [1.39, 1.50] R | +0.24% [0.15, 0.33] N |
| Look-ahead and both edge checks | refused | +142.04% R | +5.72% R | +1.87% [1.68, 2.06] R ⁴ | +108.52% R | +15.20% R |

R is NO-GO (regression), N is NO-GO (no change). Refused: a word's status differs between the builds.
¹ 79 of 80 words, ² 99 of 100, ⁴ 98 of 100: the word that is capped on one build only, or whose capped partial
output differs, is left out. ³ Not resolved, see below. Cebuano is not in the table: its load is about nine
times its parse, the build against itself read -15% [-69, +38], and three of seven cells refused.

What changed from the first trial, and why. The first trial's builds were rerun with the new tool to separate the
instrument from the code:

- **Instrument.** Amharic look-ahead read +0.95% "within noise" in the first trial. It is +3.06% on the first
  trial's builds and +3.24% on a2a35131. Amharic next-slot overlap read "within noise". It is +1.48% and +1.63%.
  The old threshold came from a max-min spread that one outlier round could widen; on Amharic the first trial's
  control spread was 146%.
- **Code.** The edge index saved 3.5% on Aweti in the first trial and 3.71% on its builds now, but costs 1.44% on
  a2a35131, where the Aweti baseline parse is about twelve times cheaper. The Aweti look-ahead and next-slot figures
  fall the same way (20.1% to 17.3%, 11.9% to 9.6%).
- **Unattributed.** The allomorph prefilter on Sena (+1.82% then, no change now) and on Indonesian.
- **Holds.** Switching the next-slot overlap off saves instructions on no grammar, on either build.

Swahili is at the instrument's limit at 77 words: its load is about five times its parse, and the build against
itself read -0.54% [-1.40, +0.32] at 5 rounds and -1.20% [-3.41, +1.01] at 10. Look-ahead off read +3.68% at 5
rounds and +6.56% [4.87, 8.25] at 10, but the same 77 words without a cap, in the pair above (the builds the
other way round), read main 0.63% cheaper [-3.03, +1.76], INCONCLUSIVE. Both runs cap no word and count the same
work. Its direction is probably a cost; its size is not
measured. Next-slot overlap read +2.19% [-0.69, +5.07] at 10 rounds.

## Deployed grammars

WASM keeps its default cap. `PanGlossGrammar.withoutWorkLimits` removes the attempt and work limits
for a host that trusts its grammar. The counters remain, but no cap can stop a word: counting costs
under 1% of a parse, and the root search cache admits entries by the work counted. On wasm32 `usize::MAX` is 2^32-1, so the limit is still about 4.29e9 units and a word can report
capped there. A word with no limit can run for minutes, blocks its thread, and can exhaust linear
memory and trap, so it belongs off the UI thread. WASM installs no
counting allocator, so it cannot report cost.

The evidence and open questions are in `openspec/changes/accurate-work-steps/design.md`.
