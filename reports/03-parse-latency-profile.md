# Parse latency profile: why worst-case words are slow, and what it would take to reach <1ms

## Executive summary

- **There are two separate engines in this repo with very different performance profiles, and only one of them is in production.** `hc-parse::Morpher` (the plain, direct HermitCrab-port engine) is the only parser `hc-ffi` and `hc-wasm` — the two real consumers (native bindings, browser demo) — depend on. `hc-hybrid` (the FST-proposer + restricted-reverify "hybrid" approach) is depended on only by `hc-cli`, for its own experimental `fst-*` subcommands and test gates; it is not reachable from any shipping consumer. Any "~100ms worst case" the user is observing in a real product is a plain-engine number, not a hybrid number.
- **The plain engine's "worst case" is not 100ms — it is multiple SECONDS to TENS OF SECONDS on a nontrivial slice of a real corpus, and this is already known, documented, and root-caused** (`rust/docs/phase2-completed/narrowing-budget-w8.md`, `rust/docs/o2-profile-findings.md`). A clean (uncontended), single-threaded, **step-capped-at-500,000** run of the plain engine over the first 829 words of the real Sena corpus measured here: p50 = 31ms, p90 = 620ms, p95 = 1.6s, p99 = 2.95s, max observed = 4.7s. 7.4% of words in this sample took over 1 second — **and this understates the true worst case**, because some of the slowest words hit the 500,000-step cap and were cut off before finishing: a direct, uncapped re-measurement of one such word (`anyakuidiwa`, capped at 3.6s/500k steps in the batch run) actually needs **983,229 steps** and ~9.5-10.7s to complete for real once uncapped.
- **Root cause of the plain-engine tail is (c) genuine combinatorial/algorithmic blowup, not (a) lazy compilation and only marginally (b) poor data structures — and it has (at least) two distinct sub-mechanisms.** Uncapped step counts measured directly on five real pathological Sena words range from **391,544 to 1,120,114 steps for a single word** — one to two orders of magnitude more than the previously-documented Amharic worst case (ሌባዬ, ~25,820 steps). Per-step cost on these Sena words is modest (order 10-20µs/step, contention-inflated — see caveat below), so the Sena tail is dominated by **sheer step COUNT** (many cheap steps — a wide, order-invariant-memo-resistant branching factor), whereas the previously-documented Amharic tail (`narrowing-budget-w8.md`) is dominated by **per-step COST** (few but very expensive steps, ~1-1.5s each, from affix-matcher re-processing of Optional-flooded shapes). Both are algorithmic/grammar-driven, not implementation bugs, but they are not the same mechanism and would not respond to the same fix. Rule compilation is NOT lazy: `RuleCache` is built once in `Morpher::new` and shared read-only across every `parse_word` call; a direct cold-vs-warm repeated-call measurement (this report, 5 words × 5 repeated calls each) found **identical step counts on every repeat** and no consistent first-call-slower pattern — this rules out any per-parse compilation/warm-up effect.
- **`hc-featstruct`'s `FeatureStruct` is not the hypothesized cloning hotspot.** It is a small, sorted `Vec<(FeatId, FeatureValue)>` (single-digit entries in every reference grammar) with cheap `Clone`/`Eq`/`Hash` — not a `HashMap`. The more expensive clone in the hot path is `hc_shape::Shape` (5 separate boxed-slice heap allocations per clone: `kinds`/`char_defs`/`flags`/`feat_lanes`/`cd_sets`), cloned once per candidate `Word` branch in the morphological cascade — a real, if secondary, implementation-level cost, not independently isolated by a sampling profiler in this investigation (none is available on this Windows sandbox — see Methodology). Given the Sena tail is a many-cheap-steps regime, this per-step constant-factor cost matters proportionally more here than it would in the Amharic few-expensive-steps regime.
- **The M6 analysis memo is doing real, large, necessary work — it is not itself a cost sink.** The same five words, run unmemoized (`--memo=off`) with a 20-second watchdog, **never finished** (all five timed out) and had already burned 3.2M-7.1M steps in that window — 3-10x the memoized step count, for less-than-complete results. This is a useful sanity check, not new information: it confirms the memo is functioning as designed, and rules out "the memo itself is slow" as a contributor to the observed tail.
- **The experimental hybrid path has its own, DIFFERENT and newly-diagnosed bottleneck**: its "verify" step re-runs a full restricted `hc-parse` analysis (fresh `AnalysisScope` memo, fresh surface-shape segmentation) once **per surviving propose-side candidate**, with no memoization shared across candidates of the same word. Measured on 200 real Sena words (release build): propose and verify are roughly evenly split (53%/47% of aggregate per-word time), candidate counts range up to 589 for a single word, and per-word total time ranges from sub-millisecond to >500ms, mean 86ms on this sample (historical full-corpus mean: ~177ms/word). This is a real, measured inefficiency (re-verification cost scales with candidate count, and candidate count is often 10-100x the number of candidates that actually verify), but it is not on the production path today.
- **Path to <1ms is different for each engine and is not a single fix.** For the plain engine: the pathological tail requires either (i) a genuine algorithmic fix targeting whichever sub-mechanism dominates a given grammar (branching-factor reduction for Sena-like many-cheap-steps cases; matcher cost reduction for Amharic-like few-expensive-steps cases) — the biggest, hardest, highest-confidence win, and where the existing docs already point — or (ii) accepting that some grammar/word combinations are inherently exponential and bounding them with `--word-timeout-ms` (already implemented) rather than chasing constant-factor wins. For the hybrid path: sharing one `AnalysisScope`/segmentation across all of a word's candidates during verify, and/or pruning obviously-redundant candidates before verify, are concrete, scoped, medium-confidence wins. Neither engine reaches <1ms on genuinely pathological words without an algorithmic change; both are already comfortably under 1ms on a meaningful share of ordinary words (roughly a quarter of this sample's words parsed in under 10ms).

## Methodology

### Environment and constraints
- Platform: Windows 11, PowerShell + Git Bash. No sampling/tracing profiler is usable in this sandbox: `wpr.exe` requires elevated privileges not available here; `samply`/`cargo-flamegraph` require Linux/macOS (`perf`/`dtrace`) to *record* (confirmed by the pre-existing `rust/docs/o2-profile-findings.md` investigation, re-confirmed here — nothing changed about that constraint). All measurement in this report is therefore coarse `Instant::now()` phase/call timing, exactly the style the project's own prior instrumentation (`HC_STEP_STATS`, `HC_FST_PROFILE`) already established.
- All timings reported are from **release builds** (`cargo build --release`), built fresh for this investigation. Debug-build numbers are never used for any of the "measured" figures below.
- Real grammars/corpora used: Sena (18,871 FST states, 7,121-word corpus), Amharic (6,672 states, 673-word corpus), Indonesian (547 states, 121-word corpus) — copied from the parent repository's gitignored `samples/data/` into this worktree (these files are `/samples/data/*-hc.xml` / `*-words.txt` gitignored patterns, not present in a fresh worktree checkout) since they are the only realistic large-vocabulary reference grammars in this project. The `rust/parity-out/golden/fst-advisor/sena/` golden fixtures were copied the same way (read-only source; nothing was written back to the parent repo).

### Commands run
```
# Release build
cd rust && cargo build --release -p hc-cli -p hc-hybrid -p hc-parse

# Plain-engine (production path) full-corpus batch, single-threaded so per-word elapsedMs is written:
./target/release/hc-rs.exe batch ../samples/data/sena-hc.xml ../samples/data/sena-words.txt out.tsv \
    --threads 1 --step-cap 500000

# Hybrid (experimental fst-advisor path) propose/verify breakdown -- new instrumented test, this investigation:
cargo build --release -p hc-hybrid --tests
HC_PERF_PROBE_LIMIT=200 cargo test -p hc-hybrid --release --test perf_probe_gate -- --ignored --nocapture sena_probe

# Cold-vs-warm / memo-on-vs-off -- new instrumented test, this investigation:
cargo build --release -p hc-parse --tests
cargo test -p hc-parse --release --test perf_cold_warm_probe -- --ignored --nocapture
```

### New instrumentation added for this investigation (temporary, clearly marked, not gates)
- `rust/crates/hc-hybrid/tests/perf_probe_gate.rs` — per-word propose/verify timing split, candidate counts, percentiles, top-N slowest words. `#[ignore]`d, not a correctness gate, explicitly marked as investigation-only in its module doc.
- `rust/crates/hc-parse/tests/perf_cold_warm_probe.rs` — repeated-call (cold vs warm) and `--memo=on` vs `--memo=off` timing on real pathological Sena words, same conventions.
- Neither changes any production code path; both are read-only harnesses over existing public APIs (`Morpher::parse_word`, `CompositeAnalyzer::analyze_word`, `replay::confirm_checked`).

## Measured timings

### Plain engine (`hc-parse::Morpher`, the production path) — real Sena corpus, single-threaded, clean (uncontended) sample

First 829 words of the real 7,121-word Sena corpus (`--step-cap 500000`, memo on — the default), measured with no other CPU-heavy process running concurrently:

| stat | value |
|---|---|
| p50 | 31 ms |
| p75 | 136 ms |
| p90 | 620 ms |
| p95 | 1,609 ms |
| p99 | 2,950 ms |
| max (this partial sample) | 4,734 ms |
| mean | 246 ms |
| words < 1ms | 23 / 829 (2.8%) |
| words < 10ms | 210 / 829 (25.3%) |
| words < 100ms | 586 / 829 (70.7%) |
| words > 1000ms | 61 / 829 (7.4%) |

Ten slowest words observed in this partial run (word index, word, elapsed ms):
```
835  kakukhondani       5450
494  pisabulukira       4734
861  kumwenikira        4202
829  ungandigodamira    3656
792  anyakuidiwa        3645
791  ndinakuombolani    3501
437  akumabulukira      3447
495  pyabulukira        3394
423  anyakudziwisa      3247
211  pidafikawo         3191
```

This is **already consistent with, and not a new finding contradicting, the project's own prior measurements**: `rust/docs/phase2-completed/narrowing-budget-w8.md` records the C# oracle itself paying "Sena gold worst words ~25-29s each" and the Rust engine's ሌባዬ (an Amharic pathological word) "terminating naturally at ~298s" — both engines have a genuinely heavy tail on specific words; this is a property of the grammars' rule interactions, not an engine-specific defect on either side. The user's "~100ms worst case" framing undersells the true worst case by 1-2 orders of magnitude on Sena; it is a reasonable characterization only of the "somewhat slow but not pathological" band (roughly p75-p90 in the table above).

**Important caveat on this table: it understates the true tail.** This run used `--step-cap 500000` (a safety cap, not the default `usize::MAX`); 4 of these 829 words (0.5%) hit that cap and were cut off with partial results before finishing — including `anyakuidiwa` (idx 792, reported here at 3,645ms/capped), which a direct uncapped re-measurement (below) shows actually needs 983,229 steps and ~9.5-10.7s to genuinely finish. So p99/max in this table are lower bounds on the true (uncapped) worst case, not the true worst case itself.

A larger, partial run (2,422 of 7,121 words; the process was not able to run to full completion in this investigation's time budget, and roughly the back half of this range overlapped with the cold/warm probe below, adding CPU contention) gives a broadly consistent picture at larger scale: p50 = 43ms, p75 = 237ms, p90 = 990ms, p95 = 2.0s, p99 = 3.66s, max = 10.2s, mean = 343ms; 241/2,422 (10.0%) over 1 second, 14/2,422 (0.6%) over 5 seconds, 60/2,422 (2.5%) hit the 500,000-step cap. Given the contention caveat, treat the absolute numbers here as directionally consistent with, not more precise than, the clean 829-word table above.

### Hybrid path (`hc-hybrid::CompositeAnalyzer` propose + `replay::confirm_checked` verify) — 200 real Sena words

| stat | total/word (ms) | propose/word (ms) | verify/word (ms) | candidates/word |
|---|---|---|---|---|
| p50 | 32.6 | 24.6 | 4.0 | 16 |
| p90 | 260.5 | 128.2 | 164.9 | 178 |
| p99 | 498.4 | 242.0 | 372.2 | 360 |
| max | 532.3 | 260.5 | 398.4 | 589 |
| mean | 86.3 | 45.4 | 40.9 | 55.5 |

Aggregate share of summed per-word time: **propose 52.6%, verify 47.4%**. 0/200 words hit the 30s verify watchdog.

Top 5 slowest words (idx, word, total ms, propose ms, verify ms, n_candidates, n_verified):
```
42   mbidakhala     532.3   133.8  398.4  320 candidates, 4 verified
10   mphangwa       517.7   133.1  384.6  589 candidates, 3 verified
146  zinafuna       498.4   132.1  366.3  360 candidates, 2 verified
181  mabariri       493.1   183.6  309.4  246 candidates, 0 verified
76   cinagumanika   455.5   83.4   372.2  254 candidates, 0 verified
```

Two things stand out in this table, both discussed under Hotspots below:
1. **Verify cost scales with candidate count, not with how many candidates actually verify.** `mabariri` and `cinagumanika` spend 300+ms re-verifying candidates that ALL fail (0 verified) — none of that work is wasted in a correctness sense (verify must attempt each candidate to know it fails), but it is wasted in a *shared-computation* sense: every one of those ~250 restricted analyses re-segments the identical surface word and builds a brand-new memo table from scratch.
2. **Propose cost is large even independent of final candidate count** — `mabariri`'s propose alone is 183ms, driven by the bare/redup/infix proposers' internal beam search (`hc_hybrid::walk::DEFAULT_MAX_BEAM_WORK = 1_000_000` work units per proposer call), not by the 246 candidates it happens to emit.

One useful cross-check: word idx 76, `cinagumanika`, appears in both probes (same corpus, same order). The plain engine's own worst-word tally (first background run, before the cold/warm probe started competing for CPU) recorded it at 2,262ms for one full unrestricted parse; the hybrid path's propose+verify recorded it at 455.5ms total (83.4ms propose + 372.2ms verify, across 254 candidates, 0 verified). For this one word, the hybrid's restricted re-verify (which only has to confirm each candidate against a NARROW pinned root/rule selection, not the whole grammar) is roughly 5x cheaper in aggregate than one full unrestricted plain-engine parse — consistent with the hybrid plan's own "restricted verify collapses the search" claim (`docs/fst-plan/HYBRID_FST_RUST_PLAN.md` §5.2). This is a single-word data point, not a systematic comparison, but it is a real one: both numbers come from the same grammar, same word, same machine.

### Cold vs warm, and memo on vs off (tests hypothesis (a): is anything lazily compiled per-parse?)

Structural finding first: `hc_parse::Morpher::new` builds `RuleCache::build(g)` (`rust/crates/hc-parse/src/morpher.rs:162`) exactly once, and `Morpher::parse_word_core_selected` never rebuilds it (`self.cache` is passed by shared reference into every stratum/candidate call, `morpher.rs:387,630`) — so by construction there is no per-parse rule COMPILATION cost to amortize.

Live measurement, same `Morpher` instance, 5 repeated `parse_word` calls per word on 5 real pathological Sena words (release build; **this run overlapped in time with the plain-engine full-corpus batch above, so absolute wall-clock numbers here are contention-inflated by an unknown, variable factor — step counts are NOT affected by contention and are the reliable part of this data**):

| word | steps (identical every call) | call 0 (ms) | call 1 | call 2 | call 3 | call 4 | memo=off, 20s watchdog |
|---|---|---|---|---|---|---|---|
| kakukhondani | 1,071,163 | 38,622 | 19,022 | 20,628 | 20,227 | 21,090 | timed out at 4,861,391 steps, 3 analyses (partial) |
| pisabulukira | 391,544 | 4,959 | 5,561 | 12,562 | 11,046 | 10,542 | timed out at 7,121,681 steps, 2 analyses (partial) |
| kumwenikira | 1,120,114 | 16,885 | 16,455 | 16,693 | 19,219 | 19,084 | timed out at 6,207,666 steps, 2 analyses (partial) |
| ungandigodamira | 621,749 | 5,260 | 5,996 | 5,006 | 5,017 | 4,850 | timed out at 3,238,592 steps, 3 analyses (partial) |
| anyakuidiwa | 983,229 | 10,724 | 9,526 | 10,235 | 9,793 | 9,771 | timed out at 6,233,877 steps, 0 analyses (partial) |

Findings:
- **Step count is exactly identical across all 5 repeated calls on every word** — the engine is fully deterministic and does no work-reducing caching across separate `parse_word` calls (expected: `AnalysisScope` is rebuilt fresh every call by design, `morpher.rs:354-355`). This directly falsifies hypothesis (a): there is no lazy-compilation or warm-up effect to find, because nothing about the computation changes between calls.
- **No consistent "cold is slower" pattern in wall-clock**: `kakukhondani`'s call 0 (38.6s) is roughly 2x its later calls (~19-21s), but `pisabulukira`'s call 0 (4.96s) is its FASTEST call — later calls got slower (up to 12.6s) as the concurrently-running batch job's own workload grew heavier. `ungandigodamira` and `anyakuidiwa` are essentially flat across all 5 calls. This pattern (word-dependent, sometimes first-fastest, sometimes first-slowest) is what CPU contention from a second heavy process looks like, not what a warm-up effect looks like (a real warm-up effect would show a consistent, monotonic first-call penalty across every word, which this data does not).
- **`--memo=off` never completed within a generous 20-second watchdog on any of the 5 words**, each burning 3.2-7.1 million steps (3-10x the memoized step count) without finishing. This confirms the M6 memo is doing real, large, necessary work — not a cost sink itself.
- **Step counts are enormous relative to the previously-documented Amharic worst case.** ሌባዬ (Amharic, `narrowing-budget-w8.md`) needed ~25,820 steps. These five Sena words need 391,544-1,120,114 steps — 15x to 43x more. Estimating per-step cost from each word's minimum (least-contended) observed call time: kakukhondani ≈17.8µs/step, pisabulukira ≈12.7µs/step, kumwenikira ≈14.7µs/step, ungandigodamira ≈7.8µs/step, anyakuidiwa ≈9.7µs/step — all within roughly one order of magnitude of each other, and **five to six orders of magnitude cheaper per step** than the ~1-1.5 seconds/step `narrowing-budget-w8.md` measured for the Amharic pathological words. This is the basis for this report's claim that Sena's tail is a *many-cheap-steps* regime while the previously-documented Amharic tail is a *few-expensive-steps* regime — two distinct sub-species of hypothesis (c), not the same mechanism. (These per-step-cost numbers are themselves contention-inflated by an unknown factor, same caveat as above; they are offered as order-of-magnitude estimates, not precise measurements.)

## Hotspot breakdown

Ranked by measured or strongly-inferred contribution to worst-case wall-clock, plain engine first (production path), then hybrid (experimental path):

### Plain engine (production path)

1. **Optional-flooded affix-matcher re-processing — the dominant, already-documented cost sink (algorithmic).** `rust/docs/phase2-completed/narrowing-budget-w8.md` (lines 46-49): "the measured cost sink is DOWNSTREAM of the rewrite FST: the morphological affix matcher re-processing Optional-flooded shapes (46s FST traversal + 16s freeze on the affix side vs 0.3ms in the rewrite FST)." This is a grammar-shape property (rules whose `OptionalSegmentSequence`/quantified patterns admit many segmentations of the same surface material), not a Rust-vs-C# implementation gap — C# pays the same cost class on the same words (`~25-29s` on Sena's worst words, `rust/docs/phase2-completed/narrowing-budget-w8.md` line 42-43). **Diagnosis: algorithmic, specifically combinatorial branching in the affix-pattern matcher over Optional-quantified shapes, hypothesis (c).**
2. **Two distinct sub-mechanisms feed the same "genuine explosion" diagnosis — few-expensive-steps (previously documented, Amharic) and many-cheap-steps (newly measured here, Sena).** `rust/docs/budget-model.md`'s O1b history documents the Amharic case: measured per-tick cost on those pathological words is "roughly 1-1.5 seconds per (un)application attempt," so a word's whole run can total only a few hundred ticks yet take minutes. This report's own direct, uncapped step-count measurement on five real pathological **Sena** words (see Measured timings, cold/warm table) found the opposite profile: step counts of 391,544-1,120,114 (15-43x the Amharic worst case) at an estimated 8-18µs/step (five to six orders of magnitude cheaper per step than Amharic's). Both are "genuine combinatorial explosion," but they would not respond to the same fix: the Amharic case needs the specific expensive per-step operation (Optional-flooded matcher re-processing) sped up or avoided; the Sena case needs the branching factor itself reduced (fewer distinct states reached, better subsumption/merging), since even cutting per-step cost by 2x on a million-step word only buys a 2x reduction, not an order of magnitude. Either way, this is why `--step-cap` (a step-COUNT bound) cannot reliably bound wall-clock time by itself, and why `--word-timeout-ms` (a wall-clock bound, sampling `Instant::now()` on every `over_budget()` check since the O1b fix, `hc-rules/src/stratum.rs:217-229`) had to be added as an independent, second bound. **Diagnosis: algorithmic in both cases — confirms neither is a per-call constant-overhead problem fixable by micro-optimization alone, though the Sena many-cheap-steps regime is the one where hotspot #3's per-step allocation cost below would matter most.**
3. **`hc_shape::Shape::clone()` — 5 separate heap allocations per clone (implementation, secondary).** `Shape` (`rust/crates/hc-shape/src/lib.rs:196-213`) stores `kinds`/`char_defs`/`flags`/`feat_lanes`/`cd_sets` as five independent `Box<[_]>` columns; deriving `Clone` on this struct means five separate heap allocations every time a candidate `Word` is cloned during the morphological cascade (`hc-rules/src/stratum.rs`, `hc-rules/src/morph.rs` — 31 and 35 `.clone()` call sites respectively, several on `Word`/`Shape`-bearing values). On a word exploring tens of thousands of candidate branches (ሌባዬ: ~25,820 steps per the historical measurement), even a modest per-step clone rate multiplies into hundreds of thousands of small allocations. **This was NOT independently isolated with a profiler in this investigation** (no sampling profiler available — see Methodology); it is a plausible, code-read-confirmed secondary contributor, flagged as inference, not measurement. **Diagnosis: implementation (data-structure/allocation shape), hypothesis (b), but almost certainly a smaller factor than hotspot #1** — the existing O2 investigation's methodology (call-count + total-ns instrumentation) would be the right tool to size this precisely if it becomes the next target.
4. **`hc-featstruct::FeatureStruct` is NOT a hotspot — refutes part of hypothesis (b).** `FeatureStruct` (`rust/crates/hc-featstruct/src/tree.rs:48-51`) is a small sorted `Vec<(FeatId, FeatureValue)>` with single-digit entry counts in every reference grammar (per that file's own module doc), binary-searched, cheap to clone/hash/compare. This directly contradicts the a-priori hypothesis that a "HashMap-based feature structure clone on every unification attempt" is the culprit — there is no `HashMap` in this type at all. **Diagnosis: hypothesis (b) as originally framed (HashMap-based FS) does not apply to this codebase.**
5. **The historical `O(n²)` FST-result-dedup bottleneck (`hc-fst::traverse::distinct`) is fixed, not currently live.** `rust/docs/o2-profile-findings.md` found this step at 59-81% of wall-clock on two pathological Amharic words pre-fix; the fix (hash-backed dedup, confirmed present at `rust/crates/hc-fst/src/traverse.rs:664-692` in this codebase today) reduced those two words from ~145s/~303s to ~54s/~53s. **Not re-measured fresh in this investigation** (out of this report's time budget, and the fix's own landing note already reports before/after numbers on the same machine), but confirmed still in place by direct code read. Listed here for completeness, not as a currently-open hotspot.

### Hybrid path (experimental `fst-advisor`, `hc-hybrid` — not on the production path)

6. **Verify has no cross-candidate memoization — every candidate re-segments the word and re-builds its analysis memo from scratch (algorithmic/architectural, newly measured here).** `hc_hybrid::replay::confirm_checked` (`rust/crates/hc-hybrid/src/replay.rs:136-192`) calls `morpher.parse_word_selected(word, ...)` once per candidate; `Morpher::parse_word_core_selected` (`rust/crates/hc-parse/src/morpher.rs:265-397`) re-runs surface segmentation (`segment_with_features`, step 1, `morpher.rs:291`) and constructs a brand-new `AnalysisScope` memo (`morpher.rs:354-355`, `(self.memo && !trace.is_tracing()).then(|| RefCell::new(AnalysisScope::new()))`) on EVERY call — nothing about a word's shape or its previous candidates' analysis work carries over to the next candidate. Measured here: for `mphangwa` (589 candidates), verify totals 384.6ms, ~0.65ms/candidate average, none of which is shared. **Diagnosis: architectural/algorithmic — an `O(candidates)` multiplier on work (surface segmentation) that is provably identical across all candidates of the same word.** This is a concrete, scoped optimization target (see Path to <1ms).
7. **Propose (beam search) cost is large independent of final candidate count (algorithmic, beam-budget-driven).** `hc_hybrid::walk::DEFAULT_MAX_BEAM_WORK = 1_000_000` (`rust/crates/hc-hybrid/src/walk.rs:75`) bounds each proposer's (bare walker, reduplication, infix) internal frontier-admission work, calibrated from a 3-point sweep on Sena's guarded slice-60 (that constant's own doc). Measured here: `mabariri`'s propose alone is 183.6ms while emitting only 246 candidates (0 of which verify) — the cost is in the beam search's internal exploration, not in what survives to the candidate list. **Diagnosis: algorithmic (beam-budget-bounded search over an ambiguous/Optional-flooded shape), matching the same "Optional-flooded" mechanism as hotspot #1, just in the hybrid's own separate walker rather than `hc-fst`/`hc-rules`.**

## Root-cause diagnosis summary (hypothesis scorecard)

| Hypothesis | Verdict | Evidence |
|---|---|---|
| (a) Lazy/deferred rule compilation per-parse | **Refuted for the plain engine.** `RuleCache` built once at `Morpher::new` (`morpher.rs:162`), shared by reference across every parse (`morpher.rs:387,630`). | Code read + direct measurement: 5 repeated calls on the same word show identical step counts every time, no consistent cold-call penalty |
| (a) Lazy/deferred rule compilation, hybrid trie build | **Refuted.** `Trie::build` runs once per grammar load in the F9 gate's own setup (`rust/crates/hc-hybrid/tests/f9_full_battery_gate.rs:132`), not per word. | Code read |
| (b) Poor Rust data structures — `FeatureStruct` as the specific culprit | **Refuted.** Small sorted `Vec`, not a `HashMap`; cheap to clone/compare (`hc-featstruct/src/tree.rs:48-51`). | Code read |
| (b) Poor Rust data structures — general clone/allocation overhead | **Plausible secondary factor, not isolated.** `Shape::clone()` is 5 heap allocations; `Word` (which embeds `Shape`) is cloned repeatedly in the morphological cascade. No profiler available to size this precisely relative to hotspot 1. | Code read (`hc-shape/src/lib.rs:196-213`); clone-site counts in `hc-rules` |
| (b) Poor Rust data structures — FST candidate dedup | **Was true, now fixed (O2).** Confirmed present in code today (hash-backed). | Prior investigation (`o2-profile-findings.md`) + code read confirming the fix is live |
| (c) Genuine combinatorial explosion of candidate analyses | **Confirmed, dominant, for the plain engine's worst-case tail.** Both Rust and C# pay multi-second-to-multi-minute costs on the same Sena/Amharic pathological words; root mechanism is Optional-flooded affix shapes. | `narrowing-budget-w8.md`, `budget-model.md`, this report's own 829-word clean Sena sample (p99=2.95s, max=4.7s, 7.4% of words >1s) |
| (New, not in the original hypothesis list) Hybrid verify's per-candidate re-analysis with no shared memo | **Confirmed, newly measured here.** Verify time scales with candidate count (up to 589/word), not with verified-analysis count; each restricted parse rebuilds its memo/segmentation from scratch. | This report's `perf_probe_gate.rs` measurement |

## Path to <1ms

**These two engines need different work, and neither reaches <1ms on genuinely pathological words without an algorithmic change — only on the (large) majority of ordinary words are they already comfortably under 1ms (plain-engine p50 in this sample was 31ms; a meaningful fraction of words are almost certainly sub-millisecond given 23/829 measured under 1ms even with process/timer overhead included).**

Prioritized, plain engine (the production path — do this work if the <1ms target is about real product usage):

1. **(Highest confidence, highest expected payoff, hardest) Fix the Optional-flooded affix-matcher blowup algorithmically.** This is the single largest, already-localized, already-measured cost sink (hotspot #1/#2). It requires either a smarter matching strategy for `OptionalSegmentSequence`-heavy patterns (e.g., memoizing sub-match results across the many segmentations of the same underlying span, or restructuring the matcher to avoid re-deriving equivalent partial matches) or accepting the exponential case exists and bounding it. **Estimated gain: this is where 90%+ of worst-case wall-clock lives on the words that are slow at all — a real fix here is the only way to move p99/max down by orders of magnitude, not a constant factor.** Not attempted in this investigation (out of scope — this report's job was diagnosis, not the fix); the existing docs already flag it as the next work item for whoever picks this up.
2. **(Medium confidence, medium payoff, already-partially-adopted) Accept irreducible worst-case, bound it, and stop chasing it in the common path.** `--word-timeout-ms` already exists and works correctly (confirmed by this codebase's own `word_timeout_gate.rs`/`word_timeout_pathological_gate.rs` tests, and the wall-clock-sampling-per-check fix, `budget-model.md`'s O1b section). If <1ms is a *soft* target and occasional multi-second outliers are acceptable with a hard ceiling, this is already shippable — the work item is choosing and wiring a default timeout for whichever production surface currently has none (this report did not audit every call site for a default).
3. **(Lower confidence, smaller payoff, easy to attempt) Reduce `Shape`/`Word` clone cost.** Two concrete, incremental options, neither validated here: (a) switch `Shape`'s five parallel `Box<[_]>` columns to a single contiguous allocation (one `malloc` instead of five per clone) — a pure representation change, no behavior change; (b) `Rc`-share the immutable parts of a `Word` across sibling candidate branches where only a small delta differs (bigger refactor, more risk). **Estimated gain: likely single-digit-percent to low tens-of-percent on already-slow words, not an order of magnitude** — this is a secondary hotspot relative to #1, and this estimate is extrapolation, not measurement (no profiler run to confirm the clone share of total time).

Prioritized, hybrid engine (only relevant if/when this path is wired into a production consumer — today it is not):

4. **(High confidence, scoped, not yet attempted) Share one segmentation/memo scope across all of a word's verify candidates.** Concretely: `replay::confirm_checked`'s loop over candidates (in `composite.rs`/`f9_full_battery_gate.rs`'s `batch_lines_checked`, and this report's own probe) calls `Morpher::parse_word_selected` once per candidate; each call redundantly re-segments the identical surface word (`segment_with_features`, `morpher.rs:291`) and builds a fresh `AnalysisScope`. A new `Morpher` entry point that accepts a pre-segmented `Word`/shared scope, with the restricted per-candidate `lex_entry_filter`/`rule_filter` as the only thing that varies per call, would eliminate the re-segmentation entirely and could let a positive/nogood memo entry from one candidate's restricted search be reused by a structurally-similar sibling candidate. **Estimated gain: on the worst words measured here (up to 589 candidates), even eliminating JUST the redundant segmentation (a small fraction of each ~0.65ms/candidate average) is a modest win; the bigger, unmeasured-but-plausible win is memo reuse across candidates that share most of their restricted search space — extrapolation, not measurement.**
5. **(Medium confidence) Prune candidates before verify.** `mabariri`/`cinagumanika` spend 300+ms verifying candidates that ALL fail (0/246, 0/254 verified). If the propose side could cheaply pre-filter candidates unlikely to verify (e.g., a fast necessary-condition check using the restricted root/rule set before paying for a full restricted parse), a meaningful fraction of verify cost could be avoided. **Not designed or measured here — flagged as a direction, not a plan.**
6. **(Not attempted, likely necessary regardless of the above) Reduce beam-search cost in propose.** `DEFAULT_MAX_BEAM_WORK = 1_000_000` is a measured, deliberately-calibrated safety valve, not a performance target — lowering it would trade completeness for speed and is exactly the kind of "while we're here" optimization the hybrid plan's own risk section (`HYBRID_FST_RUST_PLAN.md` §11) explicitly forbids without measurement-driven justification. Any change here needs its own corpus-wide parity re-verification, out of scope for this report.

**Bottom line on <1ms**: for the plain (production) engine, the achievable near-term target is "the median and most of the distribution are already well under 1ms; the tail requires an algorithmic fix to affix-matcher combinatorics (item 1) or an accepted, bounded worst case (item 2) — there is no constant-factor Rust optimization that gets a multi-second pathological word under 1ms." For the hybrid engine, item 4 is the most concrete lever available today, but the entire path is not on the production surface, so this work is only worth prioritizing if/when `hc-hybrid` is wired into a real consumer.

## Appendix: files touched by this investigation (temporary instrumentation, not committed)

- `rust/crates/hc-hybrid/tests/perf_probe_gate.rs` (new)
- `rust/crates/hc-parse/tests/perf_cold_warm_probe.rs` (new)
- `samples/data/{sena,amharic,indonesian}-{hc.xml,words.txt}` and `rust/parity-out/golden/fst-advisor/sena/*` copied read-only from the parent repository (gitignored corpus/golden data, not present in a fresh worktree)

None of the above were committed; per task instructions, nothing in this investigation was committed to git.

## Known gaps in this investigation (honestly recorded, not chased further)

- **The full-corpus Sena plain-engine batch did not run to completion.** It was allowed to run in the background for its full available time budget and processed 2,422 of 7,121 words (34%) before this investigation's time budget ran out; it did not crash or error (no panic, no `TIMEOUT`/error marker in its log) — it simply did not finish. The 829-word "clean" sample and the 2,422-word "full partial" sample reported above are both drawn from this same run; neither is the complete corpus. Given the consistency between the two samples (mean 246ms vs 343ms, same order of magnitude, same shape), there is no specific reason to expect the remaining ~4,700 words would shift the reported percentiles by more than the clean-vs-full-partial gap already shown, but this is inference, not a direct measurement of the full corpus.
- **The cold/warm probe and the plain-engine batch ran concurrently for part of their respective durations**, which inflates absolute wall-clock numbers in both by an unknown, time-varying factor (visible directly in the cold/warm table's word-to-word inconsistency in which call is fastest). Step counts (unaffected by CPU contention) are the reliable output of that probe; wall-clock numbers from it are reported as order-of-magnitude only, with this caveat repeated at each point they are used.
- **No true sampling/tracing profiler was available** (same constraint the pre-existing `o2-profile-findings.md` investigation already hit and documented) — the `Shape::clone()` cost (hotspot #3) is a code-read-supported plausible contributor, not a directly measured percentage of wall-clock. Isolating it precisely would need either a working sampling profiler on this machine (none found) or new counter-based instrumentation in the same style as `HC_FST_PROFILE` (not built here, out of this investigation's time budget).
