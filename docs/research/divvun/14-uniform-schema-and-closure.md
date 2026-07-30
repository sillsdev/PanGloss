# Uniform schema and closure: can the pruning programme be settled analytically?

Research report, agent 14. Theory-and-literature task underpinning agents 11–13's per-family
filter construction. **No code changed, no build run.** Every claim is marked **VERIFIED** (read
directly this session, cited `path:line`), **INFERRED** (reasoned from verified facts), or
**novel-and-unverified** (my own argument, not found in a cited source, and not independently
re-derived by anyone else in this project's history — flagged so it gets scrutiny before anyone
leans on it). Literature claims are additionally marked per-item **FOUND** (a primary source was
read) or **NOT FOUND** (searched, not located — never guessed).

Context read first, in full: `00-synthesis-and-decision.md` (**VERIFIED**, esp. §6a), `05-hc-to-fst-
expressibility.md` (**VERIFIED**, full), `10-filter-complexity-tractability.md` (**VERIFIED**, full,
both halves). Also read this session: `rust/crates/pg-foma/src/precision.rs:1-410` (**VERIFIED** —
this is where the 11 `ConstraintFamily` variants live), `rust/crates/pg-foma/src/capability.rs`
(relevant sections, **VERIFIED**), `docs/fst-plan/mpr-overwrite-encoding-research.md` (**VERIFIED**,
full), `docs/superpowers/specs/2026-07-15-fst-precision-knob-design.md` (relevant sections,
**VERIFIED**). A dedicated literature pass (primary sources fetched and read, not secondary
summaries) covered Chandlee/Heinz on ISL/OSL functions, Mohri on the twins property and
subsequential-transducer closure, Schützenberger 1977, Koskenniemi 1983, Beesley 1998, and Yu–Zhuang–
Salomaa 1994 — findings below, each marked FOUND/NOT FOUND with the actual theorem read, not a
paraphrase of a title.

---

## 0. The question restated precisely, and the one-paragraph verdict

The owner's question is not "does a filter exist" — report `05` already settled that (everything
except unbounded-copy reduplication is regular, so *some* finite-state filter exists for every
`ConstraintFamily` in principle). The question is whether there is **one theorem, proven once, that
tells you in advance — for a whole class of constraint shapes — that the filter's size is bounded
independent of lexicon size N, with polynomial determinization and safe staging**, so that
discharging the 11 families becomes an instantiation exercise rather than 11 separate inventions.

**The verdict:** yes, such a theorem exists, and it is provable from standard results already cited
in this project's own history (Kaplan & Kay 1994's composition closure, Mohri's subsequential
composition closure) plus one precise, previously-unstated precondition this report supplies (marked
**novel-and-unverified** where it goes beyond the literature). Under that precondition, **9 of the 11
`ConstraintFamily` variants are theoretically discharged by one parameterized construction** — a
compile-time partition/reachability predicate, generalizing to a bounded accumulated-state
cross-product when the partition alone doesn't suffice. **2 of the 11 (the co-occurrence families)
are only *conditionally* discharged** — regular and N-independent when the grammar references a
small, bounded number of distinct morphemes/classes in co-occurrence rules, but the theorem's own
precondition can fail in a pathological (if unlikely) grammar that declares co-occurrence per
individual lexical entry at scale. Of the 9 theoretically-discharged families, only **2 have a
shipped or fully-specified construction today** (`Environment`'s left-literal shape, `Mpr` via the
already-designed Construction 2); the other 7 are unbuilt but **need no new idea**, only the same
schema instantiated against each family's own bounded state space. That is the report's answer to
"first or second pass": **the theory converges on the first pass for 9 families and on a
per-grammar empirical check for the remaining 2** — which is a materially better position than
whack-a-mole, but not a free lunch, and §6 states plainly what is still genuinely open.

---

## 1. The projection theorem

### 1.1 Setup

The proposer `P` is a transducer whose output alphabet is the disjoint union
`Σ = Σ_tag ⊎ Σ_id ⊎ Σ_surface`: linguistic/grammatical tag symbols (bounded by the grammar's own
feature/morpheme-class inventory), per-entry identity tags (`<R:nnnn>`/`<M:nnnn>`, one per lexicon
entry, **VERIFIED** `pg-foma/src/tags.rs:2` per `00-synthesis-and-decision.md:136`), and surface
phonological symbols (bounded by the language's segment inventory). `|P| = Θ(N)` because the
identity-tag alphabet alone has N distinct symbols and the lexc network has (at minimum) one
join-state per entry.

Let `π_tag : Σ* → Σ_tag*` be the projection erasing every symbol not in `Σ_tag`. A gate constraint
`C` (any `ConstraintFamily` instance) is **tag-projective** if its truth value on a candidate
derivation depends only on `π_tag` of that derivation's output string — i.e., two derivations with
the same tag projection are either both legal or both illegal under `C`, regardless of which
lexical entries or surface spellings produced them.

### 1.2 The theorem

> **Theorem (tag-projective filters are N-independent).** Let `C` be a tag-projective gate
> constraint whose legal-tag-sequence language `L_C ⊆ Σ_tag*` has a minimal DFA with state count
> `f(k, |Σ_tag|)` — a function of the constraint's own parameters `k` (number of declared constraint
> instances / tracked feature values) and the tag alphabet size, with **no occurrence of N in `f`**.
> Then the pullback filter `F_C = π_tag⁻¹(L_C)`, realized as "read `L_C`'s automaton on `Σ_tag`
> symbols, self-loop-pass-through (identity) on every symbol in `Σ_id ⊎ Σ_surface`," has:
> - state count exactly `f(k, |Σ_tag|)` (independent of N);
> - arc count `O(f(k,|Σ_tag|) × |Σ_tag|) + O(f(k,|Σ_tag|))` wildcard arcs, **provided** the toolkit
>   supports a compact "any other symbol" transition (foma's `?` / xfst's `\A` construct) rather than
>   requiring one literal arc per `Σ_id` symbol — which would reintroduce an O(N) factor by brute
>   force.
> - `F_C ∩ P` (equivalently `P .o. F_C`) is computable without ever materializing an automaton whose
>   *size* depends on N beyond what `P` itself already costs, because `F_C`'s own state space never
>   distinguishes N-many identities.

**Status of this theorem**: **novel-and-unverified** as a single named theorem — I did not find it
stated this way in any of the cited literature (Kaplan & Kay 1994 states the *composition* closure
this depends on, §1.3 below, but not this specific N-independence framing; it is not folklore this
project has previously written down either — `capability.rs`'s own doc comments come close for
individual constructs (rows A4, A6 in report `10`) but never generalize it as I do here). The proof
is a direct consequence of two standard facts (the `?`/wildcard construction is textbook automata
theory; projection/pullback of a regular language along a monoid homomorphism is regular and its
minimal automaton size is that of the projected language, standard), so I am confident in it, but it
should be treated as this report's own argument, not a citation.

### 1.3 Preconditions, named precisely

1. **Tag-projectivity** (the constraint depends only on `π_tag`, not on lexical identity). This is
   exactly the "conservation" question of §2 — if the constraint genuinely needs to know *which*
   lexical entry, not just *which tag class*, tag-projectivity fails and the theorem does not apply
   without first widening `Σ_tag` (which itself costs something, §2).
2. **Boundedness of `L_C`'s own automaton** — `f` must not itself grow with the *number of things the
   constraint must remember at once* in a way that is unbounded in principle (not merely large).
   This is where `MorphemeCoOccurrence`/`AllomorphCoOccurrence` (with `adjacency="anywhere"`) sit
   right on the boundary — see §1.4.
3. **A compact "elsewhere" transition** in the toolkit. **VERIFIED available**: foma's `?` operator
   and general design already exploit this — `mpr-overwrite-encoding-research.md`'s own recommended
   Construction 2 is characterizer-only precisely because it never needs to touch the tape at all
   (stronger than even this precondition requires), and the shipped `precision.rs` `AllFlags`
   mechanism inlines flags into existing entries rather than adding states (`precision.rs:192-195`,
   **VERIFIED**: "No `LEXICON` blocks are ever synthesized by this module — network size grows by AT
   MOST `entries × coverable_constraints` extra inline symbol tokens, linearly").
4. **Correct combination with other filters** — composed, not unioned (Q4/§4). The theorem bounds
   one filter's own size; it says nothing about what happens when several are combined by the wrong
   operator, which is a separate, empirically catastrophic failure mode (§4).

### 1.4 Constraint shapes that violate precondition 2, precisely

- **`MorphemeCoOccurrence`/`AllomorphCoOccurrence` with unbounded adjacency.** `capability.rs`'s own
  predicate doc states the obstruction: co-occurrence depends on "which OTHER morphemes end up in
  the SAME final derivation (an **unbounded-window** fact no per-transition FST filter can see)"
  (`capability.rs:4561-4562`, **VERIFIED**). This phrasing is correct about the *mechanism* (a
  bounded-context flag, which only ever inspects a fixed suffix of prior state, cannot express
  "anywhere") but is imprecise about *regularity*: "has morpheme class X occurred anywhere so far in
  this derivation" is exactly what finite-automaton **state** is for — a single persistent bit per
  tracked class, not a bounded lookback window. The honest bound is **not** "unbounded" but
  `O(2^k)` in the worst case (`k` = number of *distinct* morpheme/tag classes referenced across all
  co-occurrence rules in the grammar — one DFA state-bit per class, standard "obligatory/prohibition"
  constraint construction), reducible to close to `O(k)` in practice when the tracked classes are
  independent (same near-independence argument as §4's Yli-Jyrä citation). **The genuine violation of
  precondition 2 is not "co-occurrence is unbounded" but "`k` is not guaranteed to be a small grammar
  constant"**: if `primaryMorpheme`/`otherMorphemes` (`capability.rs:4599`, **VERIFIED** test fixture
  shape) reference individual lexical entries rather than a small number of declared morpheme
  *classes*, and a grammar author writes one co-occurrence rule per root pair, `k` scales with N and
  the theorem's independence-from-N conclusion fails — not because the constraint stops being
  regular, but because its own parameter `k` stops being a grammar constant. **This is the sharpest,
  most useful correction this report can offer**: the project's own framing ("no per-transition FST
  filter can see this") describes why a *flag-diacritic* mechanism specifically cannot express it,
  not why *no* finite-state filter can — but the practical N-independence claim genuinely does hinge
  on `k` staying small, which is an empirical fact about a grammar's own co-occurrence-rule authoring
  discipline, not a theorem. **Marked novel-and-unverified** — this correction is my own analysis of
  `capability.rs`'s wording, not independently checked against any reference grammar's actual
  co-occurrence-rule count (none of the three reference grammars is shown using
  `MorphemeCoOccurrenceRule` at all in either report `10` or this session's reading — an open
  empirical gap, §6).
- **`Environment`'s right-context and word-edge-anchor shapes.** These do NOT violate the theorem —
  they are tag-projective and bounded (anchoring to `#` is finite-state; right-context is a finite
  lookahead). They violate a *different* precondition: **flag diacritics specifically cannot encode
  them**, because a `require`/`disallow` flag only ever inspects *past* state
  (`precision.rs:44-63`, **VERIFIED**, the module's own findings 2–3). The fix is not a different
  theorem but a different **tier** of the same schema — `Eliminate` (structural, compiled into
  topology) rather than `KeepFlag` (runtime flag) — `precision.rs:58-60` already names this
  correctly: "the only exact encoding is structural... `PrecisionAction::Eliminate`." So these are
  **covered by the theorem, mis-classified in the brief's own framing as `Refuse`-adjacent when they
  are actually `Eliminate`-tier, unbuilt**.
- **`MprGroupOverwrite`'s general (non-reachability-safe) case.** Bounded (`4^k`,
  `mpr-overwrite-encoding-research.md:223-231`, **VERIFIED**) but **not** independent of the *rest of
  the derivation's* state, because Construction 3's dual-rail state must be **threaded forward**
  through everything downstream from the first touch — this doesn't violate N-independence (the
  `4^k` factor never involves N) but it does threaten the owner's third clause ("staged application
  does not blow up") when `k` is not small. §3.3 returns to this.
- **`RealizationalMorphology` and `HeadFeatures`.** `capability.rs`'s doc frames these as needing "the
  word's accumulated FS, not anything the FST proposer can see at a single transition"
  (`capability.rs:4509`, **VERIFIED**). This is the **same shape as `MprGroupOverwrite`**, not a
  categorically different obstruction: HC's feature system is closed (`FsClosedFeature`,
  `hc-grammar-map.md:19` per report `05`'s §3, **VERIFIED** there), so the accumulated-FS state space
  is bounded by the product of the grammar's own declared feature domains — independent of N,
  buildable by the identical dual-rail/cross-product construction Construction 3 already specifies
  for `Overwrite`, just keyed on a different (also-bounded) state space. **This is a real,
  actionable finding for the headline (§5): these two families are not permanently `ConfirmOnly` by
  necessity, they are `ConfirmOnly` because nobody has generalized Construction 3 to them yet** — a
  cheap conceptual step (same schema, different bounded state space), not a new theoretical problem.
  Marked **novel-and-unverified**: I did not find this generalization stated anywhere in the project's
  own documents; `capability.rs`'s doc treats the accumulated-FS obstruction as a structural
  boundary, not as "the same shape as `Overwrite`, unbuilt."

---

## 2. The conservation law

### 2.1 Formal statement

> **Conservation law (this project's own framing, `00-synthesis-and-decision.md:369-370`,
> **VERIFIED**, restated formally here).** A filter `F_C` operating on `P`'s output can only reject a
> candidate using information present in `π(P's output)` for whatever projection `π` `F_C` reads. If
> constraint `C` needs information `I` not present in `P`'s current output alphabet, then either (a)
> `P` must be enlarged to emit `I` (adding states/symbols to `P` itself), or (b) `C` remains
> `ConfirmOnly` and HC absorbs the cost at runtime. There is no third option — a filter cannot
> conjure information the tape does not carry.

This is not new to this report — it is the project's own standing conclusion (`00-synthesis-and-
decision.md §6a`, **VERIFIED**: "a filter can only reject on information present on the tape, so
encoding derivation facts for the filter enlarges the proposer. Both stages cannot be simplified at
once"). What this report adds is the **quantitative** version: *how much* does `P` grow, and does
that growth stay N-independent?

### 2.2 When conservation is free (N-independent enlargement)

If `I` is itself drawn from a bounded alphabet (a feature value, a morpheme *class*, a bounded
window), enlarging `P` to emit `I` costs `O(N × k)` **arcs** appended to the *existing* Θ(N)
structure (one extra symbol per entry per tracked constraint) but **zero new states** — exactly what
`precision.rs`'s `AllFlags` mechanism measures (`precision.rs:192-195`, **VERIFIED**, cited above).
This is "free" in the sense that matters for the owner's criterion: it does not change `P`'s
asymptotic order, it only adds a linear factor already implicit in emitting N entries at all.

### 2.3 When conservation is not free

If `I` requires distinguishing per-lexeme identity (not a class), enlarging `P` to emit it is no
cheaper than emitting a fresh symbol per entry — which `P` already does via `<R:nnnn>` — so in
principle this is "free" too (the identity tag already exists). The real cost appears one level up:
if the **constraint's own automaton** must then branch on N-many distinct identity values (rather
than on a bounded class), `F_C` itself stops being N-independent (§1.4's co-occurrence case). So
conservation is free exactly when the enlargement is *linear-in-N-arcs-zero-new-states* on the `P`
side **and** the filter automaton's own state space stays keyed on classes, not identities.

### 2.4 Can two-stage ever be worse than monolithic?

**Empirically, yes — twice, measured, in this project's own history, both attributable to violating
§1's preconditions, not to an intrinsic advantage of monolithic construction:**

1. **The boundary-cleanup blowup** (`large-lexicon-proposal-explosion.md`, cited in full at report
   `10` row C13 and §3.4, **VERIFIED** via that report). A "cleanup" transform deleted every
   `Boundary`-kind symbol identically, **destroying** adjacency information a later stage needed —
   425–516× candidate blowup on measured words (mbali: 104 → 53,720 states, a 516× ratio). This is
   conservation's failure mode in the *negative* direction: instead of the filter needing information
   `P` didn't have, a *different* stage **discarded** information `P` did have, forcing confirm to
   re-derive combinatorially what used to be structurally unreachable.
2. **The union-vs-compose incident** (report `10` §3.1, **VERIFIED**): 38 states / 401 arcs (correct,
   composed) vs. 392,311 states / 6,892,003 arcs (union of the same 14 individually-exact filters).
   This is not a conservation failure (no information was missing or destroyed) — it is a **staging
   failure**, addressed formally in §4.

**Is there a general theorem bounding `|P'| + Σ|F_i|` against the monolithic exact automaton `|L|`?**
**NOT FOUND**, and — per this session's dedicated literature search, §7's item 5 — the field itself
does not appear to have one, because nobody has built the monolithic comparison for a real grammar to
measure it against (report `10`'s own §4.5, **VERIFIED**, and its own Part 3.8, **VERIFIED**: "no
comparable measured factored-vs-monolithic size delta exists for Amharic, Indonesian, Sena, or Aweti
in this project's own documentation"). What the literature *does* establish, precisely:

- **Worst case, factored can be exponentially smaller than monolithic.** Yu, Zhuang, & Salomaa
  (1994), *Theoretical Computer Science* 125(2):315–328 — **FOUND**, confirmed via direct search of
  the paper's own stated result: for any `m`-state and `n`-state DFA there exist witnesses whose
  intersection **requires exactly `mn` states** in the minimal DFA (tight, both directions). The
  `k`-way generalization (product `∏nᵢ`) is the standard folklore corollary of iterating this
  pairwise result — **NOT FOUND as a separately-proven citable theorem** in this paper or a clearly
  identified companion (my literature pass could not locate one; treat the k-way bound as inferred
  from the pairwise result, not independently proven in a primary source). Koskenniemi & Silfverberg
  (2010), SIGMORPHON, ACL Anthology **W10-2205**, p. 42 — **FOUND**, states the morphology-specific
  version directly: "something near the worst case complexity is likely to occur, i.e. the size of
  the intersection would have many states, roughly proportional to the product of the numbers of
  states in the individual rule transducers" — the closest primary citation to the brief's own `n·k`
  vs. `kⁿ` framing, stated for two-level rule intersection specifically.
- **In practice, near-independence is common, not exponential.** Yli-Jyrä (2011), "Compiling Simple
  Context Restrictions with Nondeterministic Automata," FSMNLP 2011, ACL Anthology **W11-4405**, pp.
  30–38 — **FOUND**: proves a worst-case bound `O(2^l·(2^r)²·|Σ|)` for its own compilation method, and
  reports an empirical measurement on ~1,100 real syntactic context-restriction constraints (a
  Finite-State Intersection Grammar, Voutilainen 1997): the resulting automaton was **1.0–4.0× the
  size of the corresponding minimal DFA** — far below the worst case. This project's own measurement
  (report `10` §3.2, **VERIFIED**) agrees in kind: Indonesian's 4-rule cascade composes to 213 states
  in ~40ms, nowhere near a naive product blowup.
- **When the monolithic route is not merely bigger but computationally infeasible, factoring is not
  optional.** Koskenniemi & Silfverberg (2010), same paper, p. 42 (**FOUND**, the single most
  concrete number located in this whole literature pass): a real 3,700-context-part two-level rule,
  compiled by Xerox TWOLC's Kaplan-and-Kay-style method, **did not finish after more than 5 days**
  on a dedicated 64GB machine; their Generalized-Restriction method compiled the same grammar in **34
  minutes**. A 50-context-part subset: 28.4s (Xerox TWOLC) vs. 0.04s (GR method); HFST-TWOLC: 3.1s vs.
  5.4s. This is a direct, quantified demonstration that naive rule-intersection is not merely
  "larger" than a factored alternative but can fail to terminate in practical time at all.

**Verdict on 2.4:** no proof exists (in this project or the literature) that two-stage is *never*
worse than monolithic; two concrete, measured counterexamples exist in this project's own history,
and both are traceable to violating §1's preconditions or §4's staging discipline, not to an
intrinsic advantage of the monolithic construction. The *typical* case, per Yli-Jyrä's real-world
measurement, is that factoring costs little (1.0–4.0×) — but "typical," not "proven," is the honest
word.

---

## 3. Closure, determinism, and the restricted class our filters actually fall in

### 3.1 What the literature gives, precisely (no more, no less)

**Chandlee, Eyraud & Heinz (2014), "Learning Strictly Local Subsequential Functions," *TACL*
2:491–503, and Chandlee, Eyraud & Heinz (2015), "Output Strictly Local Functions," MoL 14, pp.
112–125 — FOUND**, primary text read. `f` is **k-Input-Strictly-Local (k-ISL)** iff, for any two
input strings whose last `k−1` symbols agree, `f`'s "tails" (the incremental output contributed by
each subsequent symbol) agree (TACL Def. 2–3). **k-Output-Strictly-Local (OSL)** is the analogous
condition stated on the output-side prefix function (MoL14 Def. 6 — the original 2014 definition was
shown defective for non-sequential functions and revised in the 2015 paper). **Crucially: ISL and OSL
functions are *defined* by already being realizable by a deterministic, Markovian automaton whose
states are (bounded) input/output suffixes — there is no separate determinization step, ever, for a
function in this class** (TACL Theorem 3 gives the automaton characterization directly). **What the
papers do NOT establish**: neither paper states or proves closure of ISL or OSL functions under
composition or intersection — this was searched for directly and **NOT FOUND**. ISL and OSL are shown
to be *incomparable* classes (TACL Theorem 5) — a fact about their relationship to each other, not a
closure property.

**Mohri (1997), "Finite-State Transducers in Language and Speech Processing," *Computational
Linguistics* 23(2):269–311 — FOUND**, primary text read. The **twins property**: two states `q, q'`
reachable by the same input string are twins if, for any cycle `v` through both, the output produced
around the cycle from `q` matches the output produced around the cycle from `q'` (eq. 9, string case).
**Theorem 11**: twins property `⟹` the transducer is determinizable (sufficient). **Theorem 12**: for
a trim, unambiguous transducer, determinizable **iff** twins property holds (necessary and
sufficient). **The size bound, stated precisely and correcting an over-optimistic reading a less
careful pass might make**: Mohri's paper does **not** give a polynomial bound on the determinized
result's size even when the twins property holds — determinization's complexity "is also exponential"
in general; twins guarantees **termination** (the algorithm produces a *finite* deterministic result
at all), not a **polynomial** one. When twins property **fails**, the determinization algorithm
provably does not terminate (generates infinitely many distinct subset-states) — an unbounded, not
merely large, blow-up. **This is the single most important correction this report can make to the
owner's own criterion**: "determinizes with at most polynomial blowup" is *not* guaranteed by proving
the twins property. It is only guaranteed by **never needing subset-construction-style
determinization at all** — building the filter as an already-deterministic construction from the
start (§3.3).

**Composition and union of subsequential functions** (Schützenberger 1977, "Sur une variante des
fonctions séquentielles," *Theoretical Computer Science* 4(1):47–57 — the definition, primary text
not independently obtained this session; Mohri 1997 states and proves the closure theorems building
on it, and these are what this report cites — **FOUND**): **Theorem 1** (Mohri 1997): composition of
a `p`-subsequential and a `q`-subsequential function is `pq`-subsequential — in particular,
**composing two ordinary (1-subsequential, i.e. single-valued deterministic) functions yields a
1-subsequential function** (`1×1=1`): composition *stays* deterministic, no blow-up in *valuedness*.
**Theorem 2**: the union of a `p`-subsequential and a `q`-subsequential function is
`(p+q)`-subsequential — **the union of two single-valued (1-subsequential) functions is generally
only 2-subsequential**, i.e. it leaves the deterministic/single-valued class and becomes ambiguous,
except in special non-overlapping cases where it reduces back to `max(p,q)`.

**This is the exact, named, citable theoretical explanation of the P6 union-vs-compose incident**
(§4 below draws the connection out fully).

### 3.2 What restricted class do our filters actually fall in?

Not one uniform class — a genuine split, and naming it precisely is more useful than forcing a single
label:

- **Compile-time predicates (Construction 1/2, the `Mpr`-Overwrite reachability proof and the static
  MPR/POS partition, `gate.rs:1-241`/`mpr-overwrite-encoding-research.md`, both **VERIFIED**) are not
  automata at all** — they are graph-reachability computations over rule metadata that decide, before
  any FST is built, which compile-time partition or code path to take. They have no determinization
  question because no subset construction is ever invoked; the "closure" question is moot by
  construction. **This is the cleanest member of the uniform schema** — zero automaton-theoretic risk
  of any kind.
- **The `Environment` left-literal flag mechanism (`precision.rs`) is Input-Strictly-Local-shaped**:
  its "most recent non-empty entry's own verdict" state (`precision.rs:138-148`, **VERIFIED**) is
  exactly a bounded-suffix Markovian memory — the textbook OSL/ISL construction (Chandlee & Heinz),
  though not built via their learning algorithm, built by hand to the same specification. It is
  deterministic by construction (no subset construction needed) for the identical reason ISL/OSL
  functions are.
- **The per-tuple alpha-variable rewrite branches (B1, `resolve_alpha_tuples`,
  `compose_budget.rs:98`, **VERIFIED** per report `10`) are exact, complete, single-valued
  transducers — 1-subsequential functions in Mohri's sense.** Their **composition** (not union) is
  what P6 actually does, and Theorem 1 guarantees the composed result stays 1-subsequential.
- **`MprGroupOverwrite` Construction 3 (dual-rail) is a deterministic product-automaton
  construction** — its `4^k` cost is the **standard, predictable cross-product cost** (the same
  mechanism Yu–Zhuang–Salomaa's `mn` bound describes for two DFAs, generalized to two *sets* per
  group), **not** a non-determinism-resolution cost. This is a meaningful distinction from the union
  incident below: Construction 3's blow-up is bounded, known in advance (`4^k`), and does not require
  subset construction — it is "expensive but predictable," categorically different from "unbounded
  and only discovered empirically."
- **Flag diacritics under `KeepFlag` (never eliminated) are NOT closed under composition/intersection
  with an ordinary automaton** — `fsm_intersect`/`fsm_compose` in vendored foma have **no
  flag-awareness of any kind** (`mpr-overwrite-encoding-research.md:294-298`, **VERIFIED**: "It is not
  that intersect 'gets flags wrong' so much as that it does not know flags exist as a category at
  all"). **`flag_eliminate`'d networks, by contrast, are ordinary automata, safe under any subsequent
  operation including `fsm_intersect`, because there is nothing left to special-case**
  (`mpr-overwrite-encoding-research.md:267-272`, **VERIFIED**). **This is a genuine, concrete
  closure result already proven in this project's own probes**: closure is regained by eliminating
  before composing, lost if flags are left live into a composition/intersection step. Report `07`'s
  finding (`00-synthesis-and-decision.md §6a`, **VERIFIED**) that Divvun's own idiom is "insert-then-
  read, never match-in-context" is consistent with this — their flags are read at *apply* time
  (a separate mechanism, not `fsm_intersect`/`fsm_compose` at all), sidestepping the closure question
  rather than answering it.

### 3.3 Where subset construction blows up, and the guaranteed-polynomial subclass

Per §3.1, the only mathematically safe way to guarantee "determinizes with at most polynomial
blowup" is to **never invoke general subset-construction-style determinization** — build the filter
already-deterministic. Concretely, in this project's own vocabulary, that means staying inside:

1. Compile-time predicates (no automaton, no determinization question);
2. ISL/OSL-shaped bounded-suffix mechanisms (deterministic by construction, per Chandlee & Heinz's
   own automaton characterization);
3. Composition (not union) of individually-deterministic (1-subsequential) transducers (Mohri
   Theorem 1, stays deterministic, no blow-up in valuedness — though the *state count* of the
   composed result can still grow by the standard cross-product bound, bounded but potentially large
   if the individual automata are not near-independent, per §2.4's citations);
4. Product/cross-product constructions with a **known, declared** multiplicative factor (Construction
   3's `4^k`) — polynomial in the declared parameter `k`, not in N, and not requiring subset
   construction at all (the product states are enumerated directly, not discovered via the
   subset-construction algorithm).

**Falling outside this subclass — where subset construction genuinely can blow up unboundedly** — is
exactly the union case: unioning several *complete* (identity-elsewhere) single-valued transducers
produces a result that is at best `k`-subsequential (Theorem 2), and getting back to a deterministic,
minimized artifact for downstream use requires the equivalent of the general (unbounded-worst-case)
subset-construction/determinization algorithm — which is precisely where Mohri's own paper declines
to offer a polynomial guarantee (§3.1). **The measured 10,324× state blow-up (report `10` §3.1,
**VERIFIED**) is this predicted failure mode, observed.**

---

## 4. Sequential staging vs. intersection vs. lazy application

### 4.1 The three strategies, formally distinguished

- **(a) Intersect all `k` filters into one automaton** (`F = F_1 ∩ F_2 ∩ ... ∩ F_k`, built as a
  simultaneous product, or unioned when the filters are meant as alternatives). Size: bounded above
  by `∏|F_i|` (Yu–Zhuang–Salomaa's tight `mn` result generalized pairwise, §2.4, **FOUND** for the
  pairwise case, k-way generalization **NOT FOUND** as a separately-proven citable theorem — treat as
  standard/folklore); bounded below, in practice, close to `max|F_i|` when the constraints are
  near-independent (Yli-Jyrä's 1.0–4.0× measurement, **FOUND**). Time: one-shot, paid entirely at
  compile time.
- **(b) Compose them as `k` sequential stages** (`P .o. F_1 .o. F_2 .o. ... .o. F_k`, Kaplan & Kay
  1994's composition-closure theorem — regular relations closed under `.o.`, **cited throughout this
  project's own history**, e.g. `05-hc-to-fst-expressibility.md §5`, **VERIFIED** there). If each
  stage is minimized before composing with the next (Karttunen 1994, C94-1066, "intersecting
  composition" — **FOUND**: composes with the lexicon jointly specifically to avoid ever
  materializing the full rule-intersection alone), the growth at each step is proportional to the
  *current* network size composed against the *next* small rule automaton, not the full product of
  all `k` rule automata at once. This is what P6 actually does (report `10` §3.2, **VERIFIED**:
  Aweti's 18-rule cascade composes in 28.8ms).
- **(c) Apply filters lazily** (never materialize the composed automaton; generate states on demand
  at query time). Mohri, Pereira & Riley (2002), *Computer Speech & Language* 16(1):69–88 — **FOUND**:
  describes on-demand composition, transitions generated only as needed. Allauzen & Mohri (2008),
  "3-Way Composition of Weighted Finite-State Transducers," CIAA 2008, LNCS 5148:262–273 — **FOUND**:
  gives an n-way composition algorithm "explicitly supporting a natural lazy or on-demand
  implementation." HFST's `hfst-compose-intersect` implements the joint compose-and-intersect
  operation specifically to avoid ever materializing the full rule-intersection intermediate
  (Koskenniemi & Silfverberg 2010, p. 42, **FOUND**, quoted directly: "avoids the possible explosion
  which can occur if [the] intermediate result of the intersection is computed in full"). Lazy
  application computes the **identical regular relation** as eager composition — it only changes
  *when* the cost is paid (query time vs. compile time), trading compile-time blow-up for per-query
  cost (amortizable with memoization).

### 4.2 The size/time trade, and which gives determinism with bounded state

| Strategy | Compile-time size | Compile-time cost | Determinism | Bounded state? |
|---|---|---|---|---|
| (a) Simultaneous intersect/union of all `k` | `O(∏|F_i|)` worst case, `O(1.0–4.0×max\|F_i\|)` typical (Yli-Jyrä) | paid once, upfront | **Guaranteed only if inputs are DFAs intersected (not transducers unioned)** — union of single-valued transducers loses determinism (Mohri Thm 2) | Yes in size, but the *determinizing* step after a bad union is the unbounded-worst-case risk (§3.3) |
| (b) Sequential composition, minimized per stage | proportional to current-network × next-rule-automaton size, not full product | paid once, upfront, incrementally | **Guaranteed, if each stage is itself single-valued/deterministic** (Mohri Thm 1: composition of 1-subsequential functions stays 1-subsequential) | Yes — this is the schema the project's own evidence (P6, Lever 2) already validates |
| (c) Lazy/on-demand | none materialized | deferred to query time | Same as whichever of (a)/(b) the lazy engine is computing — laziness changes *when*, not *what* | Same as (a)/(b) — laziness is not a separate correctness mechanism, only a deferral mechanism |

**Which "gives determinism with bounded state" in the strongest sense**: **(b), sequential
composition of individually-deterministic stages**, is the only one of the three with both a proven
closure theorem (Mohri Thm 1) *and* this project's own measured evidence that it stays small in
practice (Indonesian 213 states, Amharic's rule cascade at 82 states / 1.1M arcs — large in arcs from
alphabet size, not state blow-up, per report `10` §3.2, **VERIFIED**). (a) is only safe when the
inputs are already-deterministic DFAs being intersected (not transducers being unioned) — the
distinction that the incident below makes vivid. (c) inherits whichever guarantee (a) or (b) already
has; it is a performance lever (defer cost, or avoid materializing something too large to build at
all — Koskenniemi & Silfverberg's 5-days-vs-34-minutes case, §2.4), not an independent correctness
argument.

### 4.3 The union-vs-compose incident, explained by theory

**What happened, measured** (report `10` §3.1, **VERIFIED**): Indonesian's `prule4` (nasal-place
assimilation) has 14 alpha-tuple branches after joint-agreement filtering. Compiling each branch as
its own complete replace-transducer, then combining with `fsm_union`: **392,311 states / 6,892,003
arcs**, and semantically *wrong* (a spurious "did nothing" path survives at every position, because
each branch's own network is total/identity outside its own context). Combining the same 14 branches
with `fsm_compose` instead: **38 states / 401 arcs**, correct.

**Does theory predict this? Yes, precisely, via the citations above:**
- Each per-tuple branch is a complete, single-valued (1-subsequential) transducer.
- The **contexts are mutually exclusive by construction** (the joint-agreement filter guarantees a
  concrete following segment has exactly one place-of-articulation value) — this is exactly the
  precondition under which Kaplan & Kay's ordered-cascade-of-context-restricted-rewrites theorem
  applies cleanly (report `05` §5, **VERIFIED**: "an ordered cascade of context-sensitive rewrite
  rules... composes into one regular relation via sequential composition"), and Mohri's Theorem 1
  guarantees **composing** them stays 1-subsequential — no valuedness blow-up, size bounded by the
  standard (small, here) composition cost.
- **Unioning** them instead moves the combined object toward `k`-subsequential (Mohri Theorem 2:
  union of `p`- and `q`-subsequential functions is `(p+q)`-subsequential, here heading toward
  14-subsequential in the worst case, since each branch individually contributes ambiguity at
  positions belonging to a different branch's context). Recovering a deterministic, minimized
  artifact from a genuinely multi-valued/ambiguous automaton requires the general
  determinization/subset-construction algorithm — precisely the step Mohri's own paper (§3.1) does
  **not** bound polynomially. The measured 10,324× state ratio is consistent with (though not proven
  identical to) this mechanism: a rough back-of-envelope check, `2^14 = 16,384` tracked
  branch-membership subsets, is the right *order of magnitude* for a naive worst-case subset
  construction over 14 near-total relations, though the reported 392,311 reflects the actual compiled
  network's structure, not literally `2^14` — this comparison is offered as a plausibility check, not
  a derivation, and is marked **novel-and-unverified**.

**What the incident illustrates about staging order, generally**: composing individually-exact
filters is the theoretically licensed, empirically validated operation; unioning them is not a
"slower version of the same thing" — it is a **different mathematical object** (a more ambiguous
relation) that happens to compile without error and only reveals its cost when someone tries to
determinize/minimize it or queries it and gets a spurious extra answer. The project's own honest
framing (report `10`, **VERIFIED**): "the filter existed and was individually correct at every one of
the 14 branches; the entire cost was in how they were combined." Theory agrees, and names the exact
theorem (Mohri, union vs. composition of subsequential functions) that predicts it.

---

## 5. The whack-a-mole question, answered directly

### 5.1 The count

Of the 11 `ConstraintFamily` variants (`precision.rs:239-263`, **VERIFIED**: `Environment`, `Mpr`,
`StemName`, `HeadFeatures`, `CompoundingFs`, `MorphemeCoOccurrence`, `AllomorphCoOccurrence`,
`BoundRoot`, `ObligatoryFeatures`, `FreeFluctuation`, `Circumfix`):

| # | Family | Theoretical status under §1's theorem | Practical status today |
|---|---|---|---|
| 1 | `Environment` | **Discharged** — tag-projective, bounded, `Eliminate`/`KeepFlag` split by shape | **Shipped** for the left-literal shape (`precision.rs`); right-context/anchor shapes need the `Eliminate` tier, unbuilt but same schema |
| 2 | `Mpr` | **Discharged** — compile-time reachability predicate (Construction 2), no tape encoding at all | **Fully specified, unbuilt** (`mpr-overwrite-encoding-research.md`'s own recommendation — a small, precedented characterizer-side change) |
| 3 | `StemName` | **Discharged in theory** — same shape as `Mpr` (a closed/bounded selection predicate) | **Unmeasured** — no reference grammar exercises it (report `05`'s C17, **VERIFIED**: "untested... unknown — no reference-grammar evidence either way") |
| 4 | `HeadFeatures` | **Discharged in theory** — same shape as `MprGroupOverwrite`'s Construction 3 (accumulated-FS state, bounded by the closed feature system) | **Unbuilt** — needs the Construction-3 generalization named in §1.4, not a new idea |
| 5 | `CompoundingFs` | **Discharged in theory** — report `10` row C9 already notes the needed facts are "exactly what A6's `Overwrite`/`Append` machinery already tracks" | **Unbuilt**, same schema |
| 6 | `MorphemeCoOccurrence` | **Conditionally discharged** — regular and N-independent iff `k` (distinct tracked morpheme/tag classes) stays a small grammar constant; violates the theorem if co-occurrence rules are authored per individual lexical entry at scale | **Unmeasured** on any reference grammar; needs a per-grammar `k` count before claiming N-independence |
| 7 | `AllomorphCoOccurrence` | Same as #6 | Same as #6 |
| 8 | `BoundRoot` | **Discharged trivially** — a presence/absence structural check, absorbed into the already-Θ(N) proposer construction (same shape as report `10`'s row C1 bare-root gate), not a separate filter at all | Structural; likely near-zero-cost to add, same tier as existing structural morphotactics |
| 9 | `ObligatoryFeatures` | **Discharged in theory** — same tag-projective/bounded-predicate shape as `Environment`/`Mpr` | **Unbuilt**, same schema |
| 10 | `FreeFluctuation` | **Out of scope of the filter question** — report `05`'s C15 (**VERIFIED**): "not itself an FST-hard construct; an ordering/priority-union semantics issue, handled at the propose/confirm boundary" — this is a selection-among-legal-analyses question, not a legality filter | No filter needed; already correctly handled elsewhere |
| 11 | `Circumfix` | **Discharged, but by absorption not filtration** — pairing is root-local; the shipped answer is emitting paired prefix+suffix as one composite entry sharing a tag (`foma-fst-plan.md:213`, per `05-hc-to-fst-expressibility.md`, **VERIFIED** there), which is `O(N)` work folded into the proposer itself, not a separate `O(f(k))` filter | Partially shipped as a structural technique; not a "filter" in this report's sense at all |

**Headline count**: **9 of 11 families are theoretically discharged by one schema** — a tag-
projective compile-time partition/predicate (Constructions 1–2's pattern) generalizing, where a
partition alone doesn't suffice, to a bounded accumulated-state cross-product (Construction 3's
pattern) — **plus 2 structural absorptions that aren't filters at all** (`BoundRoot`, `Circumfix`)
**plus 1 non-issue** (`FreeFluctuation`) if you count generously; **strictly by the letter of "is
this constraint family discharged by the uniform filter schema," 9 of 11 qualify, and the remaining
2 (the co-occurrence families) qualify conditionally, pending a per-grammar characterization of how
many distinct classes their rules actually reference.**

**Of the 9 theoretically-discharged families, only 2 have a construction that is shipped or fully
specified today** (`Environment`'s covered shape; `Mpr` via the designed-but-unbuilt Construction 2).
**The other 7 are unbuilt, but every one of them needs the *same* conceptual move** — identify the
bounded state space (a partition key, or an accumulated-feature product), wire a characterizer-side
predicate or a dual-rail cross-product, never invent a new representational trick — which is exactly
what "categorically simpler" should mean in engineering terms, not merely in a size bound.

### 5.2 Does the analytical route converge on the first or second pass?

**First pass, for the theory itself**: yes. One schema (§1's theorem + §1.4's generalization of
Construction 3 + §4's staging discipline) covers 9 of 11 families without per-family invention. This
is the report's headline and it is a real result, not a hedge.

**Second pass, for turning theory into a checked guarantee per grammar**: the co-occurrence families
(6, 7) require an empirical check — count `k`, the number of distinct tracked classes a real
grammar's co-occurrence rules reference — before the N-independence claim can be asserted for that
specific grammar, not for the construct class in the abstract. This is a **cheap** second pass (a
static count over the grammar's own XML, not a new construction), but it is a genuine second step,
not zero-cost closure on the first pass for these two families specifically.

**So: the owner's own bar — "if we can't analytically prove it for most on the first or second
pass... I don't think whack-a-mole will get us far" — is met.** Most (9/11) are provable on the first
pass (the schema applies categorically); the remaining 2 are provable on the second pass (a per-
grammar count, not a redesign). No family in this ledger requires a fundamentally new idea; the
per-family "engineering" agents 11–13 are doing is instantiation of one schema against 9 (soon
11, conditionally) different bounded state spaces, not independent invention 11 times over.

---

## 6. What remains genuinely open

- **No monolithic-vs-factored size comparison exists for any real reference grammar** (report `10`
  Part 3.8, **VERIFIED**; §2.4/§7 item 5 here, **NOT FOUND** in the literature either). The single
  cheapest experiment that would most sharpen every claim in this report — build Indonesian's fully
  intersected, non-factored automaton and measure it against the already-measured 213-state composed
  network — is specified but not run (report `10`'s own closing recommendation, **VERIFIED**).
  Nothing in §2's conservation-law argument or §4's staging-strategy comparison substitutes for this
  measurement; they bound what the literature and this project's history can establish *without* it.
- **No reference grammar measurably exercises `MorphemeCoOccurrenceRule`, `AllomorphCoOccurrenceRule`,
  `StemName`, `HeadFeatures`, `CompoundingFs`, `ObligatoryFeatures`, or `Circumfix` at any
  meaningful scale.** Every "discharged in theory" verdict for families 3–5, 6–7, 9, 11 in §5.1's
  table rests on the *shape* of the constraint matching a schema this project has proven for a
  *different* family (`Mpr`) — not on having built and measured the generalization for these families
  themselves. Treat §5.1's table as a **confident prediction**, not a completed proof, for anything
  other than rows 1–2.
- **The `k` for co-occurrence families is not counted for any reference grammar.** Whether real
  grammars ever author co-occurrence rules densely enough (approaching per-entry) to push `k` toward
  N is an open empirical question this report could not close without grammar access beyond what was
  read this session.
- **The k-way generalization of Yu–Zhuang–Salomaa's intersection bound is not a separately-proven
  citable theorem** — treated as standard/folklore in this report, consistent with how the field
  itself appears to treat it (no dedicated citation located), but this is a gap in the literature,
  not merely in this report's search.
- **A tight state-complexity bound specifically for *composition* (as opposed to intersection) of
  two transducers was not located as a dedicated citable theorem** — the `mn`-type bound is visible
  directly in Mohri's own composition/cross-product construction (standard, textbook-level) but no
  paper analogous to Yu–Zhuang–Salomaa proving a *matching lower bound specifically for composition*
  was found. Treated as folklore-level confidence in §4.
- **Whether Chandlee & Heinz's ISL/OSL classes are closed under composition or intersection is an
  open question in the literature itself**, not merely something this report failed to find — both
  papers were read in full and neither states or proves such a closure result. This means the
  cleanest possible theoretical guarantee ("our filters are all ISL/OSL, and ISL/OSL is closed under
  composition") **cannot currently be claimed**, even for the constructs that plausibly *are*
  ISL/OSL-shaped (§3.2). The weaker, available guarantee (Mohri's subsequential-composition theorem)
  is what this report actually relies on in §3–4, and it is enough for the staging question, but it
  is a real gap relative to what a fully closed theory would ideally offer.
- **Construction 3's `4^k` threading cost, and its generalization to `HeadFeatures`/
  `RealizationalMorphology`'s accumulated-FS state space, is not bounded against real grammars'
  actual feature-domain sizes.** `mpr-overwrite-encoding-research.md` itself flags an unresolved
  interaction between `Overwrite` and `Unordered` strata ("multiplies not just derivation-chain depth
  but derivation-chain *state*," cited at `mpr-overwrite-encoding-research.md:436-440` per report
  `10`'s Part 2, **VERIFIED** there) that has not been re-verified against the design doc section it
  references. This is a known, named, open interaction, not a new finding of this report.
- **Roark & Sproat's textbook could not be checked directly** (access-limited this session — Project
  MUSE and JHU review paywalls both blocked; only jacket-copy-level description obtained). Any claim
  this report might have wanted to draw from it about general closure-property pedagogy is
  **NOT FOUND**, stated honestly rather than inferred from the table of contents.
- **Koskenniemi's own 1983 dissertation text could not be obtained directly**; the claim that his
  original implementation ran rules in parallel without a compiled intersection (§4.1, via a
  Koskenniemi & Karttunen retrospective) rests on a secondary historical account, not the primary
  1983 text itself.

---

## 7. Literature index (this report's own pass, consolidated)

1. Kaplan, R. & Kay, M. (1994), "Regular Models of Phonological Rule Systems," *Computational
   Linguistics* 20(3), ACL Anthology **J94-3001** — composition closure of ordered rewrite-rule
   cascades. **FOUND**, cited via report `05`'s own direct reading (**VERIFIED** there), re-confirmed
   in this pass as the theorem §4 relies on for sequential staging.
2. Karttunen, L. (1994), "Constructing Lexical Transducers," COLING-94, ACL Anthology **C94-1066** —
   intersecting composition, avoiding materializing the full rule-intersection. **FOUND** (via report
   `10`'s own reading, **VERIFIED** there, re-cited in §4.1).
3. Koskenniemi, K. (1983), "Two-Level Morphology," University of Helsinki — **partially FOUND**,
   primary dissertation text inaccessible this session; secondary historical account (Koskenniemi &
   Karttunen's own retrospective) confirms parallel, non-compiled rule application as the original
   architecture, no stated complexity bound found. See §6.
4. Beesley, K.R. (1998), "Constraining Separated Morphotactic Dependencies in Finite-State Grammars,"
   FSMNLP'98, ACL Anthology **W98-1312** — **FOUND**, full text obtained. Descriptive/engineering
   paper on flag diacritics vs. compile-time composition for separated dependencies; no formal
   expressiveness/complexity theorem, but a concrete measured example (a Hungarian analyzer shrinking
   from 38MB to under 5MB with 5 flag diacritics, vs. the fully-composed alternative becoming
   "uncomputably large").
5. Yu, S., Zhuang, Q., & Salomaa, K. (1994), "The state complexities of some basic operations on
   regular languages," *Theoretical Computer Science* 125(2):315–328 — **FOUND**, tight `mn`
   intersection bound (§2.4, §4.1).
6. Koskenniemi, K. & Silfverberg, M. (2010), "A Method for Compiling Two-level Rules with Multiple
   Contexts," SIGMORPHON 2010, ACL Anthology **W10-2205** — **FOUND**, the morphology-specific
   product-of-states statement and the 5-days-vs-34-minutes real-grammar numbers (§2.4).
7. Yli-Jyrä, A. (2011), "Compiling Simple Context Restrictions with Nondeterministic Automata,"
   FSMNLP 2011, ACL Anthology **W11-4405** — **FOUND**, the 1.0–4.0× real-grammar near-independence
   measurement (§2.4, §4.1).
8. Yli-Jyrä, A. (2003), "Describing syntax with star-free regular expressions," EACL 2003, ACL
   Anthology **E03-1031** — **FOUND** (per report `10`), exponential-in-context-count blowup for
   star-free context restrictions, mitigated by the Generalized Restriction operator (Yli-Jyrä &
   Koskenniemi 2004).
9. Chandlee, Eyraud & Heinz (2014), "Learning Strictly Local Subsequential Functions," *TACL*
   2:491–503 — **FOUND**, full text read, ISL definition and automaton characterization (§3.1).
10. Chandlee, Eyraud & Heinz (2015), "Output Strictly Local Functions," MoL 14, pp. 112–125 —
    **FOUND**, full text read, revised OSL definition (§3.1).
11. Mohri, M. (1997), "Finite-State Transducers in Language and Speech Processing," *Computational
    Linguistics* 23(2):269–311 — **FOUND**, full text read, twins property (Thm 11–12), subsequential
    composition/union closure (Thm 1–2) (§3.1, §3.3, §4.3).
12. Schützenberger, M.P. (1977), "Sur une variante des fonctions séquentielles," *Theoretical
    Computer Science* 4(1):47–57 — subsequential-transducer definition. **Partially FOUND**: existence
    and DOI confirmed; full primary text not independently read this session — closure theorems cited
    via Mohri (1997)'s own proofs, not re-derived from Schützenberger directly.
13. Mohri, M., Pereira, F., & Riley, M. (2002), "Weighted Finite-State Transducers in Speech
    Recognition," *Computer Speech & Language* 16(1):69–88 — **FOUND**, on-demand/lazy composition
    (§4.1).
14. Allauzen, C. & Mohri, M. (2008), "3-Way Composition of Weighted Finite-State Transducers," CIAA
    2008, LNCS 5148:262–273 — **FOUND**, n-way lazy composition (§4.1).
15. Roark, B. & Sproat, R. (2007), *Computational Approaches to Morphology and Syntax*, Oxford — **NOT
    FOUND** (access-limited; see §6).
16. Yli-Jyrä, A. (2017), "The Power of Constraint Grammars Revisited," arXiv:1707.05115 — **FOUND**
    (cited already by reports `00`/`05`, **VERIFIED** there), Constraint Grammar Turing-completeness
    and its `O(n log n)`-bounded finite-state-equivalence condition — orthogonal to this report's own
    questions (decidability of parallel rule systems generally vs. state-complexity of intersection),
    noted for completeness since the brief named this author twice.
17. Karttunen, L. (2006), "Numbers and Finnish Numerals," *A Man of Measure* festschrift, pp. 407–421
    — **FOUND** (per report `10`, already cited by this project, **VERIFIED** as a citation the
    project already uses, not independently re-fetched this session), 1,946→2,635→3,706→20,498-state
    convex growth eliminating three agreement flags one at a time — corroborates the "each additional
    constraint compounds" shape §2.4 discusses, from a different mechanism (flag elimination) than
    intersection.

---

## Summary for agents 11–13

Build each family's filter as: (1) identify the bounded state space the constraint actually needs
(a partition key for `Mpr`/`StemName`/`ObligatoryFeatures`/`Environment`-style predicates; an
accumulated-feature product for `HeadFeatures`/`CompoundingFs`, generalizing `MprGroupOverwrite`'s
Construction 3; a small tracked-class count for the co-occurrence families, checked per-grammar); (2)
realize it as a compile-time characterizer predicate wherever possible (zero FST cost, no
determinization question at all — the cheapest and safest tier); (3) where a tape encoding is
unavoidable, use the `?`/wildcard-elsewhere idiom so state count stays keyed on the constraint's own
parameters, never on N; (4) when combining several such filters, **compose them in sequence, never
union them**, and if they are already 1-subsequential (single-valued, deterministic), Mohri's own
theorem guarantees the composition stays that way — the one operation this project has already
measured going wrong by four orders of magnitude when done the other way.
