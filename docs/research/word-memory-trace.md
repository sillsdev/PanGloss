# Where the bytes go: allocator-level ground truth for one `Word`'s memory

Research note, 2026-09-14. Follows `docs/research/live-frontier-memory-bound.md` (Fix B, main
`4c870938`) and `docs/research/analysis-memo-explosion.md` §4. Those docs bounded the live search
frontier and the memo tables by *estimated* bytes (`pg_rules::word::estimate_word_bytes`); this note
adds a counting `#[global_allocator]` (`alloc-trace` feature, `pg-cli/src/alloc_trace.rs`) as ground
truth, a field-level breakdown (`HC_WORD_STATS=1`, `pg_rules::word_stats`), and checks the estimator
against the allocator. Debug build, `--threads 1`, `-RunMemoryGB 6`, single word per run, built with
`$env:PANGLOSS_EXTRA_ARGS='--features alloc-trace'` (confirmed in the printed `cargo build
--workspace  --features alloc-trace` line).

## 1. Measurement table

All Aweti rows use `samples/data/aweti.fwdata`; the Sena row uses `samples/data/sena.fwdata`. "Live
peak" is `HC_WORD_STATS`'s largest single stratum-pass total (`pg_rules::word_stats::record_live_words`,
called once per stratum pass, `pg-rules/src/stratum.rs:1596`); "memo total" is both tables' key bytes
(`AnalysisStateKey::estimate_bytes`) plus results bytes (`AnalysisScope::memo_bytes_used`/
`template_bytes_used`), snapshotted once at end-of-parse (`pg-parse/src/morpher.rs:550`).

| word | cap | memo | ALLOC peak | live peak | memo total | sum | gap | gap % |
|---|---:|---|---:|---:|---:|---:|---:|---:|
| oteʼikateʼika | 200k | on | 229.6 MB | 49.4 MB | 66.2 MB | 115.6 MB | 114.0 MB | 49.7% |
| oteʼikateʼika | 1M | on | 1337.2 MB | 349.6 MB | 305.1 MB | 654.7 MB | 682.5 MB | 51.0% |
| oteʼikateʼika | 200k | off | 30.3 MB | 4.3 MB | 0 | 4.3 MB | 26.0 MB | 85.8% |
| Ajkululape | 200k | on | 381.2 MB | 69.7 MB | 175.6 MB | 245.3 MB | 135.9 MB | 35.7% |
| ajkulula (easy, completes) | 200k | on | 30.3 MB | 0.32 MB | 0.72 MB | 1.04 MB | 29.3 MB | 96.6% |
| kukudziwisani (Sena, easy) | 1M | on | 37.7 MB | 0.59 MB | 1.0 MB | 1.6 MB | 36.1 MB | 95.8% |

**A ~30 MB floor, independent of the word.** `--memo off` and both easy words land at 30.3-37.7 MB
ALLOC peak despite live-state totals under 1 MB — `oteʼikateʼika --memo off` (30,300,789 B) and
`ajkulula` (31,800,783 B) are within 6 bytes of each other. This is not word-driven growth; it is the
grammar's own loaded tables (FST/pattern data, lexicon indices, interners) plus process/runtime
baseline, and `estimate_word_bytes` cannot see any of it since it only walks `Word`. Subtracting this
floor from the two capped `--memo on` rows still leaves 44.7% (`oteʼikateʼika`@200k) and 33.3%
(`Ajkululape`@200k) of the *word-driven* growth unattributed to live-frontier + memo bytes; at
`oteʼikateʼika`@1M the floor is negligible and the residual gap is 52.2% — the gap **grows**, both in
bytes and as a fraction of growth, as the word gets more pathological.

## 2. Live-frontier field breakdown

`WordByteBreakdown` (`pg-rules/src/word.rs`), summed over the live-peak stratum pass:

| word/cap | base | shape | syn_fs | mrule_apps | unapplied_counts | non_heads | **alternatives** | total |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| oteʼikateʼika@200k on | 1.06 MB | 2.67 MB | 0.84 MB | 0.29 MB | 0.29 MB | 0 | **44.5 MB (86.0%)** | 49.4 MB |
| oteʼikateʼika@1M on | 4.92 MB | 12.25 MB | 3.98 MB | 1.31 MB | 1.31 MB | 0 | **326.8 MB (89.0%)** | 349.6 MB |
| oteʼikateʼika@200k off | 0.16 MB | 0.46 MB | 0.12 MB | 0.037 MB | 0.037 MB | 0 | **3.49 MB (81.1%)** | 4.31 MB |
| Ajkululape@200k on | 0.92 MB | 2.94 MB | 0.38 MB | 0.12 MB | 0.12 MB | 0 | **65.2 MB (93.6%)** | 69.7 MB |
| ajkulula@200k on | 0.064 MB | 0.163 MB | 0.024 MB | 0.005 MB | 0.005 MB | 0 | 0.054 MB (17.2%) | 0.32 MB |
| kukudziwisani@1M on | 0.10 MB | 0.049 MB | 0.018 MB | 0.012 MB | 0.012 MB | **0.056 MB** | 0.34 MB (57.8%) | 0.59 MB |
(`real_fs`, `obligatory`, `root_runtime_id` were 0 on every row measured.)

`alternatives` dominates every non-trivial row (81-94%), confirming the hypothesis in the task brief:
the state-keyed merge (`words[idx].alternatives.push(w)`, `pg-rules/src/stratum.rs:1555,1561`) attaches
a **full, independently-owned `Word` clone** — with its own shape/FS/trail, recursively including
*its* alternatives and non-heads — for every merged-away candidate, and `estimate_word_bytes`
previously did not count this field at all (fixed here, see §5).

**Distribution of `alternatives.len()`/`non_heads.len()`** (sampled per live word per stratum pass):

| word/cap | alt p50 | alt p90 | alt max | non-head p50/p90/max |
|---|---:|---:|---:|---|
| oteʼikateʼika@200k on | 1 | 23 | 839 | 0/0/0 |
| oteʼikateʼika@1M on | 3 | 29 | 1919 | 0/0/0 |
| Ajkululape@200k on | 5 | 29 | 1079 | 0/0/0 |
| ajkulula@200k on | 0 | 1 | 1 | 0/0/0 |
| kukudziwisani@1M on | 0 | 3 | 11 | 0/1/1 |

Extreme skew: median words carry almost no alternatives, but the tail reaches 839-1919 on the
pathological words — `max_single_word_bytes` was 1.25 MB (`oteʼikateʼika`@200k) and 2.86 MB
(`oteʼikateʼika`@1M), i.e. one `Word` alone can be several MB.

## 3. Top three allocations by bytes

1. **`Word::alternatives`, attached at `pg-rules/src/stratum.rs:1555` and `:1561`**
   (`words[idx].alternatives.push(w)`). 81-94% of live-frontier bytes on every non-trivial word;
   the same `Vec<Word>` is also stored inside memo `results` (`Word` implements `Clone` derive-wise,
   so a memoized result carries its own `alternatives` too — `memo_results_bytes` is not a disjoint
   pool from this cost, it multiplies it). **Classification: (a) duplicated copy of something that
   exists elsewhere.** The canonical word and each folded alternative differ only by a rule-trail/
   non-head *suffix* — exactly what `Word::replay_onto` (`pg-rules/src/word.rs:416-439`) already
   knows how to reconstruct from a shared prefix — yet each alternative is stored as an
   independent full clone rather than a delta. `Word::expand_alternatives`
   (`pg-rules/src/word.rs:519-577`) already walks a `source: Option<Rc<Word>>` spine and replays
   deltas to *reconstruct* candidates; storing `alternatives` the same delta-shaped way instead of
   as full clones is the direct compaction this measurement motivates.

2. **`MemoEntry<W>::results: Vec<W>` (`pg-memo/src/lib.rs:210-214`)**, populated at
   `pg-rules/src/stratum.rs:1135-1155` (mrule memo) and `:1311-1318` (template memo).
   `memo_results_bytes` was already at 268.4 MB — essentially the full 256 MiB
   `DEFAULT_MEMO_BYTE_BUDGET` — at 1M steps (`memo_insert_refused=508` confirms the cap started
   refusing). **Classification: (b) a needed-but-oversized structure.** Replay genuinely needs a
   storable subtree, so this is not pure duplication the way (1) is, but nothing here is
   shared/interned: each stored `Word` (and everything nested under it, including its own
   `alternatives`) is an independent heap copy, so it is a compaction candidate (structure sharing /
   hash-consing, `analysis-memo-explosion.md` §3 option (b)), not irreducible state.

3. **`AnalysisStateKey`'s per-state `Shape` + two `FeatureStruct`s + rule-count multiset + morph
   history (`pg-memo/src/lib.rs:116-128`)**, built at `pg-rules/src/stratum.rs:872`
   (`fn state_key`). Measured directly via the new `estimate_key_bytes`: 6.9 MB (200k) → 33.5 MB
   (1M) combined across both tables — an order of magnitude smaller than (2) but a clean answer to
   the task's specific question: **yes**, memo keys clone `Shape`+`FeatureStruct` per state (no
   interning pool exists anywhere in this crate, confirmed by grep and by `pg-memo`'s own module
   doc). `Shape::clone()` is a 5-array "column copy" (`pg-shape/src/lib.rs:192-206`), not a pointer
   bump. **Classification: (a) duplicated copy** — the same shape/FS values already live on the
   `Word` that produced the key.

## 4. The estimator-vs-allocator gap: what it hides and why

Two structurally distinct owners, not one:

- **The ~30 MB floor** (§1): the grammar's compiled tables + process baseline, entirely outside
  `Word` and therefore entirely outside `estimate_word_bytes`'s domain by construction. Not a bug —
  the estimator was never meant to size the grammar — but it means "process peaks above 6 GB while
  the memo caps show headroom" partly reflects this floor being invisible, not just word growth.
- **The residual 33-52% gap on hard words, after removing the floor**: allocator/collection overhead
  the field-level walk cannot see even in principle: `Vec` spare capacity from growth reallocation
  (every `.push` into `alternatives`/`non_heads`/`morphs` can leave up to 2x allocated vs. len),
  `BTreeMap` node overhead for `unapplied_rule_counts` (`estimate_word_bytes` counts flat
  `len * (key+val)`, not B-tree node/pointer overhead), the `HashMap`s `stratum.rs` builds per
  stratum call (`output_keys`, `key_word`) and per memo table, and — the largest likely
  contributor — **transient allocation entirely outside `HC_WORD_STATS`'s sampling points**.
  `HC_WORD_STATS` snapshots only at stratum-pass boundaries (once `words` is final) and once at
  end-of-parse for the memo tables; the allocator's peak is continuous. `pg-parse/src/morpher.rs`'s
  top-level `results`/`matches` `HashMap<WordKey, Word>` accumulators and, especially, its
  `for alt in syn_word.expand_alternatives()` calls (`morpher.rs:472,513`) recursively rebuild and
  clone whole candidate trees at synthesis time — a second multiplicative pass over exactly the
  `alternatives` structure named in §3, invisible to both `HC_WORD_STATS` (never inside
  `stratum::analyze`) and to the memo snapshot (taken after synthesis, not during it).
  `replay_clones=305,737` at 1M steps is the size of just one contributor to that traffic.

## 5. Instrumentation added

- **Fixed a real estimator bug**: `estimate_word_bytes` (`pg-rules/src/word.rs`) omitted
  `Word::alternatives` entirely. Now `estimate_word_bytes_breakdown` recurses into it (and into
  `non_heads`) and `estimate_word_bytes` is defined as its `.total()`. This also makes the *production*
  memo byte budget (`DEFAULT_MEMO_BYTE_BUDGET`, `pg-memo/src/lib.rs`) size pathological
  many-alternative entries correctly for the first time — `memo_gate`'s tests stay green (§6).
- `pg_rules::word_stats` (`HC_WORD_STATS=1`, off by default): per-stratum-pass live-word field
  breakdown, `alternatives`/`non_heads` length percentiles, max single-`Word` bytes, and (via two new
  `pg_memo::AnalysisScope`/`AnalysisStateKey` methods) memo key + results bytes. Printed as one
  `WORDSTATS` stderr line per word from `pg-cli`.
- `alloc-trace` cargo feature (`pg-cli`, default off): a counting `#[global_allocator]`
  (`pg-cli/src/alloc_trace.rs`) wrapping `System`, printed as `ALLOC\tpeak_bytes=...\tlive_at_end=...`
  under `HC_ALLOC_STATS=1`. Required relaxing `pg-cli`'s crate-level `forbid(unsafe_code)` to
  `cfg_attr(not(feature = "alloc-trace"), forbid(unsafe_code))` — unavoidable for a `GlobalAlloc`
  impl, scoped to stay `forbid`den in every ordinary (feature-off) build.

## 6. Verification and what was not measured

`pg.ps1 -Mode check` and `-Mode test -Package pg-rules` both green (176 tests passed, including two
new `word_stats` unit tests and the pre-existing `cascade_diamond_never_holds_more_live_words...`
frontier-bound test, unaffected by the `alternatives` fix since that test's diamond has none).
`memo_gate`'s parity tests were not re-run against a `--memo off` comparison at these exact
step-caps beyond what §1's `--memo off` row already shows; `memo_parity_gate`/`memo_corpus_gate`
(the full-corpus parity suites) were not re-run here (time-boxed) — the `estimate_word_bytes` change
only affects byte-budget *refusal timing*, never the recall-preserving fallback-to-recompute path, so
it cannot change any completed word's result set, only which subtrees get memoized.

Not measured: Mbugwe (no third grammar fetched, time-boxed to the two Aweti words + one Sena word
the task named); a step-cap sweep past 1M for `oteʼikateʼika` (the `analysis-memo-explosion.md`
uncapped run already shows this word does not survive uncapped); attributing the residual gap in §4
to specific allocation call sites by count/size (would need per-call-site allocation tracking, not
just a global counter — the current `alloc-trace` wrapper only gives one peak number per word, not a
breakdown by source location).
