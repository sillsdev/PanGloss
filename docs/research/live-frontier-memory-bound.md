# Bounding the live search frontier: where the 5.4 GB lives, and how to cap it

Research note, 2026-09-11. Follows `docs/research/2026-09-10-analysis-memo-fixes-for-machine.md`
§5 and `docs/research/analysis-memo-explosion.md`, which established that `oteʼikateʼika` aborts
around 5.4-5.8 GB peak working set on every binary, with the memo on or off and at every memo byte
budget — the memo bounds only what is *retained after* a call returns, never the in-flight set one
call builds. This note instruments that in-flight set directly (`HC_FRONTIER_STATS=1`, env-gated,
off by default) and finds the peak is one level higher than either prior doc placed it.

## 1. Where the 5.4 GB lives

Instrumentation added to `rust/crates/pg-rules/src/stratum.rs` (`pub mod frontier_profile`) and
printed from `pg-cli` under `HC_FRONTIER_STATS=1` records, per word: peak recursion depth, and
peak length + `pg_rules::word::estimate_words_bytes` for six candidate homes — `memo_apply_rules_raw`'s
flattened `local`, the memoized cascade's `OrderedDedup` accumulator, the raw (`--memo off`)
`Cascade::combination`/`permutation`'s `Acc` (read at `run_mrule_cascade`'s return, which already
equals the `Acc`'s final — and, since it never shrinks, peak — size), the template battery's `out`,
and `apply_mrules`/`apply_templates`'s own `result`. Debug build, `--threads 1`, `-RunMemoryGB 6`,
Aweti grammar, `oteʼikateʼika` alone:

| `--step-cap` | memo | depth | `local` | `OrderedDedup`/raw `Acc` | `apply_mrules` result | `apply_templates` result |
|---:|---|---:|---:|---:|---:|---:|
| 200,000 | on | 15 | 1,990 / 2.97 MB | 1,274 / 1.94 MB | 32,520 / 51.7 MB | 32,529 / 51.8 MB |
| 200,000 | off | 8 | — | 784 / 1.29 MB (raw `Acc`) | 2,786 / 4.48 MB | 2,795 / 4.51 MB |
| 1,000,000 | on | 15 | 1,990 / 2.97 MB | 1,274 / 1.94 MB | 233,627 / 366.5 MB | 233,636 / 366.5 MB |
| 3,000,000 | on | 16 | 16,356 / 25.9 MB | 7,021 / 11.48 MB | 352,103 / 551.2 MB | 352,112 / 551.2 MB |

(`max_template_len` stayed at 10/36.9 KB throughout — the affix-template battery itself is not a
contributor for this word.) `--memo off`'s row uses the raw `Cascade`'s `Acc`, the structural twin
of the memoized path's `OrderedDedup` — both are single, monotonically-growing, key-deduplicated
accumulators, so their length **is** the true distinct-state count reached, not a lower bound.

**Reading the table**: `local` and the dedup accumulator (`OrderedDedup`/`Acc`) — the two structures
both prior docs named as suspects — saturate. Between 200,000 and 1,000,000 steps they are
*byte-identical* (1,990/1,274 both rows): the word's distinct reachable states at this exploration
path were already exhausted by 200k steps; the extra 800,000 steps found no new state. But
`apply_mrules`'s and `apply_templates`'s own `result: Vec<Word>` — in `stratum.rs`'s per-stratum
orchestration, one level *above* `memo_apply_rules_raw` and the mrule cascade — grew **7.2×** over
the same step-count range with *zero* new distinct states, then another 1.5× by 3,000,000 steps as
depth extended to 16. This is the peak: at 3M steps it alone is 551 MB, already comparable to a
meaningful fraction of the 5.4 GB abort point, and — unlike the dedup structures — still climbing
with no sign of saturation when the run was stopped for time budget, not because growth had ended.

**Mechanism**: `apply_mrules` (`stratum.rs`, Unordered branch) does
`result.extend(self.apply_templates(&w)); result.push(w);` for every `w` the *already-deduplicated*
mrule cascade yields; `apply_templates` does the symmetric `result.extend(self.apply_mrules(&t))`.
Each recursive call returns its **entire** subtree as an owned `Vec<Word>`, and the caller
concatenates it into its own `result` with no deduplication and no memo at this concatenation
point — only the *inner* calls (`run_mrule_cascade`, `run_template_batch`) dedupe/memoize their own
direct output. Because `Unordered` order re-enters `apply_templates`/`apply_mrules` once per
deduplicated candidate, and many candidates share long common suffixes of unapplication history,
the same already-known subtree content gets independently re-flattened and re-appended many times
over as recursion continues — a multiplicative replay cost the dedup accumulators never see because
they dedupe by full state key, while `result` never checks a key at all. This is a distinct,
previously-unlocalized cause from both prior docs: not `memo_apply_rules_raw`'s `local` (which
saturates), not the memo table (bounded and orthogonal — this reproduces with `--memo off`, at 3.5×
smaller absolute scale but the same shape: `apply_mrules`'s 2,786 vs. the raw `Acc`'s 784, a 3.6×
multiplier at only 200k steps).

## 2. Prior art for bounding live search memory without losing completeness

1. **Streaming/generator-style yield instead of `Vec` collection.** C#'s actual original —
   `AnalysisStratumRule.ApplyMorphologicalRules`/`ApplyTemplates`
   (`machine/src/SIL.Machine.Morphology.HermitCrab/AnalysisStratumRule.cs:171-242`) — is
   `IEnumerable<Word>` built with `yield return`, and `Apply` (`:113-169`) composes them lazily
   (`ApplyTemplates(input).Concat(ApplyMorphologicalRules(input))`) and consumes one word at a time
   directly into a deduplicating `HashSet<Word>` (`:140-164`) — **never** materializing an
   intermediate `List<Word>` at this level. This is exactly the structure the Rust port lost: Rust's
   `apply_mrules`/`apply_templates` eagerly collect into `Vec<Word>` and `.extend()` it, which is
   what makes the multiplicative replay in §1 possible at all — a lazy consumer discards each
   subtree's transient state as soon as it is folded into the caller's own dedup set. C# additionally
   has `MaxAlternatives` (`Morpher.cs:81`, default 0/off) which throws
   `MaxAlternativesExceededException` mid-enumeration (`AnalysisStratumRule.cs:146-150`, comment:
   *"Stops before full enumeration because ApplyTemplates and ApplyMorphologicalRules use yield
   return"*) — a magnitude cap only laziness makes cheap to check. *Completeness*: recall-preserving
   by construction — it changes representation/timing, not what is computed, as long as the eventual
   consumer still visits every yielded item once. *Determinism/WASM*: an `Iterator`-based Rust
   translation (a hand-written state machine, since stable Rust has no generators) is exactly as
   deterministic and portable as the current recursive-`Vec` code — no threads, no async needed.
   *Cost*: high — `apply_mrules`/`apply_templates`/`memo_apply_rules_raw` are mutually recursive over
   `&self`; converting to a callback (`&mut dyn FnMut(Word) -> ControlFlow<()>`) driven by the same
   recursion is mechanical but touches every call site in `stratum.rs`. *Measure*: peak WS and
   `frontier_profile`'s counters before/after; expect `apply_mrules`/`apply_templates`'s len to
   collapse toward the dedup accumulator's size.

2. **Depth-first / iterative-deepening vs. breadth-first accumulation.** The current code is already
   depth-first in *call order* (recursive descent) but not in *memory discipline*: returning a fully
   materialized `Vec<Word>` up each frame defeats DFS's O(depth × branching) memory bound, because a
   finished branch's memory is not freed until the whole ancestor chain also finishes (§1's
   mechanism). Item 1 is what actually recovers DFS's memory advantage here — true DFS with
   generator/streaming semantics; the current shape is DFS order with BFS-style retention.

3. **Structure sharing for immutable parts (`Rc<Shape>`/`Rc<FeatureStruct>`, hash-consing).** Per
   `analysis-memo-explosion.md` option (b): interning would shrink the ~1.5 KB/word average this
   measurement found (`estimate_words_bytes` / count, stable across 200k-3M steps), turning a clone
   per attempt into a pointer bump. *Completeness*: pure representation change, recall-safe.
   *Cost*: medium — touches `Shape`/`FeatureStruct`'s `Clone`/`Eq`/`Hash` across `pg-rules`/`pg-shape`/
   `pg-featstruct`. This reduces the *constant factor* per retained word; it does not reduce the
   *count* of retained words the way item 1 does, so it is a multiplier on whatever item 1/4 leave
   behind, not a substitute for either.

4. **Early dedup at the frontier by state key** — already exists one level down
   (`OrderedDedup`/`Acc`, `pg_memo::AnalysisStateKey`); §1's finding is that this discipline stops at
   the mrule-cascade/template-battery boundary and is not applied to `apply_mrules`/`apply_templates`'s
   own concatenation. Extending it up (folding directly into a shared dedup accumulator instead of a
   per-call `Vec`) is functionally the same change as item 1, described as a data-structure change
   rather than a control-flow change — either lands the same result.

5. **Shared packed forests / feature-structure packing** (Billot & Lang,
   [*The Structure of Shared Forests in Ambiguous Parsing*](https://aclanthology.org/P89-1018.pdf);
   Oepen/Carroll-style packing, [ACL 2000](https://dl.acm.org/doi/10.3115/1034678.1034684)) and
   **XSB tabling with subsumption/tripwires**
   ([tabling restraints](https://www.swi-prolog.org/pldoc/man?section=tabling-restraints)): see
   `analysis-memo-explosion.md` §2 for the full treatment — both are recall-preserving,
   representation-level fixes analogous to items 1/3, higher implementation cost, not re-derived here.

6. **Explicit-stack search with a bounded open list plus a typed "memory budget exceeded" outcome.**
   Mechanism: replace the implicit call-stack recursion with an explicit work-list/stack the caller
   owns, capped at a configured byte budget exactly like `StepBudget` caps step count — count
   estimated bytes allocated to the live frontier (not requests satisfied, not hit rate) as a logical
   dimension, refuse further expansion and return a `CAP`-shaped outcome once exceeded, deterministic
   and identical on native and WASM. *Completeness*: recall is preserved for any word that finishes
   under budget; a word that would have found more only past the budget reports incomplete, never a
   wrong answer — exactly ADR-0003's contract. *Cost*: the cheapest option that is not already
   present in some form — no new data structure, a shared counter threaded the same way `StepBudget`
   already is. This is what §3 recommends as the ADR-0003-required backstop.

## 3. Recommended design

**Two changes, ordered by what they buy:**

**(A) A frontier byte budget — the ADR-0003-required bound, ship first.** Add a shared,
`StepBudget`-shaped counter (a `Cell<usize>` alongside the existing step counter, since both are
per-`parse_word`-call and checked at the same rule-attempt granularity) that accumulates
`estimate_word_bytes(&w)` for every word pushed into `local` (`memo_apply_rules_raw`), `result`
(`apply_mrules`/`apply_templates`), and `out` (`run_template_batch_raw`, `OrderedDedup::add`).
Checked before each push, exactly like the step cap; once exceeded, stop pushing and mark the word
`capped`, reusing the existing `CAP` outcome path (`StepBudget`'s own `over_budget()` short-circuits
already reach every recursive call site). This is a **bound**, not a **reduction**: it does not
shrink the 5.4 GB, it stops the process before that point with a typed incomplete outcome instead of
an abort. Per CONTEXT.md's "Atomic word-analysis result", a capped word reports the complete
confirmed set found *before* the budget fired (diagnostic, honestly partial) plus the typed
incomplete outcome naming the dimension (`frontier_bytes`) and the value that hit it — never
presented as definitive, retryable with a caller-selected higher budget per "Logical work budgets".

**(B) Convert `apply_mrules`/`apply_templates` to the streaming/callback shape from §2 item 1 — the
reduction, ship second, justified by (A)'s measured trip rate.** This is what actually lowers how
often (A) fires and by how much peak memory it must absorb before firing, by removing the
multiplicative re-flattening §1 measured (7.2× per 5× step increase with zero new states). Do this
after (A) is in place and only if (A)'s CAP rate on real corpora shows it firing often enough to cost
accuracy at a reasonable budget — the same "measure before building" discipline
`2026-09-10-analysis-memo-fixes-for-machine.md`'s own recommended-first-cut used.

**Distinguishing reduce vs. bound, per ADR-0003**: (B) and item 3 (structure sharing) are
*reductions* — they make the pathological case smaller or rarer but do not, by themselves, give a
deterministic ceiling; a sufficiently adversarial grammar can still exhaust any fixed multiple of
"the worst word observed." (A) is the *bound* ADR-0003 requires even if (B) makes hitting it rare:
magnitude-only, checked before allocation, typed incomplete outcome, no watchdog, identical on
native and WASM.

**Differential measurement plan**: (1) `memo_parity_gate` and `memo_corpus_gate` must stay green
before and after both changes — a capped word's *partial* set is not compared (step/byte order
legitimately differs), but every completed word's identity set must be byte-identical, per the
existing gates' own discipline. (2) `rust/tools/memo-measure.ps1` with `-Env @{ HC_FRONTIER_STATS =
'1'; HC_STEP_STATS = '1' }` on Aweti/Sena/Mbugwe before/after (B): expect
`max_apply_mrules_len`/`max_apply_templates_len` to converge toward `max_dedup_len`, and peak WS to
drop correspondingly on `oteʼikateʼika` and `ekozokotu`. (3) `parse_compare.py` (or the gates' own
harness) on a byte-budget sweep for (A) alone, mirroring the existing "Byte-budget margin" sweep
methodology in `2026-09-10-analysis-memo-fixes-for-machine.md`: smallest budget where no legitimate
corpus word trips it, largest budget still well under the process memory ceiling — the same
two-sided calibration ADR-0003 names (lower bound: every legitimate word completes under default;
upper bound: default × concurrency ≪ host capacity).

## 4. Instrumentation added

`pg_rules::stratum::frontier_profile` (`HC_FRONTIER_STATS=1`, off by default, one cached env read
when disabled): thread-local maxima for recursion depth and for length/estimated-bytes of `local`,
the memoized dedup accumulator, the raw cascade's output, the template battery's output, and
`apply_mrules`/`apply_templates`'s own result — printed as one `FRONTIERPROF` stderr line per word
from `pg-cli`, mirroring the existing `HC_MEMO_STATS`/`MEMOPROF` convention exactly (same
enable-gate shape, same per-word snapshot-not-reset semantics). Zero cost when unset: every record
call is behind an `enabled()` check, and depth tracking constructs its RAII guard only via
`enabled().then(DepthGuard::enter)`, so the disabled path never touches a `Cell`.
