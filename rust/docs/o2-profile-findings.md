# O2 profiling findings (`rust-optimizations-phase2.md` O2) — SONNET measurement, FABLE interprets

> **FIX LANDED (2026-07-10, `23f6fe0c`)** — the `distinct()` linear scan this document flagged is
> now hash-backed (hash of a *canonicalized* `FstResult` form consistent with `result_eq` — unset
> registers contribute only their `has` bit — into a hash → first-occurrence-indices table, with
> `result_eq` as the in-bucket fallback; survivor choice and output order bit-for-bit unchanged).
> Re-run of this same instrumentation, same machine: ሌባዬ **~145s → 54.3s** wall (`distinct_ms`
> 86,633 → 488; now beats the C# oracle's 64.0s), በመጨረሻ **~303s → 53.0s** (`distinct_ms`
> 235,057 → 414), በለጠ control flat (341 → 346ms). Signatures byte-identical (`+|ሌባ+?ዬ` / `-`),
> every input-side counter (steps, `nondet_total_traversed`, `distinct_max_input_len`) identical
> to the pre-fix numbers below. Full detail: `rust-optimizations-phase2.md` § O2.
> The numbers in the body below are the **pre-fix** measurements, kept as-is for the record.

> Scope discipline: this document reports measurements only. No engine behavior was changed. The
> only source edits are new, permanent, HC_STEP_STATS-style diagnostics (opt-in via env var,
> effectively free when unread) — see "Instrumentation added" below. All numbers were gathered on
> a release build (`cargo build --release -p hc-cli`) at `o2-fst-profile`'s branch point (`rust`
> @ `7f46f2ad`, this branch's own tip after landing the diagnostics).

## TL;DR

For both known-pathological Amharic words, **89–96% of total wall-clock time is inside a single
function, `Transduce::run` (`hc-fst/src/traverse.rs`)** — consistent with lead #1. But the profile
data **relocates** the hot spot *within* that function: it is not dominated by the nondeterministic
backtracking traversal itself (14–30% of wall-clock), but by the **`distinct()` post-processing
step** that runs after traversal to emulate C#'s `Enumerable.Distinct` — **59–81% of total
wall-clock**, driven by a single call processing up to ~500K raw `FstResult`s with an O(n²)-shaped
linear-scan-plus-pairwise-equality algorithm. Lead #2 (`push_remove_duplicates`, the "keep-longer"
dedup in `hc-rules/src/morph.rs`) is **negligible** for both words (<0.1% of wall-clock; its
candidate lists never exceeded length 1 before resolving). Lead #3 (template-battery interior
memoization) could not be directly measured (no engine time went through that code for either
profiled word) but a code-reading check found it at parity with C#'s design already.

## Tooling available on this system

- **`wpr.exe`** (Windows Performance Recorder) is present (`C:\Windows\System32\wpr.exe` and the
  WPT copy) but **requires elevated privileges** this sandboxed shell does not have:
  `wpr.exe -start GeneralProfile -filemode` → `Failed to enable the policy to profile system
  performance` (`0xc5585011`). Not usable here.
- **`samply`** (a modern Rust-ecosystem sampling profiler) is not preinstalled but network access
  allowed `cargo install samply --locked` (succeeded, ~58s). However `samply --help` states
  recording is **Linux/macOS only** — on Windows it can only load/view already-captured profiles.
  Not usable here either.
- **`cargo-flamegraph`**: not installed; would hit the same problem (relies on `perf`/`dtrace`,
  neither available on Windows).
- **No true sampling/tracing profiler was available.** Fell back to manual, targeted
  `Instant::now()`-based coarse phase instrumentation, in the same spirit as the existing
  `HC_STEP_STATS` mechanism — see "Instrumentation added."
- The C# oracle (`hc.dll`, `.worktrees/parse-opt`) exposes a real `--rule-stats=FILE` flag
  (`BatchCommand.cs`, requires `--sequential`) giving a per-rule-node call-tree with
  inputs/successes/outputs/elapsedMs. Used it for a live head-to-head on ሌባዬ (see below).

## Instrumentation added (kept, not reverted)

Three new counters, all thread-local `Cell`-based, all read-only snapshots (never reset — one
word per process invocation in `batch --threads 1` mode, matching how `HC_STEP_STATS` is read):

- `hc_fst::profile` (`crates/hc-fst/src/traverse.rs`): wraps `Transduce::run` (renamed the old body
  to `run_inner`) and the nondeterministic branch of `traverse_from`, plus the `distinct()` call
  site — records call counts, total/max nanoseconds, and (for the nondeterministic branch and
  `distinct()`) the max/total size of the state being processed (`traversed.len()` /
  `result_list.len()`).
- `hc_rules::morph::dedup_profile` (`crates/hc-rules/src/morph.rs`): wraps `push_remove_duplicates`
  (renamed the old body to `push_remove_duplicates_inner`) — records calls, total nanoseconds, and
  the `out` list's length just before each call (to catch any combinatorial growth there).
- `hc-cli`'s `batch` sequential loop (`crates/hc-cli/src/main.rs`) prints both snapshots to stderr
  as `FSTPROF`/`DEDUPPROF` lines, gated on a new `HC_FST_PROFILE=1` env var (default off, zero
  cost), alongside the existing `HC_STEP_STATS=1`-gated `STEPS` line.

Required adding `hc-fst` and `hc-rules` as direct `hc-cli` dependencies (both were already in the
workspace; `hc-cli` previously only depended on `hc-parse`/`hc-grammar`/`hc-featstruct`). No
existing function signatures or behavior changed. Verified: `cargo test -p hc-fst -p hc-rules
-p hc-cli --release` — all green (0 failures), and the two profiled words produced byte-identical
signatures across every repeated run (`+|ሌባ+?ዬ` for ሌባዬ, `-` for በመጨረሻ), so the instrumentation is
side-effect-free.

**Decision (per the task's discretion clause): kept, not reverted.** This is exactly the
measurement the Fable follow-up will want to re-run to confirm whichever fix it picks actually
moves `distinct_ms`/`nondet_ms` down.

## Words profiled

Both from `samples/data/amharic-words.txt` (idx 32 = ሌባዬ, idx 172 = በመጨረሻ). Ran
`hc-rs batch amharic-hc.xml <one-word-file> out.tsv --threads 1` with `HC_STEP_STATS=1
HC_FST_PROFILE=1`, three times each (once step-stats-only, twice with the full instrumentation) to
check run-to-run stability. Numbers below are from the final (fullest-instrumentation) run of each;
earlier runs agreed within run-to-run noise (~5%) on every ratio.

A third, fast control word (በለጠ, gold C# time 920ms) was profiled first to sanity-check the
instrumentation and see whether the same phase split holds on an ordinary, non-pathological word.

### ሌባዬ

- **Rust**: wall-clock 142,946–147,426ms (~145s), 25,820 `StepBudget` ticks, result `+|ሌባ+?ዬ`
  (matches the historically-recorded gold signature).
- **C# oracle** (`hc.dll --sequential batch ... --rule-stats=...`, live run against the same
  `samples/data/amharic-hc.xml`): **64,037ms wall-clock**, byte-identical signature
  `+|ሌባ+?ዬ`. **Rust is ~2.2–2.3x slower** — matches O2's "~2x per-step" framing freshly, on this
  exact word, post-P10 (P10 doesn't touch Amharic — its id-lane is disabled for tables >64
  char-defs, and Amharic has 422). The rule-stats report attributes 63,421 of the 64,037ms wholesale
  to the single `Analysis > Morphology` node with no finer per-rule-elapsedMs breakdown available
  (every child node reports `elapsedMs=0` at C#'s millisecond resolution) — it doesn't itself
  localize a hotspot, but it does confirm the wall-clock number and the exact-match signature.
- **FSTPROF** (Rust, this run): `run_calls=26712 run_ms=132137.6 run_max_ms=37001.1
  nondet_calls=26873 nondet_ms=44807.8 nondet_max_traversed=360550 nondet_total_traversed=15810476
  det_calls=370 det_ms=0.19 distinct_calls=26608 distinct_ms=86633.2 distinct_max_input_len=327360
  distinct_total_input_len=2569655`
- **DEDUPPROF**: `calls=374312 ms=111.5 max_out_len=1 total_out_len=372882`

### በመጨረሻ

- **Rust**: wall-clock 290,954–325,544ms (~303s avg), only **844** `StepBudget` ticks, result `-`
  (no analyses — a legitimate zero-parse, per the task brief's framing; not re-verified against
  gold here, out of this task's scope). Note: `golden/master/sena-full.tsv`-sibling
  `rust/parity-out/golden/master/amharic.tsv` (checked in the main repo, not this worktree) has only
  a `STARTED` row for this word with no completion row — i.e. even a historical C# gold run never
  finished it — so a fresh live C# timing for this specific word was **not attempted** (would risk
  burning a large, unbounded amount of time for a number of uncertain value; flagged as a gap, not
  measured).
- **FSTPROF** (Rust, this run): `run_calls=903 run_ms=278645.3 run_max_ms=91208.2 nondet_calls=1782
  nondet_ms=42898.2 nondet_max_traversed=542725 nondet_total_traversed=10860498 det_calls=118
  det_ms=0.13 distinct_calls=857 distinct_ms=235057.3 distinct_max_input_len=501025
  distinct_total_input_len=1906766`
- **DEDUPPROF**: `calls=165328 ms=64.7 max_out_len=1 total_out_len=165280`
- A separate full-instrumentation run (the 325,544ms one) pushed `run_max_ms` to **110,793.8ms** —
  a **single `Transduce::run` call took ~111 seconds by itself**, ~34% of that run's total
  wall-clock, and `distinct_ms` to 264,736.8ms (81.3% of that run's wall-clock).

### በለጠ (fast control, gold 920ms)

- **Rust**: wall-clock 341ms, 642 steps.
- **FSTPROF**: `run_calls=743 run_ms=287.4(84%) run_max_ms=19.1 nondet_calls=822
  nondet_ms=269.4(79%) nondet_max_traversed=5169 distinct_calls=698 distinct_ms=13.2(4%)
  distinct_max_input_len=3745`
- **DEDUPPROF**: `calls=3975 ms=1.3 max_out_len=1`

Even on this ordinary word, `Transduce::run` still eats the large majority of wall-clock (84%),
but here the split is dominated by the nondeterministic traversal itself (79%), not `distinct()`
(4%) — because the raw-result count feeding `distinct()` is small (max 3,745, vs 327K–501K on the
pathological pair). This is consistent with `distinct()`'s cost scaling **super-linearly** with the
size of the raw match list, which itself scales with how "Optional-flooded" the shape has become —
exactly the mechanism lead #1 hypothesized, just now localized to a specific sub-step.

## Summary table

| word | category | wall (ms) | steps | run_ms (% wall) | run_max single call (ms) | nondet_ms (% wall) | nondet_max_traversed | distinct_ms (% wall) | distinct_max_input_len | dedup_ms (% wall) |
|---|---|---|---|---|---|---|---|---|---|---|
| በለጠ | fast control | 341 | 642 | 287 (84%) | 19 | 269 (79%) | 5,169 | 14 (4%) | 3,745 | 1.3 (0.4%) |
| ሌባዬ | pathological | ~145,000 | 25,820 | 132,138 (90%) | 37,001 | 44,808 (30%) | 360,550 | 86,633 (59%) | 327,360 | 0.11 (0.08%) |
| በመጨረሻ | pathological | ~303,000–325,544 | 844 | 278,645–311,113 (95–96%) | 89,442–110,794 | 42,898–45,631 (14%) | 542,725 | 235,057–264,737 (80–81%) | 501,025 | 0.06 (0.02%) |

## Verdict on the three ranked leads

1. **`Transduce::all_matches()`/`run()` over long/Optional-flooded shapes — CONFIRMED, and
   localized further than the plan doc's prior wording.** ~90–96% of wall-clock for both
   pathological words is inside `Transduce::run`, matching lead #1's framing exactly. But the
   internal split shows the dominant cost is the **`distinct()` dedup step that runs once per
   `run()` call after traversal** (59–81% of total wall-clock), not the nondeterministic traversal
   loop that produces the candidates (14–30%). `distinct()` (`crates/hc-fst/src/traverse.rs`,
   bottom of file) is a plain `Vec`-scan: for each new raw `FstResult`, linearly scan everything
   kept so far and call `result_eq` (itself an O(register_count) elementwise compare) — this is
   `O(n × kept)` in the worst case, and `n` reached 327,360 and 501,025 in the two single worst
   calls observed. C#'s `Enumerable.Distinct(IEqualityComparer)` is hash-set-backed (O(n)
   amortized); `Register` (`crates/hc-fst/src/lib.rs`) already derives `Hash + Eq`, and its
   `value_eq` degenerates to plain structural equality in the cases actually constructed (`unset()`
   always produces the same bit pattern), so `FstResult` looks hashable in practice. This is a
   strong, concrete, plausible root mechanism for the ~2x-class gap — **flagging it for Fable, not
   fixing it** (out of this task's scope).
2. **Keep-longer dedup (`push_remove_duplicates`, `hc-rules/src/morph.rs`) — REFUTED as a cost
   driver for these two words.** `DEDUPPROF` shows negligible total time (111ms / 65ms, <0.1% of
   wall-clock on both) and `max_out_len=1` throughout — i.e. its candidate lists never grew past a
   single element before a duplicate was found and resolved, so there is no combinatorial blow-up
   happening in this specific function for either profiled word. Deprioritize this lead unless a
   different word's profile shows a different pattern.
3. **Template-battery interior memoization coverage (`--memo=on`) — INCONCLUSIVE by direct
   measurement, but a code-reading check found no gap.** Neither profiled word routed measurable
   time through anything outside `Transduce::run`/`distinct()`/`push_remove_duplicates` (the
   `det_ms`/non-`run_ms` remainder is negligible for both), so this instrumentation cannot speak to
   template-battery cost directly for these two words. Reading `hc-rules/src/stratum.rs`'s
   `run_template_batch` (line ~775) against `hc-memo/src/lib.rs`'s `AnalysisScope` shows Rust
   **does** have a dedicated `template_memo` table (separate from the mrule-cascade `memo` table),
   keyed by the same `AnalysisStateKey`, memoizing the **whole battery's output for a given key**
   with replay-on-hit (`entry.results.iter().map(|stored| stored.replay_onto(...))`) — this matches
   C#'s `TemplateMemo` at the same granularity ("one level of template outputs" per key, not
   memoizing individual templates within the battery), which the code's own comments describe as
   intentional parity, not a known shortfall. Given the profiling data shows the whole bottleneck
   for these two words lives in `Transduce::run`, this lead looks like a dead end for the
   pathological-word class specifically — deprioritize relative to lead #1.

## Gaps / things not done (flagged, not chased further)

- No live C# timing for በመጨረሻ (see above — the historical golden run never completed it either;
  running it live risked an open-ended time cost for uncertain payoff).
- No true sampling profiler trace (line/instruction-level) was obtainable on this system — see
  "Tooling available" above. The `distinct()`/`nondet` split is real (measured, not inferred), but
  a proper profiler would let Fable see exactly which lines inside `distinct()`/`result_eq` cost
  the most (almost certainly the `registers.iter().zip(...)` compare and/or the outer `Vec::iter()`
  linear scan itself) rather than inferring from call counts and Big-O reasoning.
- C#'s `--rule-stats` millisecond-resolution attribution bottoms out at the whole-stratum level for
  ሌባዬ (`Analysis > Morphology`, 63,421 of 64,037ms) with no deeper breakdown, so it could not be
  used to independently confirm whether C#'s own FST-matching layer shows an equivalent (but
  smaller, hash-backed) cost shape — that would need instrumenting the C# `Matcher`/`Fst.Transduce`
  directly, out of scope here.
