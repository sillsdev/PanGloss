# Sampling: how many words decide "faster" or "slower"

A verdict needs a reasonable sample of words, not every word. The measurement is Windows instruction counts (`land-an-optimization/SKILL.md`,
"Timing truth"); Linux counts only choose what to sample, labelled provisional.

This sits on top of `rust/tools/instruction-compare.ps1`, which is unchanged. For one word list it takes repeated runs of both binaries,
builds Welch's t 95% interval on the percent change of the list total, and says GO when the interval is below zero and the change is at
least `MinPercent` (0.5%). Its interval covers run-to-run noise on that list. This file covers the other uncertainty, which words went
into the list. Size the list here, then let instruction-compare give the verdict on it; do not replace its GO rule.

## Paired design

Use the same words before and after. Per word: `r = ln(after / before)` instructions. Report:
- the mean of `r`, as a percent (`100*(exp(mean)-1)`): what a typical word sees;
- the ratio of sums, `sum(after) / sum(before)`: what the corpus total sees. Heavy words dominate it, so it can differ a lot from the mean.

## Pilot and size

1. Pilot: about 100 words chosen at random from the list. Estimate the standard deviation `s` of `r`.
2. Required words `n = (z * s / d)^2`, z = 1.96 for 95%, `d` the target half-width of `r` (0.01 is plus or minus 1%).

| `s` | `n` for d = 0.01 |
|---|---|
| 0.05 | about 100 |
| 0.15 | about 900 |
| 0.5 | about 10,000 |

## Interval for the ratio of sums

Bootstrap over words: resample words with replacement, recompute `sum(after)/sum(before)`, take the 2.5% and 97.5% quantiles (a few
thousand resamples). Or stratify by word weight: heavy words (above a chosen instruction cut-off, or those containing the trigger) form
their own stratum, measured in full or oversampled with weights; the rest is sampled at random.

## Sequential

Grow the sample 100, 1,000, 10,000 only until the interval excludes zero (GO or regression) or lies inside a no-difference band
(plus or minus 0.5%, instruction-compare's `MinPercent`). Stop there. An interval that is neither, at the largest affordable sample, is INCONCLUSIVE; say so.

## Random versus chosen words

- Random sample from the word list answers "what does the corpus see".
- Chosen words answer "does the mechanism work" and "what does it cost where it cannot help": heavy words, words containing the
  trigger, words the filter rarely rejects (`land-an-optimization/SKILL.md`, "Filter cost").

Report both, labelled. Never pool them into one number.
