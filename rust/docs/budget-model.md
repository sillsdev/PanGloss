# Search-budget model (W8 step 1)

## The bug being fixed

`hc-rules::stratum::StratumAnalyzer` owns its step counter as an instance field
(`steps: Cell<usize>`, `stratum.rs`), constructed fresh — `Cell::new(0)` — every time
`analyze_stratum`/`analyze_stratum_scoped`/`analyze_stratum_scoped_filtered` is called. The
production call site, `hc-parse::Morpher::parse_word`, calls
`analyze_stratum_scoped_filtered` **once per (stratum × live candidate word)** inside a loop over
`0..n_strata` (reversed) — and the candidate set can itself grow between strata. So a single
`--step-cap=N` does not bound one `parse_word`'s search to `N` steps; it bounds *each* of the
(potentially many) stratum-analyzer instantiations to `N`, giving an effective per-word budget of
`N × (#stratum-analyze calls)` — an amplifier nobody designed, sized by incidental candidate-set
growth rather than the flag value. `NARROWING-FINDINGS.md` (the Tier-1 #6 probe) flagged this as
the reason narrowing's residual explosion could not be honestly measured against a stated cap: the
same `--step-cap=100000` explores a different total budget on every word, depending on how many
candidates survived upstream strata.

Synthesis (`synthesize_stratum`/`synthesize_template`) has the identical per-call-`Cell` shape, but
is **not** part of this fix. Synthesis is the *confirmation* pass: `guided_synth` only re-applies a
rule if it is next on the word's own unapplication stack, so its search is bounded by that stack's
length, not by open-ended nondeterministic matching — the amplifier is not a live problem there (no
finding, no measurement, ever reported it). Folding synthesis into the same shared counter would
introduce a new failure mode with no offsetting benefit: an analysis phase that is heavy (exactly
the flooded words this fix targets) would starve the synthesis calls for the very candidates it
just spent the budget finding, dropping otherwise-valid analyses at the confirmation step — a
regression risk with no upstream justification. Scope of this change is **analysis only**.

## The fix

A new `hc_rules::stratum::StepBudget`: a small `cap`/`steps: Cell<usize>`/`capped: Cell<bool>`
struct with `tick()`/`over_budget()`/`capped()`. `hc-parse::Morpher::parse_word` constructs **one**
`StepBudget` per call and passes it, by shared reference, into every
`analyze_stratum_scoped_filtered` invocation of that parse's stratum loop — across every stratum
and every candidate word. `StratumAnalyzer` now borrows `&StepBudget` instead of owning its own
`Cell`s; `over_budget()`/`tick()` delegate to it. The three public entry points
(`analyze_stratum`, `analyze_stratum_scoped`, `analyze_stratum_scoped_filtered`) now all take an
explicit `budget: &StepBudget` parameter instead of reading a `cap` out of `AnalyzerConfig`
(`AnalyzerConfig.cap` is removed — every call site must now say what it shares the budget with).
Test call sites that have no natural "one parse_word" scope construct their own
`StepBudget::new(cap)` per call, which reproduces the exact old per-instance-`Cell` behavior for
those isolated single-call tests (there is only one call to bound, so "shared across one call" and
"private to one call" coincide).

## Semantics change: the same flag value now buys less search

Before this change, `--step-cap=N` meant "N steps per stratum-analyzer instantiation," and a
multi-stratum, multi-candidate word could burn many multiples of `N` before the cap actually
stopped anything. After this change, `--step-cap=N` means "N steps total across this word's entire
analysis phase" — closing the amplifier. **A flooded word that used to complete at `--step-cap=100000`
under the old (amplified) budget may now hit the cap at the same flag value**, because the same
nominal number now buys strictly less total search. This is by design (the amplifier was never a
deliberate feature — the `NARROWING-FINDINGS.md` residual-explosion analysis explicitly flags it as
"unreconciled with anything in C#"), but it means the calibrated default cap must be measured fresh
under the new semantics rather than reused from the old one.

## Default policy

C# has no analysis step budget at all (`rust/docs/phase2-completed/narrowing-budget-w8.md`'s option
(b)); it pays wall-clock and survives on memoization + `MaxUnapplications`. Rust keeps a cap as a
configurable safety valve (`--step-cap`, unchanged flag) rather than going uncapped, per the
complexity-cap plan's stance that pathological/adversarial grammars need a hard backstop — but the
DEFAULT is now sized by calibration (smallest cap at which Indonesian stays 121/121 and Amharic
stays ≥532/673 **after narrowing lands**, not tuned to make any particular word fail), not picked
arbitrarily.

**Calibrated default: see the "Calibration" section added after step 2/3 measurement below.**

<!-- Filled in after the with-narrowing calibration sweep (step 3): closing the amplifier alone
     (step 2, deletion-only narrowing) does not change the calibrated cap, because 0 words hit the
     cap at 100k either before or after de-amplification on the reference corpora — the amplifier
     was latent, not yet triggered. The real calibration point is with general narrowing active,
     which is the workload that floods. -->

## Calibration (filled in after step 3)

TBD — run after narrowing lands. Placeholder default until then: **`--step-cap` default stays
`usize::MAX` (uncapped) when the flag is omitted** (unchanged CLI behavior); the *gate* cap used by
the corpus scripts is what gets calibrated and recorded here.

## Addendum: `--word-timeout-ms` (a second, independent bound)

`--step-cap` bounds the *number* of analysis steps a `parse_word` call may take, but per-step cost
is not uniform. Some pathological words (first seen in the Amharic corpus) legitimately spend
~300 seconds of real wall-clock time without ever hitting a generous step cap, because each step
itself does more work under the narrowing/expansion analysis (`docs/phase2-completed/
narrowing-budget-w8.md`) than a cheap grammar's steps do. A step-count cap alone cannot bound
wall-clock time per word in that regime — full-corpus batch runs need a *time* backstop as well as
a *step* backstop, and the two are not interchangeable (a small step cap on a cheap-per-step
grammar cuts off search prematurely; a wall-clock cap alone still lets a cheap-but-infinite loop
burn arbitrary step count within the time window). `--word-timeout-ms N` adds that second bound.

**Mechanism.** `hc_rules::stratum::StepBudget` (the same shared, per-`parse_word` counter
`--step-cap` already uses) optionally carries an absolute deadline, armed via
`StepBudget::with_timeout(Some(Duration))` at construction (`Morpher::with_word_timeout` threads a
`Duration` down from `hc-cli`'s `--word-timeout-ms N` flag, ms → `Duration::from_millis(N)`).
`over_budget()` — already consulted at every (un)application attempt and every recursion entry —
checks the step cap first (unchanged), then, only if a deadline was armed, samples
`Instant::now()` against it — on **every single call**, not periodically.

**O1b history (fixed; read this before "optimizing" the cadence again).** The first cut of this
addendum sampled the wall clock only every 1024 ticks (`WALL_CLOCK_CHECK_INTERVAL`, gated on
`self.steps.get().is_multiple_of(1024)`), on the theory that `Instant::now()` is more expensive than
the `Cell` compare the step-cap check already does, so reading it on every call would be wasteful.
This shipped as O1 and was found broken by O1b's real-corpus measurement: `rust-optimizations-
phase2.md`'s O1b item records በመጨረሻ completing in 489073ms against a 120000ms deadline — 4x
over — reporting `ok`, never `TIMEOUT`; በየራሳቸው ran past 8 minutes (gold: 31568ms) before being
killed. Root cause, confirmed by instrumenting tick-to-tick gaps on both words under `--step-cap 50`:
per-tick cost on these words is *not* cheap — roughly 1-1.5 seconds per (un)application attempt
(Optional-flooded affix-matcher shapes, `docs/phase2-completed/narrowing-budget-w8.md`'s cost-sink
finding), so each word's *entire* natural run totals only a few hundred ticks — well under 1024.
`self.steps.get().is_multiple_of(1024)` is true at `steps() == 0` and then not true again until
`steps()` reaches an exact multiple of 1024; a word whose total tick count never gets there samples
the wall clock exactly ONCE, at construction, and then never again for the rest of its run, however
long that turns out to be. No smaller *step-count* interval fixes this in principle — it only moves
the same failure onto slower-per-tick words (at ~1s/tick, even a 16-tick interval overshoots a 5s
deadline by ~16s). The fix: drop the step-count gate entirely and read `Instant::now()` on every
`over_budget()` call once a deadline is armed. This is sound because `over_budget()` fires at
rule-attempt/recursion-entry granularity (a handful of times per tick), not per innermost-loop-
iteration of an FST traversal (that finer-grained hot loop is `hc-fst::traverse`, untouched by this
fix) — `Instant::now()`'s real cost (tens of nanoseconds) is negligible next to the per-call work it
gates. No deadline (`None`, the default — the flag omitted) still never reads the clock at all: zero
cost when unused, confirmed unchanged (Indonesian corpus, both `--threads 1` and `--threads 4`, no
`--word-timeout-ms`: signature columns unaffected, timing within pre-existing run-to-run jitter).
The `WALL_CLOCK_CHECK_INTERVAL` constant is gone from `stratum.rs`; the cadence doc below is
historical context, not current behavior.

`hc-rules/src/stratum.rs`'s `step_budget_timeout_tests::
wall_clock_deadline_fires_even_when_total_ticks_never_reach_the_old_check_interval` is the
regression guard: 200 ticks (well under the old 1024 interval), each with 1ms of real sleep standing
in for an expensive attempt, deadline at 50ms — red under the old cadence (only one clock sample, at
step 0, so the loop ran all 200 ticks/~200ms unchecked), green after (every call samples the clock,
so the deadline fires within about one tick of the 50ms mark).

**Two independent bounds, whichever fires first wins.** `StepBudget` now latches two separate
`Cell<bool>` flags — `capped` (the pre-existing step-cap-exhausted flag) and `timed_out` (new) —
and never conflates them: a word can time out with steps to spare, or hit the step cap well inside
its deadline. `ParseOutcome` exposes both (`capped: bool`, `timed_out: bool`); `hc-cli`'s `batch`
subcommand reports them as distinct outcomes, not folded into one "something fired" flag:

- Step-cap exhausted (`capped`, unchanged): the word still gets an `ok` row with whatever partial
  signature analysis found, plus a `CAP\t{idx}\t{word}` diagnostic line on stderr and a running
  `capped_words` count in the final summary.
- Wall-clock deadline fired (`timed_out`, new): the word gets a distinct TSV row —
  `{idx}\t{word}\t{elapsedMs}\tTIMEOUT\t-` — matching the synthetic `TIMEOUT` row
  `tools/run-sena-rust.ps1`'s watchdog already writes when it has to kill+relaunch a stalled
  external process (see that script's `Get-ResumeIndex`), so downstream tooling that already
  understands that row shape needs no changes. A `TIMEOUT\t{idx}\t{word}` diagnostic line is
  written to stderr (mirroring `CAP`'s), and a `timed_out_words` count appears in the final
  summary. This works identically in both `--threads` writer modes (the sequential per-line-flush
  path and the rayon-parallel buffered path) — the row shape is the same, only the surrounding
  `STARTED` sentinel (sequential-only) differs, as it already did before this flag existed.

**Default and CLI surface.** `--word-timeout-ms N` (milliseconds) is optional on `hc-rs batch`;
omitted = `None` = no deadline, unchanged behavior (verified above). There is no default timeout —
unlike `--step-cap`'s calibrated-default ambition (see above), no calibration work has been done
for a default wall-clock bound, and none is implied by adding this flag; it is opt-in per
invocation (e.g. a nightly full-corpus wrapper that wants to bound tail latency).
