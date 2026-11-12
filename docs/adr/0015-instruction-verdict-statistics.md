---
status: accepted
---

# Instruction-comparison verdicts use Welch's t with a minimum effect

Owner decision, 2026-11-12 (#241). An instruction comparison decides from a Welch's t interval on
the per-round parse-only counts of each build, at 95% two-sided, together with a minimum effect of
0.5%. The decision also asked for the parse to be counted by its threads. That is not built: the
parse-only count stays a paired, per-round empty-list subtraction, because at `--threads 1` the
parse runs on the thread that loaded the grammar. The owner confirmed this deviation on 2026-11-12.

## Context

The former rule called a change real when it exceeded twice the larger max-min spread of either
side's repeats, plus the spread of the empty-list runs, with 0.5% as a floor. A max-min spread grows
with the number of repeats while the error of a mean shrinks, so more repeats made the rule less able
to see a real change. With two repeats the spread is a single draw. In simulation that read a build
against itself as a change on 5.6% to 7.1% of grammars, once the run noise reached 0.5%. On the
Windows counters the run noise is real: the same baseline and grammar printed noise figures from
0.12% to 1.03%. On callgrind with hash keys pinned it is under 0.005%.

## Decision

- **Statistic.** Welch's t interval (95%, two-sided) for the difference of the two means, as a
  percentage of the earlier build. GO needs the interval below zero and a change of at least the
  minimum effect. NO-GO (regression) is the mirror. NO-GO (no change) needs the whole interval inside
  ±0.5%. Anything else is INCONCLUSIVE, which has its own exit code. The interval is printed on every
  comparison.
- **Repeats.** Five rounds per side on the Windows counters by default. Callgrind keeps two.
- **Rounds, not means.** Each round's empty-list run is subtracted from the same round's full run.
  Start-up variance therefore reaches the interval and does not hide in a subtracted mean.
- **Instrument check.** A build compared with itself must read neither GO nor regression. It runs once
  per machine session or when a verdict looks odd. It is not part of every comparison and sets no
  threshold.
- **Parse-only count: per-round paired subtraction, not threads.** The parse-only count of a round is
  that round's full-list run minus the same side's empty-list run. Thread attribution cannot count the
  parse at `--threads 1`: `pangloss batch` then takes its sequential path (`rust/crates/pg-cli/src/main.rs`,
  the `if threads == 1` branch) and parses each word inline on the main thread, the one that loaded
  the grammar. The rayon pool in `pg_parse::hc_parse_batch` is only built for two or more threads. A
  per-thread trace (65 words, one pinned CPU) confirmed it: at `--threads 1` the loading thread read
  344M with no words, 573M with the list and 790M with the list twice. At `--threads 2` it stayed at
  347M and the parse, about 225M, moved to two pool threads. Both backends keep the subtraction, so they
  count the same thing.
- **Equal-length neutral run names.** Each run's output files are named `sideA`/`sideB`, never
  `before`/`after`. With the old names a build compared with itself read +0.96% to +1.12% NO-GO
  (regression), all of it on the parsing thread of the run named `after`. Swapping the names moved
  the bias with them.

## Alternatives not taken

- **Counting the parse by its threads.** It is valid only at `--threads 2` or more, and that changes
  what is measured. The parse runs on pool threads through `hc_parse_batch`, not the sequential path
  the comparison uses. The pool threads' total for the same words also varied by 2.3% between two and
  four threads.
- **A marker at the start of the parse.** The counter would be read, or a trace event emitted, where
  the batch starts its first word, and only what follows would be counted. Both builds would need the
  marker, so it could not compare against a build made before it existed. It would also put the
  measuring inside the code being measured. It is the most likely follow-up if the subtraction's
  noise becomes the limit.

## Alternatives rejected

- **Pooled-SD t test.** It assumes the two builds vary alike. A change to the parser can change how
  much a build varies, for example through hash order, and Welch's t costs nothing extra.
- **A multiple of the max-min spread, floored by a no-change control.** The spread grows with repeats,
  as described above. A control's spread is one more noisy draw and does not make a threshold. A
  build against itself checks the setup; its spread does not belong in the rule.

## Consequences

- A comparison can now end INCONCLUSIVE (exit 11). That calls for more repeats or more words and is
  never read as a quiet GO.
- A grammar whose load dwarfs its parse stays hard to measure on the Windows counters while the
  subtraction stays. The wider interval now shows this, where before it could read as a false verdict.
- On the Windows counters a list of about 65 words gives an interval about ±0.6% to ±1% wide with five
  rounds, so a change near the 0.5% minimum effect needs more repeats or more words.
