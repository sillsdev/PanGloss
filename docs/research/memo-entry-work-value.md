# Memo entry work value: a tuning curve for work-based refusal

Research note, 2026-09-15. Follows `docs/research/memory-measurement-repair.md` §7-8 (T5). Branch
`research/mem-tamp`, rebased onto main `6030f44d3b2b99ddf19a4c0687ce05d1db1b5925`, instrumentation
committed at `24360b3c`/`781fc2fb`. **MEASUREMENT ONLY: no admission policy, parse semantics, or
storage format changed.** No refusal is implemented anywhere; every number below is computed offline
from one instrumented run per word/cap, never by actually withholding a store.

## 0. The question, restated

§8 found 73.7% of mrule-memo bytes sit in entries never hit again within a parse, and flagged that as
a *ceiling* on a hit-count-based lever, not a *value* measurement -- a hit on a leaf and a hit on a
20-node subtree were counted identically. This note replaces "was it hit" with "how much work would a
hit have to redo," and asks whether refusing to store low-work entries reclaims a lot of bytes while
losing only a little of the work the memo actually saves.

**The answer is a clean negative**, for a reason more specific than "no correlation": the *economically
correct* reading of "work a hit saves" turns out to be almost constant across every stored entry in
this grammar, so there is no low-value tail to cut in that reading, and the more spread-out reading
(subtree size) makes the *opposite* trade from the one the hypothesis needs. §5-6 give the numbers;
§7 gives the recommendation.

## 1. What the work counter measures (M1)

`StepBudget` (`pg-rules/src/stratum.rs`) already counts one tick per attempted (un)application in
`apply_one_mrule` -- "how many (un)application attempts a `parse_word` call consumed," per its own doc
comment. This is the only work counter the analysis cascade already maintains, so it is what
`crate::memo_value`'s new instrumentation reads, rather than inventing a second notion of "work."
Template rules do not tick this counter at all (confirmed by reading every `.tick()` call site: the
only production call is inside `apply_one_mrule`), so a mrule-memo entry's tick count is uncontaminated
by template cost -- consistent with this diagnostic staying mrule-memo-only, as T5(b) already was.

`pg-rules/src/memo_value.rs` adds a thread-local stack of `enter_subtree`/`exit_subtree` frames.
`stratum.rs`'s `memo_apply_rules` opens one frame (`enter_subtree(budget.steps())`) immediately before
calling `memo_apply_rules_raw` for a **fresh** state (never for a memo hit or an in-flight fallthrough,
which do no new ticking), and closes it (`exit_subtree(budget.steps())`) immediately after, **whether
or not the store that follows is admitted** -- the pairing has to survive a refused store or the stack
desyncs for every entry above it. Two figures come out of each closed frame:

- **`subtree_work_inclusive`** = the raw tick delta across the frame's whole lifetime. This is what the
  state cost the *first* time it was ever computed, root to every leaf beneath it, **including every
  descendant's ticks too** -- a stored child's ticks are counted once in its own entry's inclusive
  figure and *again* inside every ancestor's inclusive figure. This is deliberate double-counting, not
  a bug: it answers "how expensive was this subtree, taken in isolation, with nothing memoized yet."
- **`subtree_work_exclusive`** = inclusive minus the inclusive cost of every direct child frame that
  closed inside this one's window. Double-count-free by construction: ticks are a single linear counter
  on one thread, sibling windows never overlap, and each child's ticks are subtracted from its parent
  exactly once. This is the ticks attributable to *this state alone*, on the premise that every
  descendant it touched already has (or would already have) its own memo entry.
- **`descendant_count`** = the number of fresh (memo-able) states reached inside the frame's window,
  transitively -- the direct reading of "how many nodes beneath it," independent of whether each one
  was actually admitted to the table (a byte-refused child still counts; it was still *reached*).

**Which one is "the work a hit saves"?** `exclusive`, not `inclusive`, and this matters for the
result: the memo table is never evicted (`pg-memo/src/lib.rs`'s "Memoization is correctness-neutral"
contract -- a refusal only ever withholds a *future* store, past entries stand for the rest of the
parse). So if a hit on state S *hadn't* happened and S had to be re-derived, that re-derivation would
still find every one of S's descendants already memoized (they were stored, if at all, before S was,
since S's own store happens only after its whole raw expansion returns) -- the re-derivation pays only
S's own local ticks, i.e. `exclusive`, not the full `inclusive` cost of the first-ever computation.
`inclusive` is still reported, per the task brief's ask, because it is the right number for a
*different* question ("how big was this subtree when nothing below it was memoized yet," e.g. what an
`--memo off` re-run or a cross-word cache miss would actually cost) -- just not for "what does THIS
memo, as it already exists, save on a hit."

## 2. Total parse work (M2)

`ParseOutcome::steps` (`pg-parse/src/morpher.rs`) already is `StepBudget::steps()` at end of parse --
the same figure `HC_STEP_STATS=1`'s `STEPS` line prints. No new counter was needed; `pg-cli` now also
folds it into the `MEMOVALUE` line as `total_work`, so one instrumented run's stderr carries both the
per-entry data and the whole-parse denominator without correlating two separate diagnostics.

## 3. Instrumentation surface

`HC_MEMO_VALUE_STATS=1` (existing): `MEMOVALUE` aggregate line, now with `mean_subtree_work_inclusive`,
`mean_subtree_work_exclusive`, `mean_descendant_count`, `total_work_saved_exclusive` (= `sum(hits *
subtree_work_exclusive)` over every stored entry), and `total_work` (M2, `outcome.steps`).
`HC_MEMO_VALUE_DUMP=1` (new, opt-in separately -- one line per stored entry): `MEMOVALUEENTRY` with
`bytes`, `hits`, `work_incl`, `work_excl`, `descendants`, `results_len`, `depth`. Every K-threshold
figure below is computed **offline** from one `HC_MEMO_VALUE_DUMP=1` run's raw per-entry rows; no
refusal is implemented, and one instrumented run feeds every K.

## 4. Method

Release build (`cargo run --release --bin pangloss`, via `rust/tools/pg.ps1 -Mode run -Bin pangloss`),
`--threads 1`, `--memo on`. Grammars: `samples/data/aweti.fwdata` (`oteʼikateʼika`, `Ajkululape`),
`samples/data/sena.fwdata` (`kukudziwisani`, control). Same word/cap grid as `memory-measurement-repair.md`
§5: `oteʼikateʼika`@200k and @1M, `Ajkululape`@200k, `kukudziwisani`@1M. **3 runs each, serial.**

**Spread: zero on every field, every run, every word.** `MEMOVALUE`'s `entries`, `total_bytes`,
`zero_hit_entries`, `zero_hit_bytes`, `mean_hits`, `mean_results_len`, `mean_depth_at_insert`, the three
new work fields, and `total_work` were **byte-identical across all 3 runs** for all 4 words (e.g.
`oteʼikateʼika`@1M: `entries=11757 total_bytes=178030448 ... total_work_saved_exclusive=1051448
total_work=1000000`, identical on runs 1, 2, and 3). Expected and unsurprising for a single-threaded,
step-capped run -- consistent with §5's prior finding for the pre-existing counters -- but checked
directly rather than assumed, per this task's self-verification requirement. `PARSEELAPSED` (not a
correctness figure) ranged 3.8-4.6s across repeats -- normal scheduling noise on a shared machine, not
reported further.

**Aggregate byte/zero-hit totals cross-check §8 exactly, from an independent code path**:
`oteʼikateʼika`@200k's `MEMOVALUE` line (`entries=2250 total_bytes=59913432 zero_hit_entries=444
zero_hit_bytes=49264104 mean_hits=1.972 mean_results_len=17.310 mean_depth_at_insert=3.235`) and the
other 3 words all reproduce §8's table row-for-row, unchanged by this instrumentation (as they must be
-- §8's counters were extended, not replaced).

## 5. Self-verification

**Unit tests, `pg-rules::memo_value::tests`** (`rust/crates/pg-rules/src/memo_value.rs`), pure stack
logic tested without the env gate (mirrors the existing `percentile_of` pattern):

- `subtree_work_differs_between_a_leaf_and_a_parent_with_one_child`: a flat leaf (push at tick 10, pop
  at tick 16) reads `inclusive=6, exclusive=6, descendant_count=0`; a parent with one child nested
  inside it (parent 100..112, child 104..109) reads `inclusive=12, exclusive=7, descendant_count=1` --
  proves `inclusive != exclusive` once nesting exists, with the exact expected numbers, not just a
  not-equal assertion.
- `descendant_count_rolls_up_transitively_through_two_levels`: grandparent/middle/leaf reads
  `descendant_count` = 0, 1, 2 respectively -- proves the rollup is transitive, not a flat
  immediate-child tally (a bug here would read `1, 1, 1`).

Both pass (`pg.ps1 -Mode quick -Package pg-rules -Filter memo_value`, 4/4 tests green, confirmed twice
-- once before and once after the comment-hygiene follow-up commit changed nothing about the logic).

**Production cross-check (stronger than the unit tests, because it is real pathological data, not a toy
fixture)**: for all 4 words, independently re-summing the raw `MEMOVALUEENTRY` dump (`sum(bytes)`,
`sum(hits * work_excl)`, count and byte-sum of `hits == 0` rows) reproduces the online `MEMOVALUE`
aggregate line's `total_bytes`, `total_work_saved_exclusive`, `zero_hit_entries`, and `zero_hit_bytes`
**exactly**, e.g. `Ajkululape`@200k: dump-recomputed `(132250800, 715585, 436, 93673272)` vs.
`MEMOVALUE`'s printed `(132250800, 715585, 436, 93673272)` -- all 4 words matched on all 4 fields, 16/16
checks. This is not tautological: the aggregate is computed inside the same process from the same
`ENTRIES` map, but through entirely separate code (`snapshot()`'s `.sum()` iterators vs. this note's
Python re-parse of the printed per-entry rows), so a bug in either the dump printer or the field
extraction would show up as a mismatch. None did.

**Counters are not degenerate (fire counts, not adjectives)**: 17,034 stored entries across the 4-word
grid, every one going through exactly one `enter_subtree`/`exit_subtree` pair, nested up to
`descendant_count=929` deep (`oteʼikateʼika`@1M's largest entry) without the `pop_frame` "called
without a matching push_frame" `.expect()` ever firing -- across three cap-hitting, pathological words
whose step budget was fully exhausted (so `over_budget()`'s early-return-without-a-frame branch fired
many times too, on every call past the cap). `work_excl` takes only 8-11 distinct values per word (see
§6); `work_incl` takes 23-135 distinct values spanning almost 4 orders of magnitude (`5` to `35636` on
`oteʼikateʼika`@1M) -- both counters visibly vary, and the SHAPE of that variation (narrow vs. wide) is
itself the finding, not a measurement artifact (§6).

## 6. Tuning curve (M3)

For each K, "bytes reclaimed" = share of `total_bytes` held by entries with `subtree_work <= K`;
"work-saved lost" = share of `sum(hits * subtree_work)` held by those same entries, both as a fraction
of that sum and as an absolute step count / fraction of `total_work` (M2). Two K-grids are shown
because the two work fields live on very different scales: `work_excl` is tightly clustered near a
grammar-specific floor (37-43 ticks for Aweti, 2-22 for Sena) with almost the WHOLE population sitting
in one narrow band, so the interesting range for `work_excl` is `{0,1,2,3,5,10,20,37,38,40,43}`, past
which the curve is flat at 100%; `work_incl` is genuinely long-tailed, so `{50,75,100,150,200,500,
1000,2000,5000,10000,40000}` is where its curve actually moves.

### `oteʼikateʼika`@200k (n=2250, total_work=200000, total_bytes=59,913,432)

**EXCLUSIVE** (total_work_saved=164,940):

| K | bytes reclaimed | work-saved lost (of saved) | work-saved lost (steps / frac of parse) |
|---:|---:|---:|---:|
| 0-20 | 0.0% | 0.0% | 0 / 0.0% |
| 37 | 1.9% | 68.6% | 113,140 / 56.6% |
| 38 | 17.7% | 99.0% | 163,300 / 81.6% |
| 40 | 99.4% | 100.0% | 164,940 / 82.5% |
| >=43 | 100.0% | 100.0% | 164,940 / 82.5% |

**INCLUSIVE** (total_work_saved=225,241):

| K | bytes reclaimed | work-saved lost (of saved) | work-saved lost (steps / frac of parse) |
|---:|---:|---:|---:|
| 50 | 18.9% | 57.8% | 130,228 / 65.1% |
| 100 | 23.0% | 75.0% | 168,911 / 84.5% |
| 200 | 34.6% | 93.6% | 210,804 / 105.4% |
| 500 | 46.5% | 100.0% | 225,241 / 112.6% |
| 1000-40000 | 57.6%-100.0% | 100.0% | 225,241 / 112.6% |

### `oteʼikateʼika`@1M (n=11757, total_work=1000000, total_bytes=178,030,448)

**EXCLUSIVE** (total_work_saved=1,051,448):

| K | bytes reclaimed | work-saved lost (of saved) | work-saved lost (steps / frac of parse) |
|---:|---:|---:|---:|
| 0-20 | 0.02% | 0.0% | 0 / 0.0% |
| 37 | 1.7% | 51.6% | 542,945 / 54.3% |
| 38 | 16.2% | 86.8% | 912,761 / 91.3% |
| 40 | 84.1% | 99.9% | 1,050,833 / 105.1% |
| >=43 | 100.0% | 100.0% | 1,051,448 / 105.1% |

**INCLUSIVE** (total_work_saved=1,550,032):

| K | bytes reclaimed | work-saved lost (of saved) | work-saved lost (steps / frac of parse) |
|---:|---:|---:|---:|
| 50 | 19.3% | 56.5% | 875,975 / 87.6% |
| 100 | 23.3% | 66.2% | 1,026,623 / 102.7% |
| 200 | 36.1% | 82.8% | 1,284,021 / 128.4% |
| 1000 | 59.7% | 94.4% | 1,463,036 / 146.3% |
| 5000 | 76.9% | 100.0% | 1,550,032 / 155.0% |

### `Ajkululape`@200k (n=2724, total_work=200000, total_bytes=132,250,800)

**EXCLUSIVE** (total_work_saved=715,585):

| K | bytes reclaimed | work-saved lost (of saved) | work-saved lost (steps / frac of parse) |
|---:|---:|---:|---:|
| 0-37 | 0.0% | 0.0% | 0 / 0.0% |
| 38 | 0.004% | 6.0% | 43,206 / 21.6% |
| 40 | 6.1% | 75.7% | 541,385 / 270.7% |
| >=43 | 100.0% | 100.0% | 715,585 / 357.8% |

**INCLUSIVE** (total_work_saved=952,194):

| K | bytes reclaimed | work-saved lost (of saved) | work-saved lost (steps / frac of parse) |
|---:|---:|---:|---:|
| 50 | 24.6% | 64.2% | 611,375 / 305.7% |
| 100 | 28.8% | 73.3% | 698,216 / 349.1% |
| 500 | 63.5% | 97.5% | 928,795 / 464.4% |
| 1000 | 67.9% | 99.6% | 948,582 / 474.3% |
| 2000 | 76.8% | 100.0% | 952,194 / 476.1% |

(`work-saved lost` exceeds `total_work` here because `total_work_saved` sums `hits * work` over the
*whole table*, and a heavily-revisited entry can be hit hundreds of times in one 200k-step capped
parse -- see §7's note on the memo's aggregate protective effect; it is not an error.)

### `kukudziwisani`@1M (Sena control, n=303, total_work=22704, total_bytes=586,448)

**EXCLUSIVE** (total_work_saved=3,227):

| K | bytes reclaimed | work-saved lost (of saved) | work-saved lost (steps / frac of parse) |
|---:|---:|---:|---:|
| 0-10 | 0.0% | 0.0% | 0 / 0.0% |
| 20 | 50.0% | 100.0% | 3,227 / 14.2% |
| >=22 | 100.0% | 100.0% | 3,227 / 14.2% |

**INCLUSIVE** (total_work_saved=7,343):

| K | bytes reclaimed | work-saved lost (of saved) | work-saved lost (steps / frac of parse) |
|---:|---:|---:|---:|
| 20 | 0.0% | 19.2% | 1,410 / 6.2% |
| 50 | 5.4% | 31.5% | 2,310 / 10.2% |
| 100 | 27.4% | 76.5% | 5,615 / 24.7% |
| 150 | 85.9% | 100.0% | 7,343 / 32.3% |

### Aggregate (all 4 words, n=17,034, total_bytes=370,781,128, total_work=1,422,704)

**EXCLUSIVE** (total_work_saved=1,935,200):

| K | bytes reclaimed | work-saved lost (of saved) | work-saved lost (frac of parse) |
|---:|---:|---:|---:|
| 20 | 0.09% | 0.17% | 0.2% |
| 37 | 1.3% | 34.1% | 46.3% |
| 38 | 10.8% | 58.0% | 78.9% |
| 40 | 58.8% | 91.0% | 123.7% |
| >=43 | 100.0% | 100.0% | 136.0% |

**INCLUSIVE** (total_work_saved=2,734,810):

| K | bytes reclaimed | work-saved lost (of saved) | work-saved lost (frac of parse) |
|---:|---:|---:|---:|
| 100 | 25.3% | 69.5% | 133.5% |
| 500 | 53.6% | 94.5% | 181.7% |
| 1000 | 62.3% | 96.7% | 185.9% |
| 2000 | 68.0% | 99.2% | 190.6% |
| 5000 | 78.5% | 100.0% | 192.2% |

**Reading the shape.** In both framings, at every K where bytes reclaimed is small (< ~30%), the
work-saved lost is already large (> ~55%) -- the opposite of what the hypothesis needs (a K that drops
a lot of bytes while losing little value). `EXCLUSIVE` is worse than "no daylight": it is a near
step-function (0% to 100% of BOTH bytes and work-saved across a ~6-tick window, `37` to `43`), because
almost every entry has almost the same exclusive cost. `INCLUSIVE` has real spread, but the spread runs
backward -- the *cheap*-by-inclusive-measure entries are disproportionately the ones that get reused
(work-saved-lost fraction is always higher than bytes-reclaimed fraction, every K, every word).

## 7. Joint distribution (M4): is this the same tail as §8's zero-hit tail?

Aggregate: 3,355 of 17,034 entries (19.7%) are zero-hit, holding 273,232,056 of 370,781,128 bytes
(73.7% -- reproducing §8's 73.7% aggregate figure from an independently-computed field set, a second
cross-check beyond §5's per-word match).

| low-value definition | overlap with zero-hit tail | reading |
|---|---|---|
| `work_excl <= 43` | 100% of zero-hit entries qualify; but so does 100% of EVERY entry (`low_n = n` for every word) | vacuous -- "low exclusive work" is not a tail, it is the whole population, so this "overlap" answers nothing |
| `work_incl <= 100` | 51.3% of zero-hit entries are also low-inclusive-work; only 12.7% of low-inclusive-work entries are zero-hit | **largely different populations** at a moderate, decision-relevant K |
| `descendants <= 2` | 59.5% of zero-hit / 13.6% of low-descendant | same shape as `work_incl` (expected -- correlated proxies) |

So at the K that actually matters for a bytes-vs-value trade-off (`work_incl` in the low hundreds),
**the never-hit tail and the low-work tail are different populations, roughly half-overlapping** --
hit-based and work-based reasoning are two distinguishable levers here, not one lever wearing two
names. But (§6) neither lever, applied as "refuse the low end," produces a good trade on this grid.

**The one place a good trade DOES appear, and why it is not a new lever.** Read the `INCLUSIVE` curves
as a *high-pass* filter instead (refuse entries with `work_incl > K`, the large, first-ever-computed
subtrees): at aggregate `work_incl > 2000` (101 entries, 32.0% of all bytes), only 0.85% of work-saved
is lost -- a genuinely good trade. But checking what that 101-entry tail actually is: 97 of them (96%)
are already zero-hit, and at `work_incl > 5000` it is 35/35 (100%) zero-hit. **This "good" high-pass
trade is not independent information** -- it is the already-known zero-hit tail's own largest members,
rediscoverable more simply and more cheaply by checking `hits == 0` directly, which the codebase could
already do without any of this work-cost machinery. It adds nothing beyond what §8 already found,
restricted to the biggest bytes in that same tail.

## 8. Recommendation: work-threshold refusal is not worth building

**No**, on both readings, and the case against is specific rather than "no signal found":

1. **`subtree_work_exclusive`** -- the economically correct reading of "what a hit saves," reasoned in
   §1 and confirmed empirically in §6 -- has essentially no variation to exploit. 8-11 distinct values
   per word, almost the entire population inside a ~6-tick band. There is no low-value tail in this
   reading because there is no meaningful *value* spread at all: every stored state costs about the
   same to regenerate locally, because by the time a hit would matter, every descendant it touches is
   already independently memoized (or was refused independently, in which case exclusive cost already
   reflects that it wasn't free).
2. **`subtree_work_inclusive`** -- the more intuitive "20 nodes beneath it" reading the task brief
   raised -- does vary widely, but a low-pass threshold on it makes the WRONG trade: at every K tested,
   for every word, the fraction of work-saved lost exceeds the fraction of bytes reclaimed, often by
   2-3x. The small-inclusive-work entries are disproportionately the ones that get revisited; refusing
   them trades away value faster than space.
3. The one K-range where `inclusive` gives a good trade is a high-pass filter on the largest
   first-computed subtrees, and (§7) that population is 96-100% the same as the already-documented
   zero-hit tail -- not new information, and simpler to act on directly (`hits == 0`) if this repo ever
   decided a *time-bounded* (not per-word, since the memo is provably a pure cache within one word --
   §8's "Is the memo a pure cache?") eviction policy were worth its own correctness review.

**A clean negative, stated plainly**: this grammar's mrule-memo cascade does not have a "cheap leaf vs.
expensive subtree" value structure to exploit. Every stored state's *marginal* recompute cost is nearly
uniform; the *apparent* variation (in bytes, in inclusive-work, in descendant count) tracks how big a
first-touch subtree was, not how much a later hit is worth, and building a refusal policy on that
apparent variation would discard high-value entries in preference to low-value ones. Building a
work-threshold refusal here would add real complexity (a new admission check, a new place for the
"a control that cannot act must say so" rule to matter, a new interaction with the byte/word/entry
caps already in `pg-memo`) for a lever this measurement shows does not exist on this data.

**One residual not settled here**: whether the near-constant `work_excl` is a property of THIS
grammar's rule count / stratum shape (Aweti and Sena both cluster near their own strata's mrule count)
or a structural property of the cascade generally. A grammar with far more mrules per stratum, or with
`max_apps` allowing much deeper per-state work before a node's own local expansion terminates, might
show real `work_excl` spread. Not tested here -- out of this task's word/cap grid, which was fixed to
match §5 for comparability.

## 9. Correctness gates

<!-- filled in after gate runs complete -->
