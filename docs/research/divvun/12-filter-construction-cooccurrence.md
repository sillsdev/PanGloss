# Filter construction for long-distance / co-occurrence constraints

Research agent 12. Scope per brief: mathematical constructions and size bounds for the three
`ConstraintFamily` variants nobody has populated yet — `MorphemeCoOccurrence`,
`AllomorphCoOccurrence`, `BoundRoot` — against the criterion "`|F_X| = O(g(k, |Σ_tag|))`, no
dependence on N". **No code changed. No build run** (`cargo`, `pg.ps1` never invoked). Claims are
marked **VERIFIED** (read directly at the cited `path:line` this session), **INFERRED** (reasoned
from verified facts), or **novel-and-unverified** (a construction with no published precedent
found). "Not found" is stated rather than invented.

Context read first, in full: `docs/research/divvun/00-synthesis-and-decision.md` (esp. §6a),
`03-pruning-and-constraint-grammar.md`, `10-filter-complexity-tractability.md`,
`07-flag-replace-source-proof.md` — all **VERIFIED** (re-read this session, not taken from memory).

---

## 0. Bottom line

Two of the three families are **PROVEN SIMPLER**, one is **OPEN** pending a small, scoped, named
tape-emission fix — not because the automaton theory is hard, but because of a fact this session
found by reading the enforcement code directly: **the analysis tape already carries the identity
information `MorphemeCoOccurrence` needs (`<R:nnnn>`/`<M:nnnn>` = `MorphemeId`,
`pg-foma/src/tags.rs:2-5`, VERIFIED), and does not carry the finer-grained information
`AllomorphCoOccurrence` needs (the tape is keyed by morpheme, not allomorph).** `BoundRoot` needs no
tape information at all — it is resolved structurally, at zero marginal automaton cost, by a
mechanism (the bare-root continuation) that already exists in the emitter today for an unrelated
purpose.

Report `10`'s own governing predicate doc calls co-occurrence an "**unbounded-window** fact no
per-transition FST filter can see" (`capability.rs:151-153,4559-4565`, quoted in `10`'s row A8).
This session's finding **refines that claim rather than overturning it**: "unbounded window" and
"regular-language-checkable by a bounded-state automaton" are not in tension — that distinction
*is* what finite-state automata are for (a bounded number of states scanning an arbitrarily long
tape). The real obstruction was never distance; it is **whether the identity information the
predicate needs is on the tape at all**, and that turns out to differ by family.

---

## 1. What each family actually constrains

### 1.1 The model (VERIFIED, `pg-grammar/src/model.rs`)

- `MorphemeCoOccurrenceRuleDef` (`model.rs:533-538`): attached to a *morpheme*
  (`MorphemeInfo::co_occurrence`, `model.rs:511-515`). Fields: `require: bool` (require vs.
  exclude, DTD default exclude), `others: Vec<MorphemeId>` (an **ordered list, not a set** — see
  §3), `adjacency: CoOccurrenceAdjacency`.
- `AllomorphCoOccurrenceRuleDef` (`model.rs:547-551`): the identical shape, one level finer —
  attached to a specific *allomorph* (`RootAllomorphDef`/`AffixAllomorphDef.co_occurrence`,
  `model.rs:793-794`, `673-676`). `model.rs:544`'s own comment cites the conformance fixture that
  pins the distinction: `rust/conformance/cooccurrence/allomorph-basic` — "two allomorphs of the
  same morpheme, only one carrying the rule" — i.e. this family is tested specifically for a case
  `MorphemeId` alone cannot resolve (§5).
- `CoOccurrenceAdjacency` (`model.rs:520-526`): five values — `Anywhere`, `SomewhereToLeft`,
  `SomewhereToRight`, `AdjacentToLeft`, `AdjacentToRight`.
- `BoundRoot`: not a rule type at all — a single boolean, `Allomorph::is_bound`
  (`model.rs:791`, DTD attribute `isBound`, `load.rs:2257`).

### 1.2 The enforcement path (VERIFIED, `pg-rules/src/validity.rs`)

All three are checked in `allomorphs_valid_impl` (`validity.rs:500-723`), the function `pg-parse`'s
real per-word pipeline calls at `pg-parse/src/morpher.rs:884-914`
(`Morpher::is_word_valid`/`is_word_valid_traced` → `pg_rules::validity::allomorphs_valid_cached_traced`,
**VERIFIED**) — this is the HC-engine-side "confirm" check these families are candidates to move
out of.

**Co-occurrence, the actual predicate** (`validity.rs:314-379`, `co_occurs`): given `key` (the
morpheme/allomorph the rule is attached to), `others` (the rule's declared list), and
`morph_list` (**every** distinct-position morph in the **whole word**, in surface order,
`validity.rs:530-531`), the five adjacency modes reduce to:

- **`Anywhere`** (`validity.rs:322-328`): for each element of `morph_list` in order, if it equals
  some remaining (not-yet-consumed) entry of `others`, consume it. Accept iff `others` is fully
  consumed by the end. This is **order-independent** — a sub-**multiset** containment test: for
  every distinct id `s` appearing `m_s` times in `others`, `morph_list` must contain at least `m_s`
  occurrences of `s`.
- **`SomewhereToLeft`/`AdjacentToLeft`** (`validity.rs:329-352`): scan `morph_list` left to right,
  **stop at the first occurrence of `key`** (`if key == cur { break; }`, `validity.rs:333`);
  `others` must appear, **in the declared order**, entirely to the left of that stop point.
  `AdjacentToLeft` additionally requires each matched `other` be immediately followed by the next
  `other` in the list (or, for the last one, immediately followed by `key` itself,
  `validity.rs:336-347`).
- **`SomewhereToRight`/`AdjacentToRight`** (`validity.rs:353-376`): the mirror image, scanning
  right to left, stopping at `key`.
- **`require`** (`validity.rs:381-396`): pass iff `co_occurs` is true; **`exclude`** (DTD default):
  pass iff false.
- **AND-across-rules** (`validity.rs:293-303`, history row `90dcee64`): every rule attached to the
  relevant morpheme/allomorph must pass — `Vec::iter().all(...)`, never "any one passes."

**Bound roots** (`validity.rs:514-521,562-563,602-603`): `distinct_count` — the count of
**distinct allomorph ids used anywhere in the word** (roots and affixes together, computed once,
`w.morphs` deduplicated by allomorph id) — a root allomorph flagged `is_bound` fails iff
`distinct_count == 1`, i.e. iff **this bound root is the word's only morph, full stop** (no
affixation, no compounding). `rust/crates/pg-rules/tests/validity_gate.rs:326-348` (VERIFIED,
test names) pins exactly this: `bound_root_alone_is_rejected` /
`bound_root_with_an_affix_is_not_rejected_by_the_bound_gate`.

**A W3.2 interaction to flag, not resolve here** (`validity.rs:628-656,692-718`): the disjunctive
free-fluctuation recheck also evaluates **unchosen sibling allomorphs'** own co-occurrence rules
and bound-root status against the same `morph_list`, to decide whether an unselected alternative
would *also* have been legal (in which case the word is ambiguous and rejected). This reuses the
identical `morph_list`/`distinct_count` facts (no new information), but it is evaluated jointly
with environment-matching (`check.envs_ok`, a phonetic-adjacency fact, not a tag-identity fact) —
a genuine cross-family entanglement, named here as a caveat (§8), not folded into either family's
core verdict, since the two `FailureReason`s this report is about
(`AllomorphCoOccurrenceRules`/`MorphemeCoOccurrenceRules`/`BoundRoot`, `pg-rules/src/trace.rs:107-126`
VERIFIED) are what the brief's three families name.

### 1.3 Today's disposition (VERIFIED, `pg-foma/src/capability.rs`, `precision.rs`)

- `precision.rs:239-263` (`ConstraintFamily`): `MorphemeCoOccurrence(usize)` at `325`,
  `AllomorphCoOccurrence(AllomorphId)` at `326` — declared, **never populated**
  (`ConstraintCatalog::build`, `precision.rs:343-382`, only ever walks `EnvironmentDef`s).
- `capability.rs:151-153`: both fold into one `CharacteristicKind::CoOccurrenceConstraint`, whose
  `default_disposition` (`capability.rs:282`) is `Disposition::ConfirmOnly` **unconditionally, no
  registered predicate at all** — the predicate module's own doc, quoted directly:
  *"which OTHER morphemes end up in the SAME final derivation (an **unbounded-window** fact no
  per-transition FST filter can see)"* (`capability.rs:4559-4565`).
- **`BoundRoot` has no `CharacteristicKind` variant at all** — grepped exhaustively across
  `capability.rs`: zero hits for `BoundRoot` in that file (confirmed this session). It is not
  merely `ConfirmOnly` by disposition; it is **absent from the capability ledger entirely**, a
  distinct and slightly stronger form of "not attempted" than the co-occurrence rows.

---

## 2. Verdict table

| Family | Verdict | Bound | Determinizable? | Tape info needed today? |
|---|---|---|---|---|
| **`MorphemeCoOccurrence`** | **PROVEN SIMPLER** | Single rule: `O(k)` states (ordered adjacency) or `O(2^k)` worst case (`Anywhere`, all-distinct singleton `others`) — `k` = that rule's own `others.len()`, independent of `N`. §3. | Yes — the construction given *is* a DFA; product of `K` such DFAs stays a DFA (no extra subset-construction cost), size `≤ ∏_r|Q_r|`. §4. | **Yes, already** — `<R:nnnn>`/`<M:nnnn>` = `MorphemeId`, `tags.rs:2-5,138-146`, emitted for every morph including zero-surface ones (`tags.rs`'s own "Tag tape convention"; corroborated by `precision.rs:172-176`). |
| **`AllomorphCoOccurrence`** | **OPEN** | Identical construction and bound as above, over `AllomorphId` instead of `MorphemeId` — the mathematics is not the open part. | Identical to above once built. | **No** — today's tag is keyed by `MorphemeId` only; `AllomorphCoOccurrenceRule` is tested specifically for the case where two allomorphs of the *same* morpheme must be told apart (`model.rs:544`, `rust/conformance/cooccurrence/allomorph-basic`). Missing piece named in §7. |
| **`BoundRoot`** | **PROVEN SIMPLER** | `O(1)` marginal states — a compile-time *omission* of one continuation arc, not a runtime automaton at all. §6. | N/A (no automaton is built; the construction has no accept/reject paths beyond what the un-omitted network already has). | **None** — resolved entirely at compile time from `Allomorph::is_bound`, a fact already known when the lexc entry is emitted. |

---

## 3. The single-constraint automaton

Fix one rule instance: `key` (a specific, statically-known `MorphemeId` — see below for why it's
always static), `others = [o_1..o_k]`, `adjacency`, `require`. Alphabet `Σ_tag` = every
`<R:nnnn>`/`<M:nnnn>` tag the grammar's morpheme count admits (size = morpheme count `M`, related
to but not equal to `N` = lexicon-entry/allomorph count).

**Why `key` is a compile-time constant, not a runtime unknown**: `morpheme_co_occurrence_ok`
(`validity.rs:421-431`) is always invoked with `key = m.morpheme` and rules read from
`g.morphemes[morpheme.0].co_occurrence` — i.e. the rule instance and its owning morpheme's id are
the *same* value, fixed at grammar-load time. **VERIFIED**, direct read.

**Why the scan position doesn't vary across repeated occurrences of `key`**: for the directional
modes, `co_occurs`'s scan (`validity.rs:329-352`) breaks at the *first* index where
`morph_list[i] == key` — this is the same index regardless of which occurrence of `key` triggered
the outer loop's call (`allomorphs_valid_impl` re-invokes the check once per morph occurrence,
`validity.rs:533`, but always against the same fixed `key` value and the same `morph_list`). So a
directional rule reduces to **one** whole-word regular-language membership question, re-asked
idempotently at each occurrence — not `O(occurrences)` independent questions. **VERIFIED** by
reading the call structure; the reduction itself is **INFERRED** from that structure.

### 3.1 `Anywhere` — the multiset/piecewise-testable case

`co_occurs` for `Anywhere` (`validity.rs:322-328`) is exactly: for each distinct id `s` occurring
`m_s` times in `others`, does `morph_list` contain at least `m_s` occurrences of `s`? This is a
conjunction of saturating-counter tests, one per distinct id in `others`.

**Construction**: state = a tuple of counters `(c_s)_{s ∈ distinct(others)}`, each capped at
`m_s` (saturating — never needs to count past what's required). Transition on tag `s`: if `s` is
one of the distinct `others` ids and `c_s < m_s`, increment; every other tag (including `key` and
every irrelevant morpheme) is a self-loop, no state change. Accept iff every counter has reached
its cap when the tape ends.

**Size**: `|Q| = ∏_{s} (m_s + 1)`. In the common special case where every distinct id in `others`
appears once (`m_s = 1` for all `d` distinct ids, `d ≤ k`), `|Q| = 2^d ≤ 2^k` — **exactly the
brief's own "2^k, if each tracks an independent bit" framing, achieved, not merely feared, in this
worst case.** This matches Simon's piecewise-testable-language family (the classical
`Σ*a_1Σ*a_2Σ*…Σ*a_kΣ*` "scattered subsequence" shape; Simon's foundational treatment is well known
in descriptional-complexity literature, e.g. surveyed in the piecewise-testability literature this
session located — Kufleitner & Lauser, "Alternative Automata Characterization of Piecewise
Testable Languages," and the "Partially Ordered Automata and Piecewise Testability" line;
**VERIFIED** these exist and use exactly this language family, **not independently re-derived from
either paper** — the `k+1`/`2^k`-style state count for the order-*sensitive* variant below is this
session's own construction, marked accordingly).

**Tightness (this session's own argument, novel-and-unverified as a formal citation, standard as a
technique)**: by Myhill–Nerode, if `others` is `d` distinct singleton ids and the predicate must
answer correctly for every possible SUBSET of them having appeared so far, the automaton must
distinguish `2^d` inequivalent prefixes (any two distinct subsets `A ≠ B` are separated by a
suffix containing exactly `others \ A`, which one subset satisfies and the other doesn't) — so
`2^k` is not just an upper bound in this worst case, it is *tight*. This is the same counting
argument underlying Yu, Zhuang & Salomaa (1994)'s tight `mn` intersection bound (`Theoretical
Computer Science` 125(2):315-328, **VERIFIED** existence/result via web search this session, cross-
checked against report `10`'s own prior verification of the same paper), generalized from two
factors to `d`.

### 3.2 `SomewhereToLeft`/`SomewhereToRight` — the ordered-subsequence case

This is Beesley (1998)'s own footnote-3 construction, read directly this session
(`aclanthology.org/W98-1312.pdf`, p.125 n.3, **VERIFIED** — full text fetched and read): for the
two-constraint Arabic example, Beesley gives *"the same constraint could be imposed by the rule
`^Art:0 /<= _ \^Def:0* .#.`"* — a **complement class** (`\^Def:0`, "every symbol pair other than
`^Def:0`") run to end-of-word (`.#.`). This is precisely: *from the trigger tag onward, require that
every subsequent tag avoid the forbidden class, all the way to the word boundary* — a 2-3 state
DFA for one bit of state ("have I seen the trigger yet").

Generalizing to `others = [o_1..o_k]` in order: state = `i ∈ {0..k}` ("have matched the first `i`
elements of `others` so far"), transition on `o_{i+1}` advances `i → i+1`, transition on `key`
decides accept/reject from the current `i` (accept iff `i == k`, for `require`), every other tag
self-loops. This is the textbook "does the input contain `others` as a subsequence" automaton — a
chain of `k+1` states, no branching, already deterministic by construction. Add one more bit for
"has `key` already been seen" (to freeze the verdict at the first occurrence, matching
`validity.rs:333`'s `break`) — **`|Q| ≤ 2(k+1)`, linear in `k`**, not exponential. This is
`AllFlags`'s own `EnvCoverage::LeftLiteral` shape one level up (`precision.rs:76-79`,
"left-literal, single-environment, require" is the `k=1` special case of exactly this family).

`AdjacentToLeft`/`AdjacentToRight` additionally require immediate adjacency between consumed
`others` (`validity.rs:336-347`) — this needs the automaton to also remember "was the immediately
preceding tag the expected next item," a small constant-factor addition (still `O(k)`, not
exponential): **`|Q| ≤ 3(k+1)`** or so, by direct construction (this session's own derivation,
novel-and-unverified as a numbered citation, standard as automata-theoretic technique).

`SomewhereToRight`/`AdjacentToRight` are the mirror construction, naturally a right-to-left scan.
Realizing this in a left-to-right toolchain needs either (a) reversing the tape at compile time
(foma's reverse operator) or (b) running the equivalent left-to-right automaton against the
*reversed* rule (swap the roles of "consumed so far" and "remaining"). Reversal of a *general* DFA
can blow up exponentially (Yu, Zhuang & Salomaa 1994 also treat reversal, per report `10`'s own
citation of that paper) — but reversing this specific **chain**-shaped automaton reverses only the
direction of a linear chain, staying `O(k)`. This reversal-stays-linear claim is this session's own
reasoning (**novel-and-unverified** as a specific citation), not found stated as a general theorem
in the sources fetched this session.

### 3.3 The `|Σ_tag|`-independence caveat (important, and easy to get wrong)

**State count** for every construction above is `O(g(k))`, independent of `N` and of `|Σ_tag|` —
this is VERIFIED by direct construction (§3.1-3.2 never reference `|Σ_tag|` in the state count).
**Arc count** is a different question: the "every other tag self-loops" transition, if compiled by
enumerating every tag in `Σ_tag` individually, reintroduces an `O(|Σ_tag|)` factor per state — and
`|Σ_tag|` scales with the grammar's morpheme count, which correlates with (though is not identical
to) `N`. This is avoidable **only if the toolchain compiles a genuine complement/wildcard
transition** rather than enumerating. This project's own vendored `foma-rs` has exactly this
primitive: `rewrite.rs`'s `NotContain`/`fsm_minus`-based construction (report `07` §2, `rewrite.rs`
line refs there, **VERIFIED** by report `07`, re-used here) and the `UNKNOWN`/`IDENTITY`
sigma-merge machinery in `products.rs` (report `07` §7, **VERIFIED**) both demonstrate the
toolchain already builds complement-class transitions without per-symbol enumeration. **The
construction only satisfies the brief's "no dependence on N" criterion in the strict (arc-count)
sense if this wildcard mechanism is used** — a plain per-symbol enumeration would silently
reintroduce an `M`-dependent (not `N`-dependent, but not free either) arc count. Flagged explicitly
because report `10`'s own §3.7/Part 3 shows this project has been burned before by a filter that
looked free and wasn't (row C13, the boundary-cleanup blanket-deletion regex).

---

## 4. Composition of `K` constraints — the `2^k` question, settled both ways

**Within one rule**: settled in §3.1 — `Anywhere` genuinely achieves `2^k` (tight, not just
feared) when `others` is `k` distinct singleton ids; the ordered adjacencies stay linear.

**Across `K` separate rule instances in a grammar**: if built as **one eager, monolithically
minimized product automaton**, the size is `∏_{r=1}^{K} |Q_r|` — and this product **is achieved**,
not merely bounded, exactly when the `K` rules' progress states are genuinely independent (no
shared `others`/`key` ids, no overlapping adjacency windows) — the same Yu-Zhuang-Salomaa
tightness argument applies compositionally. **Is this the common case?** This session found no
per-grammar measurement either way (report `10` Part 3.8/4.5 already establishes this exact
gap — "no analyzer has been built both ways" — and this report does not close it either, since
doing so would require a build, out of scope). What *is* established, empirically, elsewhere in
this exact research area: Yli-Jyrä (2011), "Compiling Simple Context Restrictions with
Nondeterministic Automata" (FSMNLP 2011, ACL Anthology **W11-4405**, **VERIFIED** existence via
report `10`'s own prior fetch, re-used here), measured ~1,100 real context-restriction constraints
from a syntactic grammar and found the compiled result **1.0-4.0× the minimal DFA size** — far
below the worst-case exponential bound, in practice, for a directly analogous compilation problem.
This is evidence *for* near-independence being common, not proof it holds for any specific PanGloss
co-occurrence rule set.

**Is the `2^K` product avoidable regardless of independence? Yes — by not building it monolithically
at all.** Two strategies, in order of recommendation:

### 4.1 Build the filters small, intersect them first, compose the proposer once

This is Karttunen (1994)'s own "intersecting composition" (`Constructing Lexical Transducers`,
COLING-94, ACL Anthology **C94-1066**, **VERIFIED** existence/claim via report `10`'s prior fetch,
re-used here), stated in his own words as avoiding materializing "the intersection of the
rule-transducers alone," and is what `hfst-compose-intersect` implements (Lindén, Silfverberg &
Pirinen 2009, cited by report `10` §4.4, **VERIFIED** there). The ordering matters, and this
session states it precisely because it is easy to get backwards: **compose the `K` small
co-occurrence filters together first** (cheap — their combined size depends only on the grammar's
own rule count and each rule's own `k_r`, never on `N`, even in the pessimal `∏` case, because
`K` and every `k_r` are grammar-authored constants), **and only then compose the single resulting
filter against the proposer `P` once** (`|P| × |combined filter|`, one multiplication, not `K`
sequential ones). Composing `P` against `F_1`, then that result against `F_2`, …, sequentially,
risks the same class of transient-blowup trap this project already measured directly: report
`10`'s union-vs-compose incident (`p6-prototype-report.md` §2.2, **VERIFIED** there) shows 14
individually-trivial per-branch filters combined with the wrong combinator cost **10,324× in
states** — not because any one filter was expensive, but because the *combination order/operator*
was. The lesson generalizes: build small, combine small, touch the large proposer last, once.

**Application strategy this names, per the brief's requirement**: lazy/lexicographic intersecting
composition of the co-occurrence filter set (`hfst-compose-intersect`-shaped), never sequential
eager `.o.` folding of the proposer against one filter at a time.

### 4.2 Flag diacritics — avoid materializing the product at all

This is Beesley (1998)'s own answer, and it is stronger than "avoids `2^K`" — it removes the
question. Read in full this session (`aclanthology.org/W98-1312.pdf`, FSMNLP'98, pp. 118-127,
**VERIFIED**, direct primary-source read, not a secondary summary):

- **The paper's own worked example is a 2-constraint case, and gives a measured size number for
  composing it in**: *"the entire sublexicon of noun stems is copied once, for the l+
  restriction, and then that result is copied again, to capture the bi+ restriction, **almost
  quadrupling** the final size of the transducer"* (p.122, **VERIFIED**, quoted verbatim) — `K=2`
  independent constraints, ~`2^2` = 4× blowup from composing them in, a *directly measured*
  real-world instance of the product bound, not a hypothetical.
- **A stronger, categorical claim about flags avoiding the product, not just shrinking it**: *"The
  4.6 megabyte machine includes more important constraints, encoded as flag diacritics, **that
  cannot be composed in because the size of the network becomes uncomputably large**"* (p.124,
  **VERIFIED**, quoted verbatim). This is not "flags are cheaper" — it is "for these constraints,
  the compose-in alternative does not terminate at a usable size at all." Beesley also reports:
  five flag diacritics took a Hungarian analyzer from 38MB (composed-in, but only a *deliberately
  restricted* subset of constraints) to under 5MB (flags, with *more* constraints included) — an
  ~8× size reduction while covering *more* ground, not less (p.124, **VERIFIED**).
- **Mechanism, exactly as this project's own `flags.rs` ports it**: Beesley's own table (p.123-124,
  **VERIFIED**, transcribed directly) — `@C.Feat@` clear, `@P.Feat.Val@` positive set, `@N.Feat.Val@`
  negative set, `@U.Feat.Val@` unify-test, `@R.Feat.Val@`/`@R.Feat@` require-test, `@D.Feat.Val@`/
  `@D.Feat@` disallow-test — is the direct, one-for-one ancestor of `foma-rs`'s `FlagType`
  (`UNIFY`/`POSITIVE`/`NEGATIVE`/`REQUIRE`/`DISALLOW`/CLEAR, `flags.rs:293-305`, per report `03`'s
  own cross-reference table, re-verified consistent here) and of GiellaLT's own
  `docu-sme-flag-diacritics.md` U/P/N/R/D/C semantics (per report `03` §3.1, re-used).
- **The theoretical framing, stated as the paper's own conclusion, not this report's inference**:
  *"Pure finite-state networks have no stack or other 'memory'... Where languages have separated
  morphotactic dependencies... capturing the dependencies in a pure finite-state network requires
  copying the structures between the dependent morphemes, with a resulting explosion in size. To
  keep such systems small, some way is required to inject a tiny bit of memory into the overall
  system"* (p.125, **VERIFIED**, quoted verbatim, Beesley's §4 "Conclusion"). Flags are that
  "tiny bit of memory," carried by the apply-time lookup process itself rather than by automaton
  states — precisely the `O(Σk_r)` additive vs. `O(∏k_r)` multiplicative trade the brief asks to
  settle.
- **Runtime cost, in the paper's own words**: *"The use of flag diacritics in general entails a
  slight runtime performance penalty, compared to composing in the same restrictions, because of
  increased backtracking"* (p.124, **VERIFIED**). This project's own prior-verified figure (report
  `03`/`10`, citing `2026-07-15-fst-precision-knob-design.md`'s framing of Beesley & Karttunen 2003)
  — a ~20-70% apply-time lookup slowdown band — is **inherited from this project's own prior
  citation, not independently re-verified against the 2003 textbook this session** (that book was
  searched for but its specific percentage figure was not locatable online; stated as "not found"
  rather than invented, consistent with report `03`'s own honesty convention for the same figure).

**Determinism**: Beesley's own footnote 7 states flags are *"intentionally nonmonotonic, so the
use of the term 'unification' for the `U` operation is not quite accurate"* (p.125, **VERIFIED**).
This matters for "is the result still deterministic in the sense the project owner wants" (brief's
own phrasing): flag-diacritic apply is a **deterministic traversal of a deterministic automaton
augmented with a small side-table of named attribute values** — the automaton itself stays a DFA
(no new states for the flags at all, since flag symbols are ordinary multichar arcs), but the
overall *system* (automaton + flag state) is not a pure automaton in the classical sense; it is
exactly the "registered automaton" model (Cohen-Sygal & Wintner 2006, "Finite-State Registered
Automata for Non-Concatenative Morphology," *Computational Linguistics* 32(1):49-82 — title/venue
**VERIFIED** via this session's citation search, content not read). The apply engine's own
traversal is still deterministic per input (no branching ambiguity is introduced beyond what
`apply_up`/`apply_down`'s ordinary backtracking search already does) — this matches report `07`'s
own finding that `obey_flags` defaults true and flag consistency is checked deterministically per
path (`apply.rs:990-1001`, **VERIFIED** by report `07`, re-used here).

---

## 5. Staged intersection vs. flag diacritics — direct comparison and recommendation

| | Staged/lazy intersecting composition (§4.1) | Flag diacritics (§4.2) |
|---|---|---|
| Build-time size | `O(∏ small filter sizes)` in the worst case (rarely achieved per Yli-Jyrä's 1.0-4.0× empirical measurement, §4), composed **once** against `P` | `O(Σ k_r)` additive — network grows by inline symbol tokens only, never a new state/lexicon per constraint (same mechanism as `precision.rs`'s already-shipped `AllFlags` preset, `precision.rs:186-195,192-195`, **VERIFIED**: "network size grows by AT MOST `entries × coverable_constraints` extra inline symbol tokens, linearly, by construction") |
| Apply-time cost | Zero marginal cost — fully resolved at compile time | ~20-70% lookup slowdown band (inherited citation, §4.2), Beesley's own "slight... penalty... because of increased backtracking" |
| Determinism | Ordinary DFA; determinizing a product of DFAs adds no extra subset-construction cost beyond the product's own size (§4) | Deterministic per-path traversal augmented with named-attribute side state (registered-automaton model); not a classical DFA in isolation, but no new branching ambiguity |
| Toolchain safety in `foma-rs` **today** | Needs the wildcard/complement primitive (§3.3) to avoid an `|Σ_tag|`-proportional arc blowup — VERIFIED present (`rewrite.rs`'s `NotContain`, `products.rs`'s `UNKNOWN`, report `07`) | **Safe by report `07`'s own finding, and unusually cleanly so**: co-occurrence flags are a pure **insert-then-read** shape — a morph's own lexc entry unconditionally sets/require-tests a flag on plain concatenation, with **no `->`/`<-` replace-rule `\|\|` context anywhere in this construct at all** (co-occurrence is checked over the *tag* tape, not synthesized by a rewrite rule). Report `07`'s entire caveat ("safe only when inserted-and-read rather than matched-in-context," `07`'s §10-11) is about flags occupying a *matched* role inside a rewrite rule's context — a role co-occurrence flags never occupy. This family is **safer** than the `Environment` family already shipped with `AllFlags` (which at least had to worry about `prule-tail-rewrite-risk`, `precision.rs:459-507`, because environment literals interact with phonology; co-occurrence keys off morpheme *identity*, which phonological rewriting never touches). |
| Precedent in this crate | Not yet built for any family | **Already built, for a different family, with the identical recipe**: `precision.rs`'s `PrecisionEmit` (`precision.rs:696-800`) — owner-side `@R@` require prefix, set-side `@P@` positive-set on every relevant entry, both spliced inline into the LOWER tape text `write_tag_entry` already threads through every entry. `MorphemeCoOccurrence` needs the same recipe keyed on `MorphemeId` membership instead of surface-literal adjacency. |

**Recommendation**: flag diacritics, for `MorphemeCoOccurrence`, reusing `precision.rs`'s existing
`PrecisionEmit`-shaped recipe almost verbatim (§9). This is not a close call given report `07`'s own
constraint: the toolchain's one *documented, reproduced* flag/replace-rule hazard is entirely absent
from this construct's shape. Staged intersecting composition (§4.1) remains the right fallback for a
future `Eliminate`-style promotion once a grammar's specific co-occurrence rule set is proven
independent enough to compose in for free (mirroring GiellaLT's own dual strategy: flags live in the
main analyser, eliminated in the generator build once measured safe, report `03` §3.1, **VERIFIED**
there).

---

## 6. `BoundRoot` — the cheapest construction in the group

The predicate (`is_bound && distinct_count == 1`) is **not** a tag-sequence property at all in the
interesting case — it is a **topological** fact about whether the compiled network offers a direct
path from a specific bound-root entry to the word-end marker with no intervening affix or second
root. `emit.rs`'s own module doc already names the exact site this needs to change:

> *"**Bare-root paths** (`trie.rs` `run()` "Bare-root paths"): every root allomorph directly
> accepting. Deviation (upward): trie gates bare roots on `bare_root_surfaces` non-empty (the
> obligatory-inflection check, which needs a live `Morpher`); this emitter admits every root
> bare — a superset; the verify pass (P2) prunes."* (`emit.rs:1-10`, **VERIFIED**)

This is a *different* gate (obligatory inflection, not `BoundRoot`) using the *same* mechanism
(whether a root's entry offers a direct bare-accepting continuation) — confirming the switch
already exists structurally and is currently wired permissively for an unrelated reason. The
`BoundRoot` construction: **for entries whose allomorph has `is_bound == true`, omit the direct
bare-accepting continuation, while leaving every affix-continuation and compound-continuation path
untouched.** This requires:

- **Zero new automaton states.** It is a subtraction of one arc/entry the permissive emitter would
  otherwise add unconditionally, not an addition.
- **Zero tape information.** `is_bound` is a `Grammar`-level, compile-time-known fact
  (`model.rs:791`) — the decision is made once, when the lexc source is written, never consulted at
  apply time.
- **No flags, no filter composition, no runtime cost at all.**

This is `PROVEN SIMPLER` in the strongest sense available in this taxonomy: cheaper than any
tag-based automaton, because it isn't an automaton — it is the compile-time absence of a
transition, the same shape as row A6/C7 in report `10` (MPR-Overwrite Construction 2's
reachability proof, and morphotactic pruning's engine-legal-adjacency filter) — both of which
report `10` already identifies as "characterizer-only... never touches the FST." §7 of report `10`
independently reaches the same "cheapest wins are structural, not tag-based" conclusion for two
other constructs; `BoundRoot` is a third instance.

**Caveat** (named, not resolved, matching report `10`'s own convention for similarly-shaped open
items): the W3.2 disjunctive recheck (§1.2) also tests **unchosen sibling allomorphs'** bound
status against the same `distinct_count`. Since a lexical entry's full allomorph set (which ones
are bound) is itself a compile-time-known, per-entry-bounded fact, this is very likely closable by
the same style of small, characterizer-only reachability argument report `10` used for row A6
(Construction 2) — but this session did not carry that proof through for every reference grammar,
and flags it explicitly as the one piece of due diligence left before calling `BoundRoot` fully
closed end-to-end (as opposed to closed for the primary check, which is unconditionally true today).

---

## 7. Tape information required that is not emitted today

- **`MorphemeCoOccurrence`: none.** `<R:nnnn>`/`<M:nnnn>` already encode exactly `MorphemeId`,
  emitted for every morph occurrence including zero-surface ones (`tags.rs:2-5`; `precision.rs`'s
  own "Tag tape convention" note that every entry's upper side is the tag symbol alone,
  `precision.rs:172-176`, **VERIFIED**, reused for a different family but the same underlying fact).
  A `MorphemeCoOccurrence` filter (flag-based or staged) can be built against **today's** emitted
  network with no emitter change.
- **`AllomorphCoOccurrence`: a targeted, scoped gap.** The tag alphabet is keyed by `MorphemeId`
  only (`tags.rs:118-136`: `root_tag_lexc`/`morph_tag_lexc` both take a `MorphemeId`, never an
  `AllomorphId`). `AllomorphCoOccurrenceRuleDef` is specifically tested against the case where two
  allomorphs of the *same* morpheme must be distinguished (`model.rs:544`'s own citation of
  `rust/conformance/cooccurrence/allomorph-basic`) — a case `MorphemeId` cannot resolve by
  construction. **The fix is narrow, not a blanket alphabet change**: only morphemes that (a) have
  more than one allomorph *and* (b) at least one of those allomorphs carries or is named by an
  `AllomorphCoOccurrenceRuleDef` need a distinguishing tag — most grammars will need this for a
  small number of morphemes, not all `N` entries. Two concrete options, not adjudicated here (out
  of scope — this is a `pg-foma`/emitter design decision, not a math question):
  1. A secondary tag co-located with the existing `<R:nnnn>`/`<M:nnnn>` (e.g. `<A:nnnn>`, an
     `AllomorphId`), emitted only for the targeted subset.
  2. Switch the *existing* tag's numeral to `AllomorphId` instead of `MorphemeId` for those same
     targeted entries only (cheaper in symbol count, more invasive to `tags.rs`'s width-sizing
     convention, which is sized from `morpheme_count`, `tags.rs:88-94`).
- **`BoundRoot`: none.** Resolved entirely off-tape, at compile time (§6).

---

## 8. The cross-family question: one parameterized schema, or whack-a-mole?

**Not one schema — exactly two, and both already have shipped precedent in this crate, which is
the actual answer to the owner's "will this converge" worry.**

1. **Tag-Sequence Regular Constraint (TSRC)**: parameterized by `(Σ_tag granularity, key id,
   others id-list, adjacency mode, require/exclude)`. `MorphemeCoOccurrence` and
   `AllomorphCoOccurrence` are **the same schema at two different tag granularities** — a single
   generic implementation (construct the automaton of §3, or the flag recipe of §9, parameterized
   by "which id space") covers both, once `AllomorphCoOccurrence`'s tape gap (§7) is closed. This
   schema is not new to this session's proposal in spirit: `precision.rs`'s `AllFlags`
   `EnvCoverage::LeftLiteral` (`precision.rs:76-79,128-166`) is already a `k=1` instance of exactly
   this pattern (adjacency = a form of `SomewhereToLeft`, over surface-literal identity rather than
   morpheme identity) — the generalization is mechanical, not conceptual.
2. **Structural Reachability Schema (SRS)**: parameterized by `(a graph derived from the grammar's
   own continuation/rule/touch structure, a property to preserve or exclude at specific nodes)`.
   `BoundRoot` (§6) is a third instance of a pattern report `10` already names twice: MPR-Overwrite
   Construction 2's reachability proof (row A6, "characterizer-only... never touches the FST") and
   morphotactic pruning's engine-legal-adjacency filter (row C7, same framing). All three are
   compile-time graph facts about the grammar's own rule/continuation structure, never runtime tag
   inspection.

**Which family goes in which schema is decided by one question**: *does the constraint need to
inspect the assembled word's tag sequence at apply time, or can it be decided once, at compile
time, from the grammar's own static structure?* Co-occurrence rules genuinely need the former
(the specific morphemes/allomorphs realized in *this* word are a runtime fact — no compile-time
graph walk can know which candidate a given proposer path represents without walking the tag tape).
`BoundRoot` needs only the latter (whether *some* legal continuation beyond this root exists is a
static fact about the grammar, not about which candidate is being checked). This is the same
"conservation law" report `10`'s synthesis names in `00-synthesis-and-decision.md` §6a — *"a filter
can only reject on information present on the tape... both stages cannot be simplified at once"* —
applied here as a **classifier** rather than a warning: it sorts every future unbuilt family
(`StemName`, `HeadFeatures`, `CompoundingFs`, `ObligatoryFeatures`, `FreeFluctuation`, `Circumfix`,
`precision.rs:244-262`) into one of exactly these two buckets before any code is written, which is
the concrete answer to "will per-family work converge": the number of *schemas* is small and fixed
(two, so far), even though the number of *families* is not.

---

## 9. A concrete `MorphemeCoOccurrence` flag recipe (novel-and-unverified as a shipped
    construction; directly modeled on already-shipped code)

Sketched here because the brief asks for explicit constructions, not just bounds. This mirrors
`precision.rs`'s `PrecisionEmit` (`precision.rs:696-800`) structurally; it is **not** built or
tested anywhere in this codebase today — marked **novel-and-unverified** as an implementation,
though every primitive it uses (inline `@P@`/`@R@` splicing on the lower tape, unconditional
overwrite semantics, `flag_id`-style zero-digit/dot-free naming) is already shipped and tested for
the `Environment` family.

For one `MorphemeCoOccurrenceRuleDef` with `others = [o_1..o_k]`, `adjacency = Anywhere`,
`require = true`:

- Mint `k` distinct flag attributes, one per distinct id in `others` (or one shared counter-style
  attribute per id with multiplicity, per §3.1) — e.g. `@P.COOC{ruleid}_{i}.y@` set unconditionally
  on every entry whose `MorphemeId` equals `others[i]` (this is already exactly what tag emission
  does at the *identity* level — no new adjacency test is needed the way `Environment`'s `could_satisfy`
  needed one, because morpheme identity is a *global* fact already fully captured by which tag fired,
  not a local surface-adjacency approximation).
- On the `key` morpheme's own entries, **require** every one of the `k` flags:
  `@R.COOC{ruleid}_{1}.y@ … @R.COOC{ruleid}_{k}.y@`, prepended (or appended — placement doesn't
  matter, per `precision.rs`'s own finding that lexc's alignment pads whichever tape is shorter,
  `precision.rs:168-184`, reused reasoning).
- For `require = false` (exclude), swap to `@D@` disallow tests, exactly Beesley's own
  `@D.Feat.Val@`/`@D.Feat@` row (§4.2's transcribed table).
- For `SomewhereToLeft`/`AdjacentToLeft`, the same recipe suffices unmodified, because the flags
  are set **unconditionally, forward, by every relevant entry as it is emitted** — the natural
  left-to-right propagation flags already have (Beesley's own p.123: *"the very finite amount of
  memory required is carried by the enhanced lookup process itself"*) reproduces "somewhere to the
  left" without any extra bookkeeping. `AdjacentTo*` needs the `@C@` clear operation to reset the
  "immediately preceding" tracking flag on every *other* entry (so a non-adjacent intervening
  morph correctly breaks the chain) — the one place this recipe needs a construct `Environment`'s
  did not (that family never needed `@C@` at all, `precision.rs:204-210`'s own note that this
  module only ever emits `@P@`/`@R@`).
- `SomewhereToRight`/`AdjacentToRight` need the flags to propagate **backward** — not natural for
  a left-to-right apply pass. The standard fix (used nowhere in this codebase; a genuinely novel
  application here) is the "positive-set now, disallow on every non-matching later position"
  technique `precision.rs:44-51`'s own module doc names and explicitly declines for right-context
  environments ("exactly the kind of wide-blast-radius, hard-to-verify transform this step
  declines"). This is the one piece of the `MorphemeCoOccurrence` recipe that is **harder than
  `Environment`'s own right-context gap**, because unlike environments (bounded literal patterns),
  co-occurrence's `others` can be *any* morpheme in the grammar — flagging this as the one place
  this report's recommendation is weaker than a flat "yes, trivially": `SomewhereToRight`/
  `AdjacentToRight` rules should be checked per-grammar for how common they are before committing
  to the flag recipe over the staged/compose alternative (§4.1), which handles right-context
  symmetrically at no extra conceptual cost (a DFA scanning right-to-left is no harder to build
  than one scanning left-to-right, per §3.2).

---

## 10. Literature — final accounting

| Citation | Status | Used for |
|---|---|---|
| Beesley, K.R. (1998), "Constraining Separated Morphotactic Dependencies in Finite-State Grammars," FSMNLP'98, ACL Anthology W98-1312, pp.118-127 | **VERIFIED**, full primary-source PDF fetched and read this session | §4.2, §6 conclusion, the flag-type table, the quadrupling/uncomputably-large/38MB→5MB numbers, footnote-3's complement-class construction (§3.2) |
| Beesley & Karttunen (2003), *Finite State Morphology*, CSLI | Existence **VERIFIED**; specific runtime-percentage claim **not found** online this session (inherited citation from this project's own prior report `03`/`10`, not independently re-verified against the book) | §4.2's ~20-70% band (marked inherited, not re-verified) |
| Kiraz, G.A. (1997), "Compiling Regular Formalisms with Rule Features into Finite-State Automata," ACL/EACL 1997, pp.329-336 | Title/venue **VERIFIED** via reference list of the positionwise-flags paper below; content **not read** this session | Named per the brief's required-reading list; not otherwise drawn on |
| Kiraz, G.A. (1994/2000), multi-tape/multi-tiered nonlinear morphology (Syriac/Arabic) | Existence **VERIFIED** via search; content **not read** this session | Named per brief; not drawn on for this report's constructions (co-occurrence here is single-tape/tag-sequence, not multi-tape) |
| Yu, S., Zhuang, Q., Salomaa, K. (1994), "The state complexities of some basic operations on regular languages," *TCS* 125(2):315-328 | **VERIFIED** (existence/tight-`mn`-intersection result, re-confirmed this session, originally verified by report `10`) | §3.1's tightness argument (generalized from 2 factors to `d`), §4's product-blowup framing |
| Yli-Jyrä, A. (2011), "Compiling Simple Context Restrictions with Nondeterministic Automata," FSMNLP 2011, ACL Anthology W11-4405 | **VERIFIED** by report `10`, re-used here | §4's "is `2^K` achieved in practice" empirical counter-evidence (1.0-4.0× measurement) |
| Yli-Jyrä, A. (2011), "Explorations on Positionwise Flag Diacritics in Finite-State Morphology," NODALIDA 2011, ACL Anthology W11-4636 | **VERIFIED**, full primary-source PDF fetched and read this session | Cross-check of the Beesley & Karttunen 2003 flag-type table (Table 1, matches `flags.rs`'s `FlagType` one-for-one); confirms flag diacritics' state-space-vs-lookup-memory framing independently of Beesley 1998; **not otherwise load-bearing** — this paper's own subject (morphophonological harmony via positionwise flags) is a different construct family than co-occurrence |
| Karttunen, L. (1994), "Constructing Lexical Transducers," COLING-94, ACL Anthology C94-1066 | **VERIFIED** by report `10`, re-used here | §4.1's intersecting-composition strategy and ordering argument |
| Lindén, Silfverberg, Pirinen (2009), HFST tools paper | **VERIFIED** by report `10`, re-used here | §4.1's `hfst-compose-intersect` attribution |
| Cohen-Sygal & Wintner (2006), "Finite-State Registered Automata for Non-Concatenative Morphology," *Computational Linguistics* 32(1):49-82 | Title/venue **VERIFIED** via citation search this session; content **not read** | §4.2's "registered automaton" framing for flag-augmented determinism |
| Simon, I. (piecewise-testable languages, foundational) | Concept **VERIFIED** to exist and match the `Anywhere`-adjacency language shape via this session's search (Kufleitner & Lauser and related surveys); the specific `k+1`/`2^k` state counts attributed to it in §3 are **this session's own derivation**, not a quoted theorem from a located primary source | §3.1's framing of the `Anywhere` case as a piecewise-testable language family |
| Chandlee, J. and Heinz/Chandlee co-authored ISL/OSL-function papers; Gainor, Lai & Heinz (2012), "Computational Characterizations of Vowel Harmony Patterns and Pathologies," WCCFL 29 | Existence **VERIFIED** via search this session | Named per the brief's required-reading list on long-distance phonological dependencies as finite-state; **not drawn on** — this report's three families are morphotactic/tag-level, not phonological-segment-level, so the ISL/harmony literature's specific machinery (input-strict-locality over segments) does not directly transfer; flagged as a boundary the brief's own literature list crosses that this report's constructions do not need |
| Kaplan, R.M., Kay, M. (1994), "Regular models of phonological rule systems," *Computational Linguistics* 20(3):331-378 | **VERIFIED** to exist (cited by both Beesley 1998's own reference list and this project's prior reports) | Background only — the general regularity-preservation result underlying why rewrite-rule composition stays regular; not this report's central mechanism |

---

## 11. Summary answers to the brief's five deliverables

1. **Verdict table**: §2.
2. **Explicit DFA constructions + `2^k` settled**: §3 (single-rule, both achieved-`2^k` for
   `Anywhere` and linear-`O(k)` for the ordered adjacencies) and §4 (composition: achieved when
   rules are independent and built monolithically; avoidable either by intersecting-then-composing-
   once, §4.1, or by flags removing the product question entirely, §4.2).
3. **Staged intersection vs. flags, with recommendation**: §5 — flags recommended for
   `MorphemeCoOccurrence`, specifically because this construct's shape (pure insert-then-read, no
   rewrite-rule context ever) sits entirely outside report `07`'s documented hazard, and because
   the identical recipe is already shipped for a different family (`precision.rs`).
4. **Cross-family schema**: §8 — two schemas (tag-sequence-regular; structural-reachability), not
   one, but both already instantiated elsewhere in this crate, which directly answers the
   whack-a-mole worry: the *count* of reusable schemas is small and already known, even though the
   *count* of unbuilt families (ten, per `precision.rs:239-263`) is not.
5. **Tape information not emitted today**: §7 — none for `MorphemeCoOccurrence` or `BoundRoot`; a
   narrow, scoped allomorph-identity gap for `AllomorphCoOccurrence`, which is why that family's
   verdict is `OPEN` rather than `PROVEN SIMPLER` despite an identical, already-solved mathematics.
