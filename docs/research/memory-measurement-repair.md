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

**This is NOT measurement-only in principle, and is disclosed as such.**
`estimate_words_bytes` feeds `AnalysisScope::has_byte_capacity` directly -- a live admission check on
the hot insert path, not a read-only diagnostic. Charging a `MemoEntry`'s Rc-shared alternatives once
per group instead of once per referrer means the budget CAN admit entries it previously refused (a
smaller charged size clears the same threshold more often), so peak memo-table memory could in
principle rise relative to pre-fix behavior at the same nominal byte budget. **§5 measured this
directly rather than stopping at the principle**, on three real words at two budgets each (default
and a deliberately tiny 4 MiB one chosen to force refusals): every T1-on/T1-off pair came back
byte-identical -- `oteʼikateʼika`@1M (11757/0 inserts/refused at both budgets' respective settings,
3980/12902 at the tiny one) and `Ajkululape`@200k (515/3808 at the tiny budget) showed no admission
shift at all, because none of these words' actual data happens to put the same `Rc<Word>` alternative
into two `Word`s within one accounting call (§5 traces exactly why). So the measured effect on this
grid is zero, not "small" -- reported as a residual (§9), not folded into a baseline as if untested.
What does NOT change regardless: per the memo module's own "Memoization is correctness-neutral"
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

Debug build (`dev` profile, `cargo build -p pg-cli --features alloc-trace`, confirmed by the printed
line below), `--threads 1`, single word per run, all diagnostics on simultaneously
(`HC_ALLOC_STATS=1 HC_WS_STATS=1 HC_WORD_STATS=1 HC_CLOCK_SAMPLE=1 HC_ALT_DELTA_STATS=1
HC_MEMO_VALUE_STATS=1 HC_MEMO_STATS=1`). 3 runs each, serial. **Release build not run: time-boxed —
see the residual list in §9.**

Printed cargo line (proof `alloc-trace` was on): `cargo build -p pg-cli  --features alloc-trace`.

Spread across 3 runs: **zero** on every deterministic counter (`ALLOC`, `WORDSTATS`, `ALTDELTA`,
`MEMOVALUE`, `MEMOPROF`) -- single-threaded and step-capped, so no run-to-run variance is expected
or observed; `WS` (`peak_working_set_bytes`) varies by under 1% between runs (OS scheduling/paging
noise), reported as one representative value per row, not averaged.

| word | cap | memo | ALLOC peak | peak WS | current WS | live-frontier model | memo model (results+key, both tables) |
|---|---:|---|---:|---:|---:|---:|---:|
| oteʼikateʼika | 200k | on | 136,626,541 B | 170,885,120 B | 122,019,840 B | 15,913,984 B | 62,928,912 B |
| oteʼikateʼika | 1M | on | 564,123,426 B | 650,240,000 B | 452,624,384 B | 111,622,656 B | 214,891,920 B |
| oteʼikateʼika | 200k | off | 31,800,930 B | 46,309,376 B | 23,998,464 B | 2,941,456 B | 0 B |
| Ajkululape | 200k | on | 334,951,940 B | 387,018,752 B | 231,751,680 B | 68,078,288 B | 142,945,344 B |
| ajkulula | 200k | on | 31,800,922 B | 46,317,568 B | 24,576,000 B | 223,992 B | 297,848 B |
| kukudziwisani (Sena) | 1M | on | 39,518,692 B | 54,636,544 B | 22,532,096 B | 615,408 B | 1,050,624 B |

`memo model` = `memo_results_bytes + memo_key_bytes + tpl_results_bytes + tpl_key_bytes` from the same
`WORDSTATS` line as `live-frontier model`. Every row's `ALLOC peak` now sits BELOW the sum of `peak
WS`'s two components would suggest is available headroom, and, notably, every `ALLOC peak` here is
substantially smaller than the corresponding row in `docs/research/word-memory-trace.md` §1 (e.g.
`oteʼikateʼika`@200k/on: 229.6 MB there vs. 130.4 MB here) and `docs/research/alt-yield.md` §4's
post-push-pruning row (188.8 MB). This is **not** attributed to T1 -- §"T1-on vs. T1-off" below shows
T1 changes nothing measurable for these exact words -- it reflects everything that landed on `main`
between those docs (2026-09-14) and this base commit (2026-09-15), most plausibly the "stream
`apply_mrules`/`apply_templates`" fix `live-frontier-memory-bound.md` §5 already recorded as merged.
Not re-attributed further here (out of scope for a measurement-only repair).

**T1-on vs. T1-off, admission counts** (§2's disclosed non-measurement-only effect, checked
directly): reverted `estimate_words_breakdown` to the pre-fix per-word-sum (the same revert §2's
unit-test self-verification used), rebuilt, and reran three cases designed to expose any admission
difference -- `oteʼikateʼika`@1M at the default 256 MiB budget, the same word at a deliberately tiny
4 MiB budget (`HC_MEMO_BYTES=4194304`, chosen to force refusals so a difference would have somewhere
to show up), and `Ajkululape`@200k at the same tiny budget (the word with this whole grid's largest
single memo entry, `results_len_max=3170` / `450` under default / tiny budget -- the best real-data
candidate for two `Word`s in one `MemoEntry::results` sharing an `Rc` alternative):

| word/cap | budget | T1-on: inserts / refused | T1-off: inserts / refused | live_peak_total (both) |
|---|---:|---:|---:|---:|
| oteʼikateʼika@1M | default (256 MiB) | 11757 / 0 | 11757 / 0 | 111,622,656 B |
| oteʼikateʼika@1M | 4 MiB | 3980 / 12902 | 3980 / 12902 | (not resampled) |
| Ajkululape@200k | 4 MiB | 515 / 3808 | 515 / 3808 | 68,078,288 B |

**Every pair is byte-identical.** This is a real, measured result, not an assumption that the fix
"must" be inert: it means the double-count T1 fixes, while provably real (the synthetic unit test in
§2 constructs it directly), does not occur naturally in any of these three real corpus words at
either budget tested. The mechanism requires the SAME `Rc<Word>` alternative to be reachable from
two different `Word`s inside the SAME `estimate_words_breakdown` call (one `MemoEntry::results` Vec,
or one stratum pass's `words` Vec); reading `stratum.rs`'s two push sites
(`words[idx].alternatives.push(Rc::new(w))`) shows every alternative is born via a fresh `Rc::new`
attached to exactly one canonical, and a canonical appears at most once in either `words` or one
`MemoEntry::results` per call -- so sharing would require a canonical to be `.clone()`d (bumping its
alternatives' refcounts) and BOTH the original and the clone to land in the same one accounting call,
which does not happen for `record_live_words`'s or the memo insert's call sites on these words.
**Conclusion, stated plainly and not overclaimed**: T1 is real, tested, and correct, but on this
grid it changed zero bytes and zero admission decisions. Whether some OTHER grammar/word exercises
the sharing case is not established either way by this measurement -- absence of evidence on three
words is not evidence of absence generally, and is reported as a residual in §9, not as "the fix
never matters."

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
sized and compared against the full-clone cost every stored alternative currently pays. Measured on
the same 6 rows as §5 (debug build, same run):

| word/cap/memo | samples | full bytes | delta bytes | ratio p50 | ratio p90 | ratio max | mean identical-field frac |
|---|---:|---:|---:|---:|---:|---:|---:|
| oteʼikateʼika@200k/on | 7,733 | 12,016,712 | 6,742,232 | 0.5479 | 0.5981 | 0.7640 | 1.0000 |
| oteʼikateʼika@1M/on | 59,529 | 92,298,256 | 48,135,432 | 0.5176 | 0.5962 | 0.7926 | 1.0000 |
| oteʼikateʼika@200k/off | 1,345 | 2,139,920 | 1,228,440 | 0.5479 | 0.5990 | 0.6719 | 1.0000 |
| Ajkululape@200k/on | 41,082 | 63,402,144 | 39,088,808 | 0.6108 | 0.7111 | 0.8555 | 1.0000 |
| ajkulula@200k/on | 33 | 57,032 | 41,312 | 0.7628 | 0.8204 | 0.8227 | 1.0000 |
| kukudziwisani@1M/on | 455 | 355,552 | 149,128 | 0.3125 | 0.6043 | 0.6486 | 1.0000 |

**Aggregate ratio** (sum delta / sum full, across the five distinct `--memo on` rows -- the `off` row
is the same word and would double-count it): 95,385,352 / 170,269,616 = **0.560**. This directly
answers the task's framing question: a delta representation is worth neither the ~90% nor the ~10%
extreme -- it lands at **~56-60%** typically (p50 ranges 0.31-0.76 across words, but the two
highest-volume pathological rows, `oteʼikateʼika` at both caps, sit at 0.52-0.55), meaning a delta
encoding would roughly HALVE stored-alternative bytes on the words that matter most for total memory,
not eliminate the cost.

**`mean_identical_field_frac = 1.0000` on every single row, no exception.** All 10 comparable fields
(`stratum`, `syn_fs`, `mpr`, `morphs`, `non_head_app_index`, `root_allomorph`, `root_runtime_id`,
`obligatory`, `unapplied_rule_counts`, `flags`) are byte-identical between every measured alternative
and its owning canonical, on every word tested, with zero exceptions across 110,177 total sampled
alternatives. Concretely: the "root-allomorph delta" branch of `estimate_alt_delta_bytes`
(`alt.root_allomorph != src.root_allomorph`) essentially never fires on real data -- an alternative's
divergence from its canonical is concentrated entirely in `shape` + the `mrule_apps`/`non_heads`
trail-suffixes + (rarely) `real_fs`, exactly the fields the delta calculation already isolates, and
none of the "does this field ever need to ride along too" uncertainty the task's framing left open.

## 8. T5(b): what a memo entry is worth

`pg_rules::memo_value` (`HC_MEMO_VALUE_STATS=1`, mrule-memo table only -- template-memo not
instrumented, time-boxed). Per stored `AnalysisStateKey`: bytes at insert time, `results.len()`, the
cascade's own in-progress-set size at insert time (a cheap, already-computed depth/position proxy --
`AnalysisScope::in_progress.len()`, no new recursion tracking added), and a hit counter incremented on
every subsequent lookup that finds the key (positive or nogood -- both avoid re-derivation). Measured
on the same runs (`--memo off` rows have no mrule-memo table at all, `entries=0`, correctly omitted
below):

| word/cap | entries | total bytes | zero-hit entries | zero-hit bytes | zero-hit byte share | mean hits | mean results_len | mean depth at insert |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| oteʼikateʼika@200k/on | 2,250 | 59,913,432 | 444 (19.7%) | 49,264,104 | **82.2%** | 1.972 | 17.310 | 3.235 |
| oteʼikateʼika@1M/on | 11,757 | 178,030,448 | 2,268 (19.3%) | 129,894,224 | **73.0%** | 2.380 | 9.817 | 2.920 |
| Ajkululape@200k/on | 2,724 | 132,250,800 | 436 (16.0%) | 93,673,272 | **70.8%** | 6.575 | 33.400 | 2.722 |
| ajkulula@200k/on | 104 | 77,408 | 71 (68.3%) | 77,408 | **100%** | 0.317 | 0.481 | 1.481 |
| kukudziwisani@1M/on | 303 | 586,448 | 207 (68.3%) | 400,456 | **68.3%** | 0.574 | 2.158 | 3.158 |

**Yes, there is a low-value tail, and it is large.** Aggregate (sum zero-hit bytes / sum total bytes
across the 5 `--memo on` rows): 273,309,464 / 370,858,536 = **73.7%** of all mrule-memo bytes on this
grid are held by entries that were stored and never hit again within the same parse. The two easy
words (`ajkulula`, `kukudziwisani`) show entry-COUNT-majority zero-hit (68.3% of entries) but the
pathological words show byte-majority zero-hit while entry-count zero-hit stays a minority (16-20%
of entries) -- i.e. the tail is concentrated in a *few large* never-hit entries on hard words
(`results_len_mean` for `Ajkululape` is 33.4 words/entry) rather than *many small* ones. `value_p50`
is 0 or near-0 on every row (the median entry contributes negligible hits per byte); `value_p90` is
1-2 orders of magnitude higher (0.0006-0.0032 on the pathological words), confirming a genuinely
skewed distribution, not a uniform low return.

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

**One important qualifier on "low-value tail":** a 73.7% zero-hit byte share is a fact about *this
parse's own single-word memo scope* (`pg-memo/src/lib.rs`'s module doc: one `AnalysisScope` per
`Morpher::parse_word` call, never shared across words). A "zero hit" entry did not deliver a hit
*within this one word's parse*, but every stored entry was, by construction, reached at least once
(the insert itself is triggered by reaching that state) -- so "zero-hit" means "reached exactly once,
never revisited by a different unapplication order," which for a single-word memo is a ceiling
imposed by how much the `k!`-permutation cascade actually revisits any given state for that
particular word, not a policy failure. Whether a *cross-word* or *cross-run* cache would find more
reuse for these same states is a different question this per-parse-scoped measurement cannot answer.

## 9. What remains unattributed after the repair

- **Release build not measured.** Every number in §5/§7/§8 is from a `dev`-profile binary. Time-boxed
  in this session; the leading hypothesis is that ALLOC/WS numbers shrink and ratios in §7/§8 stay
  materially the same (they are structural properties of the data, not of codegen), but this is
  unverified. Re-run with `-Mode release` (or `-Mode build -Package pg-cli` without `-DebugProfile`)
  plus `--features alloc-trace` to close this.
- **Whether ANY real grammar exercises T1's double-count mechanism.** §5 found zero measured effect
  on three words at two budgets each; the mechanism itself is real (unit-tested) but architecturally
  requires a canonical to be cloned with its `alternatives` intact and BOTH the original and the
  clone to land in the same `estimate_words_breakdown` call, which §5 traced as not happening for
  `record_live_words` or a single `MemoEntry::results` insert on these three words. Leading
  hypothesis: a grammar with heavier cross-stratum cloning of already-merged canonicals (more strata,
  more compounding non-heads carrying already-alternatived sub-words) is a more likely place to find
  a nonzero effect than these three did. Not tested here.
- **`estimate_words_breakdown`'s cross-call residual** (§2's own explicit disclaimer): two different
  memo entries, or two different stratum passes, still do not dedup against each other. Given §5's
  finding that even WITHIN-call sharing did not occur on these words, cross-call sharing is at least
  as unlikely to matter on this same grid, but this is inference, not a separate measurement.
  Untested directly.
- **The `~30 MB floor` `word-memory-trace.md` §1 found (grammar tables + process baseline, invisible
  to `estimate_word_bytes` by construction)** is still exactly as invisible to every number in this
  repair as it was before: `ALLOC`/`WS` include it (both show a floor around 22-24 MB `current_working_set_bytes`
  on the two easy/off rows here, consistent with that prior finding), `WORDSTATS`/`ALTDELTA`/
  `MEMOVALUE` structurally cannot, by the same reasoning as before (they only ever walk `Word`,
  never the grammar). Not re-measured directly here; the `ajkulula`/`kukudziwisani@off`-shaped rows
  in §5 are the closest proxy, and land in the same 22-24 MB `current_working_set_bytes` range prior
  work found.
- **`frontier_profile`'s own separate `live_bytes` accumulator** (`pg-rules/src/stratum.rs`, feeding
  `HC_FRONTIER_STATS=1`, a pre-existing and different diagnostic from `word_stats`) still calls
  per-word `estimate_word_bytes` at each push, one word at a time, so it still has the SAME per-word
  (not pass-level) dedup scope `word_stats` had before T1 -- i.e. it was never brought in scope of
  this repair. Deliberately out of scope (T1's brief named `record_live_words` and the memo byte
  budget specifically); flagged here rather than silently left inconsistent with `word_stats`'s now-
  corrected pass-level scope.
- **Template-memo table not instrumented by T5(b)** (`pg_rules::memo_value`, mrule-memo only, stated
  in §8's own header). `tpl_results_bytes`/`tpl_key_bytes` are visible in `WORDSTATS` (§5's table
  folds them into "memo model") but per-entry hit/depth/value data for that table was not built,
  time-boxed.
- **The residual gap between `ALLOC peak` and `peak WS` vs. the model totals** (the actual "gap" this
  whole repair was trying to make legitimate) is not separately quantified per-row in this note --
  §4's `HC_CLOCK_SAMPLE=1` machinery exists and was exercised in every run above (visible as the
  large `CLOCK` line counts per word, e.g. 2,262 rows for `oteʼikateʼika`@200k), but the per-instant
  `CLOCK` rows themselves were not aggregated into a "gap at instant X" table here -- time-boxed. The
  raw TSV rows are reproducible from any of §5's runs by re-adding `HC_CLOCK_SAMPLE=1` and piping
  stderr to a file; turning that into a reconciled table is the direct next step this repair sets up
  but does not finish.
