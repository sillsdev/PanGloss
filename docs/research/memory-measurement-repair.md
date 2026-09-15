# Memory measurement repair: what each number means now, and what it still cannot see

Research note, 2026-09-15. Follows `docs/research/word-memory-trace.md`, `docs/research/alt-yield.md`,
`docs/research/live-frontier-memory-bound.md`, and `docs/research/analysis-memo-explosion.md`. Those
docs summed `live peak` + `memo total` and subtracted from `ALLOC peak`, calling the remainder a
33-52% "unexplained gap". That subtraction was contaminated by a documented-but-uncorrected `Rc`
double-count (§2), three different sampling windows that never co-occurred, and a model with no
working-set measurement to check against. This note is MEASUREMENT REPAIR: no parse semantics
changed, no algorithmic optimization landed, and every gate result reported here was compared
against the pristine base commit, not asserted. One exception to "measurement only," disclosed
rather than hidden: T1 tightens an existing byte-budget *estimate* (§2), and since that estimate
feeds `AnalysisScope::has_byte_capacity` (a live admission check, not a diagnostic), the fix can
change which subtrees the memo admits -- never which analyses a completed word returns (§8). Branch
`research/mem-tamp`, based on main `0006b9383cdb28d2621d260eed3fb2efa3275956`.

## 1. What each number now means, and what it provably cannot see

**ALLOC peak/live** (`pg-cli --features alloc-trace`, `HC_ALLOC_STATS=1`,
`rust/crates/pg-cli/src/alloc_trace.rs`). A counting `#[global_allocator]`: `layout.size()` summed
over every `alloc`/`realloc`/`alloc_zeroed`, `PEAK` the running max, reset per word. This is
**requested bytes**, not resident memory: no allocator size-class rounding, no per-allocation header,
no fragmentation, no page ever returned to the OS but not yet reused. It is also **continuous**:
sampled on every allocation, not at discrete checkpoints, so it is strictly an upper bound on anything
the other instruments below can see, including transient allocations that never survive to a
checkpoint.

**Peak/current working set** (NEW, T2: `pg-cli/src/winmem.rs`, `HC_WS_STATS=1`, Windows only,
`K32GetProcessMemoryInfo`). The OS's own accounting: pages actually resident in RAM for the *whole
process* (grammar tables, interners, the Rust runtime, every crate's static data -- not just `Word`).
`peak_working_set_bytes` is a process-lifetime running maximum (monotonic, never reset per word, unlike
`ALLOC`'s per-word reset); `current_working_set_bytes` is the instantaneous reading. This is the first
working-set measurement anywhere in this workspace (confirmed absent by grep before this change). It
is **never** summed with `ALLOC` or either model estimator below -- three different quantities (OS
pages resident vs. allocator bytes requested vs. a field-level model of retained `Word`s), reported as
three separate columns.

**live-frontier model** (`pg_rules::word_stats::record_live_words`, `HC_WORD_STATS=1`,
`pg-rules/src/stratum.rs`, one sample per completed stratum pass). Walks every `Word` in that pass's
`words` accumulator via `pg_rules::word::estimate_word_bytes_breakdown`, a per-field size estimate
(`Shape`, two `FeatureStruct`s, morph records, the rule-unapplication multiset, `non_heads`,
`alternatives`). **T1 fix applied here** -- see §2. Still cannot see: anything inside a stratum pass
that gets built and freed before the pass completes (the mrule cascade's own `local`/`OrderedDedup`
accumulators, tracked separately by `frontier_profile` but never reconciled against this number); the
model's own known-low-by-an-unmeasured-amount constants (`PER_NODE_FIXED=16`, `ENTRY_FIXED=24`, `Vec`
spare capacity uncounted, `BTreeMap` node overhead charged as flat `len * (key+val)`); and, most
importantly, the WHOLE synthesis phase (`pg-parse/src/morpher.rs`), which runs entirely after
`stratum::analyze` returns and was invisible to this sampling point before T3 (§4).

**memo model** (`pg_memo::AnalysisScope::memo_bytes_used`/`template_bytes_used` +
`AnalysisStateKey::estimate_bytes`, `HC_WORD_STATS=1`, sampled ONCE at end-of-parse,
`pg-parse/src/morpher.rs`). Sums both memo tables' retained `results: Vec<Word>` (via the same
field-level estimator, now T1-fixed) plus both tables' key-side cost (`Shape` + two `FeatureStruct`s +
rule-count multiset + morph history, cloned per state -- no interning pool exists in this crate).
Still a single end-of-parse snapshot, not a time series: it cannot show when the tables grew, or
whether they were near-empty for most of the parse and filled explosively near the end.

**Why "gap" was never a legitimate number**: the three prior quantities are maxima over three
DIFFERENT windows (`ALLOC` continuous; `live peak` per-stratum-pass; `memo total` end-of-parse) that
never co-occurred, so subtracting them was subtracting unrelated instants, not partitioning one
instant's bytes. T3 (§4) fixes this specifically: every pool read at the SAME instant, so a "gap" at
that instant is at least a legitimate reading, never a subtraction of maxima that occurred at
different times.

## 2. T1: tightening a documented conservative estimate

`Word::alternatives: Vec<Rc<Word>>` (`pg-rules/src/word.rs`). **This was not an undiscovered bug.**
The doc comment this change replaced said so explicitly: recursing into each `Rc<Word>` with no
seen-set charges a shared alternative once **per referrer**, and that comment already named this "a
real double-count of one allocation across two totals, not merely a hypothetical one," adding that
"it biases the byte budget conservative (evicts sooner than the true retained set requires) rather
than under." That was a deliberate, documented trade-off, correct as far as it went. What this
change does is narrow it: dedup within a call (§2 below), which was always possible since
`Rc::clone` (what a `Word` derive-`Clone` does to this field) is a refcount bump, not a new
allocation, and was previously left undone.

**Scope decision, stated explicitly** (the task's own requirement): two different callers need two
different dedup scopes, and conflating them would be its own bug.

- `estimate_word_bytes_breakdown(w: &Word)` keeps a **per-word-walk** scope: a fresh seen-set per top-
  level call, so it dedups only an alternative reachable twice *within that one `Word`'s own subtree*
  (e.g. via `non_heads`). It does NOT dedup against any other `Word` passed to a separate call. This is
  the right scope for "how big is this one word's whole reachable subtree" -- `word_stats`'s
  `max_single_word_bytes`/percentile diagnostics, which are deliberately asking a per-word question.
- `estimate_words_breakdown(words: &[Word])` (NEW) shares ONE seen-set across the whole slice: an
  `Rc<Word>` alternative retained by two different canonicals in the same pass is charged once for the
  group, matching that it is one heap allocation. This is the scope `record_live_words` ("live peak",
  the whole pass's `words` accumulator) and the memo byte budget (`estimate_words_bytes`, called on one
  `MemoEntry::results` at insert time) both need.
- **Explicit residual**: `estimate_words_breakdown` does NOT dedup across separate calls -- two
  different memo entries, or this pass against a previous one. A shared allocation already paid for
  in an earlier call can be recharged in a later one. This is documented in the function's own doc
  comment rather than silently claimed fixed; it biases every deduped total slightly conservative
  (high), never low, so it cannot hide real growth, only overstate it.

**Unit test** (`pg-rules/src/word.rs::tests::shared_alternative_across_two_words_is_charged_once_by_the_pass_level_walk`):
two canonicals share one `Rc`-cloned alternative; two independent per-word calls each pay for it in
full, but one pass-level call over both words charges it exactly once. **Self-verification**: reverting
`estimate_words_breakdown` to the naive per-word-sum (no shared seen-set) makes this test fail with
`left: 1616, right: 1192` on the toy fixture -- a real, measured 424-byte double-charge the fix
removes, not a tautological assertion. Restored and reconfirmed green afterward.

**This is NOT measurement-only, and is disclosed as such.** `estimate_words_bytes` feeds
`AnalysisScope::has_byte_capacity` directly -- a live admission check on the hot insert path, not a
read-only diagnostic. Charging a `MemoEntry`'s Rc-shared alternatives once per group instead of once
per referrer means the budget now ADMITS entries it previously refused (a smaller charged size clears
the same threshold more often), so **peak memo-table memory can rise** relative to pre-fix behavior at
the same nominal byte budget -- the opposite direction from "measurement got smaller." §5's table
reports T1-on vs. T1-off memo admission counts so this is visible, not folded into a baseline as if
nothing changed. What does NOT change: per the memo module's own "Memoization is correctness-neutral"
contract (`pg-memo/src/lib.rs`'s module doc) and the pure-cache argument in §8, a byte-budget
threshold can only ever change *when* a subtree goes unmemoized (hit-rate/performance/peak-bytes),
never the analysis-identity set a completed word returns -- confirmed empirically, not just argued,
by §6's gate results being byte-identical with and without the fix.

## 3. T2: real working-set measurement

`rust/crates/pg-cli/src/winmem.rs` (Windows-only, `#[cfg(windows)]`), `windows-sys 0.61.2`
(`Win32_System_ProcessStatus`, `Win32_System_Threading`, `Win32_Foundation`) -- already the pinned
version in `rust/Cargo.lock` and already this repo's own established pattern for exactly this class of
Win32 API access (`pg-worker-containment`'s `windows.rs`, job-object/process FFI). `sysinfo` (also
already a `pg-cli` dependency, used in `recipe_optimize.rs`) was checked first and rejected: its
`Process::memory()` gives current RSS only, with no peak-working-set equivalent, and the task
specifically needs both from one `K32GetProcessMemoryInfo` call. One unsafe FFI call, matching the
existing `alloc-trace` precedent of relaxing `pg-cli`'s crate-level `forbid(unsafe_code)` (now
`cfg_attr(not(any(feature = "alloc-trace", windows)), forbid(unsafe_code))`) for a narrowly scoped,
documented reason.

`None` (not a silent zero) on Win32 call failure, printed as an explicit `ERROR=` line -- this repo's
"a control that cannot act must say so" rule. Printed as its own `WS` TSV line, `HC_WS_STATS=1`,
labeled `peak_working_set_bytes`/`current_working_set_bytes`; never summed with anything else.

## 4. T3: one clock for every pool

`pg_rules::clock_sample` (`HC_CLOCK_SAMPLE=1`), two sample points:

- **`"stratum_pass"`**: `pg-rules/src/stratum.rs`, right where `word_stats::record_live_words` already
  fires (once per completed stratum pass) -- reuses that exact call site so the two diagnostics are
  guaranteed to describe the same instant.
- **`"synthesis_loop"`**: `pg-parse/src/morpher.rs`, once per canonical in the `for aw in
  results.values()` synthesis loop (the phase named in the task brief's "Blind phase" defect --
  `word_stats` never sampled anything here at all). Samples `matches`, the loop's own growing
  accumulated match set, at that instant.

Each sample reads, together: `alloc_trace`'s current peak/live bytes (via a function-pointer hook
`pg-cli` installs once, only in an `alloc-trace` build -- `pg-rules` cannot depend on `pg-cli`, so the
hook is the seam; `None` when no `alloc-trace` build installed it, never a zero standing in for an
absent number), the live-frontier model (`estimate_words_breakdown` over whatever `Word` set is live
at that call site), and both memo tables' key/results bytes (`AnalysisScope::estimate_key_bytes`/
`memo_bytes_used`/`template_bytes_used`). One `CLOCK` TSV row per sample, `pg-cli` draining the
per-thread buffer once per word. This is what makes "the gap" a reading at an instant rather than a
subtraction of three maxima that never co-occurred (§1).

**Observer effect, caught before any reading was taken and trusted**: the first version of the
`"synthesis_loop"` sample point built `let live: Vec<Word> = matches.values().cloned().collect();`
to have an owned slice to pass, deep-cloning the whole match set once per canonical purely to feed
the sampler -- gated behind `clock_sample::enabled()`, but "when the diagnostic is on" is exactly
when that clone's own allocation would inflate the very `ALLOC` peak the sample exists to read,
potentially dominating it on a word with thousands of canonicals. Fixed by making
`estimate_words_breakdown`/`clock_sample::record` generic over `impl IntoIterator<Item = &Word>`
instead of `&[Word]`, so the synthesis call site now passes `matches.values()` directly -- a
borrowed iterator, zero additional allocation beyond the estimator's own (small) `HashSet` seen-set.
No `CLOCK`/`ALLOC` reading was taken with the clone in place before this was caught, so none needed
retraction.

## 5. Corrected evidence table

<!-- FILLED FROM MEASURED RUNS; see rust/tools/pg.ps1, --features alloc-trace, HC_ALLOC_STATS=1,
     HC_WS_STATS=1, HC_WORD_STATS=1. Word/cap grid matches word-memory-trace.md sec 1 so rows are
     comparable. Serial, single-threaded, >=3 runs, spread reported. Per §2, T1 changes memo
     admission, not just measurement -- columns below report memo_inserts/memo_insert_refused
     (HC_MEMO_STATS=1) for T1-on (this worktree) vs. T1-off (the pre-fix estimator, reverted the
     same way §2's unit-test self-verification reverted it) at the SAME nominal byte budget, so the
     admission-count shift is visible rather than folded into a baseline as if nothing changed. -->

TODO(measurement not yet run)

## 6. Correctness gates

`memo_parity_gate` (`pg-parse::memo_on_and_off_agree_on_every_fixture_word`): the task brief's stated
baseline of "exactly 67 fixtures / 690 words / 2318 analysis identities" does NOT match this base
commit -- both the pristine main checkout (zero diff) and this worktree (full T1-T3 diff) report
**68 fixtures, 696 words, 2323 analysis identities**, byte-identical between the two. This confirms
two things at once: (a) the discrepancy from the brief predates this work entirely (fixtures grew
between when the brief was authored and this exact base commit; this diff touches no fixture data,
no conformance-staging content, and no fixture-discovery code, so it cannot be the cause), and
(b) the T1-T3 diff has zero effect on the gate's own analysis-identity comparison, exactly as
predicted by the memo module's "correctness-neutral" contract -- a byte-budget or sampling-instrument
change can only affect *when* something is memoized, never *what* a completed word returns.

`memo_corpus_gate` (`pg-foma::memo_parity_survives_aweti_sena_mbugwe`, via `-Mode corpus-test`,
`PANGLOSS_CORPUS_ROOT` pointed at the main checkout's `samples/data` to avoid copying private
corpora into this worktree): pristine main and this worktree report byte-identical tallies --
`aweti: completed both=20 capped both=13 completed only on=11 completed only off=0`;
`sena: completed both=300 capped both=0 completed only on=0 completed only off=0`;
`mbugwe: completed both=56 capped both=4 completed only on=0 completed only off=0`. PASS both times.

`conformance-test -Scope local` (full workspace, `--workspace`, 2388 tests across 244 binaries): run
in this worktree, `2388 tests run: 2306 passed (2 slow), 82 failed, 174 skipped`. **Not used as a
gate** -- traced each distinct failure class to an environmental cause specific to this worktree, not
to this diff: `pg-cli::agent_docs_resolve_gate::every_path_named_in_an_agent_doc_exists` fails because
the worktree has no `machine/src` (the sibling C# checkout `.claude/skills/conformance-grammars/
SKILL.md`/`.claude/skills/dead-end-census/SKILL.md` name, which lives outside this worktree entirely,
not something a sparse submodule checkout provides); the `pg-foma`/`stats_cmd`/other failures panic
with "fixture .../... must be discoverable" or "not discoverable" -- these call
`pg_conformance_fixtures::discover()` unconditionally, and `-Scope local` (`PANGLOSS_CONFORMANCE_SCOPE=local`)
deliberately restricts discovery to `conformance-staging/**` only (`ConformanceScope::Local`'s own
doc: "this repo's own fixtures, nothing from upstream"), while these particular fixtures (confirmed
by `find`, e.g. `languages/metathesis-phase-isolation`, `edge-cases/deep-optional-affix-nesting`)
live only under `machine/conformance/` (upstream, `-Scope all`-only). None of the 82 failures are in
`pg-rules` (the crate T1/T3/T5 touch); neither `memo_parity_gate` nor `memo_corpus_gate` is among
them (both ran separately, §above, and passed). This matches this repo's own memory note recording
"sena3 drift" as a pre-existing red in this same suite, independent of this branch. **This diff did
not use the full workspace `-Mode conformance-test -Scope local` run as a pass/fail gate** for the
reasons above; the two memo gates (run individually, pristine-vs-diffed) are the actual correctness
evidence for T1-T3.

Self-verification performed for both memo gates: run on the pristine base commit (main checkout,
zero diff) AND on this worktree (full diff), same command, same corpus root, compared byte-for-byte.
Neither gate's numbers moved.

## 7. T5(a): full clone vs. delta cost per stored alternative

`pg_rules::alt_delta` (`HC_ALT_DELTA_STATS=1`). `Word::expand_alternatives` already reconstructs an
alternative from its `source` spine plus a shape + `mrule_apps` trail-suffix + `non_heads` suffix + a
real-fs diff + a root-allomorph delta -- so that delta shape is not invented for this measurement, only
sized and compared against the full-clone cost every stored alternative currently pays.

<!-- FILLED FROM MEASURED RUNS -->

## 8. T5(b): what a memo entry is worth

`pg_rules::memo_value` (`HC_MEMO_VALUE_STATS=1`, mrule-memo table only -- template-memo not
instrumented, time-boxed). Per stored `AnalysisStateKey`: bytes at insert time, `results.len()`, the
cascade's own in-progress-set size at insert time (a cheap, already-computed depth/position proxy --
`AnalysisScope::in_progress.len()`, no new recursion tracking added), and a hit counter incremented on
every subsequent lookup that finds the key (positive or nogood -- both avoid re-derivation).

<!-- FILLED FROM MEASURED RUNS -->

**Is the memo a pure cache?** Yes, provably, from the existing (unmodified) source, not a new claim:
`pg-memo/src/lib.rs`'s own module doc states "None of the three [caps] evicts: past any cap, a
subtree simply goes unmemoized, degrading hit rate but never correctness -- a miss always falls back
to full recomputation," and `AnalysisScope::has_memo_capacity`/`has_byte_capacity`
(`pg-memo/src/lib.rs`) are pure admission checks with no eviction path anywhere in the type. A refused
insert changes nothing about the analysis set a word returns -- confirmed empirically, not just by
reading the contract, by §6's gates being byte-identical before and after T1 changed the byte budget's
own admission threshold. Nogood entries (`MemoEntry::is_positive() == false`) are exact, not
approximate: `run_mrule_cascade`'s subtree was actually explored to completion before a nogood is
stored (`pg-memo/src/lib.rs`'s `MemoEntry` doc: "There is no 'budget exhausted' flag -- this branch
has no per-subtree budget, so every stored subtree was explored to completion"), so a nogood hit is a
correctness-neutral skip of re-exploring an already-proven-empty subtree, never a guess.

## 9. What remains unattributed after the repair

<!-- FILLED once §5's table is in: name each residual gap and its leading hypothesis. -->
