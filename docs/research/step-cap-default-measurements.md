# Why `batch`'s default `--step-cap` is 50,000,000

`pangloss batch` used to default `--step-cap` to `usize::MAX`: no bound at all. That made a batch's
termination depend on every word in the corpus finishing in finite wall-clock time, which is not
guaranteed -- a runaway grammar/word combination can spend unbounded unmemoized analysis work. A
finite default step cap makes every batch terminate deterministically. **This is resource
containment, not a correctness verdict**: a step-cap-exhausted word already writes a typed `CAP`
row (the partial, unconfirmed signature reached before the cap fired, kept for inspection, never
presented as a result) -- an *atomic word-analysis result* that is either the complete confirmed
analysis multiset or a typed incomplete outcome, never a wrong answer. Compare `--word-timeout-ms`:
a wall-clock deadline is machine-dependent and makes a run unreproducible; a step cap is a
deterministic logical work budget, so the same run on any machine hits the same cap at the same
word.

## Measurements (2026-09-10, one thread, `HC_STEP_STATS=1`, `STEPS` per word)

| corpus | words | max steps (legit) | p99 | p95 | median | notes |
|---|---|---|---|---|---|---|
| Indonesian | 70 | 1,254 | 1,254 | 350 | 88 | |
| Sena | 6,146 | 209,428 (`kukudziwisani`) | 36,447 | 8,118 | 791 | 0 capped at 5M |
| Amharic | 673 | 188,001 | 77,457 | 5,803 | 447 | 6 words hit a 120 s wall clock below 200k steps: per-step cost differs 1000x across grammars |
| Mbugwe | 1,645 | >5,000,000 (312 words, 19%, hit a 5M cap; largest uncapped 4,935,786) | 5,000,000 | 5,000,000 | 551,516 | legitimate words routinely exceed 5M; 17 more hit a 120 s wall clock |
| Aweti | -- | -- | -- | -- | -- | pathological words exhaust a 2 GiB job before 200k steps: the step cap is NOT a memory bound |

## Reading this table honestly

- **No "tight" default exists.** Mbugwe's legitimate words pass 5,000,000 steps routinely (median
  551,516; 312 of 1,645 words still running at the 5M mark this measurement stopped at, and the
  largest word that did finish needed 4,935,786). Any cap set anywhere near Sena's or Amharic's
  maxima would misclassify ordinary Mbugwe words as incomplete.
- **Mbugwe's capped words were never measured past 5,000,000.** The `>5,000,000` and `312 words`
  figures are a floor, not a ceiling -- this measurement run stopped at a 5M cap, so the true step
  count those words would need is unknown and could be far higher.
- **Memory is not bounded by step count.** Aweti's pathological words exhaust a 2 GiB job before
  reaching even 200,000 steps -- an order of magnitude below Mbugwe's legitimate median. A step cap
  constrains analysis-cascade work, never peak memory; the two are governed by separate mechanisms.
- **Per-step cost is not uniform across grammars.** Amharic's 6 wall-clock timeouts below 200,000
  steps, next to Mbugwe's legitimate words idling past 5,000,000, show per-step cost differing by
  roughly three orders of magnitude between grammars -- another reason a step cap is a work budget,
  not a time budget, and why `--word-timeout-ms` exists as an independent, orthogonal bound.

## The default: 50,000,000

`DEFAULT_STEP_CAP` (`rust/crates/pg-cli/src/main.rs`) is **ten times the highest step count any
measured legitimate word reached across every corpus sampled** (Mbugwe's floor of 5,000,000). It is
a runaway guard sized with a wide margin above every legitimate observation on hand, not a
performance-tuning knob and not a claim that 50,000,000 is itself a tight bound -- given Mbugwe's
capped words were never measured to completion, no tighter number can currently be justified.
`--step-cap unbounded` remains available for a caller who wants to reproduce the old behavior or
push a specific word past this default.
