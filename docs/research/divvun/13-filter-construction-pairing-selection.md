# Filter construction: pairing and selection constraints (Circumfix, StemName, FreeFluctuation) — and the `Environment` calibration

Research report, agent 13. No code changed, no build run (`cargo`/`rust/tools/pg.ps1` never
invoked), no commits made. Scope: apply the categorical-simplicity criterion — `|F_X| =
O(g(k, |Σ_tag|))`, no dependence on lexicon size `N`, polynomial-bounded determinization, safe
staged composition — to three unbuilt gate-constraint families (`Circumfix`, `StemName`,
`FreeFluctuation`) and to the one family PanGloss has actually built (`Environment`), which serves
as this report's calibration case. Context read in full: `00-synthesis-and-decision.md` (esp. §6a),
`05-hc-to-fst-expressibility.md`, `10-filter-complexity-tractability.md` (all in this directory).
Claims are marked **VERIFIED** (read directly at the cited `path:line` this session) or
**INFERRED** (reasoned from verified facts, not re-derived from a primary source); "not found" is
stated rather than guessed.

---

## 0. Headline result — read this first

**The calibration fails, and it fails in an informative way.** `Environment` — the one family
`pg-foma/src/precision.rs` actually populates — does not satisfy the categorical-simplicity
criterion, and moreover it is not built in the shape the criterion assumes at all. It is not a
separate automaton `F` composed as `P ∘ F`; it is a modification of how `P` itself is built (inline
flag text appended to every lexicon entry, `precision.rs:138-195`, **VERIFIED**), and its own module
doc states the cost directly: growth is "AT MOST `entries × coverable_constraints` extra inline
symbol tokens, linearly" (`precision.rs:193`, **VERIFIED**) — `entries` is `N`. There is no
`|F_env|` to measure independent of `N` because no such object is ever constructed.

This is not an isolated quirk of one module. Every other cheap, correct mechanism this session found
in the codebase (`gate.rs`'s MPR/POS static partition, the `Circumfix` structural-composite route)
is *also* not a post-hoc filter — each is a different way of **building `P` differently**, not a way
of filtering an unmodified `P`. §7 makes this the report's second headline: the project has (at
least) four working construction schemas, and "compose a small filter after an unmodified proposer"
is not empirically one of them.

Per-family verdicts, detailed in §§2–5:

| Family | Verdict | Bound (if any) | Missing piece (if OPEN) |
|---|---|---|---|
| **Environment** (calibration) | **PROVEN NOT SIMPLER**, as built | `O(N·k)` inline label growth (`k` = coverable constraints); `O(1)` new states | N/A — not a filter, a `P`-modification; see §1 |
| **Circumfix** (nesting bound) | **SETTLED — not a wall** | bounded by `max_apps` (default 1, `model.rs:635-636`) × a fixed structural-composite chain cap (`STRUCT_MAX_EXTRA_RULES = 3`, `emit.rs:2262-2266`) | — |
| **Circumfix** (categorical simplicity, as built) | **PROVEN NOT SIMPLER** | `O(roots × rules^depth)` enumeration (`emit.rs:3971-3972`, same mechanism that OOMs on Aweti) | — |
| **Circumfix** (a genuinely small construction) | **OPEN** | would be `O(1)` per circumfix rule if built | boundary-marker two-sided insertion (Beesley 1998; §3.3), unverified against `foma-rs` |
| **StemName**, rule-level gate (`required_stem_name`) | **PROVEN SIMPLER** | `O(1)` at runtime; **zero duplication** (a true partition, like `gate.rs`) | — (direct application of an already-shipped template) |
| **StemName**, word-validity region gate | **OPEN** | plausibly `O(distinct regions)` via reachability BFS, not `O(N)` | extend `derivable_to_category`-style BFS (`emit.rs`) to region compatibility; unbuilt |
| **FreeFluctuation** | **OPEN** (genuine constraint, not multiplicity — see §5) | depends on Environment's own declined pieces | excluded/right-context environment encoding (`precision.rs`'s own findings 2–4, declined) |

---

## 1. The `Environment` calibration, in full

### 1.1 What was actually built

`pg-foma/src/precision.rs` is the only module populating `ConstraintFamily` (the enum at
`precision.rs:239-263`, **VERIFIED** — ten of eleven variants, including `StemName` at line 246 and
`FreeFluctuation` at line 260, are declared and never constructed; only `Environment`, line 242, is
ever built). The construction sites are `push_instances` (`precision.rs:390-413`, `family:
ConstraintFamily::Environment` at line 404, **VERIFIED**) and a test fixture at
`precision.rs:927-943` (`family: ConstraintFamily::Environment` at line 932, **VERIFIED**) — these
are the two sites the task named, and both simply *tag* an `EnvConstraint` with which family it
belongs to; neither is where the network is built.

The actual mechanism (`precision.rs:128-221`, module doc "Emission mechanism," **VERIFIED**):

- For each `EnvCoverage::LeftLiteral` instance (the one shape judged safely flag-representable —
  see §1.2 for what was declined), three flag symbols are minted: a require `@R.ENV{id}.y@` and two
  set symbols `@P.ENV{id}.y@`/`@P.ENV{id}.n@` (`precision.rs:129-133`).
- **Every non-empty-surface lexc entry in the whole grammar**, whether or not it owns the
  constraint, gets exactly one of the two set symbols appended to its own LOWER-tape text
  (`precision.rs:138-148`, `PrecisionEmit::tagged_lower`, `precision.rs:769-799`). The owning
  allomorph's own entries additionally get the require symbol prepended (`precision.rs:149-155`).
- No new lexicon block, no new automaton state, is ever synthesized: "No `LEXICON` blocks are ever
  synthesized by this module — network size grows by AT MOST `entries × coverable_constraints`
  extra inline symbol tokens, linearly, by construction" (`precision.rs:192-195`, **VERIFIED**
  verbatim).

### 1.2 Why this is not `|F| = O(g(k))` independent of `N`

Read literally, the growth bound the module's own author states — `entries × coverable_constraints`
— **is** `N·k`, not `g(k)` alone. The mechanism is linear in `N` by explicit design, and the module
doc is candid about it; this is not a hidden cost, it is a stated one. Under the task's criterion
(no dependence on `N`), `Environment` **fails**, and the failure is by the plainest possible
reading of the compiler's own comment, not an inference this report had to construct.

There is a more important, structural reason this happens, and it generalizes to every family in
this report: an environment constraint is an **adjacency** fact ("this morph is immediately preceded
by literal `L`"), and the literal `L` can be assembled *across a morpheme boundary that the lexc
trie does not preserve as distinguishable state* — this is exactly the "miseru" bug the module doc
documents as its first failed encoding (`precision.rs:86-92`, **VERIFIED**: "The engine's left
context can be assembled across a morpheme boundary... no single emitted entry's surface is the
whole string"). Because the trie shares states across many entries with different actual preceding
contexts (that sharing is the whole reason lexc compiles small), the *only* place the adjacency
verdict can be attached without re-splitting the trie is the entry's own text — which means every
entry that could ever be adjacent to a gated position needs its own verdict. That is an `N`-sized
fact, not a `k`-sized one, and it survives being expressed as "just a flag" because the flag is
carried *by the entries*, not by a separate automaton.

### 1.3 A subtler, and arguably more damaging, point: this is not a filter at all

Independent of the size question: `precision.rs` does not build an object that could be inserted
into a `P ∘ F₁ ∘ … ∘ F_m` pipeline. There is no `F_env` — there is only a different `P`. The
`PrecisionConfig` knob (`precision.rs:608-623`) selects between two ways of *emitting the lexicon*
(`Strip` vs `AllFlags`); it is not selecting whether to compose an extra transducer afterward. This
matters for the task's own criterion, which is phrased in terms of a *staged* pipeline
(`P ∘ F₁ ∘ … ∘ F_m`) with a named lazy-composition escape hatch. `Environment`'s mechanism has no
"stage" to name at all — it is baked into stage zero. **If this is the template the other families
are meant to follow (task's own framing), the template itself does not produce filters in the sense
the criterion is testing.** This is this report's strongest single finding and the reason §7 answers
"one schema or bespoke" the way it does.

To be fair to the shipped design: the module doc is right that this is *cheap in state count*
(`O(1)` new states — flags are checked at apply time by a side-channel bitset, not compiled into new
topology, `precision.rs` module doc's `@P`/`@R` discussion, and confirmed by the design doc's own
bench numbers report `10` already measured: Sena 39,286→49,889 states, a 1.27× ratio, not a
combinatorial one, `2026-07-15-fst-precision-knob-design.md` §9 Step 5 as cited in
`10-filter-complexity-tractability.md` §3.5, **inherited, not re-verified this session**). So the
practical verdict is nuanced: expensive by the letter of "no dependence on N," cheap by the
practical "does the network blow up" test the task also asks about. Both readings are reported,
because the task's own criterion bundles them and the honest answer is that `Environment` passes one
half and fails the other.

---

## 2. Circumfix — the nesting question, settled

### 2.1 What HC's circumfix construct actually is

There is no distinct "circumfix" node type in the model (confirmed by grep: zero hits for
`Circumfix` in `pg-grammar/src/model.rs`, **VERIFIED**). A circumfix is an ordinary
`AffixProcessRuleDef`/`AffixAllomorphDef` whose RHS both leads and trails a copied span with an
`InsertSegments` — `pg-foma/src/emit.rs`'s own classifier: "leading insert AND trailing insert" ⇒
`Role::CircumfixPrefix` (`emit.rs:579-582`, **VERIFIED**). The reference/synthetic fixture
(`pg-grammar-gen/src/build/circumfix.rs:34-73`, **VERIFIED**, read in full) confirms the shape
concretely: one `MorphologicalInput` capturing the *entire* current word via `OptionalSegmentSequence
min="1" max="-1"` (i.e., an unbounded-length span, not a fixed-width window), wrapped by one
`InsertSegments` prefix and one `InsertSegments` suffix.

### 2.2 Is unbounded nesting reachable?

**No — for two independent, verified reasons**, either one of which would suffice:

1. **`MaxApplicationCount` bounds re-application of the *same* rule.** `AffixProcessRuleDef.max_apps:
   u16`, "`multipleApplication` attr; C# default `MaxApplicationCount = 1`" (`model.rs:635-636`,
   **VERIFIED**). This is enforced at the confirm-side (real-engine) analysis path:
   `StratumAnalyzer::apply_one_mrule` refuses a rule once `w.unapplied_rule_counts` reaches
   `rule.max_apps()` (`pg-rules/src/stratum.rs:798-806`, **VERIFIED**). A grammar author would have
   to explicitly raise this above 1 to get repeated wrapping by the *same* circumfix rule at all;
   the DTD default forbids it outright.
2. **The FST-side structural-composite chain is capped by a fixed constant regardless of what
   `max_apps` says.** `build_structural_composites`' own chain-length bound,
   `STRUCT_MAX_EXTRA_RULES = 3` (`emit.rs:2262-2266`, **VERIFIED**: "Bound on a structural composite
   chain's length beyond the root — same rationale as `crate::preexpand::MAX_EXTRA_RULES`"), caps
   *any* sequence of composite-eligible rules (circumfix included) at depth 3 in the compiled
   proposer, independent of the grammar's own declared `max_apps`. This is a **conservative cap, not
   a soundness guarantee** — worth stating plainly since it changes the character of the finding: if
   an author declared `max_apps > 3` intending genuine 4+-fold nesting, the emitter would silently
   under-cover (a recall gap, never a false-accept) rather than reflect the grammar's true bound.
   That is the honest, narrower claim: **the compiled proposer's own nesting depth is bounded by
   `min(max_apps, 3)` per rule chain**, not by an unenforced author declaration alone.

Additionally: even *stacking different* circumfix rules (rule A's output re-entering rule B) cannot
produce unbounded nesting, because HC's affix-template system is itself finite — FieldWorks caps at
3 hardcoded strata (`hc-surface-scope.md`'s "T1∖T2" finding, cited and independently re-verified by
report `05` §1 construct C4/C19, **inherited, not re-derived this session**) and templates are a
fixed, finite list of slots per stratum. So the *total* available "wrap" depth across every
mechanism that could apply a circumfix-shaped rule is bounded by a small, grammar-declared,
enumerable constant — never by an unbounded counter.

**Conclusion on the theoretical wall**: the classic concern — that unbounded nesting of a matched
pair is the textbook context-free case (`{aⁿ w bⁿ}`, not regular; the pumping-lemma argument report
`05` already applied to reduplication, §4, applies identically here: a language requiring a
finite-state machine to *count* an unbounded number of nested wraps and match the count on the way
out needs unbounded memory, which no automaton has) **does not arise** for any T2-reachable (i.e.,
FieldWorks-producible) PanGloss grammar. The bound is real, structural, and enforced (redundantly,
by two independent mechanisms) rather than merely assumed. This settles the question the task posed
as "the one family with a potential hard theoretical wall" — **there is no wall here**, for the
input space PanGloss actually targets.

One caveat, stated as **INFERRED, not verified against a hostile fixture this session**: this
analysis covers *stacked/nested* circumfixes (properly bracketing, `a₁a₂ STEM b₂b₁`). It does not
rule out — and this session did not check for — genuinely *crossing* (interleaved) dependencies
(`a₁a₂ STEM b₁b₂`, a mildly context-sensitive shape), because HC's circumfix construct always wraps
the *entire* captured span as one contiguous operation; there is no HC construct this session found
that could produce a crossing dependency between two circumfixes in the first place, so the
question does not arise, but this is an absence-of-evidence claim, not a proof of absence.

### 2.3 Is the *construction* categorically simpler? No — as shipped, it is the opposite

`is_structural_rule` admits `Role::CircumfixPrefix` **unconditionally**, not gated by any
cheapness probe (`emit.rs:2328-2362`, specifically the comment at `emit.rs:5600-5604`: "admits
`Role::CircumfixPrefix` UNCONDITIONALLY (not gated by `probe_would_refuse` at all)", **VERIFIED**).
Every circumfix rule is routed to `build_structural_composites` (`emit.rs:2794`, **VERIFIED**),
which is explicitly named, in the emitter's own doc, as sharing its cost profile with
`crate::preexpand::build_composites`: "this is the mechanism whose `O(roots × rules^depth)` eager
Rust-side enumeration is exactly what OOMs on Aweti (855 roots × 135 mrules...)" (`emit.rs:3969-3972`,
**VERIFIED** verbatim). Concretely: Circumfix's compiled artifact is built by **replaying the real
HC engine** (`pg_rules::morph::synthesize`) once per (root, rule-chain) combination up to depth 3,
and recording each result as a literal lexc entry — the same enumeration-bridge technique report
`10` measured crashing Aweti's build at 2,833,559 fusion entries / 691MB source / an 8.8GB
allocation failure (`10-filter-complexity-tractability.md` §3.3, **inherited from that report's own
direct measurement**).

This is `Θ(N)`-or-worse by construction, not `O(g(k))` — the opposite of categorically simpler. It
is also, per `capability.rs`, capped at `Disposition::ConfigPredicate` resting at `ConfirmOnly`: the
`CircumfixStructuralCompositePredicate`'s own doc states the construction is "a proven faithful,
oracle-backed compile for the SUPPORTED case" but explicitly stops short of `Admit`
(`capability.rs:2609-2643`, **VERIFIED**) — meaning even after paying the `N`-scaling cost, `confirm`
is still required; there is no compensating precision win.

### 2.4 Would a genuinely small construction be possible?

**Plausibly yes, but unbuilt and unverified — OPEN, novel-and-unverified in this codebase.** The
classical finite-state-morphology answer to circumfixation is *not* to enumerate per root at all: two
independent boundary-triggered insertions (one at the domain's left edge, one at its right edge),
composed sequentially, each an ordinary bounded-context rewrite rule of Kaplan & Kay's C1 shape
(regular by the same license report `05` cites for every ordinary rewrite rule,
`05-hc-to-fst-expressibility.md` §2(a), row C1, **inherited**). This is the literature this task
asked for by name:

- **Beesley, K. R. (1998), "Constraining Separated Morphotactic Dependencies in Finite-State
  Grammars," Proceedings of FSMNLP'98 (International Workshop on Finite-State Methods in Natural
  Language Processing), Bilkent University, Ankara, Turkey.** **FOUND** (web search this session,
  title and venue confirmed independently; full text was not fetched, so the paper's own worked
  construction is not independently re-derived here — the citation is verified to exist and to be
  on-topic, not that this report re-read its content).
- Beesley & Karttunen's *Finite State Morphology* (CSLI, 2003) is the standard reference for the
  general xfst replace-rule idiom this would use, but this session did not locate the book's specific
  circumfix worked example (its content is not freely web-accessible; **not found** beyond the
  book's existence and scope, which report `05` already cites for the general `replace` calculus,
  `05-hc-to-fst-expressibility.md` §2).

The construction such a rule would need, sketched (this sketch is this report's own —
**novel-and-unverified**, not taken from a fetched primary source): mint one pair of boundary marker
symbols per circumfix rule (`⟨CIRC_i_START⟩`/`⟨CIRC_i_END⟩`, minted at the two ends of the rule's
own captured span, the way an ordinary affix rule already marks morpheme identity via `<M:nnnn>`),
then compile two independent one-sided insertions — "insert `PREFIX_i` immediately after
`⟨CIRC_i_START⟩`" and "insert `SUFFIX_i` immediately before `⟨CIRC_i_END⟩`" — as two `.o.`-composed
rewrite rules, each `O(|PREFIX_i|+|SUFFIX_i|)` states, entirely independent of root length or root
count. This is a `set`-flag/insert-then-read idiom of exactly the kind report `07` proved safe in
this vendored `foma-rs` (insert a flag/marker, read it later, **never** match a flag inside a `->`
rule's own `||` context — `00-synthesis-and-decision.md` §6a "Blocker 5 resolved," **inherited,
VERIFIED by that report**), so the toolkit-defect concern `gate.rs` raised for a *different*
construct (MPR gating, which needed exactly the broken match-in-context shape, `gate.rs:8-53`,
**VERIFIED** this session) does not obviously apply here. But: this construction has not been built,
has not been checked against alpha-variable-agreeing prefixes/suffixes (a circumfix whose affix
material varies by root feature, which this session did not find evidence either way for in any
reference grammar), and has not been checked for interaction with the rest of the cascade (does a
later phonological rule need to see the marker before it's consumed — the same feeding/bleeding
question report `05` §5 already flags as the one place composition needs care, **inherited**). Marked
**OPEN**, not **PROVEN SIMPLER**, because "the literature has a construction" is not the same claim
as "this repo has verified it works here."

---

## 3. StemName — a lexical partition, with one part already precedented and one part not

### 3.1 What StemName actually gates (two distinct mechanisms, confirmed from source)

`RootAllomorphDef.stem_name: Option<StemNameId>` (`model.rs:796-798`, **VERIFIED**) is a property of
a *root allomorph*, fixed at grammar-load time — this is exactly the "lexical partition, not a
string constraint" shape the task's brief hypothesized. Two genuinely different gates read it:

**(a) Rule-level, backward-looking: `AffixProcessRuleDef.required_stem_name`.** An affix rule may
require the *already-chosen* root allomorph to carry one specific declared stem name
(`model.rs:644-648`, **VERIFIED**). Confirm-side enforcement, read directly:
`rule.required_stem_name.is_some() && rule.required_stem_name != root_stem_name(g, word)` ⇒ the
rule does not apply (`pg-rules/src/morph.rs:1631-1637`, **VERIFIED**, both in the uncached
`synth_affix` and its cached sibling `synth_affix_cached` at the equivalent gate). This reads only a
fact that is *already fixed* by the time the rule is being considered — the root's own declared
stem name — never anything about the word's eventual FS. **This is structurally identical to the
`SubruleGating` shape `gate.rs` already solved for MPR/POS** (a rule-level predicate over a
lexical-entry-level fact, known at grammar-load time, never changing mid-derivation for the reference
grammars this project has measured, `gate.rs:56-61`, **VERIFIED**).

**(b) Word-validity, forward-looking: region subsumption against the word's accumulated FS.**
`stem_name_required_match`/`stem_name_excluded_match` (`pg-rules/src/validity.rs:193-226`,
**VERIFIED**, read in full) check whether the *word's final, fully-assembled* syntactic feature
structure is subsumed by one of the stem name's declared `regions` (required) and is **not**
subsumed by any non-shared region of a sibling allomorph's stem name (excluded — the set-difference
construction at `validity.rs:211-226` is a direct, careful port of `StemName.IsExcludedMatch`,
StemName.cs:36-44). This runs both on the primary allomorph and, per W3.2, on passed-over
disjunctive candidates (`validity.rs:228-272`, called from the main gate loop at `validity.rs:605-
610`, **VERIFIED**). This gate genuinely depends on the word's *eventual* feature structure — a
forward dependency, the same shape report `10` already named for `RealizationalMorphology` ("depend
on the word's accumulated FS, not anything the FST proposer can see at a single transition,"
`capability.rs`'s own doc quoted at `10-filter-complexity-tractability.md` row A9, **inherited**).

Both gates are faithfully ported and enforced by the real engine today (confirm-side soundness is
not at risk); neither has any FST-side representation (`StemName` is declared, never populated, in
`ConstraintFamily`, `precision.rs:246`, **VERIFIED**, and — a further, sharper finding — `StemName`
has **no `CharacteristicKind` variant at all** in `capability.rs`'s 20-variant enum, confirmed by
direct grep returning zero hits and by reading `CharacteristicKind::ALL`'s full 20-entry list,
`capability.rs:104-204`, **VERIFIED**). This is a gap one level more basic than "unbuilt filter": the
capability ledger that is supposed to track every FST-relevant construct and assign it at least
`ConfirmOnly` does not know `StemName` exists as a characterizable construct. Confirm-side soundness
survives regardless (nothing routes through the FST that the ledger would need to gate), but this is
worth naming as a governance/tracking gap distinct from precision.rs's own honestly-declared scope
cut.

### 3.2 Verdict, part (a): PROVEN SIMPLER

Gate (a) is a direct application of `gate.rs`'s own already-shipped, already-proven template: build
the partition key by calling the real oracle's own predicate (`root_stem_name`, the same function
`synth_affix` itself calls — not a re-derivation), group lexical entries by that key
(`partition_entries`, `gate.rs:72-78`, **VERIFIED** as the mechanism, applied here by direct
analogy, not independently re-implemented this session), compile one network per group, union the
disjoint groups. **Crucially: this is a true partition, not a cross-product.** Every entry lands in
exactly one group; no entry is ever duplicated across groups (`gate.rs:79-93`'s own soundness
argument for why the union is safe applies unchanged: "each group's ENTIRE network... only accepts
underlying strings built from THAT group's own entries — groups are lexically disjoint by
construction," **VERIFIED**, this reasoning is generic to any static-fact partition, not specific to
MPR). **Cost: zero duplication, hence no multiplication of `N`.** The task's own hypothesized
"conservation law" risk (a partitioned lexicon may duplicate entries) does **not** apply to this
construction, because partitioning is disjoint grouping, not enumeration over combinations — the
total entry count stays exactly `N`, just regrouped. Runtime filter size: `|F| = 0` (the gating is
resolved into *which network a root's entry lives in*, at compile time, not into any runtime
automaton at all) — this is a **stronger** result than "categorically simpler," exactly as the task's
brief anticipated. This has not been built for `StemName` specifically (no code in this session's
search implements it), but the missing work is mechanical: thread `required_stem_name`/root-`stem_name`
pairs through `gate.rs`'s existing `find_gated_subrules`/`entry_gate_key`/`partition_entries`
machinery as one more static per-entry key dimension.

### 3.3 Verdict, part (b): OPEN

Gate (b)'s forward dependency looks, at first, like the unbounded-window problem report `10` already
declared out of reach for `RealizationalMorphology`/`CoOccurrenceConstraint` (row A8/A9). But there
is a structural escape specific to how PanGloss's proposer is built: because the compiled FST is a
**generator** — it is built by enumerating every legal derivation *path* (root → affix chain → final
tag string) as a literal lexc concatenation, not by running forward from a root with an unknown
future — the final accumulated FS for any *given, already-fixed* path is a property of that whole
path, computable by composing each rule's own `out_syn_fs` along the chain. This is exactly the same
computation shape as `derivable_to_category`'s existing bounded-BFS reachability check ("can some
chain reach this category," `emit.rs:57-61`, cited in `10-filter-complexity-tractability.md` row C3,
**inherited**, not independently re-read this session at that specific site) — a compile-time,
off-tape, rule-graph-only computation, whose cost is bounded by the *rule graph's* size (strata ×
templates × slots), not by root count `N`.

If built this way — partition roots-with-a-declared-stem-name by which region-reachability class
their possible continuations land in, using the same disjoint-partition-then-union technique as
§3.2 — the result would plausibly be `|F| = O(distinct regions × rule-graph size)`, independent of
`N`. This is **not proven**: it has not been built, and this session did not verify that the
reachable-region computation stays a true partition rather than degrading into a product (a root
reachable to *multiple* distinct regions via different affix choices would need to appear in
multiple groups' source data unless the emitter is willing to prune — refuse to emit — continuations
that provably cannot satisfy the required region, which is a recall-*safe* move only if the pruning
predicate is proven sound, an unbuilt proof). Marked **OPEN** — missing piece: extend the
`derivable_to_category`-style reachability BFS to region compatibility, and prove the partition stays
disjoint (or characterize the multiplicity if it does not).

---

## 4. FreeFluctuation — a genuine constraint, not a multiplicity artifact, and it depends on Environment's own declined pieces

### 4.1 What it actually does (read from the confirm-side code, not inferred)

`pg-rules/src/validity.rs`'s module doc names this precisely: "W3.2... the disjunctive-allomorph /
free-fluctuation re-check (`Allomorph.IsWordValid`'s second loop, Allomorph.cs:127-152)... REJECTS
the word if it does not free-fluctuate with the used allomorph" (`validity.rs:30-43`, **VERIFIED**).
The mechanism, read directly: for a morph occurrence that used allomorph index `J`, walk every
earlier-priority candidate `I` (the recorded passed-over set, or every earlier index if nothing was
recorded, `disjunctive_candidates`, `validity.rs:283-291`); if `I` does **not** free-fluctuate with
`J` (`free_fluctuates`, the adjacent-pair `ConstraintsEqual` walk, `validity.rs:274-281`) **and** `I`'s
own environment/constraints would also have held at this exact span, the word is rejected
(`validity.rs:640-656` for roots, `698-718` for affixes). The mirror-image logic on the synthesis
(generation) side explains *why* this matters for a proposer: `synth_affix` stops after the first
matching allomorph **unless** it free-fluctuates with the next one (`morph.rs:1687-1700`,
**VERIFIED**: "Disjunctive-allomorph break... stop after the first match unless... it free-fluctuates
with the next allomorph, in which case C# keeps going").

### 4.2 Constraint or multiplicity? — Both, and they pull in opposite directions

The task asked this report to check whether FreeFluctuation is a genuine well-formedness constraint
or a "how many analyses come back" concern that a filter cannot fix. The honest answer, from the
code: **it is both, and the two halves have opposite implications for filter design.**

- **The blocking half is a genuine precision constraint.** If a naive FST proposer emits *every*
  allomorph's entry unconditionally (which is exactly what an over-generating proposer does by
  design), it will offer, as a candidate, `root + affix_J` in a context where a strictly
  higher-priority, non-fluctuating `affix_I` was *also* locally applicable — a combination the real
  engine's synthesis path would never produce (it stops at `I`, per §4.1) and its analysis path
  explicitly rejects. This is not a "which of several equally-valid parses do we report" question;
  it is "is this specific (word, analysis) pair one the grammar actually generates," which is
  precisely the well-formedness question the task's brief distinguishes from multiplicity. **A
  filter is the right tool for this half**, in principle.
- **The "keep going" half is a genuine multiplicity fact, and it cuts against building an aggressive
  filter.** When two allomorphs genuinely free-fluctuate, *both* outputs are legitimate — the real
  engine deliberately produces both (`morph.rs:1691-1700`). Since the already-shipped, over-generating
  proposer already emits both allomorphs' entries unconditionally with no special-casing, this half
  needs **no filter at all** — the risk runs the other way: a filter built to catch the blocking half
  could, if its predicate is wrong, **wrongly reject one of two legitimately co-valid parses**,
  which is a recall violation the architecture's own invariant (`precision.rs:11-15`'s "Recall must
  stay 100% at every setting") forbids. The task's own instruction — "filters remove, they do not
  merge or count" — is exactly right for this half, but the conclusion is not "no mechanism is
  needed," it is "no mechanism is needed *because the proposer already over-generates correctly
  here*."

**Net: FreeFluctuation is not a case to decline as "not really a filter question."** The blocking
half is real and matters for precision; it is simply *hard*, for a specific, identifiable reason —
§4.3.

### 4.3 Why a filter for the blocking half is OPEN, not built

Testing "does allomorph `I`'s own environment also hold at the position where `J` was used" requires
exactly the machinery `precision.rs` itself declined to build for the `Environment` family: an
**exclude**-style check on a candidate that is *not* the one occupying this position
(`precision.rs`'s own finding 4, "`ExcludedEnvironments`... left out of THIS step's scope,"
`precision.rs:64-74`, **VERIFIED**), plus, in general, right-context and word-edge-anchor
environments (findings 2–3, `precision.rs:42-63`, **VERIFIED**), since a competing allomorph's own
declared environment is not restricted to the left-literal shape `Environment`'s `AllFlags` preset
covers. FreeFluctuation's filter is therefore **strictly harder than, and blocked on, a superset of**
exactly the cases `Environment`'s own module declined — not an independent piece of new work, but a
dependent one. There is a further wrinkle beyond what `Environment` needed: the check is
cross-allomorph (evaluate candidate `I`'s environment at the position where `J`'s entry sits), which
`Environment`'s existing per-owner `@R@`/per-position `@P@` scheme was never designed to do (it only
ever asks "did *this* allomorph's own environment hold," never "would a *different*, unused
allomorph's environment also have held here").

**Verdict: OPEN.** The missing piece is concrete and nameable — build the excluded/right-context
environment encoding `precision.rs` explicitly scoped out, then extend it to a cross-allomorph
"would a higher-priority alternative also have matched here" test, keyed on the grammar's own
static, document-order priority list (`Vec<AffixAllomorphDef>`/`Vec<RootAllomorphDef>` order, already
available with no new computation, `model.rs:669,784`) — but it is not a small extension, since it
inherits every hazard `Environment`'s own three failed-encoding history (`precision.rs:81-119`)
already surfaced once, on a strictly easier version of this problem.

---

## 5. The cross-family question: one parameterized schema, or bespoke per family?

The owner's stated worry is that whack-a-mole will not converge. This session's evidence gives a
specific, falsifiable answer: **not "one schema," and not "bespoke per family" either — four
distinct, reusable *construction strategies*, of which one is already known to fail at scale.** None
of the four is "compose a small filter after an unmodified proposer" (the shape the task's own
criterion is phrased in), which is the point made prominently in §0/§1: that shape has no working
instance in this codebase.

1. **Disjoint lexical partition, then union** (`gate.rs`'s MPR/POS mechanism; §3.2's `StemName`
   rule-level gate is a direct instance of the same schema). Cost: `O(1)` runtime, zero entry
   duplication, partition-count bounded by the number of *distinct gating vectors the grammar's own
   entries realize* (`gate.rs:72-78`, **VERIFIED**), not by `N`. **This is the schema that actually
   satisfies the categorical-simplicity criterion**, and it generalizes to any gate whose predicate
   is a static, load-time fact about a lexical entry that never changes before the gated site is
   reached.
2. **Per-entry inline tag/flag injection, read at apply time via `KeepFlag`** (`Environment`'s
   `AllFlags` preset, §1). Cost: `O(N·k)` label growth, `O(1)` new states — cheap in the metric the
   task's own "staged application must not blow up" bullet cares about, expensive in the metric its
   "no dependence on N" bullet cares about. Generalizes to any *adjacency* fact that must vary per
   entry because the trie shares state across entries with genuinely different local contexts.
3. **Build-time enumeration replaying the real oracle** (`Circumfix`'s `build_structural_composites`,
   §2.3; the same mechanism as the general enumeration bridge, `preexpand.rs`, report `10`'s C6).
   Cost: `O(roots × rules^depth)`, proven to OOM on Aweti. **This is the schema this project should
   stop reaching for**, not because it is wrong (it is oracle-exact for the supported case,
   `capability.rs:2609-2643`) but because it is the one schema whose own cost model is `N`-driven by
   construction, with no quotienting trick available (unlike alpha-variable enumeration's
   feature-quotient fix, report `05` §3, **inherited**) because the enumerated dimension *is* the
   root set itself, not a bounded feature alphabet.
4. **Off-tape compile-time reachability, over rule metadata only** (`derivable_to_category`'s BFS,
   `compounding_max_depth`'s reachability pass, and MPR `Overwrite`'s Construction 2, all cited in
   report `10` rows C3/A6, **inherited**; §3.3's proposed `StemName` region-gate extension). Cost:
   bounded by rule-graph size, never touches the tape or the entry count at all. This is the schema
   §3.3 recommends for `StemName`'s harder half, and it is the only one of the four that is *already
   precedented three times over* in this exact codebase for unrelated constructs — the strongest
   evidence that it generalizes cheaply.

**Answer to the owner's question:** the risk of whack-a-mole is real for schema 3 specifically (every
new construct routed through build-time enumeration adds another `N`-scaling term with no shared
infrastructure to amortize), but not for schemas 1, 2, or 4 — each of those is a single mechanism
already instantiated more than once, and extending it to a new family (as §3.2 and §3.3 do for
`StemName`) is describable as "apply the same template again," not "invent a new one." The
convergence question therefore reduces to a **classification** problem, not a construction problem:
for each of the ten unbuilt `ConstraintFamily` variants, determine which of schemas 1/2/4 its
predicate shape matches (static entry-level fact → schema 1; adjacency-with-boundary-crossing →
schema 2; whole-derivation reachability → schema 4) and decline schema 3 unless nothing else applies.
This report classified three: `StemName` (a) → schema 1; `StemName` (b) → schema 4 (proposed,
unbuilt); `FreeFluctuation` → schema 2, extended (proposed, unbuilt, blocked on `Environment`'s own
declined cases); `Circumfix`, as shipped, is schema 3, and a schema-2-like alternative (§2.4) is
sketched but unverified.

---

## 6. Determinizability and staged-composition notes (the criterion's other two bullets)

- **Schema 1 (partition-then-union)**: determinization cost is that of the *individual group
  networks*, each strictly smaller than the unpartitioned whole (a subset of entries); the union of
  disjoint-language automata is linear in the sum of group sizes, not their product — `gate.rs`'s own
  argument for why this union is safe (`gate.rs:86-93`, **VERIFIED**) is precisely the "safe under a
  named strategy" the task's criterion asks for: the strategy is "union only ever combines
  provably-disjoint-input automata," which report `10`'s union-vs-compose incident (§Part 3.1,
  **inherited**) shows is exactly the property that failed for the *wrong* case (unioning several
  *non*-disjoint-input replace transducers, a 10,324× state blowup) — schema 1 is safe specifically
  because it does not repeat that mistake.
- **Schema 2 (inline flags)**: no separate determinization step — flags are interpreted by
  `apply_up`'s side channel, not compiled into new states, **provided** the flag is only ever
  inserted-then-read, never matched inside a `->` rule's own context (report `07`'s finding,
  **inherited**, `00-synthesis-and-decision.md` §6a). `Environment`'s `AllFlags` preset already
  respects this; any extension to `FreeFluctuation` (§4.3) must too.
- **Schema 3 (enumeration)**: no meaningful "determinize the filter" step exists because there is no
  filter — the entire cost is upfront construction, and it is exactly the cost that is `N`-driven;
  this is why §5 recommends against reaching for it again.
- **Schema 4 (off-tape reachability)**: not an FST object at all, so "determinizable at polynomial
  blowup" does not apply; the cost is a graph algorithm over rule metadata (BFS-shaped, per
  `compounding_max_depth`'s own precedent, `10-filter-complexity-tractability.md` row A6,
  **inherited**), polynomial in rule-graph size by construction.
- **Staged composition (`P ∘ F₁ ∘ … ∘ F_m`) generally**: nothing in this report's three families
  produces a genuine `F` to stage, except the §2.4/§4.3 *proposed, unbuilt* constructions — for
  those, the report explicitly flags (§2.4, §4.3) that feeding/bleeding interaction with the rest of
  the phonological cascade has not been checked, which is exactly the class of risk report `10`'s
  Part 2 "interacting" list already warns is where "always manageable" stops holding
  (**inherited**).

---

## 7. Tape information required, not emitted today

Concrete, per family:

- **`Circumfix`** (for the §2.4 alternative construction, if built): a per-rule **domain-boundary
  marker pair** — a symbol marking the start and end of the circumfix's own captured span, distinct
  from morpheme-identity tags. `<R:nnnn>`/`<M:nnnn>` (`pg-foma/src/tags.rs:2`, cited in
  `00-synthesis-and-decision.md` §3 Q2, **inherited**) mark *which morpheme*, never *where a
  particular rule's own span begins/ends independent of morpheme boundaries*. Not emitted today.
- **`StemName`, region gate**: if resolved via schema 4 (§3.3, recommended) needs **nothing new on
  the tape at all** — the computation is off-tape, over rule metadata, at compile time. If instead
  attempted via a runtime flag (not recommended, but stated for completeness), it would need one
  `@P.REGION{r}@`-style flag set by *every* affix rule reflecting its own `out_syn_fs` contribution —
  not emitted today, and would reproduce `Environment`'s own `O(N·k)` cost profile for no benefit
  over schema 4.
- **`FreeFluctuation`**: a **cross-allomorph** environment-holds fact — "would allomorph `I`'s
  (unused) environment also have matched at this position" — which is a fundamentally different
  question from anything `Environment`'s existing `@R@`/`@P@` scheme answers (that scheme only ever
  reports on the *used* allomorph's own environment). Not emitted today; building it also requires
  the excluded/right-context environment pieces §4.3 already names as missing.
- **General, cross-cutting** (repeating report `10`'s own finding, **inherited**, because this
  session's three families confirm it applies beyond the constructs that report already covered):
  no construct in this report needs *derivation-history* information (which prior rules fired, in
  what order) except `StemName`'s region gate, and that one is better solved off-tape (schema 4) than
  by trying to put accumulated-FS history onto the tape at all.

---

## 8. Literature — what was checked this session, what is inherited, what is not found

- **Kaplan & Kay (1994)**, ACL Anthology `J94-3001` — the regularity/composition license for every
  ordinary rewrite rule and for the two-sided-insertion circumfix sketch in §2.4. **Inherited from
  report `05`** (`05-hc-to-fst-expressibility.md` §2(a), §5), not independently re-fetched this
  session.
- **Karttunen on lexical transducers / `replace`** — Karttunen (1994) "Constructing Lexical
  Transducers," COLING-94 (ACL Anthology `C94-1066`); Karttunen, Kaplan & Zaenen (1992), COLING-92.
  **Inherited from report `10`** (`10-filter-complexity-tractability.md` §4.2), not re-fetched.
- **Beesley & Karttunen, *Finite State Morphology*** (CSLI, 2003) — confirmed to exist and be
  on-topic via web search this session (title, publisher, ISBN all consistent across multiple
  independent listings). Its specific circumfix worked example (if any) was **not found** —
  the book's full text is not freely accessible, and this report does not claim to have read it.
- **Beesley (1998), "Constraining Separated Morphotactic Dependencies in Finite-State Grammars,"
  FSMNLP'98, Bilkent University, Ankara, Turkey.** **FOUND** this session via web search (title
  confirmed against the exact phrase the task named, "separated dependencies"); full text not
  fetched, so its own construction is not independently reproduced here — §2.4's sketch is this
  report's own, marked **novel-and-unverified**, not attributed to Beesley's actual content.
- **Regular-vs-context-free boundary for matched pairs** — the `{aⁿwbⁿ}`-is-not-regular argument used
  in §2.2 is the standard pumping-lemma result (Hopcroft & Ullman 1979), the same citation report
  `05` uses for unbounded-copy reduplication (`05-hc-to-fst-expressibility.md` §4, **inherited**);
  this report applies the identical argument to unbounded circumfix nesting rather than re-deriving
  it from a fresh source.
- **State-complexity of intersection** — Yu, Zhuang & Salomaa (1994), *Theoretical Computer Science*
  125(2), 315–328, and Koskenniemi & Silfverberg's morphology-specific restatement (SIGMORPHON 2010,
  ACL Anthology `W10-2205`). **Inherited from report `10`** §4.1, not re-fetched; used in §6 only by
  reference to that report's own already-verified numbers (Karttunen's Finnish-numeral
  1,946→20,498-state example), not re-derived.
- A dedicated search this session confirmed no additional, more specific paper on **filter
  construction for suppletive-stem/region selection** (`StemName`'s exact shape) or on
  **elsewhere-condition/disjunctive-allomorph blocking as a finite-state filter** (`FreeFluctuation`'s
  exact shape) beyond the general two-level/replace-rule literature already inherited above — **not
  found**, stated rather than stretched from an adjacent result.

---

## 9. Summary of missing pieces (for whoever picks this up next)

1. `StemName` rule-level gate (§3.2): mechanical — extend `gate.rs`'s existing partition machinery
   with one more static key dimension. Lowest-difficulty item in this report.
2. `StemName` region gate (§3.3): extend `derivable_to_category`'s BFS-reachability technique to
   region compatibility; prove the result stays a disjoint partition (or characterize the
   multiplicity if a root is reachable to more than one region).
3. `FreeFluctuation` (§4.3): build the excluded/right-context environment encoding `precision.rs`
   itself declined for `Environment`, then extend it cross-allomorph. Blocked on item 3's
   prerequisite existing first; inherits that prerequisite's own three-failed-encodings history as a
   warning, not a green light.
4. `Circumfix` small construction (§2.4): implement and verify the two-sided boundary-marker
   insertion against `foma-rs`, check it against alpha-variable-agreeing affix material and against
   cascade feeding/bleeding. Marked novel-and-unverified; has real literature precedent (Beesley
   1998) but no verified implementation in this repo.
5. Governance gap (§3.1): `StemName` and `FreeFluctuation` have no `CharacteristicKind` entry in
   `capability.rs` at all — add them (even if their disposition starts at `ConfirmOnly`) so the
   capability ledger's own completeness check (`all_kinds_have_a_default_disposition`,
   `capability.rs:179-182`, **VERIFIED** to exist as a test) actually covers every construct this
   report and `precision.rs`'s own enum already know about.
