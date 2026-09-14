# Designing `Word`'s memory shape: under 50 MB per word, recall unchanged

Research note, 2026-09-14. Read-only, worktree `mem-design` (`research/word-memory-design`, base
`4c870938`). Sibling worktree `mem-trace` does the byte measurements (`HC_ALLOC_STATS`, branch
`research/word-memory-trace`); this note is design only, no build, no run. C# sources are
`C:\Users\johnm\Documents\repos\machine\src\...` (read-only).

## 1. Byte anatomy of one `Word`

`Word` (`pg-rules/src/word.rs:183-288`) is 19 fields. Grouped by lifecycle:

| Field | Type | Cost shape | Lifecycle |
|---|---|---|---|
| `shape` | `Shape` (`pg-shape/src/lib.rs:195-206`) | per-node: 5 `Box<[]>` columns (`kinds` 1B, `char_defs` 4B, `flags` 1B, `feat_lanes` `feat_width*8`B, `cd_sets`) | (b) mutated by phonology rules |
| `syn_fs`/`real_fs` | `FeatureStruct` (`pg-featstruct/src/tree.rs:50-52`) | `Vec<(FeatId, FeatureValue)>`, ~24-32B/entry (`word.rs:624-638`); `EMPTY` is zero-capacity, no heap (`tree.rs:85-87`) | (b) |
| `mpr` | `MprSet(u64)` (`pg-grammar/src/model.rs:124`) | 8B, `Copy` | (b), free |
| `morphs` | `Vec<MorphRecord>` | ~40-48B/record (`word.rs:52-72`) | (c) report-only, also (d) in `WordKey` |
| `non_heads` | `Vec<Word>` | **recursive full `Word`**/element | (b)/(d), unbounded per compound depth |
| `mrule_apps` | `Vec<Option<MRuleId>>` | ~8B/slot, one per unapplication (depth ~15-24 on `oteʼikateʼika`, `live-frontier-memory-bound.md:19-26`) | (b)/(d) |
| `unapplied_rule_counts` | `BTreeMap<MRuleId,u32>` | node-allocated, high per-entry overhead | (d)-only: sole reader `pg_memo::AnalysisStateKey` (`word.rs:235-242`) |
| `obligatory` | `Vec<FeatId>` | 2B/entry | (b) |
| `source` | `Option<Rc<Word>>` | 8B, **shared**, not recursed by the estimator (`word.rs:641-644`) | (a) immutable, shared |
| `alternatives` | `Vec<Word>` | **recursive full `Word`**/element | §3 — populated once, at stratum merge |
| `root_runtime_id`, `trace`, flags, indices | small/`Copy`/`Option<String>` | 0-32B | mixed |

`estimate_word_bytes` (`word.rs:645-662`) recurses into `non_heads` but **not** `alternatives` —
a real gap, since `alternatives` is the other `Vec<Word>` that recurses (§3, §4-D).

**`Shape`** (`pg-shape/src/lib.rs:195-206`) is 5 SoA columns keyed by node index (`feat_lanes` is
`feat_width * len()` `u64`s, `lib.rs:200-203`), so cost is `n*(const) + n*feat_width*8`, matching
the crate's own coarse estimator (`estimate_shape_bytes`, `word.rs:617-621`, `PER_NODE_FIXED=16`).
A 12-node (10-segment) Aweti word at `feat_width≈1`: `12*16 + 12*8 ≈ 290B`, plus ~90B of `Box<[]>`
fat pointers ⇒ **~380B/clone**.

**`FeatureStruct`** (`pg-featstruct/src/tree.rs:50-52`) is a flat, sorted `Vec<(FeatId(u16),
FeatureValue)>` — **not** interned, **not** a symbol id: `FeatureValue` is `Symbolic(SymbolBits)`
(a `u64` bitset, `tree.rs:38-41`, `lib.rs:33`) or `Complex(FeatureStruct)`, recursive through the
`Vec`'s indirection. A grammar-tier `Interner<FeatureStruct>` exists, assigning `FsId(u32)`
(`pg-featstruct/src/interner.rs:23-55`, `lib.rs:25-34`) — but `Word::syn_fs`/`real_fs` stay the
owned tree form, **never** `FsId`, since rule application mints new values via
`unify`/`priority_union` that cannot be interned into the frozen grammar table (`word.rs:6-9`).
For 1-2 symbolic features: **~64-90B**.

**Memo key**: `AnalysisStateKey` (`pg-memo/src/lib.rs:116-128`) clones a full `Shape` + two
`FeatureStruct`s + a `BTreeMap` + `Vec<MorphHistoryKey>` (`lib.rs:79-85`, mostly `None` runtime
identities) — the same per-instance cost as a `Word`'s own state, paid again on every lookup
(`AnalysisStateKey::new_with_state_and_morph_history`, `lib.rs:177-197`).

**Aweti-typical word total**: fixed struct size (19 fields, ~8 `Vec`/`Box` trios at 24B/16B each
plus the `Shape` sub-struct) ≈ 350-420B, + shape ~380B + `syn_fs` ~80B + `morphs` (3 entries)
~150B + `mrule_apps` (depth ~20) ~160B + `unapplied_rule_counts` (~10 rules, B-tree overhead)
~250-400B ⇒ **roughly 1.3-1.6 KB for a shallow, non-compound word** — corroborated by the
live-frontier note's own measured average, "~1.5 KB/word... stable across 200k-3M steps"
(`live-frontier-memory-bound.md:96`). Depth-15/24 chains and compound `non_heads` multiply this
baseline, which is why totals reach GB scale despite a small per-word figure (§3).

## 2. What C# does differently

Checked directly against `machine` (all citations below are file:line in that repo):

- **`Word` clone**: the copy ctor (`Word.cs:77-92`) does `_shape = word._shape.Clone()` (:83),
  `SyntacticFeatureStruct = ...Clone()` (:85-86), and `new List<Word>(word._nonHeadApps.CloneItems())`
  (:92) — **full deep copies**, not references. `Shape.Clone()` is `new Shape(this)`
  (`SIL.Machine\Annotations\Shape.cs:413-416`), and `FeatureStruct.CloneImpl` recursively copies
  the feature graph (`FeatureStruct.cs:1070` on). **C# does not share here either** — it pays the
  same clone cost Rust does. `IsFrozen`/`Freeze()` (`FeatureStruct.cs:1189-1203`) gate *mutation*,
  not *sharing*: freezing never turns a later clone into a reference.
- **One genuine sharing win, worth mirroring exactly**: `AnalysisScope.TryReplay`
  (`AnalysisScope.cs:60-91`) clones the query's non-head prefix **once per memo hit**
  (`CloneNonHeadsForReplay`, `Word.cs:546-549`) and passes that single list into every replayed
  result (`ReplayOnto`, `Word.cs:511-543`, doc at :507-509: "one memo hit clones them once rather
  than per stored result"). Because C# `Word` is a reference type, `AddRange(queryNonHeadPrefix)`
  (`Word.cs:533`) copies **object references**, so all *K* replays of one hit alias the same frozen
  non-head objects. Rust's `replay_onto` (`word.rs:416-439`) runs once per stored result inside a
  `.map()` (`stratum.rs:1093-1102`, again at `:1281-1291` for the template memo), and each call
  does `clone.non_heads.extend_from_slice(&query.non_heads)` (`word.rs:434`) — a **deep clone of
  every element, K times per hit**, since `Vec<Word>` owns by value. This is the one place C#
  demonstrably shares where Rust copies, and it is directly portable (§4-B).
- **Merged analyses**: C#'s `MergeEquivalentAnalyses` fold (`AnalysisStratumRule.cs:130-168`) is
  the same shape as Rust's: `canonicalWord.Alternatives.Add(mruleOutWord)` (:159) stores the **full**
  `Word`, exactly like `words[idx].alternatives.push(w)` (`stratum.rs:1555,1561`). Not a Rust
  inefficiency to fix — `ExpandAlternatives`/`expand_alternatives` (`Word.cs:446-489`,
  `word.rs:519-577`) needs the alternative's shape, rule-trail delta, and non-head delta to graft
  onto deeper strata (§3), so a bare identity would lose recall.
- **Streaming**: `ApplyMorphologicalRules`/`ApplyTemplates` are `yield return`-based
  (`AnalysisStratumRule.cs:171-242`), consumed one word at a time into a `HashSet` (`Apply`,
  :137,140) — already ported to Rust by Fix B (`live-frontier-memory-bound.md §5`).
- **Bound**: `MaxMemoWords`/`MaxMemoEntries` (`AnalysisScope.cs:27-28`, `Store` at :97-108) are
  already mirrored in `pg-memo` (`MAX_MEMO_WORDS`/`MAX_MEMO_ENTRIES`, `lib.rs:236,239`) plus a byte
  budget C# has no analog for (`lib.rs:241-245`).

**Summary**: C# shares almost nowhere structurally — the only "C# shares, Rust copies" finding is
the non-head-prefix hoist. Everything else in §4 is a genuine Rust-side improvement, not a port.

## 3. Multipliers: where a whole `Word` is copied when a reference/delta would do

| Site | File:line | What's copied | Needed? |
|---|---|---|---|
| Rule-attempt clone | throughout `pg-rules` (`ana_*`/`synth_*`) | whole `Word`/candidate | Yes — each is a distinct state |
| `AnalysisStateKey::new` | `stratum.rs` call sites | `shape`+2×`FeatureStruct`+`BTreeMap`, **per lookup**, hit or miss | No — read-only compare; a hash-consed id would do (§4-C) |
| `replay_onto` | `word.rs:416-439`, called `stratum.rs:1093-1102`, `:1281-1291` | full `self.clone()` (:422) + non-head-prefix re-clone/result | Partially — see §2 |
| Memo store | `stratum.rs:1140` (`let cloned_results = results.clone();`) | every result cloned **twice**: once computed, once to store | No — move `results` in, return a fresh copy to the caller instead |
| `OrderedDedup::add` | `stratum.rs:111-119` | one clone, only on a **novel** key (:110) | Yes, minimal |
| `alternatives.push` | `stratum.rs:1555,1561` | move, not clone (`w` by value) | Cheap; cost is in what `w` contains |
| `non_heads` clone (every `Word` clone) | `#[derive(Clone)]` of `Word` | recursive full sub-`Word` | No — write-once after `non_head_unapplied` (`word.rs:392-395`; no `non_heads[i]` mutation found anywhere in `pg-rules`) — a prime `Rc<Word>` candidate |
| `expand_alternatives` | `word.rs:519-577` | `original.clone()`/alternative, `self.morphs.clone()` (:560) | Report-time only, once per completed parse |

**`alternatives` specifically**: it does **not** nest quadratically — a candidate reaching the
fold (`stratum.rs:1547-1588`) always arrives with `alternatives` empty (only the fold populates it,
nothing feeds a folded canonical back into the same stratum's cascade), and a non-head built by
`ana_compound_subrule` (`morph.rs:3147,3156`) starts from `Word::new` (`word.rs:325-346`) with
empty `alternatives`, never populated before the outer word finishes. So it *is* bounded — by the
number of distinct equivalent arrivals at one canonical state, itself bounded by the frontier/step
budgets — but that bound is **not currently charged** against the frontier byte budget
(`live-frontier-memory-bound.md §3(A)` only names `local`/`result`/`out`) nor against
`estimate_word_bytes` (`word.rs:645-662` recurses `non_heads`, not `alternatives`). Downstream,
`expand_alternatives` needs each alternative's shape, rule-trail delta, and non-head delta, not
just its final identity, to graft it onto deeper strata — so alternatives cannot be collapsed to
bare `WordAnalysis` identities, but they **can** be stored as a delta against `source` instead of a
fully independent `Word` (§4-D).

## 4. Design to under 50 MB, ranked by bytes saved / change cost

**A. `non_heads: Vec<Word>` → `Vec<Rc<Word>>`.** Non-heads are write-once (verified §3), so plain
`Rc` (no `RefCell`/COW) suffices; cloning a compound word becomes `Rc::clone` instead of a
recursive deep copy — directly mirrors the C# sharing found in §2. Changes: `word.rs:201` field
type; `non_head_unapplied` (:392-395) wraps in `Rc::new`; `current_non_head`/`root_runtime`
(:483-488, 579-587) deref transparently; `replay_onto` (:432-435) becomes `Rc::clone`s, not element
clones; `estimate_word_bytes` (:658-660) must count each distinct `Rc` once (a small `seen` set).
Recall: representation-only, identity-preserving (no read site changes what it observes). Memo
key: `AnalysisStateKey` holds only `non_head_count: u32` (`lib.rs:121`), unaffected. Golden TSV:
unaffected. Test: extend `cascade_diamond_never_holds_more_live_words_than_distinct_outputs_plus_depth`-
style unit test asserting compound clone byte cost is O(1) in `non_heads.len()`. Biggest single
win on any grammar with compounding; cost low-medium (one field, a handful of use sites).

**B. Hoist the non-head-prefix clone out of the replay loop**, mirroring `AnalysisScope.TryReplay`
exactly (§2). In `memo_apply_rules`/`run_template_batch` (`stratum.rs:1093-1102`, `:1281-1291`),
clone `input.non_heads` once before the `.map()` (or, after A, clone the `Rc` slice — free) and
pass it into `replay_onto`. Recall: identity-preserving. Memo key: untouched. Test: a >1-result
memo entry asserting one prefix allocation, not K. Cost low; ship even before A.

**C. Hash-cons `Shape`/`FeatureStruct` used as `AnalysisStateKey` inputs (interning, not COW).**
A per-word `Interner<Shape>`/`Interner<FeatureStruct>` (machinery already exists —
`ShapeInterner`/`Interner<V>`, `pg-shape/src/lib.rs:572-579`, `pg-featstruct/src/interner.rs:23-88`,
doc-labeled "per-parse scope... plan §6.2" but **not wired into `Word` today**, per `word.rs:1`'s
own "per-parse interning is not done") shrinks `AnalysisStateKey` from cloning a `Shape`+2
`FeatureStruct`s to 3 `u32` ids (§3 row 2), turning the memo's dominant per-lookup cost into an
integer compare/hash — `analysis-memo-explosion.md`'s option (b). Recall: pure representation
change (interning is structural equality by construction). Memo key: this **is** the change —
`shape`/`syntactic_fs`/`realizational_fs` fields become `ShapeId`/`FsId`. Golden TSV: unaffected.
Test: `memo_parity_gate`/`memo_corpus_gate` byte-identical (interning changes no decision, only
storage); unit test that two structurally equal shapes/FSes intern to the same id. Cost medium —
every `AnalysisStateKey::new*` call site, plus threading the interner through `Analyzer`/`MemoScope`.

**D. Store `alternatives` as a delta against `source`, not an independent `Word`.**
`expand_alternatives` (`word.rs:519-577`) already reconstructs an alternative from
`original.clone()` plus a shape + trail-suffix + non-head-suffix + real-fs-diff + root-allomorph
delta (:524-567) — the stored full `Word` duplicates content already recoverable from `source`.
Store just that delta and materialize the full `Word` only inside `expand_alternatives`, called
once per completed parse, never per candidate. Recall: identity-preserving by construction (the
delta *is* what `expand_alternatives` already extracts). Memo key: `alternatives` isn't part of
any key (`word.rs:274-276`). Test: pin `expand_alternatives`'s output unchanged via a snapshot test
on a grammar with a real merge; extend `estimate_word_bytes` to count the delta form. Cost medium —
new struct, and `alternatives.push` (`stratum.rs:1555,1561`) constructs the delta instead of moving
`w` whole. **Also close the two undercounting gaps from §1/§3**: make `estimate_word_bytes` recurse
into `alternatives`, and add the `alternatives.push` sites to Fix A's frontier byte budget.

**E. `Rc<Shape>`/`Rc<FeatureStruct>` copy-on-write for the top-level fields.** Many rule attempts
touch only one of `shape`/`syn_fs`/`real_fs`; `Rc`+`Rc::make_mut` turns the untouched field's clone
into a pointer bump. Recall: representation-only (`live-frontier-memory-bound.md §2` item 3). Memo
key: same `Rc`s, or C's interned ids if C lands first. Cost medium-high — `Clone`/`Eq`/`Hash`
across three crates and every mutation site (`ShapeBuilder`, `pg_featstruct::ops`). Ranked below
A-D because those fields are *already* immutable after creation, while `shape`/`syn_fs` are
genuinely mutated most attempts, so COW's win is smaller and its surface larger.

**F. Byte budget (Fix A, `live-frontier-memory-bound.md §3`) as the deterministic backstop.** Not a
reduction — ship regardless of A-E, since an adversarial grammar defeats any fixed multiplier
(ADR-0003). Extend it to charge the `alternatives.push` sites (§4-D's gap) once D lands.

**Not recommended now**: a `bumpalo` arena for `Word` itself — `pg-parse`'s arena (`lib.rs:3-7`) is
already scoped to per-word transients at a different layer, and rewriting `Word`'s ownership onto
arena lifetimes touches every `pg-rules` signature for a win A/C capture more cheaply.
`SmallVec`/`shrink_to_fit` for `morphs`/`mrule_apps`: real but small (§1's baseline is dominated by
`Shape`+`FeatureStruct`+`BTreeMap`, not `Vec` slack) — a follow-up after A-D.

## 5. Staged plan

| Stage | Change | Measurement | `oteʼikateʼika` @1M steps | `Ajkululape` @200k steps |
|---|---|---|---|---|
| 0 | baseline | — | 1,465 MB (`live-frontier-memory-bound.md:225`, after Fix B) | ~2 GiB abort (`analysis-memo-explosion.md:5`) pre-Fix-B, unmeasured post-B |
| 1 | B: hoist replay clone | alloc *count* before/after, not peak WS | unchanged bytes (fewer allocations, same retained set) | same |
| 2 | A: `Rc<Word>` non-heads | `estimate_word_bytes` before/after on a compound-heavy fixture | small — this word has few/no compounds | small — its 278,908 attempts are non-compound (`analysis-memo-explosion.md:5`); payoff is on compound-heavy grammars outside this pair |
| 3 | D: delta-form alternatives, close both estimator gaps | `HC_ALLOC_STATS` before/after on a fixture with a real merge (Sena/Mbugwe, `memo_corpus_gate`) | modest — low merge rate here | modest, same reasoning |
| 4 | C: hash-cons shape/FS into the memo key | entries-by-kind histogram + retained bytes (method: `analysis-memo-explosion.md §4`'s prototype branch) | **largest expected drop** — the key clone is paid at every lookup, and this word is lookup-volume-dominated | **largest expected drop** — `analysis-memo-explosion.md`'s "one state stores 77,600 result words" means shrinking the key multiplies against lookup count, not the smaller insert count |
| 5 | F: byte budget as backstop | `parse_compare.py` byte-budget sweep, `live-frontier-memory-bound.md §3`'s two-sided calibration | deterministic cap regardless of residual peak | same |

Estimation method: stages are ranked qualitatively from where each word's known cost concentrates
(`Ajkululape`: key-clone-dominated, non-compound, per `analysis-memo-explosion.md`; `oteʼikateʼika`:
lookup-volume-dominated, shallow compounding, per `live-frontier-memory-bound.md`) — no measurement
ran for this note, so no numeric peak-after figure is asserted per stage. Each stage's proof is
`HC_ALLOC_STATS` before/after on `mem-trace`'s branch, plus `memo_parity_gate`/`memo_corpus_gate`/
`parse_compare.py` green before advancing. Stage 4 is where both words are expected to cross under
50 MB, since it is the only stage shrinking a cost paid at lookup frequency, not insert or clone
frequency.
