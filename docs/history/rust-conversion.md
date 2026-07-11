# Rust conversion plan: low-level HermitCrab parsing

**Goal:** a native Rust engine for HermitCrab morphological parsing — *words in, morphemes
out* — callable from a .NET Framework 4.8 host (FieldWorks), switchable at runtime with the
existing managed engine, carrying forward the order-invariant memoization from
`parse-optimization` (PR #451), and engineered from the start for L1-cache behavior,
multi-threaded scaling, and a bounded memory footprint **without any GC tuning at all** —
the Server-GC memory blowup (the 45 GB incident, PR #438's out-of-process workaround, PR
#451 Phase 8b's `GCHeapHardLimit` guidance) simply ceases to exist as a problem class.

Target size: **~13,000 lines of Rust** product code (module budget in §5.7), plus tests.

---

## 1. Evidence base — what got traction, and what it teaches the port

This plan is built on the audit trail of two branches (plus their PRs) and the phased dev
journal `parse-optimization.md` (readable at `git show faa069d3:parse-optimization.md`,
2,034 lines, Phases 0–10).

### 1.1 `hc-rustify` (PR #446) — the C# engine was already reshaped Rust-ward

The branch name is literal: every winning change moved the engine from object-graph C#
toward data-oriented layout. These are **not things to port — they are the shapes the Rust
code should be born with**:

| #446 change (retrofit in C#) | Rust-native form (design in from day one) |
|---|---|
| Flat Shape backing: parallel int-linked arrays, `ShapeNode` = (Owner, Index) handle | Shape = struct-of-arrays owned by the parse arena; nodes are `u32` indices, no node objects at all (§5.2) |
| Copy-on-write Shape clone (frozen source shared until mutation) | Immutable frozen shapes behind `Arc`/arena refs; "clone" of a frozen shape is a pointer copy by construction (§5.2) |
| `FeatureStruct` bit-packed `ulong[]` flat-unify **fast path** with fallback | Bit-packed vectors as the **primary** representation for closed-class symbolic FS; DAG unifier only where the grammar demands it (§5.3) |
| `Fst<Word,int>`: int-offset traversal, flattened `Register[,]` → `[]`, per-Transduce register scaffold reuse | CSR arc storage, `u32` state/arc ids, flat register array, explicit-stack traversal (§5.4) |
| Cheap `GetHashCode` overrides replacing CLR identity-hash; `StringComparer.Ordinal` on hot sorts | Interned `u32` ids everywhere; hashing is integer hashing; all comparisons ordinal/`Ord` by construction |
| Filtered-annotation-view cache on frozen `AnnotationList` (~89% of Transduce calls re-derived the same view) | Compute the filtered projection once at shape-freeze; it *is* the shape's traversal form (§5.2) |
| `MaxDegreeOfParallelism = 1` so the host parallelizes across words | Same contract: the Rust engine is single-threaded per word; all parallelism is across words (§7) |

Measured result of #446 in C#: 1.3×–2.8× faster, 47–64% less memory, byte-identical output.
That is the *retrofit* ceiling; Rust removes the two costs #446 could only mitigate —
GC pressure from transient search garbage, and pointer-chasing object headers.

**#446's negative results, re-examined for Rust** (do not blindly inherit conclusions that
were artifacts of the CLR):

- *Per-word arena/pool for traversal instances* — *reverted in C#* because pooled objects
  survived Gen0 and promoted to Gen2, serializing parallel parsing on stop-the-world
  collections. **This is a GC artifact.** In Rust, a per-word bump arena (§6) is the
  cornerstone of the memory design; the failure mode does not exist.
- *Thread-static pooling of scratch collections* (from #451) — cut allocation 15–17% but
  regressed wall-clock 8–15%, because Gen0 allocation is nearly free. **Also a GC
  artifact** — in Rust, arena allocation is the moral equivalent of Gen0 (pointer bump),
  so we get the allocation win without a pooling layer.
- *CSR compilation of the frozen FST* — closed in C# as "arc-chase is not hot (0.1–0.4%)".
  Correct measurement, wrong to inherit: in C# the arc walk was already drowned out by
  allocator/hash costs that Rust removes. CSR is free to adopt at design time (it is just
  the natural flat layout), so we adopt it — but we do **not** budget optimization effort
  on arc-chase until profiles say so. The discipline transfers even where the conclusion doesn't.
- *FST output delta-log* — confirmed dead code: `Matcher.Compile()` never passes
  `operations`, so HermitCrab only ever exercises the **FSA** path of the FST engine.
  **Port consequence: the Rust FST engine only needs the acceptor/registers machinery
  actually reachable from HermitCrab** — a significant scope cut (§5.4).
- *Lazy determinization* — rejected because hot matchers need `AllSubmatches`. Same in Rust.

### 1.2 `parse-optimization` (PR #451) — the memoization to carry over

The core insight (measured, not conjectured): the unordered-strata analysis cascade is a
k! walk over a 2^k lattice; on Sena's worst words, 158,227 node expansions hit only ~2,546
distinct order-invariant states (one state re-visited 7,000+ times); ~98% of work was
provably repeated; and **the per-stratum template battery, not the mrule cascade, was 93%
of wall time**. What shipped:

- **`AnalysisStateKey`** — order-invariant state identity: (shape, stratum, syntactic FS,
  realizational FS, non-head *count*, unapplied-rule *multiset*). Verified against what
  every analysis-side rule actually reads; deliberately excludes trail order and
  result-dedup-only fields. Commutative (XOR) multiset hash.
- **Two memo tables per parse** (`AnalysisScope`): a *nogood* table (subtree provably
  yields nothing) and a *positive template memo* (per-stratum template battery outputs) —
  kept separate because they record different computations over the same key space.
- **Trail replay** (`Word.ReplayOnto`): a second arrival at a known state grafts its own
  rule/non-head trail prefix onto the stored subtree's suffix instead of re-searching.
- **In-flight re-entrancy guard**: a self-loop arrival falls back to plain expansion rather
  than reading a partial entry.
- **Entry cap** (100,000) as an OOM guard: past the cap, stop memoizing — correctness
  unaffected, hit rate degrades.
- **Corpus scheduling**: longest-surface-first + work-stealing closes a measured 2.9×
  packing-slack to 1.36×. Reference numbers: 313-word Sena batch, 1,051 s sequential →
  74.4 s at 16-way.
- **Ground rules that made all of this safe** (§9 carries them forward verbatim):
  tracing bypasses every cache; exhaustion poisons caches; byte-identical signatures are
  the only acceptance gate; one revertible phase per commit, measurements in the message.

**Rust upgrades to this design** (§6.3): with interned shapes and feature structs, the
state key collapses from "deep ValueEquals over shape + two FS + a dictionary" to a
~32-byte struct of `u32` ids compared by integer equality — the memo lookup itself gets an
order of magnitude cheaper. Trail replay becomes a persistent (cons-list) trail share
instead of clone-and-graft. Phase 9c's *packed parse forest* (Maxwell 1994) stays a
non-goal, same as in C#: memoization already removed the redundancy that motivated it.

### 1.3 Adjacent branches — what to take, what to leave

- **#438 (closed)**: proved the workload is allocation/GC-bound (~371 MB, ~8,800
  `Word.Clone`s per word pre-#446) and that Server GC doubles throughput but at
  deployment-hostile memory cost — hence the out-of-process worker. **Rust obsoletes the
  entire mechanism.** The `IMorphologicalAnalyzer`-behind-a-transport pattern from its
  `HermitCrabServerClient`, however, is exactly the switchable-facade shape §4 uses.
- **#448 (complexity-cap)**: soft-stop step/time budgets, structural bounds, grammar lint.
  Not in scope for parity v1 (the `rust` branch baselines against master, which lacks
  #448), but the Rust engine reserves budget checkpoints in the search loop from day one
  (§6.4) so the port is a small follow-on, and the memo's "exhaustion poisons caches" rule
  is honored the moment budgets exist.
- **#441 (fst-advisor)**: a *different* strategy (precompiled whole-word FST + certification,
  engine as source of truth). Not ported. Two ideas are stolen: **verification-by-parity
  against the engine as the product's safety story** (§8 layer 5, shadow mode), and the
  grammar census idea reused as a **loader lint** that refuses grammars using constructs
  the Rust engine doesn't implement yet, falling back to C# instead of miscomputing (§8 layer 6).

---

## 2. Scope

### 2.1 In scope (the Rust engine)

The full `Morpher.ParseWord` pipeline, non-tracing path:

1. Surface word → NFD normalize → segment via `CharacterDefinitionTable` → `Shape`.
2. **Analysis (unapply)**: per stratum — phonological rules (rewrite, metathesis,
   epenthesis/deletion) unapplied; affix-template battery unapplied; morphological rule
   cascade (affix process, compounding, realizational) unapplied in the configured order
   (`Linear`/`Unordered`), with the §1.2 memoization.
3. **Lexical lookup** of candidate roots (`RootAllomorphTrie` indexing from #451).
4. **Synthesis (confirm)**: reapply rules/templates to each candidate, filter by surface
   match, allomorph environments, (dis)junct allomorph co-occurrence, morpheme co-occurrence
   rules, obligatory syntactic features, `NonFinal`/partial gating — everything
   `Morpher.ParseWord` checks today.
5. Dedup + canonical ordering → morpheme sequences out
   (= C# `WordAnalysis`: ordered morpheme IDs, root index, POS id — `Morpher.cs:637`).

Plus: HC XML grammar loading in Rust (§3), the memoization (§6.3), the batch/benchmark CLI
(§8 layer 2), and the FFI + C# bridge (§4).

### 2.2 Out of scope (stays managed; the switchable facade routes to C#)

- **Tracing** (`TraceManager`) — FLEx "Try a Word". Ground rule 1 of #451 already bypasses
  all caches under tracing; here, tracing selects the managed engine entirely.
- **Generation** (`IMorphologicalGenerator.GenerateWords`) and root guessing (`guessRoot`).
- Programmatic rule construction API (FieldWorks `HCLoader` builds XML for us; see D1).
- `#448` budgets/lint as *shipped features* (checkpoints reserved; port is follow-on).

This split is not a compromise; it is the switchability story: the managed engine remains
fully functional and is the reference oracle forever (§8).

---

## 3. Decision D1 — grammar input: Rust loads the HC XML directly

**Decision:** the Rust engine consumes the same HermitCrab XML grammar file
(`*-hc.xml`, the format of `XmlLanguageLoader` and of FieldWorks' `HCLoader` output),
parsed with `quick-xml`, then compiled to the immutable runtime tables of §5.

Rejected alternative — C# loads and marshals a compiled grammar across FFI: creates a
version-locked binary contract between the two engines, makes the Rust CLI unusable
standalone (killing the benchmark/parity harness), and saves little (the loader is ~1,500
of the 13k lines). The XML file is the natural, already-deployed contract: FieldWorks
writes it today, both engines read it, A/B switching needs no new plumbing.

Consequences the plan owns:
- The loader must replicate `XmlLanguageLoader` semantics (1,513 lines C#). The §8
  conformance-fixture suite is the drift detector.
- **Loader lint**: any construct outside the implemented surface → structured
  "unsupported" error → the C# facade logs and falls back to the managed engine. Never
  a silent wrong parse (stolen from #441's census/certification stance).
- Unicode parity: `CharacterDefinitionTable` normalizes to **NFD**
  (`CharacterDefinitionTable.cs:59,112,278`). Rust uses `unicode-normalization`; a
  dedicated gate (§8 layer 1) diffs .NET-vs-Rust NFD output over every corpus word and
  every grammar string rep at load time, catching Unicode-version skew before it can
  become a parse difference.

---

## 4. Switchability: the facade, the FFI, and the net48 host

### 4.1 C# facade (new project `SIL.Machine.Morphology.HermitCrab.Bridge`, netstandard2.0 + net48)

```csharp
public enum HcEngine { Managed, Rust, Shadow }   // Shadow: run both, compare, return managed

public static class MorphologicalAnalyzerFactory
{
    // engine resolved from: explicit arg > HC_ENGINE env var > config > default(Managed)
    public static IMorphologicalAnalyzer Create(string hcXmlPath, HcEngine engine = default, ...);
}
```

- `RustMorphologicalAnalyzer : IMorphologicalAnalyzer, IDisposable` — owns the native
  grammar handle; `AnalyzeWord(string)` → `WordAnalysis[]` reconstructed against the
  *managed* object model: the managed `Language` is loaded too (cheap, once), and native
  results carry stable **morpheme IDs** that map back to managed `IMorpheme` instances. The
  host sees identical types either way — that is what makes the switch invisible.
- Any native error, panic, unsupported-grammar lint, or version mismatch →
  log + transparent fallback to managed for that call (Shadow semantics: managed always wins).
- Tracing/generation entry points always route managed.
- FieldWorks flips engines via its own config plus the `HC_ENGINE` override; no FieldWorks
  code change beyond constructing through the factory.

### 4.2 FFI (crate `hc-ffi`, `cdylib`, C ABI)

```c
int32_t  hc_abi_version(void);                       // bump on any struct/semantic change
int32_t  hc_grammar_load(const uint8_t* xml_utf8, size_t len,
                         HcGrammarHandle* out, HcError* err);   // err carries lint results
void     hc_grammar_free(HcGrammarHandle);
int32_t  hc_parse_word(HcGrammarHandle, const uint8_t* word_utf8, size_t len,
                       HcResultBuf* out);            // one word, caller's thread
int32_t  hc_parse_batch(HcGrammarHandle, const HcStr* words, size_t n,
                        int32_t max_threads,         // 0 = all cores; rayon inside (§7)
                        HcResultBuf* out);
void     hc_buf_free(HcResultBuf*);
```

- Results: one flat length-prefixed binary buffer (no per-analysis allocations, no JSON in
  the hot path): per word → count, per analysis → POS id, root index, morpheme-ID array.
  Canonically ordered (§8 layer 0) so output is deterministic and diffable.
- **No panic crosses the boundary**: every entry wraps `catch_unwind` → error code +
  message buffer. `extern "C"`, UTF-8 in/out, x64 (`x86_64-pc-windows-msvc`); i686 target
  kept building in CI until FieldWorks confirms x64-only (open question §11).
- Grammar handle is immutable and `Send + Sync`: one load, any number of concurrent
  `hc_parse_word` callers (the FieldWorks parallelize-across-words pattern), or one
  `hc_parse_batch` call that parallelizes internally.
- Packaging: NuGet with `runtimes/win-x64/native/hermit_crab.dll`; net48 P/Invoke resolves
  via standard probing (explicit `LoadLibrary` with the package path on net48, where
  `runtimes/` probing isn't automatic).

---

## 5. Rust architecture

Workspace `rust/` at repo root. All product crates `#![forbid(unsafe_code)]` except
`hc-ffi` (the boundary) and, if profiles ever justify it, one reviewed hot-loop exception
gated by Miri (§8 layer 7).

### 5.1 Crate map and dependency direction

```
hc-grammar   XML load + lint + compile → immutable GrammarTables
hc-featstruct  bit-vector FS + interner + DAG unifier + variable bindings
hc-shape     shapes, annotations-as-spans, builders
hc-fst       pattern compile (from hc-grammar), FSA traversal, registers
hc-rules     phonological + morphological rules, templates, strata, cascades
hc-memo      AnalysisStateKey, nogood + template memo, replay
hc-parse     Morpher pipeline: segment → analyze → lookup → synthesize → dedup
hc-ffi       C ABI (cdylib)
hc-cli       `hc-rs` binary: batch, parity-diff, bench (mirrors `hc batch` TSV protocol)
```

`hc-parse` is the only crate `hc-ffi`/`hc-cli` call; everything below it is
engine-internal and free to change shape under the parity gates.

### 5.2 Shapes and annotations — no linked list, no node objects

The C# flat Shape kept an in-array doubly-linked list because it had to preserve
`ShapeNode` reference identity and O(1) mid-list splice under the existing API. Rust owes
nothing to that API:

- A **frozen shape** is a contiguous arena slice: per-node `(char_class_row: u32,
  flags: u8)` in struct-of-arrays form, plus its precomputed traversal projection (the
  filtered annotation view #446 had to cache — here it is just materialized once at
  freeze). Frozen shapes are **interned per parse** (hash-consed → `ShapeId(u32)`):
  clone = copy an id; memo-key equality = integer compare; `ValueEquals` disappears.
- **Mutation** (rule RHS application) goes through a `ShapeBuilder`: copies the frozen
  slice into a scratch `Vec` (a bump-arena pointer copy — this is exactly the memcpy the
  C# COW design existed to avoid *because the CLR made it expensive*; an arena memcpy of
  a ~30-entry SoA is nanoseconds), applies inserts/deletes/feature-modifies positionally,
  freezes back to a new interned slice. Insertion order/`Tag` bookkeeping vanishes.
- Annotations (morph boundaries, mods) become **span records** `(start: u16, end: u16,
  kind, data: u32)` in a side `SmallVec` — no annotation tree, no skip lists
  (`BidirList`'s towers, `AnnotationList`: all unported).

Node feature bundles: one row in the shape's feature matrix, §5.3 representation.

### 5.3 Feature structures — bits first, DAG when forced

Grammar-compile-time census (per `FeatureSystem`):

- **Segment-domain FS** (phonetic features on shape nodes, natural-class constraints on
  FST arcs): in every real grammar these are closed-class symbolic features — exactly
  what #446's flat path handles, with #446 having proven the fallback rarely fires on the
  hot path. Representation: `[u64; W]`, one lane per symbolic feature, `W` fixed per
  grammar at load (Sena-scale grammars: W small enough that a bundle is 1–3 cache lines).
  Unify = lane-wise AND + per-lane zero test; subsumption/overlap likewise — straight-line
  branch-poor code the compiler can vectorize. The 64-symbols-per-feature boundary keeps
  #446's regression test (mask arithmetic at exactly 64 — `1u64.checked_shl` semantics
  differ from C#'s masked shift; the fixture from #446 pins it).
- **Syntactic / head / foot / realizational FS**: may contain complex (nested) features,
  string features, and **variables**. These get the full port: a small DAG unifier with
  `VariableBindings`, ~faithful to `FeatureStruct`'s semantics (unify, priority-union for
  the realizational path, subsumption). Frozen instances are **interned globally per
  grammar** (`FsId(u32)`) — the memo key and all gating compares become id compares, with
  a hash-cons table absorbing the deep-equality cost exactly once per distinct value.
- `SymbolicFeature.FlatIndex`'s known process-lifetime leak (#446's accepted limitation)
  disappears: flat indices are per-grammar by construction.

### 5.4 FST engine — only the reachable machinery

#446 proved HermitCrab only exercises the FSA/acceptor path with registers
(`AllSubmatches`, capture groups) — never the transducer-output path. Port surface:

- Pattern → NFA (Thompson-style over `Input` constraints) → the same
  quasi-determinization `Fst.cs` does today where it does it (parity requires matching
  which matchers are deterministic, since result *sets* must match; result enumeration
  order is canonicalized at §8 layer 0).
- Storage: CSR — `states: Vec<StateMeta>`, `arcs: Vec<Arc>` sorted by source state;
  `Arc = { target: u32, constraint: ConstraintId(u32), commands: CmdRange }`; arc
  constraints interned and stored as §5.3 bit-vectors (dense, shared, cache-resident).
- Traversal: iterative with an explicit `SmallVec` stack (the C# recursion depth is
  bounded but unknown; explicit stack also enables the §6.4 budget checkpoint), flat
  `registers: Vec<i32>` scaffold reused across the thousands of per-word `Transduce`
  calls (the #446 scaffold, arena-based), `VisitedStates` as the same inline-u64 epsilon
  set. Variable bindings only materialize when the pattern census says the FST can bind
  variables (most phonological-rule FSTs cannot — skip the machinery entirely per-FST).

### 5.5 Rules, strata, cascades

Rules become **enum dispatch**, not trait objects, in the hot path:

```rust
enum AnalysisMRule { AffixProcess(..), Compounding(..), Realizational(..) }
enum PhonRuleKind { Rewrite(..), Metathesis(..) }   // iterative/simultaneous as modes
```

Rule *data* (subrule specs, output actions: `CopyFromInput`, `InsertSegments`,
`ModifyFromInput`, `InsertSimpleContext`, metathesis reorder) lives in flat per-grammar
tables indexed by `RuleId(u32)`. A `Word` in flight is:

```rust
struct Word<'p> {                     // 'p = parse arena
    shape: ShapeId,
    stratum: StratumId,
    syn_fs: FsId, real_fs: FsId,
    morphs: SmallVec<MorphRecord>,    // allomorph id + span, morph-order key
    trail: TrailRef<'p>,              // persistent cons-list: (RuleEvent, parent)
    rule_counts: CountsRef<'p>,       // persistent multiset (small sorted vec, shared)
    non_heads: SmallVec<WordRef<'p>>,
    flags: WordFlags,                 // partial, last-rule-final, …
}
```

`Word` "clone" — the ~8,800-per-word cost center of #438 — is a **copy of ~5 machine
words** plus a persistent-trail cons; the shape and FS are ids. This single design choice
is where most of the C#-vs-Rust gap will come from.

### 5.6 What is deliberately *not* ported

`OrderedBidirList`/`BidirList` skip lists, `AnnotationList` trees, `Freezable`/
`ValueEquals` infrastructure (immutability is the type system's job; equality is id
compare after interning), the FST transducer-output path, `ShapeNode`/`Annotation`
reference identity, LINQ-shaped enumeration layers, `InstrumentedRule` (replaced by
`tracing` spans + counters behind a cargo feature).

### 5.7 Line budget (~13,000)

| Crate | Est. LOC | Notes |
|---|---:|---|
| `hc-grammar` | 3,000 | XML loader+lint 1,600; compile-to-tables 1,400 |
| `hc-featstruct` | 1,600 | bit-vector ops 400; DAG unifier + variables 900; interner 300 |
| `hc-shape` | 900 | SoA shape, builder, spans, interner |
| `hc-fst` | 2,200 | pattern compile 900; traversal + registers 1,100; visited/epsilon 200 |
| `hc-rules` | 3,200 | phonological 1,300; morphological+templates+strata 1,900 |
| `hc-memo` | 500 | state key, two tables, replay, in-flight guard, cap |
| `hc-parse` | 800 | pipeline, trie lookup, synthesis gating, dedup, canonical order |
| `hc-ffi` | 500 | C ABI, buffers, catch_unwind, abi version |
| `hc-cli` | 400 | batch TSV (protocol-compatible with `hc batch`), bench, parity-diff |
| **Total** | **13,100** | tests/fixtures/benches excluded (≈5–6k more) |

Estimate sanity: C# surface is ~27k lines (HC 12.4k + FiniteState 4.7k + FeatureModel 4.0k
+ Annotations 1.6k + Matching 2.0k + Rules 0.7k + DataStructures 2.7k), and §5.6 removes
roughly half of it structurally. If tracking shows >15k trending, the cut line is
pre-agreed: realizational-rule and metathesis long-tail cases lint as unsupported-v1
(managed fallback) rather than inflating the port.

---

## 6. Memory model — the no-GC answer to the Server-GC problem

### 6.1 Ownership tiers

1. **Grammar tier** (immutable, shared): all §5 tables, interned constraint bit-vectors,
   tries, FSTs — built once by `hc_grammar_load`, wrapped in `Arc`, `Send + Sync`. This is
   the only long-lived allocation in the system. Expected size: low tens of MB for
   Sena-scale grammars (33k-line XML).
2. **Parse tier** (per word): one `bumpalo::Bump` arena from a thread-local pool. Every
   transient of the search — words, trails, shape slices, interner tables, memo tables,
   FST scaffolds — allocates here by pointer bump and is freed *en masse* by arena reset
   when the word completes. No free-list churn, no fragmentation, perfect temporal
   locality.
3. **Result tier**: analyses are copied out of the arena into the flat result buffer
   before reset; nothing borrows the arena past the parse.

Consequence: **peak memory ≈ grammar + Σ(active threads × that thread's current word's
arena high-water)**, deterministically, on any host, with zero configuration. The #451
memo cap (100k entries) carries over as the bound on the one structure that can grow with
search size; arena high-water per worst-known word (cinacemerwa class) is a tracked
benchmark metric (§10) with a regression threshold, not a hope.

### 6.2 Interning discipline

Per-parse interners (shape, in-flight FS) live in the arena and die with it — no
cross-word cache invalidation problem, matching #451's explicit "per-parse scope, cross-word
extension deferred" decision. Grammar-tier interners (constraints, static FS, strings,
morpheme/rule ids) are frozen at load.

### 6.3 The memoization, ported

- `AnalysisStateKey` → `{ shape: ShapeId, stratum: StratumId, syn_fs: FsId, real_fs: FsId,
  non_head_count: u8, rule_counts_hash: u64, counts: CountsRef }` — 32 bytes hot;
  equality = five integer compares then a short sorted-vec compare on the rare hash hit.
  Commutative multiset hash kept (XOR of `(rule_id_hash, count)`).
- `AnalysisScope` → `ParseMemo { nogood: HashMap<Key, ()>, template: HashMap<Key,
  MemoSubtree>, in_flight: HashSet<Key>, entries_cap: u32 }` — `hashbrown` maps,
  arena-backed. **No `ConcurrentDictionary` equivalent needed**: the within-word search is
  single-threaded by design (§7), so the C# thread-safety layer (and its cost) is dropped,
  matching the C# comment that only the sequential cascade ever used it.
- `ReplayOnto` → `MemoSubtree` stores results as (suffix trail, result core); a replay
  arrival builds each result as `arrival.trail ++ suffix` over the persistent trail — no
  clone-and-graft deep copies. Phase 7b/9-pre's finding (replay materialization tax is
  real but bytes track CPU) predicts this is where Rust's persistent trail wins again.
- Order of application: memoize the **template battery first** (93% of wall time on the
  worst words — Phase 3b), then the mrule cascade nogood/positive tables (Phases 2–3).
  That ordering is also the port's own de-risking: each table lands as a separate commit
  behind its own parity run, and each is independently feature-flagged (`--memo=off`
  in `hc-rs` reproduces the unmemoized engine for A/B, the §8 "fair-baseline knob").

### 6.4 Budget checkpoints (reserved now, filled by the #448 follow-on)

The traversal stack loop, the cascade expansion loop, and the template battery each call
an inlined `budget.check()?` that is a no-op constant-fold when budgets are disabled.
When #448's port lands: soft-stop with partial results + diagnostics struct, and any
exhausted subtree is not memoized (ground rule 2).

---

## 7. Threading model

- **Across words only.** One word's search is sequential (the memo made within-word
  parallelism obsolete — #451 measured the sequential memoized cascade beating the
  parallel unmemoized one; the ParallelCombinationRuleCascade has no Rust descendant).
- `hc_parse_batch`: `rayon` scoped pool sized by `max_threads`; words **sorted
  longest-surface-first** into the work-stealing queue (Phase 8a: 2.9× → 1.36× packing
  slack), results written to a pre-sized `Vec<Option<..>>` by original index — output
  order independent of completion order, and no false sharing (each slot written once;
  per-thread stats structs are cache-line padded).
- `hc_parse_word` from many host threads: identical behavior — the grammar tier is
  `Sync`, each call takes a thread-local arena. FieldWorks can keep its own
  parallelize-across-words pattern unchanged.
- Determinism requirement: same word → byte-identical result buffer at any thread count,
  because per-word computation shares nothing mutable. This is a §8 property test, not
  an aspiration.
- Scaling target: ≥ 0.8× ideal through 8 threads, ≥ 0.6× at 16 on the Sena reference
  batch (the C# engine's own scaling curve from #446's table is the floor to beat; its
  words/sec plateaued ~dop 12 under GC pressure — Rust has no collector to saturate).

---

## 8. NO REGRESSIONS — the multi-layered parity plan

The prime directive, inherited verbatim from both branches: **parse results are not
allowed to change.** Every layer below exists to make a behavioral difference between the
managed and Rust engines impossible to ship and hard to even *create* locally.

**Layer 0 — canonical result form.** A parse result set is compared as its **signature**:
per analysis `join("+", morpheme IDs in morph order) + "|" + surface`, set sorted,
joined with ";" (empty = "-") — the exact `BatchCommand` protocol from Phase 0, so every
existing C# TSV baseline is directly reusable. The Rust engine additionally *emits* in
canonical order (§4.2), making raw buffers diffable too.

**Layer 1 — foundation parity gates (before any rule logic exists).**
- *NFD gate*: for all 7,915 corpus words + every string rep in all three grammars, .NET
  `Normalize(FormD)` output vs Rust `nfd()` output — byte diff must be empty (Unicode
  version skew detector; re-run whenever either toolchain bumps).
- *Segmentation gate*: `CharacterDefinitionTable.Segment` per word: identical node
  count + character-definition ids from both engines.
- *Loader gate*: a `dump-grammar` command on both sides emits a normalized inventory
  (strata, rule counts/ids/order, template slots, lexicon sizes, feature systems,
  natural-class expansions) — diffed per grammar. Catches loader drift structurally,
  before it can hide inside parse behavior.

**Layer 2 — conformance fixture suite (the unit-test port, made cross-language).**
The 68 HC C# unit tests build grammars *in code*, so they can't be re-run against Rust
directly. Instead: a one-time C# exporter walks every test fixture, serializes its
grammar via the existing `XmlLanguageWriter` (crash fix for dangling co-occurrence rules
already on PR #450 — take it), and records `(word → expected signatures)` for every
assertion, producing ~200 data-driven fixture files (`tests/fixtures/*.xml` + `.tsv`).
Both engines run the suite in CI: C# proving the export is faithful (round-trip check
against the original in-code assertions), Rust proving conformance. Every future engine
change on either side runs the same fixtures. This suite is the *contract* that keeps two
implementations from drifting for as long as both exist.

**Layer 3 — corpus golden parity.** `hc-rs batch` vs C# `hc batch` (BatchCommand exists
on `parse-optimization`; cherry-pick it to the working branch as part of M0):
- Per-PR gate: Indonesian full (121), Sena fast gate (first 300 words + the 13
  measured-worst: `cinacemerwa kukucitirani cinagumanika pinacemerwa kamatamisa
  anatawirirwambo pidafikawo manyeredzero musandilesera ndinakuikhirani katambirambo
  ndinakupangani atawirambo`), Amharic full (673). Diff of signature columns = empty.
- Nightly: full Sena (7,121 words) — **with the watchdog recipe** (one word has crashed a
  host; crash-resumable TSV, `--start I` resume, memory watchdog) until the #448 budgets
  port lands and makes the watchdog obsolete.
- Stress config: the #446 uncapped `MaxUnapplications=0` 60-word Sena subset (the
  combinatorially expensive setting that exercises search-order edge cases).
- Every corpus run is done at 1 thread and N threads and diffed against itself
  (determinism) and against the managed TSV (parity).

**Layer 4 — differential fuzzing + property tests.**
- `cargo-fuzz`/`proptest` word generator: corpus words mutated (segment swaps/dups/drops,
  boundary chars, combining marks, invalid shapes → both engines must agree on
  `InvalidShape` too) + random strings over each grammar's character definitions; both
  engines, signatures diffed; minimized counterexamples become permanent fixtures.
- Properties, each a `proptest`: (a) thread-count invariance; (b) memo on/off invariance
  (`--memo=off` is the fair-baseline knob); (c) replay-vs-research invariance (force the
  in-flight fallback path); (d) parse determinism across repeated calls; (e) for words the
  managed generator can synthesize: generate → parse recovers the source analysis.
- Grammar-level fuzzing: fixture grammars with strata/rules randomly ablated (this is
  also the **partial-grammar** mechanism of §10) — parity must hold on every ablation,
  which corners loader/rule bugs far better than the full grammars alone.

**Layer 5 — shadow mode in the product.** `HcEngine.Shadow` (§4.1) runs both engines,
returns managed, logs any signature mismatch with word + grammar hash. This is the field
safety net: FieldWorks beta cycles run Shadow; the switch to `Rust` default is gated on
**zero shadow mismatches over an agreed soak period**, not on our own test taste.

**Layer 6 — refuse rather than risk.** Loader lint (§3): grammars using any construct the
Rust engine hasn't implemented (or that fixture coverage hasn't certified) are rejected
with a structured reason → automatic managed fallback. The unsupported list is data, in
one file, burned down milestone by milestone.

**Layer 7 — Rust-side soundness.** `#![forbid(unsafe_code)]` outside `hc-ffi`; Miri on
the FFI + any future unsafe block; `cargo clippy -D warnings`; ASAN job on the fuzz
targets; panic-across-FFI covered by a dedicated abort-safety test (panicking grammar
injected via test hook → error code, host process healthy).

**Process rules (from `parse-optimization.md` ground rules, still binding):** one
phase per PR-sized commit, independently revertible, measurements in the commit message;
results-identity is the only acceptance gate; corpora stay untracked local files with
self-skipping `[Explicit]`/`#[ignore]` tests; any cache/gate is bypassed under tracing
(trivially true: tracing never reaches Rust); exhaustion poisons caches once budgets exist.

---

## 9. Performance engineering: L1, and how we keep ourselves honest

Design-time choices already made for cache behavior: SoA everywhere (§5.2), `u32` handles
instead of pointers (halves working set, kills pointer-chase), interned constraint rows
shared across arcs (the hot unify operands stay resident), CSR arc arrays walked
sequentially, `SmallVec` for the ubiquitous 0–4-element collections, hot structs audited
`≤ 64` bytes (`static_assert`-style size tests so a refactor can't silently fatten
`Word`/`Arc`/state-key), arena allocation giving temporal locality for free, enum dispatch
keeping the branch predictor trained per rule kind, no hashing inside the traversal loop
(hash only at memo boundaries).

Measurement discipline (the #446/#451 lesson — profile, don't guess; land only measured wins):
- `criterion` micro-benches per crate (unify, transduce, shape-build, memo lookup) with
  CI trend tracking; a >5% regression on any tracked bench fails the PR.
- Macro benches via `hc-rs batch` on the reference sets; wall time, words/sec, peak
  working set (Windows: `PeakWorkingSetSize` via `GetProcessMemoryInfo`; the CLI prints it).
- Hardware-counter passes at each milestone on Windows (VTune or AMD uProf: L1d miss
  rate, IPC, branch miss) and cachegrind under WSL for reproducible cache simulation.
  L1 targets are set from the first M2 baseline, then ratcheted — not invented up front.
- Every optimization PR carries its numbers; anything that doesn't clear a 10% local win
  (or 3% end-to-end) is closed as a documented negative result, `parse-optimization.md` style.

---

## 10. Milestones, intermediate comparisons, and the final benchmark

Each milestone = one PR-sized unit with its own parity gate and (from M2) its own
timing/memory comparison. Intermediate comparisons use **progressively larger grammars**
(Indonesian 2.5k-line XML → Amharic 17.6k → Sena 33k) and **ablated partial grammars**
(layer-4 mechanism: e.g. phonology-only, single-stratum, templates-off) so each layer's
cost/win is attributed before the next lands — per your instruction, partial-grammar
numbers are for steering only; nothing is *concluded* until the M9 full matrix.

- **M0 — Harness first (no Rust yet).** Cherry-pick `BatchCommand` (+ PR #450's writer
  fix) onto the working branch; capture managed golden TSVs for all three grammars from
  both baselines: `master` (what FieldWorks ships) and `parse-optimization` (the strongest
  C# engine — the honest comparison target), workstation GC. Build the layer-2 fixture
  exporter; land the fixture suite green on C#. *Gate:* two consecutive runs identical
  (determinism); fixtures round-trip.
- **M1 — Workspace + grammar foundation.** Crates scaffolded; `hc-grammar` loads all
  three XMLs; `hc-featstruct` bit-vectors + interner; NFD/segmentation/loader dumps.
  *Gate:* layer-1 gates green on all three grammars.
- **M2 — FST engine.** Pattern compile + traversal + registers. *Gate:* fixture-derived
  pattern-match cases green. *Bench:* transduce micro-bench vs a C# harness on identical
  pattern/word pairs — first L1/IPC baseline recorded.
- **M3 — Phonological rules.** Rewrite/metathesis/epenthesis, iterative+simultaneous,
  analysis and synthesis directions. *Gate:* phonology fixture subset + **phonology-only
  ablated grammars** parity on all three corpora. *Bench:* phonology-only timing/memory
  vs managed on the same ablations.
- **M4 — Morphology analysis side, unmemoized.** Affix process, compounding,
  realizational, templates, strata, linear+unordered cascades. *Gate:* full analysis
  candidate sets match managed (managed instrumented to dump candidates) on Indonesian +
  Amharic; Sena fast gate under a step cap.
- **M5 — Lexical lookup + synthesis.** Trie, environments, co-occurrence rules,
  obligatory features, full `ParseWord` parity. **First end-to-end "words in, morphemes
  out."** *Gate:* layer-2 fixtures fully green in Rust; layer-3 per-PR corpus gate green.
  *Bench:* full-pipeline unmemoized vs managed-unmemoized (fair baseline), 1 thread.
- **M6 — Memoization.** Template memo first, then nogood, then positive replay (§6.3
  order), each behind `--memo` flags with its own parity run including the
  `MaxUnapplications=0` stress set. *Bench:* Sena heavy-13 sequential — expect the ~5×
  analysis win of #451 to compound with the native gains; memo hit rates printed.
- **M7 — Threading + batch.** rayon batch, longest-first, determinism property tests.
  *Bench:* 1/2/4/8/16-thread curve on the Sena 313-word reference batch; arena high-water
  and RSS recorded per thread count.
- **M8 — FFI + bridge + shadow.** `hc-ffi`, NuGet packaging, `Bridge` facade, Shadow
  mode; a net48 smoke-test app P/Invoking the DLL (FieldWorks stand-in). *Gate:* layer-7
  abort-safety; net48 end-to-end parity on all three grammars via the facade.
- **M9 — Final one-to-one benchmark + parity certification.** See below. *Gate:* full
  Sena 7,121-word parity (nightly config, both thread extremes), zero diffs, all three
  grammars.
- **M10 (follow-on, post-parity):** #448 budgets/lint port; fuzzing soak; FieldWorks
  Shadow-mode beta; only then default-flip evaluation.

**M9 final matrix** (fixed, documented hardware; N repetitions with median + spread):

- Engines: C# `master`, C# `parse-optimization`, Rust — all **workstation GC** for C#
  (Server GC excluded by deployment policy; one Server-GC column may be recorded for
  reference, clearly marked non-deployable).
- Grammars: Sena, Indonesian, Amharic — full grammars, full standard word sets (Sena:
  313-word reference batch as the headline + full 7,121 with budgets/watchdog for the
  record; Indonesian 121; Amharic 673) plus the heavy-13 set reported separately.
- Threads: **1, 2, 4, 8, 16** (host-side parallelism for C# per its own supported
  pattern; `hc_parse_batch` for Rust).
- Metrics per cell: wall-clock, words/sec, peak working set, (Rust) arena high-water,
  (C#) GC counts + allocated bytes; parity column (must read 0 diffs).
- Deliverable: results table appended to this document + the raw TSVs archived, exactly
  as #446/#451 published theirs.

---

## 11. Risks and open questions

| Risk | Mitigation |
|---|---|
| Loader semantic drift vs `XmlLanguageLoader` (1.5k lines of accreted behavior) | Layer-1 structural dump diff; layer-2 fixtures exercise loader output behaviorally; lint-and-fallback for anything uncertain |
| Unicode/NFD skew between .NET and Rust Unicode tables | Dedicated corpus-wide NFD gate (§8 layer 1), re-run on toolchain bumps |
| Result-*set* divergence from traversal-order differences (dedup keeps first-seen) | Canonical ordering before dedup on both sides is part of layer 0; the `MaxUnapplications=0` stress set exists precisely to catch order sensitivity |
| FS variable/complex-unification long tail (metathesis groups in optional constructs, realizational edge cases — #446 documented a pre-existing `.Success` gap here) | Fixtures target these explicitly; lint-and-fallback shrinks exposure; the known master gap is reproduced bug-for-bug, not "fixed" silently (parity first, fixes as separate flagged commits on both engines) |
| 13k LOC underestimate | Budget tracked per milestone (§5.7); pre-agreed cut line = lint-as-unsupported for the long tail, never scope-creep into the parity gates |
| Dual-engine maintenance drift after ship | Layer-2 conformance suite is the permanent contract; any engine PR (C# or Rust) runs it; Shadow mode telemetry in the field |
| FieldWorks bitness (x86 builds?) | Keep `i686-pc-windows-msvc` compiling in CI until FieldWorks confirms x64-only; decision needed before M8 packaging |
| One Sena word historically crashed hosts | Watchdog + resumable TSV until #448 budgets port (M10) makes runaway words a soft-stop |
| `parse-optimization`/#446 not yet merged to master | Plan does not depend on merge timing: M0 captures golden TSVs from both C# baselines; the fixture exporter runs on the `parse-optimization` build; the Rust engine is parity-gated against **behavior** (identical on both baselines — both PRs proved byte-identity to master) |

---

## 12. Definition of done

1. `HC_ENGINE=rust` in a net48 host returns, for every word of Sena, Indonesian, and
   Amharic corpora and every layer-2 fixture, **byte-identical analysis signatures** to
   the managed engine — at 1 and 16 threads, memo on and off.
2. The M9 matrix is published in this document, with Rust beating C# `parse-optimization`
   (workstation GC) on words/sec at every thread count and on peak working set at 8+
   threads, on all three grammars. (Anything less triggers a documented investigation,
   not a quiet ship.)
3. Shadow mode has run a full FieldWorks test cycle with zero mismatches.
4. Switching engines requires exactly one configuration change, and every unsupported or
   failing native path falls back to managed, loudly, per call.

---

## 13. Parity audit (2026-07-06): remaining gaps and the testing strategy to close them

M1–M8 landed and were independently verified (build/clippy/tests green, plus an out-of-band
re-diff against the golden TSVs for every milestone that touched parity). At that point measured
parity was **Indonesian 68/121 (56%)**, **Amharic 487/673 (72%)**, **Sena not tractable** (≈10/300
words complete in 120s at `--step-cap=200000`). M9's own gate requires zero-diff parity on all
three grammars at both thread extremes — not close, and the remaining distance is an open-ended
set of correctness residuals rather than a bounded architectural milestone. Per the advisor's
explicit guidance at that checkpoint: **do not publish a timing/benchmark matrix against a
non-parity engine** — the numbers would be real but misleading (part of any "Rust is faster" delta
would come from skipping work, not doing it faster), so M9's matrix stays gated on this section's
work landing, not attempted early.

This section is a full line-by-line C#↔Rust audit (four tracks — morphology, phonology/FST,
loader/pipeline, and test coverage — each run as an independent read-only pass over the C#
reference source at `.worktrees/parse-opt/src/` against every corresponding Rust crate) plus the
testing strategy to close the gaps and then hold parity permanently. The four raw reports, with
full file:line citations for every row below, are archived at `rust/parity-out/audit/{A-morphology,
B-phonology-fst,C-loader-pipeline,D-testing}.md` — this section is their consolidated,
priority-ordered summary; consult the raw reports before implementing any item, they contain the
exact C#/Rust line numbers and fix sketches this summary compresses out.

Two of the highest-impact findings were independently re-verified (not just read from the
sub-agent report) before being trusted: the `AffixTemplate final` DTD-default bug (§13.2.2) by
reading `XmlLanguageLoader.cs:209-218`'s `XmlReaderSettings` directly and confirming
`DtdProcessing.Parse` + `ValidationType.DTD` really does inject DTD-declared attribute defaults
into the parsed tree — this *corrects* a wrong conclusion reached by an earlier session that had
looked at the same question without noticing the DTD-injection mechanism; and the boundary-marker
`Type` dimension gap (§13.2.1) by reading `CharacterDefinitionTable.cs:56-81`'s `FeatureStruct`
construction and `hc-fst/src/fst.rs`'s own doc comment confirming an empty lane vector matches any
segment.

### 13.1 Consolidated gap inventory (severity-ranked, deduplicated across the four audits)

Severity: **BLOCKER** = causes wrong results on many words or makes a grammar intractable;
**MAJOR** = wrong results on some words; **MINOR** = narrow/latent, not exercised at scale by
these three grammars; **N/A-V1** = confirmed zero-occurrence scope cut per §5.7, verified still
accurate. Where two audit tracks found the same defect from different consumer/producer sides
(e.g. "environments loaded" from the loader side and "environments never read" from the pipeline
side), the rows are merged with both citations.

#### Tier 0 — fix before anything else (poisons every other gate)

| # | Gap | Source | Fix effort |
|---|-----|--------|-----------|
| T0 | **Step-cap-truncated results are non-reproducible across separate process invocations** — confirmed, reproduced (Sena first 20 words, `--step-cap=20000`, `--threads 1`, 3 runs, 2/20 words differed pairwise every time). ~14 `HashMap<_,_>` accumulators across `hc-parse`/`hc-rules`/`hc-memo` use the default per-process-random-seeded hasher; when the step cap fires mid-iteration, *which* candidates already got processed depends on that seed. Uncapped runs are unaffected (final sets are order-independent and signatures sort) — this only bites truncated (capped) words, which is most of Sena today. | Audit C §5 (site list), Audit D §(d)/D1 | **S** — swap every listed site to a fixed-seed hasher or `IndexMap`; re-run the exact 3× repro to confirm zero diffs |

This is listed separately from the tiers below because it **blocks trustworthy measurement** of
several of them: any fix that touches Sena cannot be verified by a single before/after diff while
this bug stands, since two "after" runs of the *same* binary can legitimately disagree on a capped
word. Fix T0 first, or budget for 3×-repro-style verification on every subsequent Sena change.

#### Tier 1 — BLOCKERs (do first; highest word-count impact)

| # | Gap | C# source | Rust status | Grammars/words affected | Fix effort |
|---|-----|-----------|-------------|--------------------------|-----------|
| 1 | ~~**Boundary-marker pattern nodes (`+`) have no `Type=Boundary` discrimination.**~~ **FIXED** (`Type` lane, real fix), **but the audit's headline impact claim was wrong — corrected here.** The bug itself was real and is now fixed: every char-def (segment and boundary) previously compiled to only phonological-feature lanes, so a boundary's lane vector was empty and `hc-fst`'s `flat_unifiable` treats an absent lane as "matches any segment" (confirmed by reading `CharacterDefinitionTable.cs:56-89`: C# unconditionally injects a `Type=Segment`/`Type=Boundary` symbolic feature into *every* char-def's `FeatureStruct`, even in the `fs==null` branch, and `NaturalClass.cs:7-15` unconditionally injects `Type=Segment` into every natural class). Fix: `Type` modeled as a real, always-appended extra feature lane in `hc_grammar::featsys::PhonFeatureSystem` (2 symbols, appended last so no existing `FlatIndex` shifts) — every char-def's lane row now carries a correctly-pinned `Type` lane, `Feature`-kind natural classes get an injected `Type=Segment` pin (mirroring C#), `Segments`-kind classes get it for free via the existing member-union logic, and the `hc-rules/src/shape_feat.rs` boundary-node hardcoded-unconstrained-lanes bug is removed. **But** re-measurement shows this closed exactly **one** Indonesian word (`meolah`, 67/121→68/121 — precisely undoing the Tier-1 #3 regression above, as predicted), not "53/121." Independently re-verified via matches-vs-gold-set diffing: zero other Indonesian words moved, Amharic unchanged (531/673), Sena unaffected (flagship `mbali` repro byte-identical before/after). **The audit's "100% of the 53 misses" claim is disproven** — the other 52 Indonesian `meN-`/`peN-` mismatches do involve boundary-marker environments (so the gap was real and worth fixing) but are gated by something else that independently causes C# to reject/accept differently; confirmed the affected roots (`olah`, `ambil`) are real, correctly-loaded lexicon entries, ruling out a missing-entry explanation. **Open, undiagnosed**: what actually causes the remaining 52 Indonesian misses. Leading hypothesis, not yet confirmed: Tier-1 #5's phonological-environment-enforcement gap below — `meN-`/`peN-` nasal assimilation is exactly the kind of rule C# gates on a phonological environment, and Indonesian's own `<Environment>` count (5) is small enough that this is worth checking first when #5 is tackled. | `CharacterDefinitionTable.cs:56-89`; `NaturalClass.cs:7-15`; `XmlLanguageLoader.cs:1392-1403` | `hc-grammar/src/featsys.rs` (`Type` lane); `hc-grammar/src/chardef.rs` (per-char-def pin); `hc-grammar/src/load.rs` (natural-class pin); `hc-rules/src/shape_feat.rs` (boundary-lane bugfix) | Indonesian: 68/121, +1 (not +53 as originally estimated); Amharic/Sena: unaffected, no regression | **L** — done; the remaining 52-word Indonesian gap is a **new, separately-tracked open item**, not closed by this fix |
| 2 | **`AffixTemplate final` defaults to `false` in Rust; the DTD says `"true"`, and C#'s XML reader materializes DTD defaults** (`DtdProcessing.Parse` + `ValidationType.DTD`, confirmed by direct read, corrects an earlier session's wrong conclusion on this exact question). All 15 Amharic + 24 Sena `<AffixTemplate>` elements omit the attribute → every one loads as non-final in Rust → every synthesis path needing a template affix is dropped at the final-word gate. | `HermitCrabInput.dtd:259`; `XmlLanguageLoader.cs:209-218,1304` | `hc-grammar/src/load.rs:1334`: `parse_bool(temp.attr("final"), false)` | Amharic (15 templates) + Sena (24 templates): every template-mediated word | **S** — one-line default flip; audit A recommends sweeping *every* DTD ATTLIST default while there (ATTLIST defaults were spot-checked for `blockable`/`multipleApplication`/`partial`/slot-`optional`/`isBound` and all others already match) |
| 3 | ~~**No `StrRep` analog for inserted/underspecified segments.**~~ **FIXED, commit `b0f49aea`.** Corrected diagnosis: the real bug was in `matching_str_reps` (`hc-parse/src/surface.rs`), not `InsertSimpleContext` — that call has zero occurrences on this path, and concrete (non-inserted) nodes were equally affected, since nothing there consulted `char_def` either. On a zero-phonological-feature grammar (Sena) or any `NO_CHAR_DEF` inserted-class node, `flat_unifiable(&[], &[])` was vacuously true against every table entry, rendering the full segment inventory instead of actual class members. Empirically confirmed against the golden: `mbali` now renders `[mn]` (the nasal class), matching golden's bracket pattern — **confirmed the Sena "mbali" blowup mechanism**, superseding the earlier reduplication hypothesis. Fix: `hc-shape` gained `CdSet`/`CdBits` (a variable-length bitset — Amharic's char-def table has 418 members, not ≤64, overturning the audit's u64-suffices sizing estimate), threaded through `ShapeBuilder`, `surface.rs`'s `matching_str_reps`, and the four `InsertSimpleContext` sites in `morph.rs`. Re-measured: Amharic 531/673 (unchanged, real-feature grammars unaffected as expected); Indonesian 68/121 → 67/121, one regression (**meolah**, a `meN-`-prefixed word — not a new defect, since all 53 Indonesian misses are already attributed to Tier-1 #1's boundary-Type gap below, and meolah's prior `-` was itself an accident of this same vacuous-match bug; tracked to re-check once Tier-1 #1 lands). **Known residual, deliberately deferred**: `bridge.rs`'s `nat_class_lanes` still over-approximates `Segments`-kind class membership (lane union) when matching *existing* segments in a pattern LHS/environment — `hc_fst::Segment` carries no char-def dimension, so this needs either a frozen-FST representation change or a positional post-match filter. Sena's `mrule1`/`nc1` exercises this path, so it remains a real contributor to Sena's over-generation; `rewrite.rs`'s epenthesis path has the same gap in principle but zero `Kind::Epenthesis` occurrences across all 3 grammars, so it's unexercised. | `CharacterDefinitionTable.Add`'s `fs==null` branch gives inserted-class nodes a real `StrRep` disjunction (cs:68-76); consumed by `GetMatchingStrReps`/`IsMatch` (cs:96-106,274-282) | `hc-shape/src/lib.rs` (`CdSet`/`CdBits`/`EffectiveCdSet`); `hc-parse/src/surface.rs` (`matching_str_reps`); `hc-rules/src/morph.rs` (`ctx_cd_set`) | Sena (fixed, catastrophic case); Indonesian (67/121, -1 tracked); Amharic (531/673, unaffected) | **L** — done |
| 4 | **Missing `MaxApplicationCount` unapplication gate** — C# unapplies each rule at most once per analysis path by default (every rule in all 3 grammars has the DTD default `multipleApplication="1"`); this is C#'s strongest combinatorial bound, capping trail length before memoization even helps. Rust has no such gate — `apply_one_mrule` relies on a self-loop guard that never fires (the unapplication trail always grows) — so a rule can re-unapply indefinitely, both exploding the Sena search space and, more importantly, **producing wrong results**: guided synthesis re-confirms whatever the trail says, so a doubly-unapplied rule re-applies twice and can synthesize a surface C# would never generate. | `AnalysisAffixProcessRule.cs:46-52`; `AnalysisCompoundingRule.cs:48`; `Word.cs:393-398` | `hc-rules/src/stratum.rs:270-308` (`apply_one_mrule`) — no count check; `Word.unapplied_rule_counts` (the exact needed data) already exists and is only read by the M6 memo key | All three grammars: search-space blowup (compounds with T1#3 into the Sena mechanism) *and* end-to-end over-generation | **S** — 3-line gate: check `word.unapplied_rule_counts` against `rule.max_apps` (both fields already exist) before `tick()`, mirroring C#'s placement inside the rule's own `Apply` wrapper |
| 5 | ~~**Final-validation cluster (`Allomorph.IsWordValid`) is loaded but never enforced.**~~ **FIXED for (a)/(b)/(c); (d) deliberately deferred.** New `hc_rules::validity` module, wired into `hc-parse/src/morpher.rs`'s `is_word_valid`. (a) **Environments**: reuses `rewrite.rs`'s `EnvFst`/`compile_env`/`left_env_ok`/`right_env_ok` verbatim (widened to `pub(crate)`, no duplicated matching logic) — per-morph spans are *derived*, not stored: `MorphRecord` still only carries `order` (leftmost interior position), so a morph's span end is computed as the next morph's `order − 1` (or the shape's last interior index for the rightmost morph), verified exact for concatenative morphology and confirmed non-issue for all three grammars' actual environment-bearing allomorphs (none are on a discontinuous morph today — flagged as a residual if a future grammar changes that). (b) **Bound roots**: `RootAllomorphDef.is_bound` + `distinct_count == 1` (dedup on allomorph id, matching C#'s dict-keyed `Word.Allomorphs.Count`). (c) **Required syntactic FS**: `hc_featstruct::subsumes` (pre-existing primitive, unused for this purpose until now) checked against the word's accumulated `syn_fs` at final-validity time, not just at rule-application time. **Re-measured** (matches-vs-gold-set diffing against a `2f238cee` baseline built in a throwaway worktree): Amharic 531/673→**532/673** (+1, zero regressions); Indonesian 68/121→68/121 (unchanged — see finding below); Sena: zero regressions on a 20-word bounded sample (flagship `mbali` repro unchanged, still capped by unrelated gaps). Build/clippy/test green; new `hc-rules/tests/validity_gate.rs` drives real loaded grammars end-to-end (not hand-built structs), including an explicit left/right-anchor-mixup regression test. **Finding, not assumed**: checked whether this explains Tier-1 #1's open 52-Indonesian-misses question — **ruled out**. Indonesian has zero `RequiredEnvironments`/`ExcludedEnvironments` at the allomorph level; its five `<Environment>` blocks are all inside `<PhonologicalRuleDefinitions>` (phonological-rule environments, a separate mechanism, already ported in `rewrite.rs` and unaffected by this fix). The 52-word gap remains open with no new lead from this fix. (d) **Disjunctive-allomorph/free-fluctuation re-check**: deliberately not ported — needs `Word` state for "which allomorph indices were passed over during guided synthesis" that this port's synthesis path never populates; already a separately-tracked Tier-3 residual ("free-fluctuation escape condition too loose... rarely fires on these grammars"), not a regression from leaving it out here. | `Morpher.IsWordValid` → `Allomorph.IsWordValid` (`Allomorph.cs:105-156`); `RootAllomorph.cs:52-63`; `AffixProcessAllomorph.cs:87-105` (`MorphologicalRules/`) | `hc-rules/src/validity.rs` (new); `hc-parse/src/morpher.rs:265-281` (wiring); `hc-rules/src/rewrite.rs`/`morph.rs` (visibility widened for reuse, no logic duplicated) | Amharic +1 (532/673); Indonesian/Sena unaffected, no regression | **M** — done for (a)/(b)/(c); (d) is a documented, separately-tracked residual |
| 6 | **Narrowing/expansion rewrite rules are architecturally live in Amharic, not a hypothetical edge case.** `ana_narrow` bails out unconditionally whenever the RHS is non-empty — the general count-mismatch case (C#'s *common* case for this rule family) is entirely unported for analysis. 5 of Amharic's 7 phonological rules are exactly this shape: e/o-creation glide mergers (3→1, 2→1 segments) and CV merger (2→1, driven by up to 20 alpha variables) — core verb-stem alternation machinery, unported in **both** directions (even where `syn_narrow` fires, it drops all alpha-variable-governed RHS content, producing a degenerate any-segment output). **IMPLEMENTED and unit-tested (branch `wip/tier1-6-narrowing-blocked-on-dedup`, commit `54e250b4`), but NOT merged — real-corpus measurement showed a net regression, zero gains, and this surfaced a new, separate, previously-unknown bug that must be fixed first (see Tier-1 #6b below).** The `rewrite.rs` port itself (`syn_narrow`'s missing alpha-variable resolution; `ana_narrow` split into `ana_narrow_deletion` (unchanged) + new `ana_narrow_general`, mirroring `NarrowAnalysisRewriteRuleSpec.Unapply`; a `new_seg_node` `NodeKind` bug fix for `prule7`'s `BoundaryMarker`-in-LHS shape) is verified correct in isolation via 7 new passing tests and direct C# comparison. But activating `prule6`/`prule7` (the CV-merger, 20-alpha-variable rules) drops Amharic 532/673→516/673 — 16 possessive-suffixed-noun words regress, none gain. Root-caused via direct tracing (not guessed): `prule6`/`prule7`'s legitimately broad matching (Ge'ez script fuses C+V into single glyphs, so a loosely-constrained natural class matches 2-5 segments per word) changes analysis's candidate-discovery/dedup process enough that the correct candidates (using mrule 8/mrule 10) are never produced in the first place, only a dead-end candidate survives. This is **not** a `rewrite.rs` defect — no principled, C#-faithful way exists to gate "safe" vs. "unsafe" narrowing rules, so no ad-hoc exclusion was added. | `AnalysisRewriteRule.cs:69-87` (`NarrowAnalysisRewriteRuleSpec` — "works for expansion, too") | `hc-rules/src/rewrite.rs` on the WIP branch (not on `rust`) | Amharic: implementation ready, blocked from landing | **M** — implementation done; **blocked** on Tier-1 #6b |
| 6b | **NEW, discovered while investigating #6b's own regression — one real bug found and FIXED (commit pending on `rust`), the original 16-word Amharic regression still open.** ~~`WordKey`-completeness and M6 memoization were both directly checked and ruled out~~ (WordKey matches C#'s `Word.ValueEquals`/`FreezeImpl`, `Word.cs:508-546`, field-for-field; the single-word repro fails identically with `--memo=on` and `--memo=off`). **Bug found and fixed**: C#'s analysis-side phonological matcher filter is `Segment|Anchor` only (`AnalysisRewriteRule.cs:34`) — boundary nodes are never presented to the FST traversal at all (`TraversalMethodBase.cs:41-46`) — so `AnalysisRewriteSubruleSpec.CreateEnvironmentPattern` (`AnalysisRewriteSubruleSpec.cs:26-32`) strips every `BoundaryMarker` out of left/right environment patterns via `HermitCrabExtensions.DeepCloneExceptBoundaries` (`HermitCrabExtensions.cs:143-198`) before compiling them (a literal boundary requirement could never match, and C# instead makes the boundary transparent). Rust's `compile_env` was shared, unstripped, across every caller — any analysis-side environment referencing a morpheme boundary could **never** match, silently killing that subrule. Fixed via a new `compile_env_analysis` (boundary-stripping) used by `ana_feature`/`ana_narrow`/`ana_epenthesis`; `compile_env` (synthesis + `crate::validity`'s allomorph-environment gate, which is correctly *not* stripped, matching `AllomorphEnvironment.cs`'s own `Segment|Boundary|Anchor` filter) is untouched. **Confirmed pre-existing and grammar-agnostic, not narrowing-specific** — reachable on `rust` HEAD today, independent of the unmerged Tier-1 #6 branch. Re-measured (matches-vs-gold-set diffing against a clean baseline, independently re-verified by me): **Indonesian 68/121→82/121 (+14, zero regressions)** — Indonesian's `meN-` nasal-assimilation analysis environments reference morpheme boundaries and were silently always failing before this fix, explaining 14 of the 52 previously-unattributed Indonesian misses left open since Tier-1 #1/#5. Amharic 532/673→532/673 (byte-identical — this fix alone doesn't touch `ana_narrow_general`, which only exists on the unmerged branch). Sena: byte-identical on a 20-word sample, zero movement. **The original 16-word Amharic regression from Tier-1 #6's `prule6`/`prule7` activation is still unresolved** — after this fix, `prule5` (a different Amharic rule) now correctly fires for the first time on the repro word `ሌባው` (confirmed via a C#-reference trace comparison), but the word still fails to produce gold's two analyses. Investigation found only 2 matching sites in Rust vs. ~10 in the C# trace for the same structure, not yet reconciled; leading unconfirmed hypothesis has shifted to `hc-parse`'s root-allomorph trie lookup failing to find the root inside the much-larger, heavily-optional reconstructed shape the narrowing cascade produces — a different subsystem than originally suspected, not yet instrumented. | `AnalysisRewriteRule.cs:34`; `AnalysisRewriteSubruleSpec.cs:26-32`; `HermitCrabExtensions.cs:143-198`; `TraversalMethodBase.cs:41-46` | `hc-rules/src/rewrite.rs`'s new `compile_env_analysis`/`strip_boundary_nodes` (on `rust`, landed); the remaining 16-word Amharic blocker still lives only on `wip/tier1-6-narrowing-blocked-on-dedup` | Indonesian +14 (82/121, landed); Amharic 16-word regression still open, root-lookup hypothesis unconfirmed | **S** (boundary-stripping fix, done) + **unscoped** (remaining 16-word blocker, needs further investigation, likely in `hc-parse`'s root-lookup trie) |
| 6c | **RESOLVED. The remaining 39 Indonesian `meN-` non-parses (after #6b's +14) were five compounding bugs, all in the analysis↔synthesis round-trip for the `meN-` archiphoneme cascade (`mrule14` inserts a placeholder `meⁿ+`; `prule4`/`prule5` resolve it via alpha-variable nasal-place assimilation + voiceless-obstruent deletion) — every word hit the first bug, which masked the rest.** (1) `ana_feature`'s analysis-target lanes and "changed" feature set both used `node_pins`, which deliberately excludes alpha-variable-governed features (correct for FST matching, wrong for these two other uses) — `prule4`'s target over-constrained to the archiphoneme's own literal place value (a real assimilated nasal could never match) and its changed-set was empty (nothing ever reverted); fixed via `pattern_var_occurrences(&sr.rhs)` in both places, mirroring `FeatureAnalysisRewriteRuleSpec.cs:47-48,52-55`. (2) `segs_of`/`MutShape::segs` silently dropped a `Segment`-kind node's own `Optional` flag when building the FST match sequence (only boundaries got `Segment::optional`), blocking the morphological un-insertion pattern from treating uncertain re-inserted segments as skippable, despite `hc_fst::traverse.rs:277` already having real skip-path support. (3) `RootAllomorphTrie`'s lexical-lookup consume-edge required exact `char_def` equality, which a natural-class-only reinserted segment (`NO_CHAR_DEF`, from `prule5`'s class-typed LHS) can never satisfy — C#'s `GetMatchingStrReps` is always pure feature unification with no identity gate; fixed by treating `NO_CHAR_DEF` query segments as matching any edge whose lanes unify. (4) `subrule_applicable` was a documented stub that unconditionally rejected any MPR-feature-gated subrule during synthesis — `prule5` declares `excludedMPRFeatures="mpr1"` and so *never fired forward*, for any word, so resynthesis could never reproduce the input surface; fixed via `synthesize_with_mpr` threading `Word.mpr`, mirroring `SynthesisRewriteSubruleSpec.IsApplicable` (confirmed analysis-side has no such gate at all, `RewriteSubruleSpec.cs:46-49`, so `analyze()` was already correct). (5) Surfaced once (1)-(4) let the cascade actually run end-to-end: `syn_feature`'s rewritten nodes kept their pre-rewrite `char_def`, so `hc_shape::node_cd_set`'s Tier-1 #3 identity-lock (added for a *different*, still-valid Sena fix) permanently restricted a feature-changed node's renderable identity to its original literal char-def — the assimilated nasal rendered as nothing instead of "m"; fixed by resetting `char_def` to `NO_CHAR_DEF` on exactly the nodes `syn_feature` touches (untouched/lexical nodes keep their lock, so the Sena fix stays intact). Verified independently (matches-vs-gold-set diffing against a clean baseline): **Indonesian 82/121→118/121 (+36, zero regressions)** — every one of the 33 non-reduplicated `meN-` words now matches gold exactly, plus 3 of 6 reduplicated words fixed as a side effect (`membagi-bagi`, `mengamat-amati`, `mengayuh-ngayuh`); 3 reduplicated words still fail (`memijit-mijit`, `menulis-nulis`, `menyewa-nyewa` — likely `prule3`/reduplication-specific, not investigated here). Amharic: byte-identical raw output (673/673 rows unchanged). Sena: byte-identical on a 20-word sample. Build/clippy/test green; one pre-existing test's assertion (`rewrite_gate.rs`) was corrected from pinning the stale-`char_def` bug to pinning the fixed `NO_CHAR_DEF`-reset behavior. | `FeatureAnalysisRewriteRuleSpec.cs:22-30,47-48,52-55,77-114`; `RewriteSubruleSpec.cs:46-49`; `SynthesisRewriteSubruleSpec.cs:31-70`; `CharacterDefinitionTable.cs:96-106` (`GetMatchingStrReps`) | `hc-rules/src/rewrite.rs` (`ana_feature`, `subrule_applicable`/`synthesize_with_mpr`, `syn_feature`'s char_def reset); `hc-rules/src/morph.rs` (`segs_of`); `hc-parse/src/root_trie.rs` (`NO_CHAR_DEF` edge matching); `hc-rules/src/stratum.rs` (call-site update) | Indonesian +36 (118/121); Amharic/Sena unaffected, no regression | **Done** |

#### Tier 2 — MAJOR (do after Tier 0/1; re-measure before investing further)

| # | Gap | Source | Grammars affected | Fix effort |
|---|-----|--------|--------------------|-----------|
| 7 | Non-head root allomorph never recorded on compounding analysis — every compound's signature is missing its non-head morpheme (the filter added in M5c is accept/reject only, self-flagged at the time) | Audit A #4 (`AnalysisCompoundingRule.cs:119-124` vs `stratum.rs:330-349`) | Indonesian (2 compounding rules), Sena (8), Amharic (1) | M |
| 8 | Reduplication unmodeled — **the port's own "not exercised" flag is stale for Indonesian** (3 true reduplication subrules exist; the flag was accurate only for Sena) | Audit A #5 | Indonesian: 7-8/121 golden words (membagi-bagi, meminta-minta, …) | M |
| 9 | Analysis-side syntactic-FS merge uses the wrong operator: C# `FeatureStruct.Add` is per-feature value-set **union** (widening, keeps later gates open); Rust uses `unify`/`priority_union` (narrowing) — kills analysis candidates C# would keep, causing under-generation | Audit A #6 | Amharic most exposed (52 rule-level + 49 subrule-level `RequiredHeadFeatures`, POS chains) | M |
| 10 | Duplicate-analysis collapse uses exact shape equality (keep-first); C# ignores optional nodes and keeps the longer — near-duplicate variants that C# collapses survive distinctly in Rust, each seeding its own memo subtree (compounds with T1#3/T1#4 into Sena's blowup) | Audit A #7 | Sena tractability + result-selection on all three | M |
| 11 | Analysis-side feature reconstruction over-generalizes a changed feature to fully-unconstrained instead of C#'s negation/complement (`AntiFeatureStruct`) | Audit B #3 | All three grammars wherever a `Kind::Feature` rule's analysis path is reached | M |
| 12 | A quantifier between two alpha-variable-bearing nodes breaks the pattern bridge's positional variable-occurrence bookkeeping (assumes 1:1 pattern-node-to-matched-segment alignment) | Audit B #4 | Indonesian's one reduplication-nasal-harmony rule (prule3) | M (targeted) |
| 13 | Three template/partial-interaction gates missing: non-final-template+partial-rule prohibition; template-applicability requires a non-partial root; partial-word empty-template passthrough | Audit A #8/#9 | Sena (25 partial entries, 6 partial rules, 24 templates) most exposed; Amharic 1 partial rule | S-M each |
| 14 | `MergeEquivalentAnalyses`/`Alternatives` unmodeled (self-flagged, argued result-equivalent for single-stratum Indonesian, explicitly open for 3-stratum Sena/Amharic) — **left open, not disproven**: fix Tier 0/1 first and re-measure before investing here, since Indonesian's own 56% floor shows most misses lie elsewhere | Audit A (pipeline notes) + Audit C #4 | Sena, Amharic (3 strata each) | L, open scope |

#### Tier 3 — MINOR / latent (low priority; mostly stale-comment corrections)

Synthesis-side `MaxApplicationCount` twin (Audit A #10 — nearly moot once Tier-1 #4 lands);
free-fluctuation escape condition too loose (Audit A #11 — rarely fires on these grammars); MPR
group All/Any/Overwrite flattened to set-overlap (Audit A #12 — behaviorally inert today, every
MPR set in all 3 grammars is a singleton); `ModifyFromInput` and `Untruncate` "zero occurrences"
comments are stale (Audit A #13/#14 — Amharic has 1 and 4 respectively, but both are exercised
only in the case Rust already handles correctly — comment-only fix); `GetSkippedOptionalNodes`
unmodeled (Audit A #15, needs synthesis-side optional nodes none of the 3 grammars produce);
`IsTemplateRule` distinction absent (Audit A #16, masked by pipeline flow today);
`CurrentNonHead` indexing shape differs but is equivalent while `MaxStemCount=2` (Audit A #17);
C#'s Phase-5 `HasReachableRoot`/`MaxAnalysisLength` perf gates omitted — results-neutral in C# by
design, but **MAJOR for Sena tractability** specifically, worth porting alongside Tier-1 #3/#4
(Audit A #18); loader silently drops an unrecognized `HeadFeatures` tag where C# hard-crashes —
contract violation, zero occurrences today (Audit C #6); `RewriteMode::Simultaneous` parsed but
unread — zero grammar occurrences today, flag if one is ever added (Audit B #6); single-pass vs
C#'s bounded-repeat deletion/narrowing reapplication loop (Audit B #5).

#### Confirmed N/A-V1 (no action — re-verified accurate, not just repeated)

Realizational rules, `StemName`, `Family`/blocking, morpheme/allomorph co-occurrence rules (all
loader-lint per plan §5.7, zero occurrences in all three grammars, re-confirmed);
`MatchingMethod.Subsumption` (re-verified architecturally dead — every HermitCrab call site sets
`Unification` explicitly, not merely unused by these 3 grammars); `UseDefaults` (re-confirmed 0
`defaultSymbol` declarations); lazy quantifiers (HermitCrab's XML surface has no lazy-quantifier
attribute — architecturally unreachable).

#### Verified PORTED (no gap found — spot-checkable via the raw audit reports' line citations)

The three rule cascades, the memoized combination cascade + `AnalysisStateKey`/`AnalysisScope`,
`WordKey`/`Word.ValueEquals`, `Word::replay_onto`, stratum orchestration (both directions),
template slot walks, `RuleBatch` union semantics, guided synthesis, analysis-LHS construction,
compounding head-capture acceptance (Audit A); FST determinization/registers/`ResultCompare`,
alpha-variable binding scoping, the tree-vs-DAG feature-struct split (Audit B); `RootAllomorphTrie`
+ `Word.SetRootAllomorph`, surface rendering *logic* (correct given non-degenerate inputs — the
divergence is owned by Tier-1 #3, not this layer), the rest of `IsWordValid` (obligatory features,
realizational-unify, rule-completion), the `BatchCommand`/`hc-cli` TSV protocol, every §5.7 loader
lint, and the whole `hc-ffi` surface (Audit C).

### 13.1.1 Deep-review addendum (2026-07-07, post-Tier-1 #6c) — new findings and corrections

A full read-only faithfulness/architecture review at HEAD `b7168840` (every claim verified with
dual file:line citations against the parse-opt C# branch in `.worktrees/parse-opt/`) found three
**new** parity bugs, sharpened several Tier-2 rows, and disproved one Tier-3 theory:

**New PARITY findings (not previously in any tier):**
- **R1 — `syn_narrow` inserts its RHS segments as OPTIONAL** (`rewrite.rs:853-859`: `new_seg_node(g,
  table, n, true)` — the `true` is the `optional` param, apparently abused to get `dirty` as a side
  effect). C# inserts narrow-RHS nodes non-optional and sets dirty separately
  (`NarrowSynthesisRewriteSubruleSpec.cs:31-45`). A forward-narrowed replacement segment becomes
  skippable everywhere downstream (`is_match` accepts a surface without it; spurious `?` in
  signatures). Independent of, and additive to, Tier-1 #6's known `syn_narrow` alpha gap. **S.**
- **R2 — deletion-unapply site enumeration misses the word-initial gap.** C#'s empty-target
  analysis match yields n+1 insertion sites including before the first segment
  (`RewriteRuleSpec.cs:55-77` `isTargetEmpty` branch, `NarrowAnalysisRewriteRuleSpec.cs:24-31`);
  Rust enumerates gaps after each segment only — n sites, never word-initial
  (`rewrite.rs:923-931`). Also inserts on the opposite side of an adjacent boundary node vs C#'s
  `AddAfter(range.Start)`. A deleted word-initial segment is never re-inserted → those analyses
  unreachable. Directionally consistent with #6b's "2 sites in Rust vs ~10 in C#" observation. **S.**
- **R3 — free-fluctuation omitted from the disjunctive-allomorph break.** C# breaks out of the
  allomorph loop after a success only if `!allo.FreeFluctuatesWith(next)` in addition to
  no-env/no-required-FS (`SynthesisAffixProcessRule.cs:235-242`; `Allomorph.cs:80-103`); Rust breaks
  unconditionally (`morph.rs:606-611`) — constraint-equal alternative allomorphs never produce their
  variant words. Couples with the already-deferred #5d under-filtering; the two do not cancel. **M.**

**Sharpened existing rows:**
- **Tier-2 #9 (Add-vs-unify):** confirmed with exact operator semantics — C# `FeatureStruct.Add` is
  per-feature value-set **union** (`SM/FeatureModel/FeatureStruct.cs:453-503`), used at three
  analysis sites (`AnalysisAffixProcessRule.cs:63-68`, `AnalysisCompoundingRule.cs:133-138`,
  `AnalysisAffixTemplateRule.cs:66`); Rust narrows instead at `morph.rs:558-571`,
  `morph.rs:1100-1110`, `stratum.rs:604`, and **hc-featstruct has no widening primitive at all**
  (`ops.rs` exports only `is_unifiable/unify/subsumes/priority_union`). Top-ranked non-narrowing
  Amharic suspect (52 rule-level + 49 subrule-level `RequiredHeadFeatures`).
- **Tier-2 #10 (dedup):** C#'s `Duplicates`/`RemoveDuplicates` (`HermitCrabExtensions.cs:180-207`)
  compares shapes **ignoring Optional nodes** and keeps the **longer** variant, applied
  per-allomorph (`AnalysisAffixProcessRule.cs:58`) and per-subrule
  (`AnalysisCompoundingRule.cs:99-117`); Rust is exact-shape keep-first shared across allomorphs
  (`morph.rs:967-970`, `1152-1155`). Prime named suspect for #6b's candidate-discovery mechanism.
- **Tier-2 #12 (quantifier+alpha):** confirmed as **exactly** the last 3 Indonesian words —
  `prule3`'s left env `[nc3, char29, nc10(α), (nc6)*, char17]` has an unbounded quantifier between
  the α-bearing node and the anchor-adjacent literal; `resolve_bindings` (`rewrite.rs:419-473`)
  assumes 1:1 node↔segment alignment so the α-check hits the wrong segment whenever `(nc6)*`
  consumes ≠1 segments (and silently skips the whole binding when `env.node_vars.len() > s`,
  `rewrite.rs:445`). Fix shape: capture-based positions from the FST, not positional indices.
- **Tier-2 #13 (template/partial gates):** exact missing sites confirmed —
  `SynthesisAffixTemplatesRule.cs:59-77` partial-passthrough vs `stratum.rs:1010-1017`;
  `cs:40` non-partial-root gate absent at `stratum.rs:981-987`;
  `SynthesisAffixProcessRule.cs:86-105` non-final-template+partial prohibition absent in
  `morph.rs::synth_affix`.
- **Tier-2 #7 (compound non-head):** sharpened to three coupled divergences — C# pins a concrete
  root allomorph per surviving candidate and **replaces the non-head's shape/FS with the root
  entry's** (`AnalysisCompoundingRule.cs:119-124`, `Word.cs:148-169`), gates synthesis on the
  non-head FS (`SynthesisCompoundingRule.cs:81-99` — Rust's gate at `morph.rs:989` is vacuously
  true on an empty FS), and records the non-head ROOT morph (`cs:288` — Rust's `attribute_morphs`
  drops all `Origin::NonHead` material, `morph.rs:448-457`). Every compound signature is missing
  its non-head morpheme id even when the surface matches.
- **Tier-2 #8 (reduplication):** census correction — **Amharic has 5 `redupMorphType` subrules**,
  not zero (row previously listed Indonesian-only as stale). C#'s `_nonAllomorphActions` morph
  attribution (`SynthesisAffixProcessAllomorphRuleSpec.cs:23-124,137-259`) remains unmodeled.
- **Tier-2 #14 (MergeEquivalentAnalyses):** the C# oracle runs with `MergeEquivalentAnalyses=true`
  (ctor default; BatchCommand sets nothing) — per-stratum shape-keyed canonical words + `Alternatives`
  suffix-grafting (`AnalysisStratumRule.cs:150-177`, `Word.cs:491-533`, `Morpher.cs:478`). Both a
  parity divergence on 3-stratum grammars and the #2 Sena perf lever.
- **Tier-1 #6 context:** analysis iterative unapply order — C# sweeps reversed-direction
  (rightmost-first for LtoR rules) continuing from match end (`AnalysisRewriteRule.cs:34`,
  `IterativePhonologicalPatternRule.cs:17-47`); Rust restarts from scratch leftmost-first
  (`rewrite.rs:205-218`, `799-835`). Low-moderate Amharic risk on overlapping spans; benign for
  Indonesian empirically. Watch-list.

**Disproven (correct the Tier-3 row):** the C# oracle's Gate-B/Phase-5 pruning is **OFF** for all
three grammars — `GrammarAnalyzer.ComputeMaxAnalysisLength` returns null whenever any stratum has a
compounding rule (`GrammarAnalyzer.cs:48-54`; Sena has 8, Amharic 1, Indonesian 2) and
`EnableLexicalGating` defaults false. So `HasReachableRoot`/`MaxAnalysisLength` **cannot** be the
Sena tractability delta ("MAJOR for Sena tractability" is withdrawn). What C# actually has that
Rust lacks, in measured-impact order: **(1) compile-once patterns** — C# compiles every
matcher/trie at `Morpher` construction; Rust compiles FSTs (Thompson + determinize) inside the hot
loop per-allomorph-per-application (`morph.rs:626`, `958-963`), per-rule-invocation
(`rewrite.rs:616-621`, `853-861`, `917-918`), and per-env-per-morph (`validity.rs:119-120`);
**(2)** Tier-2 #14; **(3)** Tier-2 #10's near-duplicate memo-subtree seeding; **(4)** deep-clone
`WordKey`/`AnalysisStateKey` costs vs C#'s cached frozen hashes (an unused `ShapeInterner` already
exists, `hc-shape/src/lib.rs:526-531`).

**Architecture verdicts (keep / fix):** keep the frozen `hc-fst` CSR core, flat-lane bitsets
(verified adequate: zero disjunctive FS usage anywhere on the oracle branch), `hc-memo`, the crate
boundaries, and the hand-built root trie. The recurring parity-risk pattern is **segment identity
as an accreting patch stack** (`CdSet` + `NO_CHAR_DEF` wildcard + `syn_feature` reset + R1's
overloaded `optional` param) — consolidate node identity (kind, char-def-or-set, lanes, optional,
dirty) into one struct shared by `MutShape`/`OutNode`/`Segment` construction when next touching
that code. Also verified faithful this review (previously unaudited): guided synthesis vs
`TrailDirectedRuleCascade` (same reachable set), the memo replay grafting, `root_trie.rs` matching
semantics post-#6c, `WordKey` ≡ `ValueEquals` field-for-field, and the 6b/6c fixes themselves.

### 13.2 Recommended fix order (revised 2026-07-07; supersedes the original ordering, which is
preserved in git history at `b7168840` and earlier)

Steps 1–5 of the original order (T0, #2, #4, #3, #1) are done, as are #5(a-c), #6b's
boundary-stripping fix, and #6c. Remaining order, with impact/size/deps:

1. ~~**R1** (`syn_narrow` optional-RHS bug)~~ **DONE `2bba819d`** (`new_seg_node_dirty` decouples
   optional from dirty; latent — byte-identical on all three corpora, as this section predicted).
2. ~~**R2** (word-initial deletion-unapply gap + boundary-side placement)~~ **DONE `29241a84`**
   (site 0 added; the boundary-side-placement half resolved by hand-trace, no code needed — no
   RtoL narrowing/deletion rule exists in any of the three grammars, and for LtoR the existing
   placement already matches `AddAfter(range.Start)`; also fixed a crash the new site exposed in
   `ana_feature`'s positional `changed`-vector alignment when an Optional segment is transparently
   consumed mid-span). Latent, byte-identical.
3. ~~**Tier-2 #10** (optional-blind longer-wins dedup)~~ **DONE `b41d75c9` + `bbe3fa6d`** (per-
   allomorph/per-subrule scope; comparator = lanes + effective cd-set — review correction to the
   first draft: C#'s `NodeComparer` compares the whole FS and StrRep lives *inside* the FS, but
   only on zero-phon-feature grammars, `XmlLanguageLoader.cs:670-673` +
   `CharacterDefinitionTable.cs:68-81`, so a lanes-only comparison would over-collapse exactly
   Sena; on feature-bearing grammars Rust is now finer than C#, the safe direction, documented
   residual). Latent, byte-identical.
4. **Land Tier-1 #6** (narrowing) — **BLOCKED, reordered after the compile-once cache (step 5).**
   2026-07-08 landing attempt (clean rebase onto `bbe3fa6d` = R1+R2+#10, branch
   `wip/tier1-6-rebased-bbe3fa6d`, commits `01c84c8e`/`311dbf33`/`7e660d6b`): build/clippy/tests
   green, Indonesian/Sena byte-identical, but **Amharic became catastrophically slow, superseding
   the old 16-word-regression framing** — 8/673 words in 590s; the ordinary verb `ሄደ` never
   completes (~9.2ms/step at `--step-cap=5000` vs µs-scale steps normally; HEAD-without-narrowing
   does the full corpus in <2s at the same cap). A/B toggle isolates it to `prule6`/`prule7`
   (environment-free, 20-alpha-variable CV-merger rules): `ana_narrow_general` reconstructs at
   every matched span per call, flooding shapes with Optional nodes, which compounds against the
   compile-in-hot-loop debt — and #10's keep-longer dedup (correct per C#) now *prefers* the
   bloated shapes, making the old repro `ሌባው` ≥5x slower than pre-rebase. Not a rebase artifact
   (the pre-rebase tip hangs on `ሄደ` too). C# runs these same rules fine because its matchers are
   compiled once at Morpher construction, so per-step cost stays flat as shapes grow. **Do not
   re-attempt until step 5 lands; then re-measure before any semantic work.**

   **UPDATE 2026-07-08, post-cache + root-cause (`wip/tier1-6-plus-cache-probe` @ `97504528`,
   `NARROWING-FINDINGS.md`):** Two things resolved, one reframed. (i) Found + fixed the true
   Rust-specific over-generation: `ana_narrow_general` accepted match spans WIDER than the RHS
   pattern (over an Optional-flooded shape the nondeterministic FST reports an `ENTIRE_MATCH` whose
   offsets straddle transparently-skipped Optional nodes), so a single-node CV-merger target matched
   multi-segment windows and spliced a reconstruction at each — the exact guard `ana_feature`
   already has (C#'s target binds each position with a named `Group` capture, `Matcher.cs:174-216`,
   so its range is always tight). The guard cuts `ሄደ` 87s→1.8s and makes `ሌባው` complete + match
   gold. (ii) C# ALSO floods (30M-candidate Sena) — confirmed narrowing itself is byte-faithful
   (`NarrowAnalysisRewriteRuleSpec.cs:41-58`), Gate B off, `UseDefaults` a no-op — so the residual
   flood is inherent, not a bug. **(iii) REFRAME — narrowing has NO demonstrated corpus gain and a
   real downside.** The words it was thought to fix (`ሌባው` and the other risk-set words) **already
   match gold at HEAD** (they're in the 532, deletion-only path); activating narrowing pushes 3 of
   them (`ሌባዬ`/`በቅሎው`/`በቅሎዬ`) past the step cap → **−3 regression, 0 gain**. So #6 is **not landable
   on its own merits yet**: it needs the shape-flood collapsed so any words it *would* newly help
   can complete in budget. That collapse is **Tier-2 #14 (MergeEquivalentAnalyses)** — now #6's
   real dependency. Also flagged: Rust's step budget is **per-stratum-analyzer-instance** (effective
   `cap × #stratum-calls`), an amplifier worth reconciling against C#'s. Guard fix + findings
   preserved on the probe branch; revisit #6 only after #14 lands and is measured.
5. ~~**Compile-once pattern cache**~~ **DONE `5db72bf0`** (eager per-id `RuleCache` built in
   `Morpher::new`, no lock — plain indexed `Vec`s shared read-only across threads; `_cached` sibling
   fns, uncached originals kept for standalone-fixture tests). Byte-identical + thread-invariant.
   **Amharic full 673: 1.94s → 0.46s (4.2x); Sena first-100: 45→100/100 complete in 300s, per-word
   p50 5.4s→0.5s.** But the narrowing probe (cache applied to `wip/tier1-6-plus-cache-probe`) shows
   ሄደ/ሌባው **still** don't complete — cost is semantic (Optional-node candidate explosion), not
   compilation. So the cache is necessary-not-sufficient for #6; step 4 still needs real semantic
   work (root-cause investigation in flight).
6. ~~**Tier-2 #9** (featstruct widening ops `add/union` + swap the three analysis merge sites)~~
   **DONE `5025851a`** (`hc_featstruct::ops::add` incl. C#'s delete-key-when-union-covers-all-
   symbols rule, `FeatureStruct.cs:499-500`; bonus finding: `analyze_template`'s old code was
   additionally mislabeled — it called `priority_union`, a "b-wins" operator, not any Add analog).
   Byte-identical on all three corpora but heavily exercised (21.6k differing accumulations on
   Amharic alone); expected payoff deferred to Tier-2 #14, which depends on it.
7. ~~**Tier-2 #13** (three template/partial gates)~~ **DONE `b378a6e1`** (all three:
   final-template prohibition already existed; added non-final-template prohibition in `synth_affix`
   + the root-morpheme-`IsPartial` template gate and the partial-word passthrough in
   `synth_apply_templates`, cited to `SynthesisAffixProcessRule.cs:84-105` /
   `SynthesisAffixTemplatesRule.cs:37-77`). Byte-identical Ind/Amh; Sena tightened one
   over-generated branch (`ndimo`), no match-status change. Amharic saw no movement — the grammar
   has only one `partial` rule and zero partial roots, so the gates rarely fire (corrects this
   section's earlier "Sena + Amharic beneficiaries" expectation).
8. ~~**Tier-2 #12** (capture-based alpha-variable positions in `resolve_bindings`)~~ **DONE
   `57f3ef8e` — Indonesian now 121/121, FULL PARITY.** Wrapped each var-bearing top-level env node
   in a named `CompileNode::Group` and read its matched segment via `Fst::get_offsets` instead of
   positional arithmetic (no hc-fst change; reuses the same traversal already run for the accept
   gate). Closed exactly `memijit-mijit`/`menulis-nulis`/`menyewa-nyewa`. Amharic/Sena
   byte-identical (Amharic's alpha rules are quantifier-free, as predicted).
9. ~~**Tier-2 #7** (first-class non-head resolution)~~ **DONE `3c36cbd3`** (analysis-side
   `SearchRootAllomorphs` on the non-head split + non-head-FS/MPR gates + pin the resolved root
   allomorph replacing the non-head shape/FS + record the non-head ROOT morph, cited to
   `AnalysisCompoundingRule.cs:61-124` / `Word.cs:148-169` / `SynthesisCompoundingRule.cs:81-99`).
   Verified at rule level on real Sena entries; byte-identical zero-regression on all three corpora
   because Sena compounds don't yet reach a confirmed signature (93/100 first-100 words still cap
   out — the compound-specific blowup awaits Tier-2 #14). Gains are latent, mechanism proven.
10. ~~**Tier-2 #8** (reduplication morph attribution) + **R3** (free-fluctuation break)~~ **DONE
    `168c2004`.** #8: ported C#'s `_nonAllomorphActions` window algorithm
    (`SynthesisAffixProcessAllomorphRuleSpec.cs:23-124`); census correction — only 3 subrules across
    all grammars ever repeat a part (all Indonesian); Amharic's 5 `redupMorphType` subrules each
    reference their part once, so C#'s own `redupParts.Count>0` gate never fires → no Amharic gain
    possible (not a miss). R3: `free_fluctuates_with`/`constraints_equal` gate on the allomorph-loop
    break (`SynthesisAffixProcessRule.cs:235-242`, `Allomorph.cs:80-98`). Byte-identical Ind/Amh; one
    real correct-direction Sena movement (`ana`: `-` → 3 of gold's 4 sub-analyses recovered).
11. ~~**Tier-2 #14** (`MergeEquivalentAnalyses` + `Alternatives`/`ExpandAlternatives`)~~ **DONE
    `88855c50`.** `Word::source`/`alternatives` + `expand_alternatives` (delta-replay of each
    merged word's mrule/non-head history), per-stratum canonical-shape fold + `merge_equivalent`
    flipped true + expand between lexical lookup and synthesis (`AnalysisStratumRule.cs:150-177`,
    `Word.cs:485-533`, `Morpher.cs:478,509`). Byte-identical on all three corpora (merge fires 651×
    Amharic, ~1.5M× Sena — heavily exercised, no signature-changing case in the corpora, so
    corpus-level correctness is vacuous; unit test is the real evidence). **Real ~21-26% Sena
    per-word speedup**, but does NOT bring capped words under budget.
12. ~~**Tier-1 #6** (narrowing)~~ **CLOSED, not landed** (see the reframed step-4 note above). The
    `#14` narrowing-tractability probe (`wip/tier1-6-plus-14-probe`) confirmed #14 does not rescue
    the 3 capped Amharic words: narrowing has no demonstrated corpus gain (target words already
    match at HEAD) and a −3 regression, so it stays deferred; guard fix + impl preserved on
    `wip/tier1-6-plus-cache-probe`.
12. **Watch-list** (re-check via cheap oracle micro-diffs only if gaps remain): analysis unapply
    order, Tier-2 #11 anti-FS, dirty-reset scope, `IsTemplateRule`, boundary-LHS synthesis targets,
    `ModifyFromInput` preference, `Untruncate` quantifier copies.
13. Re-run the full three-grammar parity measurement after each step lands (same discipline as
    M1-M8: one reviewed change, one independent re-diff against golden, one commit).
14. Only once all three grammars read 0 diffs at both thread extremes: run and publish the M9
    final benchmark matrix (§10).

### 13.3 Testing strategy to reach and then permanently hold full parity

The C# HermitCrab unit-test suite is **exactly 68 tests** (all plain NUnit `[Test]`, no
parameterized cases — the count is final). Rust today covers roughly a third of the suite's
sub-cases by rule *shape* (e.g. "suffix/prefix rules work") without porting most of its specific
edge cases (16/19 `AffixProcessRuleTests` sub-cases — infix, circumfix, truncate, disjunctive
allomorphs, required environments, reduplication — and ~11/16 `RewriteRuleTests` sub-cases —
quantifiers, merges, long-distance, disjunctive rules — have no Rust test evidence either way, not
confirmed-working). Co-occurrence rules and stem names have zero dedicated Rust tests. The full
per-file coverage matrix and sub-case breakdown live in `parity-out/audit/D-testing.md` §1; this
subsection is the actionable strategy, refined from plan §8's seven layers into concrete work
items (effort: S = half-day, M = few days, L = week+).

**(a) Layer-2 fixture exporter** — the mechanism that ports the 68 C# unit tests' *specific*
assertions to Rust (the corpus goldens in (b) only prove parity on 3 grammars' word lists, not on
the unit suite's deliberately adversarial small grammars):
- **A0** (S) Confirm the C# suite is green right now (`timeout 300 dotnet test --filter
  FullyQualifiedName~HermitCrab`) — the exporter's entire premise is capturing live parser output
  as ground truth, so a red or skipped test would bake a wrong answer into a permanent fixture.
- **A1** (M) Build the export harness: intercept every `new Morpher(...)` construction, snapshot
  `Language` via `XmlLanguageWriter.Save`, record every subsequent `ParseWord`/`AnalyzeWord`/
  `GenerateWords` call + its expected value against that snapshot. **Fixture granularity is
  per-`Morpher`-instantiation, not per-`[Test]` method** (several tests mutate the grammar and
  construct a second `Morpher` mid-method) — count fixtures empirically once this runs rather than
  trusting the plan's original "~200" estimate, which assumed a uniform record shape that turned
  out not to hold (see A3).
- **A2** (S) Take PR #450's XML-writer dangling-co-occurrence-reference crash fix — **narrow hunk
  only** (~35 lines, the `writableMorphemeCoOccurRules`/`writableAllomorphCoOccurRules` filtering
  change), not the full commit, which drags an unrelated `Pattern<Word,int>`→`Pattern<Word,
  ShapeNode>` API change from `master` that conflicts with `parse-optimization`'s current API.
  Several `MorpherTests`/`LexEntryTests` fixtures mutate the shared grammar mid-method in exactly
  the way that triggers this crash — plausible, not hypothetical, to hit during export.
- **A3** (M) Two fixture record shapes, not one: record type 1 (`word`, `signature`) reuses the
  exact corpus/layer-0 format and covers ~41 of 68 tests' `ParseWord`/`AssertMorphsEqual`
  assertions; record type 2 adds an optional serialized output `SyntacticFeatureStruct` and/or
  output category field, needed for 18 `AssertSyntacticFeatureStructsEqual` sites + 6 `AnalyzeWord`
  tests. Explicitly **out of scope, tracked not silently dropped**: 2 `GenerateWords`-from-
  structured-analysis tests (no existing serializer for a bare `WordAnalysis`) and 4 internal-API
  tests with no Rust analog (`MatchNodesWithPattern`, `EnableLexicalGating`, 2×
  `IsEdgeStripperQualified`) — write a one-line manifest entry per excluded test naming why.
- **A4** (M) C#-side round-trip proof: reload each exported fixture XML, re-run the same call,
  assert it matches the original in-code assertion byte-for-byte — this is what makes a fixture
  trustworthy as a contract (an exporter that silently drops a constraint, the same bug class #450
  fixed, would otherwise ship a fixture that's green on both engines for the wrong reason).
- **A5** (L) Rust conformance runner: load each fixture XML, run the recorded words, compute the
  same layer-0 signature (+ type-2 fields when present), diff against the `.tsv`. `#[ignore]`
  fixtures pinning unsupported-v1 constructs (metathesis: 3, realizational: 1) until that scope
  decision changes — an executable spec waiting on scope, not a disabled test hiding a bug.

**(b) Layer-3 corpus golden gates** — what exists: Indonesian full 121, Amharic full 673, Sena
"fast" first-300 sample, thread-count invariance (1-16, byte-identical) for Indonesian. Missing:
- **B1** (M) Sena full 7,121-word nightly with watchdog. **Hard-blocked on T0/D1** — do not build
  this runner before the determinism fix lands, or it generates false red/green noise from day one.
- **B2** (M) Heavy-13 sequential set, named in a code comment (`stratum_gate.rs:416`) but not wired
  as an actual test list or CI job.
- **B3** (S) `MaxUnapplications=0` 60-word Sena stress subset (the #446 edge-case set) — recover the
  word list from the #446 branch/PR; confirm whether an existing `hc-rules` knob maps to it.
- **B4** (S) Formalize "two consecutive runs identical" as a standing CI check rather than an
  ad-hoc manual reconfirmation — only meaningful for uncapped words until T0 lands.

**(c) Rule-level oracle testing** — a live C# `hc.dll` process (parse-optimization branch, dotnet
10) already runs on this machine and reproduces golden TSVs byte-for-byte (validated in an earlier
session), so it can serve as a live oracle during triage, not just a frozen TSV:
- **C1** (S) Promote `parity-out/work/oracle_diff.sh` (already exists as a first cut) into the
  standard inner loop for closing each Tier-1/2/3 gap: build the smallest grammar that isolates the
  construct, run both engines, read the diff. This is exactly how the M6 `MaxStemCount`/non-head-
  root-filter bugs were closed earlier in this project — this item formalizes and speeds that
  pattern up.

**(d) Determinism property tests** — thread-count invariance exists and is verified (Indonesian,
1-16 threads). **Process-rerun invariance does not hold today** (T0's confirmed violation):
- **D1** (M) = **T0** above. The single highest-priority item in this whole section — it currently
  sits underneath both the Sena nightly (b) and the two-consecutive-runs gate (b/d).
- **D2** (S) Add the process-rerun property test itself (N separate process invocations on a
  capped word set, assert identical output) once D1 lands, so this bug class cannot silently
  regress.
- **D3** (M) Extend `memo_gate.rs`'s memo-on≡memo-off check from two hand-built grammars to a
  property-test-generated grammar/word space.
- **D4** (M) "Generate → parse recovers the source analysis" property test (no evidence this
  exists yet) — also useful raw material for (e)'s fuzzer.

**(e) Differential fuzzing** — minimal useful version, not full `cargo-fuzz` infrastructure:
- **E1** (M) A word mutator (segment swap/dup/drop, boundary-char insertion, combining-mark
  insertion, truncation to invalid shapes) over the three corpora, running both engines and diffing
  signatures (including agreeing on `InvalidShape`/`SKIPPED`/`"-"`). Bounded per-CI-run budget
  (~500 cases) plus a larger nightly soak (~50k).
- **E2** (S) Any diff E1 finds becomes a new permanent fixture feeding (a)'s Rust conformance
  runner — this is what makes fuzzing pay for itself past the first soak.
- Grammar-level ablation fuzzing (plan's layer-4 idea) needs no separate infrastructure — point the
  existing partial-grammar ablation tooling at both engines instead of Rust-alone timing.

**(f) FFI-layer tests** — exists post-M8: FFI-vs-in-process byte-identity on the full Indonesian
corpus, a genuine rayon-path abort-safety test. Missing:
- **F1** (M) Concurrent `hc_parse_word` stress from multiple OS threads against one grammar handle
  (not just `hc_parse_batch`'s internal rayon pool) — FieldWorks' actual threading model isn't
  guaranteed single-threaded.
- **F2** (M) Buffer/string-marshaling fuzzing at every FFI entry point — `hc-ffi` is the only crate
  permitted `unsafe` (verified zero unsafe leaked elsewhere), making this the highest-value
  memory-safety fuzz target; pair with the Miri pass §8 layer 7 already calls for.
- **F3** (skip until needed) i686 — no evidence FieldWorks requires 32-bit.

**(g) CI wiring** — no Rust job exists in CI today (dotnet-only: build, csharpier, `dotnet test`).
Per-PR (blocking): `cargo build/clippy -D warnings/test --workspace`; layer-1 gates (NFD/
segmentation/loader dumps); the layer-2 fixture suite once (a) exists (the gate that makes "68 C#
unit tests" mean something for Rust); layer-3 per-PR corpus gate (Indonesian 121 + Sena fast-300 +
Amharic 673, each at 1 thread and N threads); FFI parity + abort-safety; a `criterion` micro-bench
regression check (>5% regression fails the PR). Nightly (non-blocking): full Sena 7,121 with
watchdog (**only after T0/D1** — otherwise permanently flaky, training the team to ignore its red
state); the `MaxUnapplications=0` stress set; the fuzzing soak; the process-rerun determinism
sweep across all three grammars; the full M9 timing/memory matrix (**only once M9's zero-diff gate
is actually met** — publishing a timing number against a non-parity engine was already correctly
refused once this session; CI must not institutionalize doing that by accident via an
unconditional nightly bench job).

**Dependency summary** (full graph in `parity-out/audit/D-testing.md`): `A0`+`A2` → `A1` → `A3` →
`A4`/`A5` → CI gate (g.3). `T0`/`D1` → `D2`, and independently → `B1`, `B4` (meaningful only
post-fix). `C1` is independent and usable immediately — it accelerates closing every Tier-1/2/3
gap in §13.1. `B2`/`B3` are independent, needing only word-list recovery. `E1`/`E2` are most
valuable once `A3`'s fixture format exists. `F1`/`F2` are independent, gating on nothing else.
