# Parse cost

Parse cost is a deterministic estimate of a word's instructions:

```
cost = work_steps × 500 + bytes allocated × 2 + registers copied × 330
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

Fitted on four grammars and scored on the fifth, the weights stayed close (work 311-700, bytes
2.09-2.55, registers 259-385) and predicted the held-out grammar's instruction rate within 2-14%.
They follow the code more than the grammar, so a stored per-project weight earns nothing. A code
change can move them, which is what `cost-calibrate` is for.

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

## Deployed grammars

WASM keeps its default cap. `PanGlossGrammar.withoutWorkLimits` removes the attempt and work limits
for a host that trusts its grammar. The counters remain, but no cap can stop a word: counting costs
under 1% of a parse, and the root search cache admits entries by the work counted. On wasm32 `usize::MAX` is 2^32-1, so the limit is still about 4.29e9 units and a word can report
capped there. A word with no limit can run for minutes, blocks its thread, and can exhaust linear
memory and trap, so it belongs off the UI thread. WASM installs no
counting allocator, so it cannot report cost.

The evidence and open questions are in `openspec/changes/accurate-work-steps/design.md`.
