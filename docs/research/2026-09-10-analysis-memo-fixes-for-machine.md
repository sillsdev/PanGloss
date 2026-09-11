# Analysis-memo fixes: what we found and what to port

Written for the maintainers of `sillsdev/machine` (C# HermitCrab). Measured 2026-09-11. See
`docs/research/analysis-memo-explosion.md` for the original OOM report and
`docs/research/2026-09-10-hc-analysis-fs-port-measurements.md` for the unrelated PriorityUnion
port this doc's "binary B" also carries.

## 1. Summary

`pangloss batch`'s analysis memo (`pg-memo`, porting C#'s `AnalysisScope`/`AnalysisStateKey`)
exhausted a bounded job object on Aweti words under an unbounded step cap, even though the
unmemoized path (`--memo off`) does not. Instrumenting the memo (rather than guessing) found three
independent causes: (1) the order-invariant key kept each rule's full unapplication count, though
exactly one reader (`count >= max_apps`) ever consumes it, so counts above that threshold split
states that behave identically; (2) `MemoEntry::results` is unbounded per entry — one Aweti state
stored 77,600 result words while the entry-count cap sat at 6% full; (3) a replayed memo hit was
cloned twice into the search accumulator. We fixed all three: saturate the key at each rule's
`max_apps`, add the retained-word budget C# already has (`MaxMemoWords`), add a byte budget C#
lacks, and clone a replay result once instead of twice — each proved recall-neutral by construction
and pinned by a test. Not fixed: one Aweti word (`oteʼikateʼika`) still exhausts memory at every
memo cap, because the *unmemoized cascade itself* — not the memo — retains every candidate word in
the live search frontier. That bound applies to C# too.

## 2. What C# already has, what the Rust port had before

C#'s `AnalysisScope` (commit `5d26fac6`, "Tighten memo resource bounds, diagnostics, and
parallelism cap") already keeps two count caps: `MaxMemoEntries = 100_000` and `MaxMemoWords =
1_000_000` (`AnalysisScope.cs:27-30`). `Store` (`AnalysisScope.cs:97-108`) refuses once either is
hit: `if (table.Count >= MaxMemoEntries || _storedWordCount > MaxMemoWords - results.Count)
return;`. Neither evicts — past either cap a subtree just goes unmemoized (`AnalysisScope.cs:22-23`),
costing time, never soundness. C#'s own comment names entry size as unbounded and the word budget
as "load-bearing" (`AnalysisScope.cs:22-26`) — exactly the finding below.
`AnalysisStateKey` (`AnalysisStateKey.cs:26-34`) keeps the *full* per-rule unapplication-count
dictionary, unsaturated.

The Rust port, before this round (commit `27cb405d`), had only the first of C#'s two caps:
`MAX_MEMO_ENTRIES = 100_000` (`pg-memo/src/lib.rs:236`, C#'s pre-tightening state) — no byte
budget, no key saturation, and a double clone on every replayed hit.

## 3. The fixes

### 3.1 Key saturation at each rule's `max_apps`

`state_key` (`stratum.rs:687-716`) keyed on shape, stratum, both feature structures, non-head
count, morph history, plus — until now — every `unapplied_rule_counts` entry verbatim; each count
is now capped (`count.min(max_apps)`), zero-saturated entries dropped (`stratum.rs:704-710`).
*Recall-safe:* the field has exactly one reader outside `state_key`, the `>= max_apps` gate
(`stratum.rs:758`); agreeing-once-saturated states decide identically everywhere. Pinned by
`unapplied_rule_counts_reader_gate.rs` (occurrence count: 2 in `stratum.rs`, 0 in `morph.rs`) and
`memo_gate.rs::state_key_saturates_unapplication_counts_past_max_apps`. *Measured:* not isolated
from the fixes below (landed together); the audit found the unsaturated key drove Ajkululape's
278,908 distinct states (`analysis-memo-explosion.md` §4). *C# proposal:* two call sites, not
one — `AnalysisAffixProcessRule.cs:45`, `AnalysisCompoundingRule.cs:46`, both
`>= _rule.MaxApplicationCount` — so the argument holds for both; `PinAndKey`
(`AnalysisStateKey.cs:26-46`) should saturate each `UnappliedRuleCounts` entry the same way before
hashing (its own doc comment, `:14-15`, already assumes this but the key doesn't enforce it).

### 3.2 Retained-word budget (parity with C#, already fixed here)

`MAX_MEMO_WORDS: usize = 1_000_000` (`lib.rs:239`), shared across both tables;
`has_memo_capacity`/`has_template_capacity` (`lib.rs:590-600`) admit only if
`stored_words + results_len <= MAX_MEMO_WORDS`. Added in `b9ff6365`, a direct C# port — before it,
only the entry-count cap existed. *Recall-safe:* same refuse-never-evict discipline
(`AnalysisScope.cs:22-23`). *Test:* `pg-memo::word_budget_refuses_a_store_past_the_cap_but_keeps_serving_existing_hits`.
*Measured:* the audit found one Aweti state storing tens of thousands of result words
(`results_len_max`) while the entry cap sat single-digit-percent full. *C# diff:* none in
substance — C# refuses (`_storedWordCount > MaxMemoWords - results.Count`), Rust admits
(`stored_words.saturating_add(results_len) <= MAX_MEMO_WORDS`), same inequality, opposite polarity,
one shared counter either way. No change proposed; already parity.

### 3.3 Per-table byte budget (no C# analog)

`DEFAULT_MEMO_BYTE_BUDGET: usize = 256 * 1024 * 1024` (`lib.rs:251`): `has_byte_capacity`/
`record_stored_bytes` (`lib.rs:562-586`) track accounted bytes per table, overridable via
`Morpher::with_memo_byte_budget` or `HC_MEMO_BYTES` (`pg-cli/src/main.rs:799-809`; `"none"`
disables it). The estimator, `pg_rules::word::estimate_word_bytes` (`word.rs:645-660`), sums
`size_of::<Word>()` plus shape/FS estimates and every rule-trail/non-head vector, recursing into
`non_heads` — already exists. *Recall-safe:* same stop-storing-never-evict discipline. *Test:*
`pg-memo::byte_budget_refuses_a_store_past_the_budget`, `::byte_budget_disabled_by_none_never_refuses`,
`memo_gate.rs::a_tiny_byte_budget_refuses_stores_but_preserves_memo_off_parity`. *Measured:* see
"Byte-budget margin" below — 256 MiB is the smallest tested budget keeping one Aweti word's step
count at its unbounded-memo value while cutting the pair's peak memory ~21% (2578 vs. 3259 MB).
*C# proposal:* `AnalysisScope.Store` has no byte check; add an `EstimateWordBytes` mirroring the
Rust estimator plus a `MaxMemoBytes` field. If that cost is a concern, word count is a defensible
proxy — the word budget alone still bounded the Aweti pair to ~3259 MB, so bytes are a refinement.

### 3.4 Single clone per replay

`OrderedDedup::add` (`stratum.rs:97-119`) took an owned `Word`, always called `out.add(r.clone())`
— a clone regardless of whether the key was already present. It now takes `&Word`, clones only on
first insertion, returns whether it inserted; the replay site counts a clone only when `out.add(r)`
returns `true`. `Word::replay_onto` (`word.rs:416-439`) still pays its own unavoidable clone
(`word.rs:422`) — never the bug; the bug was the second, unconditional clone into the flattened
accumulator. *Recall-safe:* pure clone-count change, dedup semantics unchanged. *Test:*
`stratum.rs::add_clones_only_when_novel`,
`memo_gate.rs::replaying_a_repeat_word_still_clones_at_least_once_per_result_and_matches_memo_off`
(`replay_clones >= hit_results_len`). *Measured:* `bd7ed6f7` added `hit_results_len_total` beside
`replay_clones` so a hit's clone cost is now directly comparable to its result count. *C# check:*
`AnalysisScope.TryReplay` (`:60-91`) clones the non-head prefix once per hit (`Word.cs:546-549`),
then `ReplayOnto` once per replayed result (`:511-543`, itself a clone); both consumers
(`AnalysisStratumRule.cs:203`, `MemoizedCombinationRuleCascade.cs:44`) use the returned `List<Word>`
directly, no further per-result clone. C# lacks this bug — no flattening accumulator shaped like
`OrderedDedup` — so there is no equivalent change, only this confirmation.

### 3.5 MEMOPROF diagnostics vs. C#'s

`pg_memo::profile` (`lib.rs:258-`, gated on `HC_MEMO_STATS=1`) is a thread-local, near-zero-cost
counter set: lookups, positive/nogood hits, inserts/refusals by cap (entry/word/byte), in-flight
depth, per-insert size samples, separately for the mrule and template memos; `pg-cli` prints one
`MEMOPROF` line per word (`main.rs:902-908`). `5d26fac6`'s "diagnostics" is the two resource caps
and a parallelism cap, documented structurally — no equivalent per-word counter set is exposed for
external measurement. *C# proposal (reversed direction):* add an opt-in counter snapshot to
`AnalysisScope`/`Morpher.ParseWord` — this doc's own key-saturation and byte-budget findings were
only discoverable *because* Rust had this instrumentation (an initial "all-nogood" hypothesis was
wrong; only the counters caught it). No test pins this — diagnostic-only, not correctness-bearing.

## 4. Measurement matrix

Debug builds (`-DebugProfile`; fat-LTO release crashes under the run-pool memory cap),
`--threads 1`, `-RunMemoryGB 6`, `HC_STEP_STATS=1`. **A** = pre-port (`c659ccaa`), **B** = upstream
PriorityUnion port only (`27cb405d`), **C** = port + all memo fixes (main, `0bbbe03c`). A predates
the `--step-cap unbounded` literal (default cap already `usize::MAX` there), so its rows pass
`18446744073709551615` instead. Noise caveat: other worktrees may build concurrently on this
machine; step counts and hashes are deterministic, wall time and peak WS are indicative only.

### Fixture 1 — Aweti single words, `--step-cap unbounded`, memo on

| Binary | Word | Exit | Wall (s) | Peak WS (MB) | Steps | Result |
|---|---|---:|---:|---:|---:|---|
| A | Ajkululape | 1 | 100.5 | 5518.3 | — | abort: `memory allocation of 1695936 bytes failed` |
| A | ekozokotu | 1 | 105.5 | 4971.3 | — | abort: `memory allocation of 1220679680 bytes failed` |
| A | oteʼikateʼika | 1 | 105.4 | 5799.7 | — | abort: `memory allocation of 1461768704 bytes failed` |
| B | Ajkululape | 0 | 65.2 | 708.9 | 283,572 | completes |
| B | ekozokotu | 0 | 75.3 | 3260.6 | 2,032,063 | completes |
| B | oteʼikateʼika | 1 | 115.1 | 5425.6 | — | abort: `memory allocation of 169201120 bytes failed` |
| C | Ajkululape | 0 | 65.2 | 669.0 | 283,572 | completes; MEMOPROF: inserts=3786, refused_bytes=1, results_len_max=22200, replay_clones=186278 |
| C | ekozokotu | 0 | 125.0 | 2577.6 | 7,877,597 | completes; MEMOPROF: inserts=17132, refused_bytes=159986, results_len_max=12552, replay_clones=201212 |
| C | oteʼikateʼika | 1 | 256.8 | 5348.6 | — | abort: `memory allocation of 676804480 bytes failed` |

A dies on all three words — it has neither the port's algorithm nor any bound past the
pre-tightening entry cap. B and C agree exactly on Ajkululape's and ekozokotu's step counts (the
fixes are step-count-neutral here); C's byte budget visibly refuses 159,986 ekozokotu inserts
(`memo_refused_bytes`), §3.3's mechanism.

### Fixture 2 — Aweti first 44 words, `--step-cap 200000`, memo on (C also off)

| Binary | Memo | Exit | Wall (s) | Peak WS (MB) | Capped | Steps | TSV SHA-256 |
|---|---|---:|---:|---:|---:|---:|---|
| A | on | 0 | 120.9 | 1378.6 | 24 | 5,388,284 | `d86474fc...00d8` |
| B | on | 0 | 126.1 | 447.5 | 19 | 4,576,759 | `08850d92...e15be1` |
| C | on | 0 | 125.7 | 447.7 | 19 | 4,576,759 | `08850d92...e15be1` |
| C | off | 0 | 55.1 | 40.5 | 24 | 5,423,628 | `d637f897...9515bdaf588` |

B and C agree byte-for-byte with memo on — the fixes changed neither recall nor step consumption
at this scale. C's memo-off row differs (different code path), as expected; parity is asserted by
the in-repo gates in §6, not by raw TSV equality here.

### Fixture 3 — Mbugwe first 60 words, `--step-cap 2000000`; Sena first 300 words, default step cap

| Binary | Corpus | Memo | Exit | Wall (s) | Peak WS (MB) | Capped | Steps | TSV SHA-256 |
|---|---|---|---:|---:|---:|---:|---:|---|
| A | Mbugwe 60 | on | 0 | 721.2 | 222.8 | 15 | 65,217,148 | `647feec6...ce11bb1` |
| B | Mbugwe 60 | on | 0 | 424.2 | 411.9 | 4 | 27,992,018 | `7227d416...c8f67aa92` |
| C | Mbugwe 60 | on | 0 | 432.3 | 412.4 | 4 | 27,992,018 | `7227d416...c8f67aa92` |
| C | Mbugwe 60 | off | 0 | 326.1 | 227.3 | 4 | 32,575,111 | `7227d416...c8f67aa92` |
| A | Sena 300 | on | 0 | 37.1 | 213.6 | 0 | 712,547 | `53222f27...0ec0375` |
| B | Sena 300 | on | 0 | 37.8 | 388.3 | 0 | 436,608 | `9150e40b...87b031ca9` |
| C | Sena 300 | on | 0 | 38.5 | 388.5 | 0 | 436,608 | `5ca4d754...651e9789174` |
| C | Sena 300 | off | 0 | 36.1 | 206.0 | 0 | 502,150 | `1534c231...41b5dca5` |

Sena: no word approaches any cap on any binary (0 capped throughout). Mbugwe: B, C-on, and C-off
all three share one TSV hash despite differing step counts — output row order is stable here
regardless of memo mode or cap set, unlike Sena. B and C-on agree exactly (step count and hash) on
both corpora; C-on vs. C-off differ in steps (expected, different code paths) but not in row order
for Mbugwe. Byte-for-byte TSV equality is stronger than the port needs; recall itself is what §6's
gates assert, both re-run and green on this branch (404 corpus cases, 0 divergences).

## 5. What remains: the live search frontier, not the memo

`oteʼikateʼika` aborts identically on all three binaries and at every tested byte budget (the
"Byte-budget margin" section below shows the same ~5000-5800 MB abort point at 16 MiB, 256 MiB, and
no budget). The memo is not what is growing: `memo_apply_rules_raw`, the unmemoized per-arrival
rule application that populates the search frontier *before* anything reaches a memo table,
accumulates every candidate `Word` it produces, unbounded. Every cap in §3 bounds what gets
*retained after* a rule application returns; none bounds the *in-flight* set one call can
construct. C# has the identical exposure: `MemoizedCombinationRuleCascade` sits on the same
unmemoized per-arrival expansion this port mirrors, and no `AnalysisScope` cap reaches code that
never inserts into `AnalysisScope`. This is the next bound both implementations need.

## 6. Test inventory

| Test | Crate | Guards | Fails if |
|---|---|---|---|
| `unapplied_rule_counts_reader_gate.rs` | pg-rules | §3.1 premise | a new unaudited reader appears |
| `memo_gate.rs::state_key_saturates_unapplication_counts_past_max_apps` | pg-rules | key saturation | equivalent states stop collapsing |
| `pg-memo::word_budget_refuses_a_store_past_the_cap_but_keeps_serving_existing_hits` | pg-memo | §3.2 | word budget stops refusing, or evicts an existing entry |
| `pg-memo::byte_budget_refuses_a_store_past_the_budget` / `::byte_budget_disabled_by_none_never_refuses` | pg-memo | §3.3 | byte accounting stops refusing, or `None` refuses |
| `memo_gate.rs::a_tiny_byte_budget_refuses_stores_but_preserves_memo_off_parity` | pg-rules | byte budget + parity | a tiny budget changes the identity set |
| `stratum.rs::add_clones_only_when_novel` | pg-rules | §3.4 | a repeat clones, or the first insert doesn't |
| `memo_gate.rs::replaying_a_repeat_word_still_clones_at_least_once_per_result_and_matches_memo_off` | pg-rules | §3.4 end to end | clones fall below result count, or the set changes |
| `memo_gate.rs::memo_on_equals_memo_off_unordered` / `::_with_template` | pg-rules | general parity | memo on/off diverge, synthetic or templated |
| `pg-parse/tests/memo_parity_gate.rs` | pg-parse | every conformance fixture (re-run: 67 fixtures, 690 words, 2318 identities, 0 diverge) | any word's set or `capped` flag differs |
| `pg-foma/tests/memo_corpus_gate.rs` | pg-foma | real corpora (re-run: Aweti 44@200k, Sena 300@50M, Mbugwe 60@2M — 404 cases, 0 diverge) | a both-completed word's set differs |

## How to reproduce the measurements

1. **In-repo parity gate** (synthetic, always runs, must be green before and after the fix):
   `rust/tools/pg.ps1 -Mode test -Package pg-parse -TestTarget memo_parity_gate`
   Every conformance-fixture word, memo on vs off: same deduplicated analysis-identity set, same
   `capped` flag.

2. **Corpus parity + survival gate** (needs `samples/data/`, or `PANGLOSS_CORPUS_ROOT` pointed at a
   checkout that has it):
   `rust/tools/pg.ps1 -Mode corpus-test -Package pg-foma -TestTarget memo_corpus_gate`
   Aweti first 44 words @ step cap 200,000; Sena first 300 @ default (50,000,000); Mbugwe first 60
   @ 2,000,000. Asserts identical analysis-identity sets for every word completing on both sides,
   and that a capped word is capped on both (a capped word's partial set is not compared -- step
   order legitimately differs, see `analysis-memo-explosion.md`).

3. **Wall time / peak memory**: build once, then measure each mode:
   ```
   rust\tools\pg.ps1 -Mode build -Bin pangloss -DebugProfile
   rust\tools\memo-measure.ps1 -Grammar <grammar> -Words <words.txt> `
     -Exe <build output>\pangloss.exe -StepCap <N> -MemoModes on,off
   ```
   Add `-Env @{ HC_STEP_STATS = '1' }` for a total-step column, and
   `-Env @{ HC_STEP_STATS = '1'; HC_MEMO_STATS = '1' }` for one `MEMOPROF` stderr line per word
   (lookups/hits/inserts/refusals by cap, insert-size samples, replay clones).

## Byte-budget margin

`pg_memo::AnalysisScope`'s per-table byte budget (`DEFAULT_MEMO_BYTE_BUDGET`) is a second guard
alongside the entry and word caps. This section records the sweep that set its value.

Debug build, `--threads 1`, `-RunMemoryGB 6`, `--step-cap unbounded`, budget swept via
`HC_MEMO_BYTES` (mirroring `HC_STEP_STATS`/`HC_MEMO_STATS`).

**Aweti** (`Ajkululape`, `ekozokotu` in one word file; `oteʼikateʼika` run separately since it
aborts before either budget matters -- see below):

| Budget | Ajkululape steps (x unbounded) | ekozokotu steps (x unbounded) | Peak WS (both words) |
|---|---|---|---|
| 16 MiB | 2,135,886 (7.53x) | 13,613,118 (6.70x) | 2249.7 MB |
| 64 MiB | 1,777,964 (6.27x) | 9,622,166 (4.74x) | 2331.7 MB |
| 256 MiB | 283,572 (1.00x) | 7,877,597 (3.88x) | 2578.2 MB |
| 1 GiB | 283,572 (1.00x) | 2,032,063 (1.00x) | 3262.8 MB |
| none (word cap only) | 283,572 (1.00x, unbounded baseline) | 2,032,063 (1.00x, unbounded baseline) | 3259.4 MB |

TSV SHA-256 (ms-stripped) is `866c5a3e872c61990ee1aefd73cc53128d0b83d2274a5db139565dfa1336e707` at
every budget above -- byte-identical output regardless of the byte budget.

`oteʼikateʼika` aborts with a Rust allocation failure at both tested extremes -- 16 MiB (peak
~4999 MB reached before the abort) and none (~5430 MB) -- so its failure is independent of the
memo byte budget. This matches the upstream port note that this word still aborts at 8 GB; it is a
pre-existing limitation this constant cannot fix (the memo bounds its own tables, not the
unmemoized cascade's own live working set).

**Sena** (300 words, default step cap) and **Mbugwe** (60 words, step cap 2,000,000): both
insensitive to the budget -- peak WS stays within a few MB across {64 MiB, 256 MiB, none} (Sena:
42.5-52.3 MB; Mbugwe: 411.7-419.4 MB, CAP=4 throughout), and the ms-stripped TSV SHA is identical
at every budget for each corpus. Neither corpus's words are large enough to exercise the byte
budget.

**Decision rule**: the smallest budget at which no word's step count exceeds 1.5x its
unbounded-memo count and every word stays under 2 GiB peak. No tested budget satisfies both --
even 256 MiB leaves `ekozokotu` at 3.88x steps and the pair's peak at 2578 MB. Falling back to
"prefer bounded memory and say what it costs": **256 MiB** is the smallest point where one word
(`Ajkululape`) matches its unbounded step count exactly and the other's ratio is best among the
meaningfully-bounding options (3.88x vs. 64 MiB's 4.74x and 16 MiB's 6.70x), cutting peak memory
~21% versus no budget (2578 vs. 3259 MB). Smaller budgets buy only marginal further memory
reduction (2331.7 MB, 2249.7 MB) at worse step costs; 1 GiB/none give no bounding effect at all.
Cost of the choice: a pathological word can still need several times its unbounded step count, and
peak memory for a genuinely adversarial word is not guaranteed under 2 GiB -- the byte budget
bounds the memo tables, not the process's total working set.
