# Bounding the analysis memo: what C# does, what the field does, and what to try

Research note, 2026-09-10. Answers three questions about the OOM in `pg-memo`
(`rust/crates/pg-memo/src/lib.rs`) surfaced by the Aweti grammar: `pangloss batch`
exhausts a 2 GiB job object on `Ajkululape` (278,908 morph-rule attempts / 200,000
step budget, zero successful affix uses); `--memo off` reaches a 1,000,000-step cap
in 1.15s flat-memory, `--memo on` OOMs before the cap. Sources: C# HermitCrab at
`C:\Users\johnm\Documents\repos\machine` (checked out on `main`, no worktree needed
— the port's source files are there); web literature per citations inline.

## 1. The C# original

The C# memo is newer than the Rust port's doc assumed and already answers most of
the "how does C# bound this" question directly, via three commits
(`git log --oneline -- AnalysisStateKey.cs AnalysisScope.cs`): `8121049f` (initial
memoization), `3999fd71` (review fixes), `5d26fac6` "Tighten memo resource bounds,
diagnostics, and parallelism cap" (Aug 19 2026). The last commit's message states
the exact gap Rust has now: **"Bound the memo by retained Words, not just entry
count."** Before that commit, C# had only the Rust port's `MAX_MEMO_ENTRIES`-style
cap; it turned out insufficient there too.

**Bound type**: two caps, both counts, not bytes — `AnalysisScope.cs:27-28`:
`MaxMemoEntries = 100_000`, `MaxMemoWords = 1_000_000`. `Store` (`AnalysisScope.cs:97-108`)
refuses to write once either cap is hit: `if (table.Count >= MaxMemoEntries ||
_storedWordCount > MaxMemoWords - results.Count) return;`. The comment at
`AnalysisScope.cs:22-26` is explicit that word-count, not entry-count, is the
real constraint: *"The Word budget is the load-bearing one: entry size is
unbounded (a node's list holds every descendant, undeduplicated) and storing them
keeps every intermediate of the search alive for the whole parse... It is a coarse
backstop, not a figure derived from measured memory."* That is a direct match for
Rust's situation — `MemoEntry { results: Vec<Word> }` with recursive `non_heads`
has the same unbounded-entry-size problem, and PanGloss's `MAX_MEMO_ENTRIES =
100_000` is the pre-tightening C# state, not the current one.

**Eviction**: none. Both caps are stop-storing, not evict-and-replace — quote,
`AnalysisScope.cs:22-23`: *"past either cap subtrees simply go unmemoized,
degrading hit rate but never correctness."* This is deliberate for the recall
guarantee: an unmemoized subtree is just recomputed by brute force, so capacity
loss costs time, not soundness.

**Per-word or per-run**: per word. `AnalysisScope.cs:10-13`: *"One instance per
`Morpher.ParseWord(string, out object)` call. A state key does not encode the
target surface word, so sharing a scope across parses of different words would be
unsound."* Both caps therefore reset for every word — a pathological word gets a
fresh 100k/1M budget, it does not inherit exhaustion from a prior word, but also
cannot borrow headroom from an easy one.

**Interning / "~32 bytes of ids"**: the Rust doc's characterization does not match
what's in `machine` today. `AnalysisStateKey` (`AnalysisStateKey.cs:26-34`) holds
live references — `Shape _shape`, `FeatureStruct _syntacticFS`, `FeatureStruct
_realizationalFS`, and `IReadOnlyDictionary<IMorphologicalRule, int> _ruleCounts`
— not interned ids. `Shape.Clone()` (`Shape.cs:413-416`) does `new Shape(this)`,
a real copy of the annotation list; `FeatureStruct.CloneImpl` (`FeatureStruct.cs:1070`)
recursively copies the feature graph. A grep for `Intern`/`HashCons` across
`src/` returns nothing relevant. So the C# key is cheap only in the sense that it
holds *references* to the word's own frozen `Shape`/`FeatureStruct` rather than a
second serialized copy for hashing (`PinAndKey`, `AnalysisStateKey.cs:43-46`,
freezes and reuses the word's live objects) — there is no shared interning pool
across different words' shapes/FSes. Treat the Rust doc's "~32 bytes" claim as
aspirational, not descriptive of C#; flag it for correction there.

**Full trees or references**: `MemoEntry.Results` (`AnalysisScope.cs:121-133`) is
`IReadOnlyList<Word>` — full `Word` objects, deep-cloned like Rust's `Vec<Word>`.
But replay is not a re-clone-from-scratch: `Word.ReplayOnto` (`Word.cs:511-543`)
splices a stored result's rule-trail/non-head *suffix* onto the querying word's
own prefix (`clone._mruleApps.AddRange(queryNode._mruleApps); ...AddRange(mruleSuffix)`),
and `AnalysisScope.TryReplay` (`AnalysisScope.cs:60-91`) clones the non-head
prefix once per *hit*, not once per stored result within a hit
(`CloneNonHeadsForReplay`, `Word.cs:546-549`). This is a template-and-graft
scheme — closer to a packed-forest node than a naive cache — but it still pays a
full clone on every replay and stores full clones on every write; it is a
structural optimization on top of, not instead of, the "store real Words" design
Rust already has.

## 2. Prior art

**Shared packed parse forests (chart parsing).** An SPPF is a DAG where OR-nodes
represent local ambiguity and AND-nodes share common leaves/subtrees, giving a
worst-case cubic-size shared forest instead of an exponential set of trees
([Billot & Lang, *The Structure of Shared Forests in Ambiguous Parsing*](https://aclanthology.org/P89-1018.pdf);
[Zaytsev, *Coupled Transformations of SPPFs*](https://grammarware.net/text/2016/sppf.pdf)).
Stops blow-up by changing representation (share, don't copy), not by dropping
results — fully recall-preserving, and the closest structural analog to what
PanGloss's per-word search actually needs: today's `Vec<Word>` per memo entry is
the un-shared tree Billot's technique replaces.

**Tabling in XSB Prolog.** Answer/call subsumption reuses one tabled answer for
many subsuming calls; *subgoal abstraction* bounds term size to guarantee
termination even for infinite models, and *radial restraint* adds sound
termination on top; "tripwires" cap subgoal/answer term size as a safety net,
with call/table subsumption for large but recall-relevant terms
([XSB tabling restraints](https://www.swi-prolog.org/pldoc/man?section=tabling-restraints);
[Swift & Warren, tabling survey](https://arxiv.org/abs/1012.5123)). This is
policy, not pruning — abstraction changes what counts as "the same state" (closer
to option (g) below) and is explicitly built to preserve soundness/completeness
of the fixpoint, not to trade away recall.

**Quasi-destructive unification / packing in unification grammars.** Tomabechi's
quasi-destructive unification shares structure across unification attempts by
marking temporary changes that are discarded unless "made permanent," avoiding
full copies per attempt ([Tomabechi 1991](https://aclanthology.org/P91-1041.pdf)).
*Packing* collapses feature structures that are equivalent modulo a
distinguishing disjunct into one packed structure, unifying shared material once
and unpacking only on demand for output
([Oepen & Carroll-style packing, ACL 2000](https://dl.acm.org/doi/10.3115/1034678.1034684)).
Used in LKB/PET/ACE to make large feature grammars tractable. Directly analogous
to option (b): the FeatureStruct in `AnalysisStateKey`/`Word` is exactly the kind
of graph these systems avoid re-copying.

**Cache admission/eviction with byte awareness.** TinyLFU/W-TinyLFU is an
*admission* policy layered on any eviction policy (LRU, SLRU); it estimates
recent frequency before admitting a new item, extended to variable-sized ("byte
budget") entries with competitive byte-hit-ratio versus size-aware algorithms
like AdaptSize ([Einziger, Friedman & Manes, TinyLFU, TOS 2017](https://dl.acm.org/doi/10.1145/3149371);
[Caffeine's W-TinyLFU notes](https://mintlify.wiki/ben-manes/caffeine/advanced/efficiency)).
This *does* evict, so for PanGloss it is recall-unsafe unless paired with
recomputation-on-miss (which the memo already guarantees) — i.e., safe here
specifically because a miss just re-derives, never omits.

**Selective memoization (Acar, Blelloch, Harper).** Lets a program memoize only
where the programmer judges it worthwhile, making the space/time cost of
memoizing explicit and analyzable rather than blanket-caching everything
([Selective Memoization, POPL 2003](https://arxiv.org/pdf/1106.0447)). Directly
the model for options (d)/(e): memoize by policy, not universally, while every
state remains soundly recomputable if skipped.

**Datalog tabling vs. magic sets.** Subsumptive tabling reuses answers across
subsuming subgoals and is shown to beat bottom-up magic-sets rewriting in both
time and space in several benchmarks ([Tekle & Liu, *Subsumptive Tabling Beats
Magic Sets*](http://logicprogramming.stanford.edu/readings/tekle.pdf)). Relevant
as another case where *changing which states get treated as identical*
(subsumption, i.e. option (g)) outperforms raw caching.

**HermitCrab/XAmple/foma literature.** No published account of bounding search
directly was found; general morphology-combinatorics literature (affix-ordering
theory) is about linguistic constraints, not implementation memory bounds, and
doesn't transfer.

## 3. Ranked options for PanGloss

1. **(a) Byte- or Word-count-accounted budget, stop-storing.** Mechanism: track
   retained node/byte count across both memo tables (mirror C#'s
   `_storedWordCount`, or go one step further and sum actual heap bytes via a
   size estimate per `Word`); refuse writes past budget, never evict. Effect on
   Aweti: with zero successful affix uses, essentially every attempted state is a
   *nogood* (empty `results`) — a nogood costs one key, not a `Vec<Word>`, so a
   nogood-aware count already shrinks memory ~278,908×-fold for this pathological
   word without touching the (rare) real-result case. Recall-safe: identical to
   current design, just correctly sized. Cost: small (add a counter + threshold
   check in `stratum.rs`); measure: peak RSS and hit-rate on Aweti and on a
   grammar that currently has many successful analyses.

2. **(c) Store nogoods distinctly, cap positives separately.** Mechanism: give
   nogood entries (empty `Vec<Word>`) a dedicated, much larger count budget than
   positive entries, since a nogood's cost is O(key size) regardless of the
   subtree explored to prove it. This is the single highest-leverage fix for the
   *measured* failure mode (zero successful affix uses ⇒ the memo is 100% nogoods
   yet still explodes) — meaning the current OOM isn't from storing huge `Vec<Word>`
   result lists at all, it's from the *key* side: `Shape`/`FeatureStruct` clones
   and the `BTreeMap<MRuleId, count>` per entry, at up to 278,908 distinct keys.
   Recall-safe (nogoods are exact, not approximate). Cost: low, mostly bookkeeping
   in `stratum.rs`/`pg-memo`; measure: entries-by-kind histogram before/after on
   Aweti.

3. **(b) Intern/hash-cons `Shape` and `FeatureStruct`.** Mechanism: a
   per-`AnalysisScope` (per-word, matching C#'s scope lifetime) interning table
   keyed on structural content, so equal shapes/FSes across the ~279k attempts
   share one allocation; `AnalysisStateKey` becomes small ids instead of full
   clones (making the Rust doc's original "~32 bytes" claim true, where it isn't
   in C#). Directly modeled on LKB/PET packing and Tomabechi's structure-sharing.
   Recall-safe (pure representation change). Cost: medium — touches `pg-rules`'s
   `Shape`/`FeatureStruct` types and their `Clone`/`Eq`/`Hash` impls; biggest win
   if Aweti's 278,908 attempts revisit a much smaller number of distinct shapes
   (the 24-level chains and unordered-cascade permutations suggest heavy reuse).
   Measure: distinct interned shape/FS count vs. attempt count.

4. **(g) Drop per-rule unapplication counts from the key where irrelevant.**
   Mechanism: C#'s key includes the full `UnappliedRuleCounts` multiset because
   *some* analysis rule reads it (`AnalysisStateKey.cs:14-15`, doc comment
   audits every reader); Rust's `BTreeMap<MRuleId, count>` presumably plays the
   same role. If a stratum's ruleset never actually consumes the count (e.g. no
   `MaxStemCount`/gate-like rule active), the key can drop it, collapsing
   permutation-order states that otherwise all get separate entries — this is
   the `k!`-walk PanGloss's own doc names as the root combinatorial cause.
   Recall risk: real if done wrong — must audit every consuming rule the way the
   C# doc comment does, per stratum, not once globally. Cost: medium (needs the
   audit); highest expected impact on entry *count* specifically because it
   attacks the permutation explosion directly, but do only with the same rule-by-
   rule literature review C# keeps as a maintenance obligation.

5. **(d)/(e) Adaptive/selective memoization (hit-rate or arrival-count gated).**
   Mechanism: per Acar et al., track hits vs. insertions per stratum/word;
   disable memoization (fall through to `pg-rules`'s already-fast unmemoized
   path, which the 1.15s/flat-memory `--memo off` run shows is not the
   bottleneck) once the ratio suggests thrashing, or require ≥2 arrivals at a
   state before paying to store it. Effect on Aweti: with reportedly zero
   successful uses, hit rate on the *positive*-result table is likely already
   near zero — an adaptive gate would turn the memo off for this word almost
   immediately, degrading gracefully to the known-safe unmemoized behavior.
   Recall-safe by construction (never omits, only stops caching). Cost:
   medium-high (needs per-word/per-stratum counters and a threshold, plus
   tuning); measure: time-to-first-disable on Aweti vs. hit-rate retained on
   grammars where the memo currently helps.

6. **(f) LRU/size-bounded eviction.** Lowest priority: unlike a cache serving
   repeated queries, this memo's whole value is that a *later* state in the same
   parse can still hit an *earlier* entry; evicting the wrong entry converts a
   would-be hit into full recomputation, which is only a performance loss (never
   a recall loss, since miss ⇒ recompute) but the eviction bookkeeping cost is
   pure overhead when (a)/(c) already stop the memory growth. Worth adding only
   after (a) if profiling still shows the cap is hit often and evicting by
   estimated future value (not raw LRU — TinyLFU-style frequency estimate is
   closer to right for a k!-permutation workload where the same state recurs)
   beats stop-storing on measured hit-rate.

**Recommended first cut**: (a) + (c) together (cheap, directly targets the
measured failure — an all-nogood table exploding past a 2 GiB job object), then
measure whether (b) or (g) is still needed once the entry cost itself is small;
both are larger changes and should be justified by data from the (a)/(c) fix
rather than built speculatively.

## 4. What the instrumented audit measured the same day (corrects section 3)

Env-gated counters (`HC_MEMO_STATS=1`, branch `research/memo-audit`) on `Ajkululape`:

| step cap | mrule lookups | mrule hits | inserts | refused by entry cap | template hits |
|---|---|---|---|---|---|
| 200,000 | 13,677 | 11,130 (81.4%: 1,914 positive, 9,216 nogood) | 2,547 | 0 | 76.8% |
| 600,000 | 53,054 | 88.3% | 6,183 (6% of the 100,000 cap) | 0 | 92.6% |

Three corrections to the reasoning above. The table is **not** all nogoods: positive hits are
real and the hit rate is high, so the memo is not pure cost here. Memory is dominated by
**entry size, not key count**: one state stores 77,600 result words (`results_len_max`), and
total stored words at 600k steps are ~267,000 while the entry gauge reads 6% full — the C#
comment ("entry size is unbounded") describes the Rust port exactly. And the key **is** too
fine in a provable way: `unapplied_rule_counts` has exactly one reader
(`stratum.rs`, the `count >= max_apps` check), so counts above a rule's `max_apps` distinguish
states whose every future decision is identical. Saturating each count at its rule's
`max_apps` inside the key is recall-neutral by construction and is the first fix to make;
the retained-word budget (C#'s `MaxMemoWords`) is the second. `replay_onto` also deep-clones
every replayed word twice (once in `replay_onto`, once in `out.add(r.clone())`). Memo on and
off produce identical rows on every word that completes (44 Aweti, 200 Sena words checked);
on cap-bound words the partial signature differs because step consumption order differs,
which is expected and not a recall difference.

A prototype branch (`research/memo-adaptive`) measured three policies behind env switches:
a 64 MiB per-table byte budget kept parity everywhere and let the word survive 1,000,000
steps at 1.2 GB peak (5x lower); adaptive disable (2,000 inserts, <2% hits) kept parity but
peaked at 3.6 GB; store-on-second-arrival changed which words hit the cap and is rejected
as a default.
