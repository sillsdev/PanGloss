# P5 — Cross-table FeatureStruct unification in root lookup: design

Status: **design only** (plan `rust/rust-optimizations-phase2.md` §P5, [FABLE-PLAN]). No engine
code changes accompany this doc. Target implementer: a Sonnet-tier agent working mechanically
from §6-§8.

Driving fixture: `rust/crates/pg-parse/tests/csharp_port_rewrite.rs` ignored test `anchor_rules`,
sub-case (1): `parse_word("gap")` must yield roots `{"10","11","12"}`; Rust yields `{"11","12"}`,
missing root "10" (`"ga̘p"`). The test's doc comment (lines 81-117) contains the verified
root-cause diagnosis; this doc designs the fix.

---

## 1. Problem, restated precisely

Root "10"'s stored vowel is the concrete char-def `cAUnderdot` (ATR-). Surface "gap" segments its
vowel as the concrete char-def `cA`. `RootAllomorphTrie::search_segs_opt` →
`edge_matches` (`rust/crates/pg-parse/src/root_trie.rs:215-224`) requires **literal char_def
equality** between a concrete query segment and a concrete trie edge before feature lanes are ever
consulted, so two different concrete char-defs can never match — regardless of whether their
feature structs unify. Wave 4's `CdSet` membership arm only exists for `NO_CHAR_DEF` (pattern)
edges and does not reach concrete-vs-concrete pairs (re-confirmed post-wave-4, per the test doc).

In C#, root lookup is `FeatureStruct.IsUnifiable` with **no** separate char-def-identity gate
(`RootAllomorphTrie.cs:39-40,61-63`, FSA `UseUnification = true`). In the reference test base,
entry "10" lives in the Morphophonemic stratum (Table3: "a" = ATR+, "a̘" = ATR-) while "gap" is
segmented against Table1 (its "a" has no ATR feature at all), and the ATR-unpinned Table1 node
unifies with either Table3 vowel.

### 1.1 The actual root cause: an over-extended `StrRep` model (bigger insight than "cross-table")

The port's identity model rests on this premise (`root_trie.rs` module doc, `surface.rs:88-91`):

> the C# trie arc condition is a shape node's full `FeatureStruct`, **which always includes
> `StrRep`** (`CharacterDefinitionTable.cs:73-74`)

That citation is the `fs == null` branch of `CharacterDefinitionTable.Add`
(`.worktrees/parse-opt/src/SIL.Machine.Morphology.HermitCrab/CharacterDefinitionTable.cs:68-81`).
When a segment definition **carries phonological features** (`fs != null`), C# adds only the
`Type` symbol — **no `StrRep` at all**. `XmlLanguageLoader.cs:670-673` passes a non-null fs
whenever `PhonologicalFeatureSystem.Count > 0`, and `HermitCrabTestBase.AddSegDef`
(`HermitCrabTestBase.cs:830-844`) always passes a non-null fs. So the faithful C# model is:

| Grammar | C# segment char-def FS | C# root-lookup match | Rust today |
|---|---|---|---|
| zero authored phon features (Sena, en, sp) | `Type + StrRep{reps}` | StrRep-set intersection ≡ (within one table) char-def identity | char_def equality — **faithful** |
| ≥1 authored phon feature (Indonesian, Amharic, test fixtures) | `Type + features` — **no StrRep** | **pure feature unification**; char-def identity plays no role | char_def equality AND lanes — **strictly over-restrictive** |

(Boundaries always take the `fs == null` path in C# — `AddBoundary` — so boundary identity gating
stays correct in all grammars. `surface.rs` already special-cases boundaries.)

So this is not narrowly a "multi-table" gap: in any feature-bearing grammar — even single-table —
two distinct concrete char-defs whose feature structs unify (underspecified segments,
archiphonemes, duplicate-FS authoring artifacts) cross-match in C# and never in Rust. The
multi-table test-base layout is just the arrangement that makes such a pair (unpinned "a" vs
pinned "a̘") unavoidable. Rust's gate is strictly *stricter* than C#, so the divergence class is
pure **under-generation** (missing parses), never over-generation.

### 1.2 Second affected site (scope finding: the test needs TWO fixes, not one)

The same over-extended identity model gates synthesis-confirm. Even with the trie fixed, candidate
root "10" would still be rejected: synthesis leaves the vowel node's `char_def == cAUnderdot`
(no rule touches it), and `pg-parse/src/surface.rs::matching_str_reps` restricts a concrete node's
matching representations to `EffectiveCdSet::Singleton(own char_def)` — so `is_match("gap", shape)`
fails (`"ga̘p" ≠ "gap"`). In C#, `GetMatchingStrReps` (`CharacterDefinitionTable.cs:96-106`) is
pure `IsUnifiable` — the ATR- node matches Table1's "a" and the word confirms. The `anchor_rules`
doc comment diagnoses only the trie gate; flipping the test requires relaxing **both** sites. Both
share the one root cause (§1.1), so one shared mechanism fixes both.

No third site exists: `grep` for concrete `char_def ==` gates in matching logic finds only
`root_trie.rs:217`; rewrite/morph rule matching goes through `pg-fst`, which is lanes-only (no
char-def dimension — already at-least-as-permissive as C#, and a frozen contract untouched here).

---

## 2. How much this matters in practice (census, 2026-07-09)

Checked every sample grammar (`samples/data/*-hc.xml`) for the exposure condition — two distinct
same-table segments with unifiable feature structs — plus table topology:

- **Sena** (`sena-hc.xml`): no `<PhonologicalFeatureSystem>` at all; 1 table; 3 strata all
  `table1`. C# gates on StrRep ⇒ Rust's identity gate is exactly faithful. **Immune.**
- **Indonesian** (`indonesian-hc.xml`): 29 segments, all features pinned; **0 unifiable distinct
  pairs**. 1 table. **Immune today.**
- **Amharic** (`amharic-hc.xml`): 417 segments; **exactly 1 unifiable distinct pair** — `ቂː` and
  `ሺ` have byte-identical 22-feature structs (an authoring artifact; C# cross-matches them).
  Neither character occurs in any of the grammar's 171 `<PhoneticShape>`s (roots or affixes), so
  no root lookup or rendering can reach the pair. **No observable divergence on this grammar.**
- **en/sp** (`en-hc.xml`, `sp-hc.xml`): no phon feature system. **Immune.**
- No sample grammar uses more than one character-definition table; all strata reference `table1`.
  True multi-table layouts exist only in the C# test base (`Table1`/`Table2`/`Table3`).
- `rust/parity-out/audit/phase2/` (main repo; gitignored) mentions no char-def/table-mismatch
  divergence on real corpora beyond the test-base finding itself (A/B/C audits all reference the
  *Sena mbali* StrRep fix, which is the zero-feature direction and stays untouched here).

**Urgency: LOW for current corpora — zero observed real-word impact.** The value is (a) closing a
whole structural under-generation class before FLEx-authored grammars with genuinely
underspecified phonemes arrive (FLEx users commonly underspecify), (b) un-ignoring the last
`anchor_rules` sub-case, and (c) correcting a documented-wrong premise (§1.1) before more code is
built on it.

---

## 3. Candidate designs

### Design A — build-time unifiability closure, consulted as an equality-miss fallback (RECOMMENDED)

Precompute, per character-definition table of a feature-bearing grammar, the static unifiability
closure over its segment char-defs: `closure[i] = { j | flat_unifiable(lanes_i, lanes_j) }` as a
`CdBits` bitset (always contains `i`). For a zero-authored-feature grammar
(`PhonFeatureSystem::is_empty()`, `featsys.rs:196` — exactly C#'s `Count > 0` test) the closure is
**not built** and behavior is bit-for-bit today's (C#'s StrRep gate ≡ within-table identity).

- Trie: `edge_matches`'s concrete×concrete arm becomes
  `e.char_def == cd || closure[e.char_def].contains(cd)` — equality stays the fast path; the miss
  path is one bitset probe instead of an instant reject.
- Surface: `matching_str_reps`'s `Singleton(x)` gate becomes
  `id == x || closure[x].contains(id)`; the existing `flat_unifiable(node_lanes, cd_lanes)`
  conjunct and table-document-order iteration are unchanged.

Soundness: for an **unmodified** concrete node, `node_lanes == static table lanes of its
char_def`, so `closure` membership + the existing lane conjunct ≡ C#'s `IsUnifiable` exactly. For
a **rule-modified** node, wave 3's invariant already clears `char_def` to `NO_CHAR_DEF` (both
`ana_feature` and `syn_feature`), routing it down the existing wildcard/lanes path — so the
closure never sees stale identities. Wave 4's `CdSet` pattern-edge arm is untouched (the closure
is additive on the concrete×concrete arm only), as is `add_path`'s edge-grouping key (two
closure-related but distinct char-defs correctly keep separate edges, like C#'s `ValueEquals`
arc grouping).

Cost: closure build is `O(n²)` `flat_unifiable` calls at grammar load (Amharic: 417² ≈ 174k ≈
sub-millisecond); memory `n × ⌈n/64⌉ × 8` bytes (Amharic ≈ 23 KB). Query cost on the hot path
(root lookup runs per analysis candidate — `morpher.rs:189,263` — millions of times on Sena-class
corpora): Sena/en/sp pay **zero** (closure absent, one `Option`/flag branch after the existing
equality miss); feature-bearing grammars pay one indexed bitset probe only on an equality miss
that today rejects instantly.

Residual gaps, deliberately NOT covered (document at the code): true multi-table grammars —
per-table dense `CharDefId` spaces mean a shape segmented against table T1 cannot be queried
against a T3-built trie at all (id collision), independent of this gate; and zero-feature
multi-table StrRep-set intersection. Both are C#-test-base-only shapes today (census §2); they
stay flagged in `root_trie.rs`'s "M5b invariants" doc block.

### Design B — genuine per-table identity model (REJECTED for now)

Make node identity table-qualified — `(TableId, CharDefId)` throughout `pg-shape` (or drop
char_def identity from `Shape` entirely and carry FS + optional StrRep-set, C#'s literal model) —
build tries per stratum-table, and unify across tables via pairwise closure matrices; extend
`CdSet`/`EffectiveCdSet` id spaces to match.

Correct and fully general (covers true multi-table grammars, including the C# test base without
the merged-table fixture approximation), but it is exactly "the root-trie identity model that
wave 4 just stabilized": `char_def`/`CdSet` columns thread through `pg-shape`, `pg-rules`
(`morph.rs` `OutNode.cd_set`/`ctx_cd_set`, `rewrite.rs` wave-3 clearing), `pg-parse` (trie,
surface, dedup comparator per audit fix `bbe3fa6d`). High regression surface, and §2 shows the
only beneficiary is a test-base topology no real grammar uses. Revisit only if a real multi-table
grammar ever appears.

### Design C — conditional gate removal (considered, folded into A)

Semantics-minimal variant of A: for feature-bearing grammars simply skip the char-def gate and let
`flat_unifiable` decide (C# verbatim). Identical observable behavior to A (see A's soundness
argument), but the trie loses its equality short-circuit: every edge at every node pays a
multi-word lane-AND scan (Amharic: ~23 lane words × edge fan-out, per query segment, on the
per-candidate hot path) where today a `u32` compare rejects. A **is** C plus an O(1) memo of that
scan; there is no correctness reason to prefer C, so C survives only as the property-test oracle
for A (assert A ≡ C on random inputs).

---

## 4. Recommendation

**Design A.** It restores C#'s two-regime semantics exactly (identity where C# has StrRep,
unification where it doesn't), is additive on the wave-3/wave-4 invariants rather than a rework of
them, costs zero on the corpora where root lookup is hottest, and is small enough to implement
mechanically (§6). Design B solves a problem no real grammar has; Design C is A without the memo.

---

## 5. Fixture correction (required to flip `anchor_rules`)

The shared merged-table fixture (`crates/pg-parse/tests/csharp_port_common/mod.rs`) pins
`cA` = ATR+ — emulating Table3's "a". But every ported test *segments surface words* the way C#
segments against **Table1**, whose "a" has **no ATR feature**. Correction: **drop `cA`'s
`fAtr` pin** (leave `cAUnderdot` = ATR-). Verified safe: `fAtr` is referenced nowhere else in the
fixture or any ported test (no natural class, no rule); only entry "10" (`ga̘p`) contains `a̘`; no
ported test parses a word containing a literal `a̘`. The engine change alone, against the current
pinned fixture, deliberately does NOT flip the test (ATR+ vs ATR- genuinely conflict) — that is
correct behavior, already empirically confirmed in the test's doc comment ("dropping cA's fAtr pin
… has NO effect" was pre-fix; post-fix it is exactly what makes the case expressible).

Update the fixture's header comment (its "one merged table" rationale) and the `anchor_rules` doc
comment + `#[ignore]` removal in the same commit. Note in the fixture comment the remaining known
limit of the merged-table approximation: C#'s Table3-"a" ATR+ pin is not representable
simultaneously — acceptable while nothing tests ATR-conditioned rules.

---

## 6. Implementation sketch (for the follow-up code task)

No `pg-fst` changes (frozen). Two crates touched: `pg-grammar` (closure storage), `pg-parse`
(two consumers). `pg-grammar` already depends on `pg-shape` (`segment.rs` uses `CdBits`), so
`CdBits` is available.

### 6.1 `pg-grammar/src/chardef.rs`

```rust
pub struct CharDefTable {
    // ... existing fields ...
    /// Static unifiability closure over segment char-defs (Design A, P5).
    /// `None` ⇔ the grammar declared zero authored phonological features
    /// (`PhonFeatureSystem::is_empty()`) — C#'s StrRep regime, identity gating stays exact.
    /// `Some(v)`: `v[i].contains(j)` ⇔ `flat_unifiable(lanes_i, lanes_j)` for segment
    /// char-defs i, j (reflexive; symmetric). Boundary rows are empty (identity only —
    /// C# boundaries always carry StrRep).
    unif_closure: Option<Vec<CdBits>>,
}

impl CharDefTable {
    /// O(1). None when the closure is disabled (zero-feature grammar) or `cd` is a boundary.
    pub fn unifiable_cds(&self, cd: CharDefId) -> Option<&CdBits>;
}
```

Populate at the end of table construction (where `feature_lanes` are already resolved), gated on
`!feat_sys.is_empty()`. Only `CharDefKind::Segment`×`Segment` pairs; use the existing
`pg_featstruct::flat_unifiable`. (The synthetic `Type` lane is identical across segments, so it
never blocks a pair — no special-casing needed.)

### 6.2 `pg-parse/src/root_trie.rs`

- `RootAllomorphTrie::search` already resolves `table_ref`; thread the closure down:
  `search_segs_opt(&self, segs: &[...], closure: Option<&[CdBits]>)` and
  `fn edge_matches(e: &TrieEdge, cd: u32, lanes: &[u64], closure: Option<&[CdBits]>) -> bool`.
- New concrete×concrete arm in `edge_matches` (the ONLY predicate change):

```rust
let cd_ok = cd == NO_CHAR_DEF
    || e.char_def == cd
    || (e.char_def != NO_CHAR_DEF
        && closure.is_some_and(|c| c[e.char_def as usize].contains(cd)))
    || (e.char_def == NO_CHAR_DEF && /* existing CdSet arm, unchanged */);
```

- The `#[cfg(test)]` `search_segs` helper passes `None` (all existing unit tests unchanged).
- Module doc: correct the "always includes StrRep" claim per §1.1 (cite
  `CharacterDefinitionTable.cs:68-81` two-branch behavior and `XmlLanguageLoader.cs:670-673`).
- Do NOT touch: `add_path` grouping, the `CdSet` pattern arm, the optional-skip branch, the
  `NO_CHAR_DEF`-query wildcard arm.

### 6.3 `pg-parse/src/surface.rs`

In `matching_str_reps`, the segment-loop membership gate becomes closure-aware only for the
`Singleton` case:

```rust
let in_set = match cd_set {
    EffectiveCdSet::Singleton(x) => x == id.0
        || table.unifiable_cds(CharDefId(x)).is_some_and(|b| b.contains(id.0)),
    other => other.contains(id.0),
};
```

`Members`/`Unrestricted` (inserted-class nodes) keep their wave-4 semantics. Keep the boundary
early-return, the `flat_unifiable` conjunct, and table document-order iteration. Fix the module
doc's StrRep claim here too. This automatically corrects `is_match` (synthesis-confirm),
`to_regex_display`, and `to_plain_string` together — all three are C#-`GetMatchingStrReps`-backed.

### 6.4 Fixture + test

Per §5: drop `cA`'s `fAtr` `FeatureValue`; un-ignore `anchor_rules`; update its doc comment
(diagnosis → fixed, citing this doc).

Estimated size: ~40 lines engine, ~15 lines fixture/doc, plus tests (§7). One Sonnet agent, one
branch, commit incrementally.

---

## 7. Test & verification plan

1. **Unit (new, `root_trie.rs` tests mod):** feature-bearing closure trie — two concrete cds with
   unifiable lanes cross-match via `Some(closure)` and do NOT match via `None` (Sena regime);
   conflicting-lane cds never match either way; closure arm respects the lane conjunct when the
   query carries divergent shape lanes.
2. **Unit (new, `surface.rs` tests):** concrete node with a closure-sibling renders both cds' reps
   in table order (`to_regex_display`), first-match rep for `to_plain_string`, and `is_match`
   accepts the sibling spelling; all three unchanged under a zero-feature table.
3. **Property (cheap):** for the Amharic table, assert Design A ≡ Design C (gate-free lane scan)
   on random (edge cd, query cd) pairs — the closure is exactly a memo.
4. **The driving test:** `anchor_rules` un-ignored; sub-case (1) = `{"10","11","12"}`;
   sub-cases (2)-(4) stay green.
5. **Wave-4 regression guard (the stabilized identity model):** full `pg-parse`/`pg-rules` suite +
   `loader_n3_*_gate.rs` + `affix_shapes_conformance.rs` + the root_trie pattern-edge tests —
   all must stay green with zero expectation edits outside `anchor_rules`. Any OTHER test whose
   expectation moves is a red flag: it was passing under the too-strict gate for the wrong reason;
   stop and diff against the C# oracle before adjusting anything.
6. **Bounded corpus checks (per the FST stats reporting preference, include state counts/build
   time/p50/p95 alongside coverage when reporting):**
   - **Sena**: byte-identical by construction (closure disabled); verify by re-running the
     sena-fast gold subset compare (join by word text, not index).
   - **Indonesian**: closure = identity (census §2) ⇒ expect 121/121 and byte-identical FFI
     parity (`ffi_indonesian_parity.rs`).
   - **Amharic**: closure has exactly one pair, unreachable from the lexicon ⇒ expect
     byte-identical `coverage/amharic-out*.tsv` re-runs; plus a root-lookup micro-bench
     (equality-miss now probes a bitset) — expect noise-level; regression >2% on Amharic parse
     throughput is a stop-and-profile gate.
7. **Perf guard for Sena:** confirm the `None`-closure branch adds no measurable cost to
   `search_segs_opt` (single-threaded Sena benchmark, p50/p95 vs current branch).

---

## 8. Go/no-go

**GO — but low priority, and sequence it after V2's Sena run completes.** Rationale: the fix is
small, mechanically specified, additive on wave-3/4 invariants, and pays zero on the hot corpora;
the census (§2) already answers the "wait for V1/V2 data" question — current-corpus exposure is
zero, so no measurement will raise the urgency, and none is needed to justify the design. The
reasons to do it anyway: it closes a structural under-generation class that WILL bite
FLEx-authored grammars with underspecified phonemes, retires the last `anchor_rules` sub-case, and
corrects a false premise (§1.1) currently written into two module docs. The only sequencing
constraint is hygiene: don't land an identity-model change while the W10/V2 Sena byte-compare
baseline is mid-flight — land immediately after, with §7's checks as the landing gate. If
implementation pressure is high elsewhere, this can wait indefinitely without real-corpus cost;
do NOT let it be picked up casually as a drive-by, and do NOT expand it toward Design B without a
real multi-table grammar in hand.
