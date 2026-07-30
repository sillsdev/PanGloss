# Filter construction for the unification gates: `Mpr`, `HeadFeatures`, `ObligatoryFeatures`, `CompoundingFs`

Research report, agent 11. No code changed, no build run (`cargo`/`rust/tools/pg.ps1` never
invoked). Scope: the four feature-structure/unification gate families in
`pg-foma/src/precision.rs`'s `ConstraintFamily` enum that are declared but unpopulated. Claims are
marked **VERIFIED** (read directly at the cited `path:line` this session) or **INFERRED** (reasoned
from verified facts, not itself read at a citation); a construction not found in the cited
literature is marked **novel-and-unverified** rather than given a fabricated citation. Context read
first: `00-synthesis-and-decision.md` §6a, `05-hc-to-fst-expressibility.md`,
`10-filter-complexity-tractability.md` (all **VERIFIED**, read in full this session).

## 0. The one-paragraph verdict

All four families reduce to two primitive tape mechanisms already proven safe elsewhere in this
codebase — an unconditional-overwrite **run-flag** per finite-domain value (the same idiom
`precision.rs`'s shipped `ENV` family uses) and a **gate-instance flag** per constraint occurrence —
composed one of two ways depending on *when* HC itself defines the check to fire: **locally**
(against whatever has already been read, the `ENV` family's own shape) for `Mpr`, or **deferred to a
shared word-final tail** (because the real check target is the word's *final* accumulated feature
structure, not its value at the gating position) for `HeadFeatures`, `ObligatoryFeatures`, and the
feature-structure half of `CompoundingFs`. Because HC's own `is_unifiable`/`unify`/`subsumes` are
defined as **per-feature, non-interacting recursive walks** (`pg-featstruct/src/ops.rs`, verified
below), every one of these constructions decomposes into `n·k` states/flags across independent
feature dimensions rather than the `kⁿ` joint product a naive compiler would reach for — this is the
report's central, load-bearing finding, and it holds because of a specific verified property of the
grammar's own unification semantics, not by assumption. The MPR bit-vector side of the picture
already has this fully proven and shipped in miniature (report `10`'s Construction 1/2); the
syntactic-feature-structure side does not have an equivalent shipped proof, and this report says so
explicitly rather than papering over it.

---

## 1. Verdict table

| Family | Verdict | Bound (states/flags) | N-dependence | Determinizable? |
|---|---|---|---|---|
| **`Mpr`** (static, root-declared) | **PROVEN SIMPLER** | O(1) marginal — folded into the existing lexical partition, zero new automaton states | None (already shipped, `gate.rs`) | Trivial — disjoint-language union, no subset construction needed |
| **`Mpr`** (dynamic, `out_mpr`-propagated, Append or reachability-provable Overwrite) | **PROVEN SIMPLER** | O(k) flag symbols, k = MPR group size (≤6 measured, ≤64 hard cap) | None | Standard subset construction, same order as `precision.rs`'s own measured ENV numbers (§4.5) |
| **`Mpr`** (dynamic, Overwrite, reachability *not* provable) | **PROVEN NOT SIMPLER** (for this residual case only) | O(4^k) states, k = group size — but *threaded through the rest of the derivation from first touch*, not a self-contained add-on | Couples to P's own downstream state count in the affected region | Same order as the existing Construction 3, already characterized as non-characterizer-only in report `10` |
| **`HeadFeatures`** | **PROVEN SIMPLER**, conditional on an unbuilt reachability check (named explicitly, §5) | O(F·D) run-flags + O(k) gate flags + one shared end-of-word tail; F = syntactic feature count, D = max declared domain size (≤37 measured, ≤64 hard SymbolBits width), k = number of `AffixAllomorphDef.required_syn_fs` instances | None in the construction itself; the *unbuilt* reachability check is the open item | Standard subset construction over a disjoint-by-flag-name union at the tail; no blowup beyond the baseline the flags themselves cost |
| **`ObligatoryFeatures`** | **PROVEN SIMPLER**, same conditional as `HeadFeatures` | O(F) run-flags (presence only, not per-value) + O(k) gate flags, k = number of `obligatory_features` declarations | None, same caveat | Same as `HeadFeatures` |
| **`CompoundingFs`** | **PROVEN SIMPLER**, same conditional, plus reuses the `Mpr` schema for its MPR sub-gates | O(F·D) run-flags (shared alphabet with `HeadFeatures`) + O(k) gate flags + O(k') MPR flags, read from **two** tape regions (head, non-head) at one join transition | None in the construction; same open reachability item, now needed at the join point specifically | Same order; the two-region join needs no new machinery beyond reading both sides' already-shipped flags |

No family in this set is **OPEN** outright — but `HeadFeatures`/`ObligatoryFeatures`/`CompoundingFs`
carry a named, specific, unbuilt precondition (§5) that a full "PROVEN SIMPLER" ought not to leave
implicit, so it is stated here rather than folded silently into the verdict word.

---

## 2. The foundational fact: why `n·k`, not `kⁿ`

**VERIFIED**, `pg-featstruct/src/ops.rs:1-91,103-...`: `is_unifiable`, `unify`, and `subsumes` are
each a **merge-walk over the two operands' sorted `(FeatId, FeatureValue)` entry lists**
(`ops.rs:106-120` for `is_unifiable`) — a feature present on only one side is copied through
unmodified; a feature present on both sides is recursed into *independently*; the whole operation
never inspects one feature's value to decide what another feature's value must be. `subsumes(a, b)`
(`ops.rs:50-60`, module doc) walks only `a`'s own features and requires each, independently, to be
present and subsumed in `b`. `priority_union` (`ops.rs:61-75`) is the same shape: `b`'s value simply
overwrites `a`'s per-feature, with recursion only inside a shared nested `Complex` value.

This means: **whether a required feature structure is satisfied decomposes into an AND of
independent per-feature tests.** A construction that tracks "what is the word's current value for
feature `f`" *separately per feature* — rather than tracking "what is the word's current *whole*
feature structure" as one joint state — never loses information, because no feature's satisfiability
test ever needs another feature's value. This is exactly the distinction the brief asks for: **check
dimensions independently and intersect (`n·k`), don't track the joint state (`kⁿ`).** The literature
match is Koskenniemi & Silfverberg's own statement of the analogous fact for two-level rule
intersection — *"the size of the intersection would have many states, roughly proportional to the
product of the numbers of the states in the individual rule transducers"* (Koskenniemi, K., &
Silfverberg, M. (2010), "A Method for Compiling Two-level Rules with Multiple Contexts," SIGMORPHON
2010, ACL Anthology **W10-2205**, p. 42 — **VERIFIED**, already read and cited in report `10` §4.1)
— stated there as the failure mode when rules are *not* decomposed; the same paper's own proposed
fix (Generalized Restriction, splitting into single-context rules) is the two-level-morphology
analogue of "track per-dimension, don't join." Yu, Zhuang & Salomaa (1994, *Theoretical Computer
Science* 125(2), 315–328 — **VERIFIED** cited in report `10` §4.1) supplies the worst-case-tight `mn`
bound for *intersecting two already-built* automata, which is exactly the cost this report's
constructions are built to avoid triggering by never materializing the joint automaton in the first
place — each feature dimension gets its own small tracker, composed (not unioned; report `10` §3.1's
union-vs-compose incident, `p6-prototype-report.md` §2.2, measured 392,311→38 states from that single
combinator change, is the standing warning for why composition, not union, is the only safe way to
combine them).

**The feature-domain sizes are bounded and small, verified from source, not assumed:**
- Phonological/syntactic symbolic feature values are packed into a `SymbolBits(u64)`
  (`pg-featstruct/src/bitvec.rs:50`, **VERIFIED**) — a hard ceiling of 64 values per feature.
  Measured maxima: Sena's widest feature has 20 symbols (`bitvec.rs:246`, **VERIFIED** test
  comment); the syntactic system's own doc states "POS max across the reference grammars is 37
  symbols, well under 64" (`pg-featstruct/src/tree.rs:36`, **VERIFIED**).
- MPR features are packed into an `MprSet(u64)` (`pg-grammar/src/model.rs:125`, **VERIFIED**) with
  an explicit doc comment: "max 6 MPR features across the reference grammars; the loader lints >64"
  (`model.rs:107-108`, **VERIFIED**) — i.e. a hard structural cap of 64, empirically 6.
- The syntactic feature *count* itself (`FeatId(u16)`, `pg-featstruct/src/tree.rs:30`) has **no
  equivalent hard cap found this session** — unlike `MprId`'s explicit ">64" lint, no lint on total
  declared syntactic feature count was located in `pg-grammar/src/load.rs`'s `build_syn_features`.
  **INFERRED, not a verified hard bound**: in practice this is small (a handful of agreement
  features plus POS, per the reference grammars), but no structural ceiling like `MprSet`'s exists to
  cite. Flagged as an open measurement, not a risk to the argument (the construction's cost scales
  with whatever F actually is, declared and finite by HC's own closed-feature-system design,
  `hc-grammar-map.md:19`, cited **VERIFIED** in report `05` §3).

---

## 3. `Mpr` — MPR feature gating on an allomorph

### 3.1 What it gates (read, not theorized)

Two distinct sites carry `required_mpr`/`excluded_mpr`/`out_mpr`, and they are **not the same
mechanism as `gate.rs`'s already-shipped, `Proven` `SubruleGating`** — this distinction is
load-bearing and undocumented anywhere in the four `ConstraintFamily` files read together, so it is
stated explicitly here:

- **`RewriteSubruleDef.required_mpr`/`excluded_mpr`** (`pg-grammar/src/model.rs:426-427`,
  **VERIFIED**) gates *phonological* subrule applicability against a **root's own static, declared**
  MPR facts. This is what `gate.rs`'s static partition covers
  (`capability.rs:133-135`: "drives `gate.rs`'s partition, already Proven by that mechanism").
  `gate.rs`'s own module doc states its scope explicitly: root-only, computed once at load time
  because "the ONLY place `pg_rules::rewrite::subrule_applicable` is ever consulted" is the trailing
  phonological cascade, which runs after the root's own MPR facts are fixed and *before* any affix
  rule could dynamically change them (`gate.rs:56-61`, **VERIFIED**).
- **`AffixAllomorphDef.required_mpr`/`excluded_mpr`/`out_mpr`** (`model.rs:680-682`, **VERIFIED**)
  and **`CompoundingSubruleDef.required_mpr`/`excluded_mpr`/`out_mpr`** (`model.rs:732-734`,
  **VERIFIED**) gate whether an *affix allomorph* (or compounding subrule) may attach, checked
  against the **word's currently accumulated `MprSet`** — `word.mpr: MprSet`
  (`pg-rules/src/word.rs:17-19`, **VERIFIED** — "the brief's own semantics list requires MPR gating
  ... which read/write the word's MPR set"). The read side: `g.mpr_group_ok(allo.required_mpr,
  allo.excluded_mpr, word.mpr)` (`pg-rules/src/morph.rs:1658`, **VERIFIED**, inside `synth_affix`).
  The write side: `w.mpr = g.mpr_add_output(word.mpr, sr.out_mpr)` and, for compounding, a second,
  chained call adding `rule.output_prod_restrictions_mpr` too (`morph.rs:3196-3199`, **VERIFIED**).

`gate.rs` itself names the gap: *"`AffixAllomorphDef::out_mpr` (an affix rule DYNAMICALLY adding an
MPR feature ... ) is not threaded into the partition key ... A grammar whose recall genuinely depends
on affix-time MPR propagation into a gated prule is a real, uncovered gap"* (`gate.rs:101-113`,
**VERIFIED**). `precision.rs`'s `ConstraintFamily::Mpr` ("MPR feature gating on an allomorph") reads
most precisely as *this* uncovered construct, not a restatement of what `SubruleGating` already
proves.

### 3.2 Group semantics, exactly as coded

`MprGroup { match_type: All|Any, output: Overwrite|Append, members: MprSet }` (`model.rs:842-854`,
**VERIFIED**); every MPR bit belongs to **at most one** group (`model.rs:866`, **VERIFIED** doc
comment). `mpr_group_buckets` (`model.rs:872-887`) partitions a test set into per-group buckets plus
one "ungrouped" bucket; `mpr_required_ok`/`mpr_excluded_ok` (`model.rs:895-925`) then AND together,
across buckets, either a subset test (`All`) or an overlap test (`Any`) — **each bucket's test reads
only that bucket's own bits**, never another group's. `mpr_add_output` (`model.rs:927-945`,
**VERIFIED**, cross-referenced with report `10` row A6/A7): `Append` unions monotonically; `Overwrite`
clears every other member of a touched group.

### 3.3 The construction

**Static case (root-declared, no `out_mpr` reaches the gate before it fires):** identical in shape to
`gate.rs`'s own shipped `Proven` mechanism (`gate.rs:56-84`, **VERIFIED**) — partition lexical entries
by the exact vector of which gated allomorphs/subrules apply (computed once, at compile time, by
calling the real oracle predicate directly, never re-derived), compile one network per partition
group, union the disjoint groups. This adds **zero marginal automaton states** beyond the baseline
lexc partition already needed for `SubruleGating`; extending it to `AffixAllomorphDef`/
`CompoundingSubruleDef` gates is a straightforward re-application of the same technique to a
different field, not a new construction. **INFERRED** (the extension itself is not built —
`gate.rs`'s own scope note above says so — but the mechanism is identical to what is already proven,
so this is a low-risk inference, not a speculative one).

**Dynamic case — the genuinely new construction.** Because the read side (`mpr_group_ok`) is checked
**at the allomorph's own rule-application point, against whatever `word.mpr` already is** —
i.e. a **left-context-only, past-state check**, exactly the shape `precision.rs`'s own `ENV` family
already proves safe (no right-context problem the way `HeadFeatures` has, §4) — the construction
reuses `precision.rs`'s own shipped idiom (`precision.rs:128-166`, **VERIFIED**) verbatim, applied to
MPR bits instead of adjacency literals:

- **Per MPR bit `b` in an `Append`-semantics group (or an ungrouped bit):** one flag pair,
  `@U.MPR{b}@` — Beesley & Karttunen's *unification*-type flag (Beesley, K.R., & Karttunen, L.
  (2003), *Finite State Morphology*, CSLI Publications, ch. 8 — **VERIFIED** as an existing citation
  this project already uses for flag-diacritic semantics, not independently re-fetched against the
  primary text this session), which is exactly "once set, stays set, never conflicts" — the correct
  semantics for a monotonic union. No joint state across bits is needed: `mpr_required_ok`/
  `mpr_excluded_ok`'s own bucket test (§3.2) is already an AND/OR over *independent* per-bit facts,
  so `k` independent flags suffice, not `2^k`.
- **Per `Overwrite`-semantics group of size `k_g`, when Construction 2's reachability predicate holds
  (proven vacuously true for 5 of 6 groups, and by an algebraic identity for the sixth, across all
  three reference grammars — `mpr-overwrite-encoding-research.md` §2-3, **VERIFIED** via report
  `10` row A6):** the group behaves as **one symbolic register with `k_g + 1` values** (each member,
  or "none currently set" — since setting any member clears every sibling, at most one is ever
  logically live). One `@P.GROUP{g}.{member}@`-style flag, unconditionally overwritten, same idiom
  as `precision.rs`'s own `@P@` adjacency mechanism. Cost: **O(k_g)**, not O(2^{k_g}) or O(4^{k_g}).
- **Per `Overwrite`-semantics group where reachability is *not* provable** (two derivation paths with
  different touch histories reconverge before the gate reads the group): this is *exactly* the
  already-characterized Construction 3 (`mpr-overwrite-encoding-research.md` §3 Construction 3,
  **VERIFIED** via report `10` row A7) — an `(asserted, denied)` dual-rail pair *per bit*, needed
  because a merged automaton state must safely over-admit either history, costing **O(4^{k_g})
  states threaded through the rest of the derivation from the first touch onward** — report `10`'s
  own words, "not characterizer-only... a genuine new construction." This is the one sub-case of
  `Mpr` that earns **PROVEN NOT SIMPLER**: the `4^{k_g}` bound is itself independent of N, but it
  does not sit beside P as an inert add-on — it multiplies into every state P has downstream of the
  first touch, in the affected region, which is not decoupled from P's own lexicon-scale structure
  there. No reference grammar measured in this project has hit this case (report `10` D, "5 of 6
  groups... vacuously").

### 3.4 Size bound and determinism

- Static: O(1) marginal, no N dependence (folded into P's own partition).
- Dynamic, Append/reachable-Overwrite: **O(k)** flag symbols total across the whole grammar, k =
  number of MPR bits actually touched by any `out_mpr` (≤6 measured, ≤64 hard cap,
  `model.rs:107-108`). No `|Σ_tag|` dependence beyond needing the allomorph tags already on the tape
  to know *which* allomorph is attaching (already present, `<M:nnnn>` tags, `pg-foma/src/tags.rs:2`
  per report `05`). No N dependence: the flag *alphabet* is fixed-size; the flag *text* emitted per
  entry scales with N×(flags relevant to that entry), which is a proposer text-size cost already of
  the same shape as every other inline-flag mechanism this codebase ships (`precision.rs:193-195`'s
  own "network size grows by AT MOST `entries × coverable_constraints` extra inline symbol tokens,
  linearly" bound, **VERIFIED**, applies identically here).
- Determinizable by ordinary subset construction; the flags are literal alphabet symbols to the
  automaton-theoretic machinery (foma's special zero-width *apply-time* semantics is a runtime
  interpretation layer, not a change to the compiled network's own determinizability — confirmed via
  report `00`/`10`'s citation of `foma/apply.c:1084`, **VERIFIED** by those reports). The closest
  measured real-world proxy for this idiom's determinization cost is `precision.rs`'s own bench:
  Sena's `AllFlags` vs `Strip` states, 39,286→49,889 (1.27×) (report `10` §3.5, **VERIFIED**).
- Dynamic, non-reachable Overwrite: O(4^{k_g}) states, k_g = group size — bounded but coupled to P's
  downstream size in the affected region (§3.3).

---

## 4. `HeadFeatures` — head-feature re-check

### 4.1 What it gates

`AffixAllomorphDef.required_syn_fs` (`model.rs:678`, **VERIFIED** — "Head/foot requirement FS on the
subrule itself (no POS at this level in C#)") is re-checked **not at the moment the rule applies**,
but once, at **word-final validity time**, against the word's fully **accumulated** syntactic FS:

> "**Required syntactic FS** (`AffixProcessAllomorph.CheckAllomorphConstraints`,
> AffixProcessAllomorph.cs:87-105): `RequiredSyntacticFeatureStruct.Subsumes(word.syn)`, re-checked
> at final-validity time against the word's *accumulated* syntactic FS — not just at the moment the
> rule applied (this port's `synth_affix`/`ana_affix` in `morph.rs` never gate on this per-allomorph
> FS at apply time; only the rule-level `required_syn_fs` is enforced there)."
> — `pg-rules/src/validity.rs:24-28`, **VERIFIED** module doc.

The actual check: `pg_featstruct::subsumes(g.fs_interner.get(def.required_syn_fs), &w.syn_fs)`
(`validity.rs:668`, **VERIFIED**), where `w.syn_fs` is the word's syntactic FS **after every rule in
the derivation has run**, not just the rules up to this allomorph's own position. This is the
"re-check" the `ConstraintFamily::HeadFeatures` doc comment names — the literal word "re-check"
appears in both places (`precision.rs:247` and `validity.rs`'s own module doc), which is the
strongest available signal these two are the same construct.

This is materially harder than `Mpr`'s dynamic case: **the check target is the word's *future*
value, not its current one.** By the time this allomorph's own tape position is walked, later
rules — which may still modify `syn_fs` via `priority_union` (`ops.rs:61-75`) — have not run yet. A
plain `@R@` require (tests only what has already been set) cannot express "the eventual final value
will subsume this," because nothing has *finished* setting it yet. This is the same left-vs-right
asymmetry `precision.rs`'s own module doc already names for its `ENV` family ("Right context needs a
mechanism this step does not build ... nothing to its right has been read yet,"
`precision.rs:42-52`, **VERIFIED**) — except here the "right context" is not a bounded lookahead
window but the entire rest of the derivation.

### 4.2 The construction — "trigger, then defer to a shared word-final tail"

**novel-and-unverified**: this specific composition (below) is not drawn from a cited paper; it is
assembled from primitives that *are* independently attested (Beesley & Karttunen's `@P@`/`@R@` flag
semantics, ch. 8 of *Finite State Morphology*, and this codebase's own shipped `ENV` mechanism,
`precision.rs:128-166`). No paper found in this session's search states "defer a flag check to a
shared end-of-word tail" as a named technique; it is a direct consequence of the primitives, not
something claimed as prior art.

1. **Run-flags, one per (feature, value) pair any `out_syn_fs` in the grammar can set.** For each
   syntactic `FeatId f` with declared symbol domain `{v_1..v_D}` (bounded, `SymbolBits`, ≤64,
   §2), mint `@P.SF{f}.{v}@` — unconditionally overwritten wherever a rule's `out_syn_fs`
   sets `f = v` (via `priority_union`, `ops.rs:61-75`), identical mechanism to `precision.rs`'s own
   `@P.ENV{id}.y/n@` unconditional-overwrite idiom (`precision.rs:128-166`, **VERIFIED**, "the value
   visible at any later point is always the MOST RECENT non-empty morph's own verdict — true
   adjacency, not 'ever seen'"). For a nested `Complex` value (head/foot substructure), the flag name
   encodes the feature *path*, not just the top-level `FeatId` — bounded depth, since head/foot share
   one flat namespace and HC's own loader never builds deeper re-entrant structures
   (`pg-featstruct/src/tree.rs:1-11`, **VERIFIED**: "C# `FeatureStruct` supports re-entrancy ...
   but authored HC grammars cannot express it").
2. **One gate-instance trigger flag per `AffixAllomorphDef.required_syn_fs` occurrence** (k of them,
   the family's own "number of constraint instances"): `@P.TRIG{id}.y@`, set unconditionally at that
   allomorph's own tape position (the SAME structural point `precision.rs`'s owner-side `@R@` prefix
   is already emitted at, `precision.rs:149-155` — reuse the threading, change the payload).
3. **One shared word-final tail**, walked by every word immediately before its accepting `#`: for
   each gate-instance `id`, a two-branch disjoint union — `[@R.TRIG{id}.n@] | [@R.TRIG{id}.y@
   (@R.SF{f1}.{v ∈ legal(f1)}@ ... @R.SF{fn}.{v ∈ legal(fn)}@)]` — pass silently if this allomorph
   was never used; else require every feature the allomorph's `required_syn_fs` constrains to have
   landed, by word-final, in a value the requirement's own subsumption test accepts. Because
   `subsumes` is a per-feature AND (§2), this is a plain conjunction (composed, not unioned) of
   independent per-feature require tests. The two branches are keyed on **disjoint** flag values (a
   trigger flag is either `y` or `n`, never both), so the union at this one point is the safe kind —
   two mutually exclusive zero-width flag branches at the same tape position, not the "union of
   overlapping complete replace-nets" hazard report `10` §3.1 measured a 10,000× blowup from.

### 4.3 Size bound and the open precondition

**Bound**: O(F·D) run-flags (F = syntactic feature count, D ≤ 64 hard, ≤37 measured, §2) + O(k)
trigger flags (k = `AffixAllomorphDef.required_syn_fs` instances with non-empty content) + one
shared tail whose own size is O(k) branches. **No N dependence** in the flag alphabet or the tail;
per-entry emission cost is the same linear "entries × coverable constraints" shape §3.4 already
cites. This is the `n·k` (here `F·D + k`) answer, not the `kⁿ` (or, worse, `Dᶠ`) joint-FS-enumeration
answer a naive compiler reaching for "enumerate every possible accumulated feature structure" would
produce.

**The named, unbuilt precondition**: the construction in §4.2 is sound only if **no two derivation
paths with genuinely different accumulated `syn_fs` histories reconverge onto a shared automaton
state before the word-final tail reads the flags** — otherwise the flags at the shared state are
ambiguous (whichever path set them last "wins," silently, which is exactly wrong if the two paths
needed different final checks). This is the **same shape of risk** as `Mpr`'s Overwrite-reachability
problem (§3.3), but for the syntactic-FS dimension, and — checked directly this session —
**`capability.rs`'s `CharacteristicKind` enum (20 variants, `capability.rs:104-174`, **VERIFIED**)
has no entry for `HeadFeatures`, `ObligatoryFeatures`, or `CompoundingFs` at all.** No predicate is
registered, no reachability check like Construction 2 has ever been attempted for this dimension.
Report `10`'s own evidence that path-reconvergence is a *real, live* phenomenon in this compiler —
not a hypothetical — is directly on point: row C2 ("Template group-sharing decouples a template's
prefix side from its suffix side... more paths than trie, never fewer") and row C5 ("Junction-aware
affix emission... offers a root-initial-stripped spelling to every root uniformly... trades a little
overgeneration for not needing lane-level gating at all") are both named, shipped instances of
exactly this kind of path-sharing, for *other* dimensions. Whether it recurs for `syn_fs` accumulation
specifically is unmeasured. This is stated as the report's sharpest **OPEN** item (§7), not folded
silently into a clean verdict.

---

## 5. `ObligatoryFeatures`

### 5.1 What it gates

`AffixProcessRuleDef.obligatory_features: Vec<FeatId>` (`model.rs:641-643`, **VERIFIED** —
"`outputObligatoryFeatures` — syntactic features that must be present in the final word FS for a
parse that used this rule") and `CompoundingRuleDef.obligatory_features` (`model.rs:725`,
**VERIFIED**, same field shape). Accumulated onto `word.obligatory: Vec<FeatId>`
(`pg-rules/src/word.rs:20-21`, **VERIFIED** — "the brief calls for 'obligatory_features recorded'";
flagged as an addition, not in the brief's original struct sketch) at every rule application that
declares them: `morph.rs:1674` (into `synth_process_allomorph`'s obligatory-features parameter),
`morph.rs:1794` (its analysis-direction twin), `morph.rs:3201` (`w.obligatory.extend_from_slice(&rule.obligatory_features)`,
compounding — **VERIFIED**, read directly). Checked **once, at word-final validity**, in
`pg-parse`:

```
for &f in &w.obligatory {
    if !contains_feature(&w.syn_fs, f) { ... fail ... }
}
```
(`pg-parse/src/morpher.rs:906-913`, **VERIFIED**). `contains_feature` (`morpher.rs:1678-1686`,
**VERIFIED**) is a **presence** test — is feature `f` present *at all* (any value), at the top level
or nested inside any `Complex` sub-structure — not a value-membership test.

### 5.2 The construction

Simpler than `HeadFeatures` in exactly one respect: because it is a presence check, not a
value-legality check, and because `priority_union` never *removes* a feature once set (`ops.rs:61-75`
— an existing feature can be overwritten to a new value, but a feature present on only one side
"passes through unchanged"; presence is monotonic once any rule sets it), the run-flag side only
needs **one flag per feature** ("has any rule ever set this feature," `@U.SFPRESENT{f}@`, Beesley &
Karttunen's unification-type "once set, stays set" flag — the same idiom §3.3 uses for `Append`-type
MPR bits) rather than one flag per (feature, value) pair. The trigger side is identical in shape to
`HeadFeatures`'s: one gate-instance flag per **rule** declaring a non-empty `obligatory_features`
list (k = that count), set at the rule's own application point, tested at the same shared word-final
tail (§4.2 step 3), conjoined per declared feature (`@R.SFPRESENT{f}@` for each `f` in that rule's own
list).

### 5.3 Size bound

O(F) run-flags (not O(F·D) — presence only) + O(k) trigger flags (k = number of distinct
`obligatory_features`-declaring rules) + reuse of the same shared tail construction §4.2 builds. No N
dependence, same reasoning as §4.3. **Same open precondition as `HeadFeatures`** (§4.3): no
reachability proof exists that two different "which features got set" histories never reconverge
before the tail reads the flags. `capability.rs` has no `CharacteristicKind` for this construct
either (§4.3, **VERIFIED**).

---

## 6. `CompoundingFs` — compounding-rule feature-structure gates

### 6.1 What it gates

`CompoundingRuleDef` (`model.rs:714-727`, **VERIFIED**) carries `head_required_syn_fs`,
`non_head_required_syn_fs`, `out_syn_fs`, and `obligatory_features` — no separate allomorph-level FS
field exists on `CompoundingSubruleDef` (`model.rs:729-738`, **VERIFIED**: only `vars`,
`required_mpr`/`excluded_mpr`/`out_mpr`, `head_lhs`/`non_head_lhs`, `rhs` — no `required_syn_fs` of
its own). Both syntactic-FS gates fire **at the compounding rule's own application point**, not
deferred:

- `is_unifiable(g.fs_interner.get(rule.non_head_required_syn_fs), &nh.syn_fs)`
  (`morph.rs:2939`/`morph.rs:3022`, **VERIFIED**) — checks the **non-head stem's own, already fully
  derived** syntactic FS (`word.current_non_head()`, a separately-completed sub-word) against the
  rule's requirement. This is genuinely a **two-tape-region** check: the non-head's accumulated
  `syn_fs` lives at a different position in the eventual compound word than the head's.
- `synth_syn_fs(g, rule.head_required_syn_fs, rule.out_syn_fs, word)`
  (`morph.rs:2942`/`morph.rs:3034`, **VERIFIED**) — the same shape as `AffixProcessRuleDef`'s own
  rule-level gate (`morph.rs:1641`, unify-then-priority-union), checked **against the running word's
  CURRENT syn_fs at this point**, not deferred to word-final.
- `rule.head_prod_restrictions_mpr.compound_match(word.mpr)` (`morph.rs:2948`, **VERIFIED**) — uses
  the **flat, group-unaware** `MprSet::compound_match` (`model.rs:155-162`: "`Count == 0 ||
  Intersect(stemMprFeatures).Any()`"), simpler than the group-aware `mpr_group_ok` §3 uses for
  per-subrule gating (`morph.rs:2961` uses `g.mpr_group_ok(sr.required_mpr, sr.excluded_mpr,
  word.mpr)` for the subrule; the rule-level restriction is the flat test).
- `w.obligatory.extend_from_slice(&rule.obligatory_features)` (`morph.rs:3201`, **VERIFIED**) — the
  same deferred, word-final mechanism as `ObligatoryFeatures` (§5), just fed from a compounding rule
  instead of an affix rule; `word.obligatory` is one flat, rule-kind-agnostic `Vec<FeatId>`
  (`word.rs:20-21`).

### 6.2 The construction — a composition of the other three schemas, not a fourth one

`CompoundingFs` does not need a bespoke mechanism. It is the direct sum of:

1. **The `Mpr` schema, flat-variant** (§3.3): `head_prod_restrictions_mpr`/`non_head_prod_restrictions_mpr`/
   `output_prod_restrictions_mpr` are simpler than the grouped case — `compound_match` is a single
   "declared set empty OR overlaps" test, O(k') flags where k' = bits referenced, no group-subset
   register needed at all (the flat test doesn't even need the per-group `Overwrite`/`Append`
   distinction §3 spends most of its complexity on).
2. **The `HeadFeatures`/rule-level schema, but *local*, not deferred**: because
   `head_required_syn_fs` is checked at the compounding rule's own application point against the
   *running* `syn_fs` (not the word-final value), this is a **past-context-only** check — the same
   shape as `Mpr`'s dynamic case (§3.3), not the "trigger, defer to the end" schema `HeadFeatures`'s
   *allomorph-level* re-check needs (§4.2). A plain `@R.SF{f}.{v}@` require, read at the compounding
   join transition, against whatever the head's own run-flags (§4.2 step 1 — the SAME run-flag
   alphabet, reused, not duplicated) already hold, suffices.
3. **The `non_head_required_syn_fs` check — the one genuinely new element**: this reads the
   **non-head sub-word's own** run-flags (built by that sub-word's own derivation, using the identical
   §4.2-step-1 mechanism, before it is ever combined into the compound) at the **same join
   transition**. Because flags are ordinary tape symbols and lexc-style concatenation is
   order-preserving (the non-head's own flag-setting history is walked, on the tape, strictly before
   the join point where the head's continuation attaches), no new tape mechanism is required beyond
   what (2) already needs — only that both sides' run-flags use the *same* flag alphabet (so the
   join point's require tests can read either side's history uniformly). **INFERRED, not built or
   measured**: this composability claim rests on how lexc concatenation and flag persistence are
   documented to work elsewhere in this codebase (`precision.rs`'s own "placement... doesn't matter
   for correctness" finding, `precision.rs:168-184`, **VERIFIED** for the single-tape ENV case); it
   has not been independently verified for a two-sub-word compounding join specifically.
4. **`obligatory_features` — identical to §5**, no new mechanism, just fed from a different rule
   kind into the same flat accumulator and the same shared word-final tail.

### 6.3 Size bound

O(F·D) run-flags (shared alphabet with `HeadFeatures`/§4, not additive) + O(k) gate-instance flags
(k = `CompoundingRuleDef` instances with non-empty `head_required_syn_fs`/`non_head_required_syn_fs`/
`obligatory_features`) + O(k') MPR flags (reusing §3's flat-variant construction). No N dependence,
same reasoning throughout. **Same open precondition as §4.3/§5.3, now needed at the join point
specifically**: no reachability proof exists that the non-head sub-word's own flag history is unique
per join (i.e. that two structurally different non-heads sharing a lexc continuation state don't
present ambiguous flags at the join). `capability.rs` has no `CharacteristicKind` for `CompoundingFs`
either (§4.3).

---

## 7. The cross-family question: one schema, or four bespoke constructions?

**Direct answer: one parameterized schema, with exactly one free parameter.** All four families
reduce to the same two primitives — a **run-flag** per (dimension, value) pair (dimension = an MPR
bit or a syntactic feature; value = its finite declared domain, §2) emitted by unconditional-overwrite
wherever the grammar's own output mechanism (`out_mpr`/`out_syn_fs`) sets it, and a **gate-instance
flag** per constraint occurrence (the family's own `k`). The one parameter that varies *across*
families — and it is a property of **when HC itself defines the check to fire**, not an arbitrary FST
design choice — is:

- **Local** (check against whatever has already been read, no deferral): `Mpr`'s dynamic case
  (`mpr_group_ok`, checked at the allomorph's own application point, `morph.rs:1658`) and
  `CompoundingFs`'s rule-level `head_required_syn_fs`/MPR-flat gates (checked at the compounding
  rule's own application point, `morph.rs:2939-2948`). Uses the plain `@R@`-require-against-past-state
  idiom, identical to `precision.rs`'s shipped `ENV` mechanism.
- **Deferred** (the real check target is the word's *final* value, unknowable at the gating
  position): `HeadFeatures`'s allomorph-level re-check (`validity.rs:668`, explicitly documented as
  running "at final-validity time... not just at the moment the rule applied") and
  `ObligatoryFeatures` (`morpher.rs:906-913`, checked once, word-final). Needs the "trigger, then
  defer to a shared word-final tail" construction (§4.2), which is **novel-and-unverified** as a
  specific composition (§4.2's own framing) but built entirely from cited primitives.

`CompoundingFs` is not a fifth case — it is the schema's most complex *instantiation*, invoking the
local variant once (head, at the join point) and the flat `Mpr` variant once, while its
`obligatory_features` component invokes the deferred variant identically to §5. Its only genuinely
new wrinkle is reading run-flags from **two independently-derived tape regions** (head, non-head) at
one join transition (§6.2 item 3) — which the schema already supports, since a run-flag is just a
tape symbol readable wherever the tape has been walked, not a mechanism scoped to "the current
word" in any way that would need re-deriving for a second sub-word.

**What would refute this "one schema" claim**: if a real grammar exercised a case where a
*single* gate needed **both** local and deferred semantics simultaneously (e.g., a requirement
checked once immediately AND again, differently, at word-final) — no such case was found in the
four families' own source this session, but it was not exhaustively searched for either; this is
listed as an open item (§8) rather than asserted closed.

---

## 8. What tape information is missing today, and its cost

| Construct | On tape today? | What must be added | Cost to the proposer |
|---|---|---|---|
| `Mpr` (dynamic) | **No** — `out_mpr` is purely engine-side; `crate::emit` never touches it | O(k) run-flags at every affix entry whose `out_mpr` is non-empty | Linear text-size growth, same shape as `precision.rs`'s own measured 1.27× Sena states (§3.4) |
| `HeadFeatures` | **No** — `out_syn_fs`/`required_syn_fs` never reach `crate::emit` today (confirmed by the absence of these field names anywhere in `pg-foma/src/precision.rs`'s populated code, and their absence from `capability.rs`'s `CharacteristicKind` enum, §4.3) | O(F·D) run-flags at every rule entry whose `out_syn_fs` is non-empty, O(k) trigger flags at gated allomorphs, one new shared word-final tail construction (genuinely new topology, not present in any emitter path read this session) | Linear text-size growth plus one new, grammar-wide shared construction; magnitude unmeasured (no reference-grammar F·D product was computed this session) |
| `ObligatoryFeatures` | **No**, same absence | O(F) run-flags, O(k) trigger flags, same shared tail | Smaller than `HeadFeatures`'s (presence-only, not per-value) |
| `CompoundingFs` | **No**, same absence, plus the non-head's own flags must be readable at the join (§6.2 item 3, itself unverified) | Same run-flag alphabet as `HeadFeatures` (shared, not additive) + O(k) + O(k') MPR flags | Same order as `HeadFeatures`; the two-region join is the only untested wrinkle |

This is the brief's own conservation law in direct effect: every one of these four filters is cheap
**only if** the proposer is willing to emit substantially more per-entry tape state than it does
today (`out_mpr`/`out_syn_fs` currently touch nothing `crate::emit` writes) — the filter's
cheapness is bought by enlarging P, not obtained for free.

---

## 9. Literature

- **Kaplan & Kay 1994** (ACL Anthology `J94-3001`): licenses treating an ordered cascade of
  obligatory, directional, non-self-recursive rewrite rules as one composed regular relation —
  already the load-bearing citation for how `out_syn_fs`/`out_mpr` accumulate across a derivation at
  all (each rule application is itself a regular relation; composing them is what makes tracking
  "current accumulated state" a well-defined, finite automaton-state notion in the first place). Not
  independently re-fetched this session; cited as already **VERIFIED** in reports `05`/`10`.
- **Karttunen (1994), "Constructing Lexical Transducers," COLING-94 (ACL Anthology `C94-1066`)** and
  **Karttunen, Kaplan & Zaenen (1992), "Two-Level Morphology with Composition," COLING-92**: the
  "intersecting composition" technique — compose with the lexicon rather than materializing the rule
  intersection alone — is the direct ancestor of this report's own "compose per-dimension trackers,
  never build the joint state" recommendation (§2). **VERIFIED** already read/cited in report `10`
  §4.2; not re-fetched this session.
- **Beesley & Karttunen (2003), *Finite State Morphology*, CSLI Publications, ch. 8**: source of the
  `@P@`/`@R@`/`@U@`/`@D@` flag-diacritic vocabulary this report's constructions are built from
  (unification-type "once set" flags for monotonic Append/presence tracking, positive-set-overwrite
  flags for Overwrite/value tracking). **VERIFIED** as an existing citation this project already uses
  (`precision.rs`'s own module doc references "Beesley & Karttunen's own cited band" for flag-lookup
  cost); the primary text was not independently re-fetched this session.
- **Beesley, K.R. (1998), "Constraining Separated Morphotactic Dependencies in Finite-State
  Grammars," Proceedings of FSMNLP 1998, pp. 118-127** — **VERIFIED found** this session (web
  search): confirms the paper exists, covers exactly this report's problem class ("dependencies
  between separated (non-adjacent) morphemes"), and surveys the same technique family this report
  draws from — "running separate constraining transducers at runtime, composing in constraints at
  compile time, feature unification, and the use of flag diacritics." The primary PDF was not
  fetched and read this session (search-result summary only); the specific "trigger, defer to
  word-final tail" construction in §4.2/§6.2 is **not** claimed to be in this paper — it is offered
  as the closest attested survey of the *general technique family*, not as the source of this
  report's specific composition.
- **Yli-Jyrä on two-level-as-intersection**: searched this session; no paper with exactly this
  framing title was found. The closest confirmed Yli-Jyrä results (already read/cited in report `10`
  §4.3, **VERIFIED** there) are Yli-Jyrä (2003, EACL, `E03-1031`, exponential-in-context-count
  blowup for star-free context restriction) and Yli-Jyrä (2011, FSMNLP, `W11-4405`, `O(2^l·(2^r)²·|Σ|)`
  worst case, empirically 1.0–4.0× on ~1,100 real constraints). Reused here as the best available
  empirical evidence that near-independent constraints stay close to their minimal size in practice
  even when the worst-case bound is exponential — directly relevant to whether this report's `n·k`
  claim (§2) holds up in practice, not just asymptotically. **Not found**: a paper stating
  "two-level rules as intersection, feature-structure subsumption case" in exactly the form this
  report's constructions would want to cite as a direct precedent — this is stated as **not found**,
  not silently omitted.
- **Koskenniemi & Silfverberg (2010), SIGMORPHON, `W10-2205`**: already covered in §2 above and in
  report `10` §4.1/§4.2 (**VERIFIED** there); the `>5 days → 34 minutes` real-grammar number is the
  strongest available evidence that naive joint intersection is not merely large but computationally
  infeasible, which is the practical stakes behind this report's insistence on the `n·k` construction
  over any joint-FS-enumeration alternative.
- **Yu, Zhuang & Salomaa (1994), *Theoretical Computer Science* 125(2)**: the `mn`-state tight bound
  for intersecting two already-built automata (**VERIFIED** already cited in report `10` §4.1) — the
  formal statement of the cost this report's per-dimension decomposition is built to avoid ever
  triggering.

---

## 10. Open items, stated exactly

1. **The reachability precondition for `HeadFeatures`/`ObligatoryFeatures`/`CompoundingFs`'s deferred
   construction (§4.3, §5.3, §6.3) has no registered predicate, no `CharacteristicKind` entry
   (`capability.rs:104-174`, **VERIFIED** absence), and no attempted proof anywhere in the four files
   read this session.** This is the single largest gap between "the construction exists and is
   `n·k`" and "the construction is proven sound for a specific grammar." The precedented, cheap fix
   shape already exists in this codebase (MPR's own Construction 2, a characterizer-only,
   `O(touch-points × (V+E))` graph-reachability pass, report `10` row A6) — porting that reachability
   *style* of proof to the syntactic-FS dimension (do any two derivation paths with different
   accumulated `syn_fs` histories reconverge onto a shared lexc/rule-cascade state before a
   `HeadFeatures`/`ObligatoryFeatures` check reads them?) is not attempted here and is the concrete,
   specific next step this report identifies rather than leaving as a vague caveat.
2. **The total syntactic feature count `F` has no verified hard structural cap** (§2) the way `MprSet`
   does — the construction's cost scales with whatever F is for a given grammar, and no reference
   grammar's F was measured this session (no F·D product was computed for Indonesian/Amharic/Sena).
   This is a measurement gap, not a soundness gap.
3. **The two-region join for `CompoundingFs` (§6.2 item 3) is inferred, not verified**, from a
   single-tape precedent (`precision.rs`'s ENV placement-independence finding). Whether flag
   persistence genuinely survives a lexc-style sub-lexicon-to-sub-lexicon concatenation the way it
   survives within one continuous entry's own text was not independently tested this session.
4. **Whether any real grammar's `AffixAllomorphDef.required_syn_fs` or `obligatory_features` is ever
   non-empty at all** was not checked this session (unlike `precision.rs`'s own test suite, which
   confirms Sena has 144 `RequiredEnvironments` and Indonesian has zero for the `ENV` family — no
   equivalent count was pulled for `HeadFeatures`/`ObligatoryFeatures`/`CompoundingFs` against the
   reference grammar XML). If these constructs are vacuous in all three reference grammars (as
   `MprGroupOverwrite`'s groups mostly are, report `10` row A6), the whole apparatus in §4-§6 may be
   currently unexercised by any grammar this project measures — worth checking before investing in
   building it.
5. **A single case needing BOTH local and deferred semantics on the same gate** (§7's stated
   refutation condition) was not exhaustively searched for; none was found in the fields read, but
   the search was not exhaustive over every grammar construct that touches `syn_fs`/`mpr`.
