## Context

Production pipeline: `pg_parse::Morpher` per-word parsing, under `pg_rules::stratum::StepBudget`,
which owns the attempt cap and a `pg_fst::work::WorkBudget`. Both consumers of the counters are in
scope: output-affecting caps, and ranking in `backend_optimizer::Score::key`.

Evidence comes from a five-grammar PMU experiment on native Windows (i7-12700), recorded on branch
`worktree-agent-a9d5b7a58398d52af`. Key numbers are in `proposal.md`.

## Goals / Non-Goals

**Goals**

- One work unit, deterministic on every machine.
- A small change in work is a reliable up/down signal, by a stated test.
- Hosts get a before/after number that does not depend on machine load.

**Non-Goals**

- `work_steps` as a time estimate. Time ≈ k · `work_steps` with k per grammar is a report, not a
  contract, and comparison across different grammars stays out of scope.
- A hardware counter in any cap or decision that affects parses.
- Changing any parse below the cap. HC-Rust parity with `hc.dll` is unchanged; C# has no such cap.

## Decisions

### D1: Hardware counters calibrate; they never decide

A cap in cycles or instructions would make parses machine-dependent. Instructions are used offline
to validate work units and to decide A/B questions. Alternative rejected: cycles as the unit, on
determinism and availability.

### D2: Instructions are the ground truth, with two tools

- Windows: PMC on context switch through the `\PanGloss\` scheduled tasks or an elevated shell.
  Includes kernel time. Heavy on a busy host (about 20 MB/s).
- Linux and WSL: Valgrind. User-mode only, slow, and exposes per-function attribution.

Valgrind measured: a known loop reads 2.0000004 instructions per iteration, per-word log R² against
PMC is 0.995-0.999, and the grammar ordering of cost matches. It runs roughly 30-100x slower. Two
conditions apply: `std`'s random hash keys must be pinned (`LD_PRELOAD`, otherwise up to 0.5% per
word), and the word list must be identical between runs (malloc history moves a word by up to 0.5%).

Counts from the two differ by platform and are only ever compared within one tool. Valgrind's cache
simulation is not used: its manual calls it unrepresentative of a modern machine. Changes that touch
memory behaviour also report wall time, peak memory and allocation count, separately.

### D3: Charge by size, not by a fitted class table

Measured: per-class weights vary up to 18x across grammars and, held out by grammar, do no better
than one per-grammar constant. Site-level weights are erratic, and the fitted weights are collinear,
so they are not unit costs. A class table would also silently go stale.

So hidden work is charged in natural units: segments copied, features unified, bytes hashed, items
sorted. This follows the Kissat ticks model. Coarse integer weights are allowed only where a
measurement justifies them.

First result (experiment branch `cost-units-experiment`, 52a74075; parses and `work_steps` unchanged
over 7,375 words): held out by grammar, adding bytes allocated and FST registers copied took mean
median error from 18.6% to 9.0%, Sena from 33.5% to 6.3%, and the per-grammar k ratio from 1.50 to
1.22. Fitted weights: work 676, allocated bytes 2, registers copied 268 instructions per unit. The
audit's other suspects (feature entries, `dedup_key`, shape and word clones, trie scans) added about
0 once those two were present, and per-class a+b·x was not stable across grammars. Five grammars
only, and model selection used the held-out scores, so these figures are optimistic. Bytes allocated
is the most stable weight (1.8x across grammars); registers copied varies 3.6x.

### D4: Report-only before charged

New counts first appear beside `work_steps`, in a separate `ParseOutcome.cost` value that no decision reads.
Only after the acceptance test passes do they move into the budget. Moving them changes which words
hit `CAP`, so that step is output-affecting: it bumps `SEARCH_BUDGET_SEMANTICS`, recalibrates the
default cap and updates divergence 052.

### D5: Reliable means sign agreement, measured on maintained pairs

A pair is two builds or settings with identical parses. Acceptance, provisional until the A/B set
reports:

- A no-change control pair moves `work_steps` by exactly 0 and instructions by no more than the noise
  floor.
- For pairs where total instructions change by at least 1%, the sign of the change in total work
  matches on at least 95% of pair × grammar results.
- Every disagreement is listed with a cause. An unexplained disagreement fails the gate.

Known hard cases must be in the set: the memo on/off pair and the `distinct` fix.

First result, on today's `work_steps`, with six real pairs plus one no-change control over five
grammars (stratified samples, words `ok` on both sides, PMC instructions):

- Control: `work_steps` moved exactly 0, instructions at most 0.28%, cycles up to 17%.
- Where instructions moved at least 5%, work agreed in sign on 11 of 11; at least 2%, 14 of 15; at
  least 1%, 16 of 18. Steps agreed on 6, 9 and 10 of the same results, and often did not move.
- Read from the host side: where `work_steps` moved at least 5%, instructions agreed on 9 of 9; at
  least 2%, 16 of 17.
- Steps fail on coverage, not direction: when they moved at least 1%, instructions agreed on 11 of
  12, but they stayed still on 8 of the 18 results where instructions moved at least 1%.
- Correlation of the changes: 0.91 for work, 0.24 for steps.
- Direction is reliable, size is not: one pair moved work 20.3% and instructions 3.7%.
- The two disagreements were `root-search-cache` on one grammar (work -2.2%, instructions +1.2%) and
  turning required-slot lookahead off on another (work -0.7%, instructions +4.0%).

Today's `work_steps` therefore misses the 95% bar at 1% and reaches it at about 5%. The results are
not independent, since seven pairs produce them.

Second result, using work + bytes + registers (experiment branch `cost-units-experiment`). Counts for
the lookahead pair were joined to its stored instructions. The root cache was measured as a runtime
switch in one binary, with a repeat run as control.

- Results where instructions moved at least 0.5%: work alone 5 of 6 in sign, mean error 7.3 points,
  worst 21.7. Fixed weights 6 of 6, mean 1.9, worst 6.7. Six results from two pairs prove direction
  on these cases only; the 95% bar is still unproven.
- The lookahead miss is fixed: weighted cost +2.3% against measured +4.0%.
- The cache miss did not reproduce. In one binary the cache saves 0.6% on that grammar and work agrees
  in sign. The first reading crossed two builds, so that commit's other effects were in it.
- The worst remaining gap is the cache on the largest grammar: measured 3.8%, work 25.5%, weighted
  10.5%. Direction is right and size is still 2.7x high.
- Control: weighted cost moved exactly 0, instructions at most 0.12% in one binary.

### D6: The attempt cap goes last, behind a recall gate

Scar: the first shared-unit cap took a 400-word sample from 73 to 6 words with analyses, which is why
`d23bfa613` restored attempts. Removing the attempt cap therefore requires showing that no word that
completes today newly caps under the recalibrated default, on all local real grammars and on
conformance. The default has only about 1.4% margin over the observed maximum at the 200,000-attempt
setting, so recalibration precedes removal.

### D7: Ranking moves with an explicit raw-path weight

`Score::key` sums attempts and `raw_paths` 1:1. Work units are about 30x finer than attempts, so a
plain swap would shrink `raw_paths` to almost nothing and reopen the mis-ranking recorded in the
`Score::key` docs (575 vs 127 proposals). The swap needs a measured cost of a raw path in work units.

### D8: Clock reads leave the per-unit path

`WorkBudget::consume` polls the deadline every 64 units; `stopped()` still always polls. Timeouts
are wall-clock and machine-dependent already, so a bounded overshoot of 63 units changes no
guarantee, and the count of work is unchanged. A loop that only polls `stopped()` still observes the
deadline, so the control can always act.

### D9: Host comparison rules

The Motif number is the ratio of total cost on the same word list and PanGloss version, under
`--work-cap` with no deadline. It sits beside parse changes and never compares two languages. Total,
not geometric mean: total tracks time, and a mean lets many cheap words outvote one expensive word.
The display band comes from D5's results: provisionally, a change of at least 5% reads as better or
worse, 2-5% as probably, and under 2% as no clear change. That covers code changes on one grammar;
grammar edits are not measured yet (task 7.1).

A capped cost is a lower bound for that side, so a word that newly caps would shrink the total and
read as better. The verdict rules are:

- Total only words that are `ok` on both sides; report how many words were excluded, which includes
  words capped on both sides.
- If any word's status differs between the runs, the verdict is "inconclusive" whatever the percent.
- A different signature on an `ok` word is reported beside the verdict, not folded into it.
- A row whose status is not `ok`, `CAP`, `TIMEOUT` or `SKIPPED` is refused as an unexpected shape.
### D10: Weights are constants of the code, and calibration is a developer feature

Cost = work × a + bytes allocated × b + registers copied × c. The weights are fixed constants in
`pg-cost`, currently 676, 2 and 268 instructions per work unit, byte and register copy. Costs depend on the code, not on the
grammar: fitted on four grammars and scored on the fifth, the weights stayed close and predicted the
held-out grammar's instruction rate, so a stored per-project weight earns nothing. Both sides of a
comparison use one weight set, so the verdict is deterministic and load cannot touch it. Weights
never reach a cap, deadline or parse decision (D1).

Calibration is `pangloss cost-calibrate`, behind the off-by-default `cost-calibration` feature, and
no released build has it. It prints suggested weights; changing the constants is a code change.

Measured on the same words and build, raw wall time moved +5-10% with three copies running, +19-82%
beside 20 spinning processes and +89-121% with 20 copies running. Counts did not move.

| Where | Source of weights |
|---|---|
| Development machine, calibrating | Instructions; load does not affect them |
| Development machine without counters | Wall time on a word subset, behind the load gate |
| WASM, deployed grammar | None; see D11 |

Rules for a calibration run:

- About 40 words: a fit on 20 is already within about a point of the fit on 100.
- One thread. Refuse to update if thread CPU time / wall time is under 0.95. Measured: idle
  0.983-0.994, 20 spinners 0.48-0.72, 20 copies 0.50-0.57. Three copies read 0.983-0.987 and pass;
  that case slows everything evenly and is harmless to relative weights.
- Remove machine speed first, then bound each weight to ×0.75-1.5 of the current one. Across 30 random
  subsets, unbounded wall-time fits reached 13 points of error at p95 and bounded ones about 7.5,
  without loss of direction.
- Per-word median load checks do not work. Beside spinners the median word was unchanged while the
  total rose up to 82%, because preemption lands on a few heavy words.

Size variables: one per-site class weight each, with or without input length, is not stable. Held
out by word, classes with a size term beat work + bytes + registers on some grammars and lost badly
on others, and fitted class weights fell to 0 on several grammars. Bytes and registers already carry
most of what segment count and environment length would add. Affix count was not tested directly.

### D11: Deployed grammars keep the default cap; removing limits is opt-in

WASM keeps its default cap. `PanGlossGrammar.withoutWorkLimits` is an opt-in for a host that trusts
its grammar; it is not the default. Full removal of accounting buys nothing: an uncapped real word has
run over 10 minutes and counting costs under 1%, and removing the counters would disable the root
search cache, which admits entries by the work counted. Where `usize` is 32 bits the "unlimited" cap
is still 2^32-1 units, and an unlimited word can exhaust linear memory and trap. Allocation counting
is never built into WASM, so WASM cannot report cost.
## Risks / Trade-offs

- A grammar edit may change k, not just work → measure small grammar edits before Motif ships a band.
- Charging more can newly cap a word → D4 and D6.
- Instructions miss cache effects → D2's memory-side report, and memo pairs in the acceptance set.
- Removing a public flag breaks hosts → versioned interface, and a release that warns before it
  refuses.
- Instrumentation overhead on the hot path → measured in instructions before any charge lands.

## Migration Plan

1. `--step-cap` and the `steps` column stay until task group 6, with a warning release first.
2. `interfaces.batchTsv` is at 4 with `--cost-column`; removing `steps` moves it to 5, and a host pinned to 4 refuses rather than misreads, matching the
   existing `--describe` contract. Stats schema moves up and is wiped, not migrated.
3. Rollback before a release is a revert. After one, the old version remains usable through the
   interface version.

## Open Questions

- The final acceptance percentages, once the A/B set reports.
- Whether Motif needs the attempt column for one transition release.
