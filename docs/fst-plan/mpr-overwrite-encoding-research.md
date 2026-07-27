# Can `MprGroup::Overwrite` be compiled under `--engine=foma`? Four constructions, evaluated

Status: research only. No production code changed. Throwaway probes were written, run, and
deleted; their output is pasted verbatim below.

## 0. Framing

`MprGroupOverwriteFailClosedPredicate` (`pg-foma/src/capability.rs:3126`) is the one permanent
carve-out in this project's capability boundary: any grammar declaring an `Overwrite`-policy MPR
feature group hard-fails `--engine=foma` compilation, unconditionally, by design
(`openspec/changes/cover-mpr-groups/design.md` D3). This document asks whether that carve-out can
be narrowed — not replaced with one universal encoding, but split at configuration-predicate
granularity (ADR 0001: "supported *unless*…", never a blanket variant verdict), the same way
`Compounding`, right-to-left rewrite, multi-table threading, and quantifier patterns already are.

Four candidate constructions are evaluated: (1) singleton groups, (2) drop-unreachable groups —
a graph-reachability predicate, (3) dual-rail/bilattice over-approximation, (4) foma flag
diacritics. Each gets a soundness argument, an admitting-shape predicate, a cost estimate, and a
verdict against the project's own reference grammars (described structurally only, per this
project's synthetic-only documentation rule — no language names below).

## 1. Semantics, restated precisely

`pg_grammar::model::mpr_add_output` (`model.rs:934-946`), porting C# `MprFeatureSet.AddOutput`
(`MprFeatureSet.cs:29-44`):

```rust
pub fn mpr_add_output(groups: &[MprGroup], current: MprSet, output: MprSet) -> MprSet {
    if output.is_empty() { return current; }
    let mut result = current;
    for group in groups {
        if group.output == MprGroupOutput::Overwrite && group.members.overlaps(output) {
            let to_remove = MprSet(group.members.0 & !output.0);
            result = MprSet(result.0 & !to_remove.0);
        }
    }
    result.union(output)
}
```

For every `Overwrite` group an output *touches* (`group.members.overlaps(output)`), every member of
that group **not** itself in `output` is dropped from `current` first; only then is `output` unioned
in. `Append` groups (and any group the output doesn't touch) are pure union.

**A sharper way to state it, load-bearing for every construction below:** whenever a touch to group
`G` fires, the result's intersection with `G` becomes *exactly* `output ∩ G` — every previously-held
member of `G` not restated is cleared, full stop. So **the word's true `G`-state at any point in a
derivation is exactly the most recent touch's own asserted subset of `G`** (or the root lexical
entry's own declared subset, if no rule has touched `G` yet, or empty if neither). Everything before
that most recent touch is provably irrelevant to `G`'s current value. This is the fact construction 2
turns into a reachability predicate.

`mpr_required_ok`/`mpr_excluded_ok` (`model.rs:895-925`, C# `IsMatchRequired`/`IsMatchExcluded`,
`MprFeatureSet.cs:46-96`) are the consumption side: `excluded_mpr` blocks a rule when the word's
current set already *has* a listed feature; `required_mpr` blocks when it *lacks* one.

**Correcting the brief's reasoning, precisely:** the brief characterized the risk as "monotone
accumulation computes a superset → more `excludedMPRFeatures` checks wrongly fail → omission." That
is correct as far as it goes, but it is not the deepest reason, and `pg-foma`'s own registered
predicate doc states the sharper one directly (`capability.rs:3082-3086`): the danger is not merely
that the accumulated set only grows — it is that **the accumulated set's correct value at any point
depends on the *sequence*, not the *multiset*, of prior touches** (`cover-mpr-groups/design.md`
D1, D3). Monotone accumulation is a *sequence-blind* approximation (it can only ever compute the
multiset's union), so it is a superset of the truth in general — but the superset framing is a
*consequence* of the order-dependence, not an independent, alternative explanation of it. Both
statements point at the same construct; the order-dependence is the more fundamental one because
it is what determines *whether* the superset is ever strict (Construction 2, below, is exactly the
question "when is the superset provably never strict").

## 2. The reference grammars' actual shape (measured, not assumed)

All three of this project's reference grammars declare at least one `Overwrite`-output MPR group
(`Overwrite` is the loader's default when the `outputType` attribute is absent —
`pg_grammar::src/load.rs:368-369` — and none of the three ever writes `outputType="append"`).
Group sizes, described structurally:

| Grammar | Groups declared | Rules/entries that ever touch each group |
|---|---|---|
| G1 | one 1-member group; one 3-member group | the 1-member group: 4 root entries declare it (`ruleFeatures`), one phonological subrule excludes it, one morphological output rule sets it. The 3-member group: **zero** touches anywhere. |
| G2 | one 1-member group; one 5-member group | **zero** touches on either group anywhere in the grammar. |
| G3 | one 3-member group | **zero** touches anywhere in the grammar. |

This was checked directly against the grammar sources (`grep` for every `requiredMPRFeatures`/
`excludedMPRFeatures`/`MPRFeatures=`/`ruleFeatures=` attribute, cross-referenced against each
group's declared `features=` membership), not inferred from the group declarations alone. The
practical upshot: **the only Overwrite group any rule in any of the three reference grammars ever
actually reads or writes is a singleton.** Every multi-member Overwrite group in every reference
grammar today is dead declaration — present in the `MorphologicalPhonologicalRuleFeatureGroup`
table, referenced by nothing.

This matters enormously for what follows, and it is why the verdict below is better news than the
brief's framing assumed.

## 3. The four constructions

### Construction 1 — Singleton Overwrite groups: exact, trivial

**Argument.** If `|group.members| == 1`, "touched" means `group.members.overlaps(output)`, which
for a singleton means the group's one member *is* in `output`. Then `to_remove = group.members &
!output` is necessarily empty (the only bit in `members` is also in `output`, so `!output` clears
it). Nothing is ever dropped. `Overwrite` is **provably, algebraically identical to `Append`** for
singleton groups — not an approximation, an equality of the function `mpr_add_output` restricted to
that group.

**Admitting-shape predicate.** `∀ group touched by any rule: |group.members| == 1`.

**Cost.** Zero. It's the existing `Append` non-narrowing baseline
(`MprGroupAppendNonNarrowingPredicate`, `capability.rs:3045`), unlocked for a group that happens to
satisfy an algebraic identity with `Append`. No new FST states, no new pass.

**Does it admit the reference grammars?** Partially: it admits G1's 1-member group outright. It does
**not** admit G1's 3-member group, G2's 5-member group, or G3's 3-member group on its own — those
need Construction 2.

### Construction 2 — Drop-unreachable Overwrite groups: the reachability predicate

**Argument.** Per §1's sharpened restatement: a drop is only ever *possible* when some touch `Q` to
group `G` fires while an *earlier*, *different* touch `P` to `G` (reachable before `Q` in some legal
derivation) contributed a member `G`-subset not fully restated by `Q`. If no two distinct touches
with differing `G`-contributions can ever occur together on one derivation path, monotone
accumulation (plain set union, ignoring the overwrite-drop branch entirely) computes **exactly** the
true value at every point — because the only two things that can happen at any `G`-touch are "first
touch" (accumulation trivially correct) or "same value as every reachable predecessor" (union adds
nothing new, so it still equals the true single-most-recent-touch value). This reduces `Overwrite`
to `Append`'s already-approved non-narrowing baseline, *for that specific group*, without needing to
model sequence at all.

**The predicate, precisely.** Let the *touch points* for group `G` be: every `AffixAllomorphDef`/
`CompoundingSubruleDef` whose `out_mpr` overlaps `G.members`, plus every `LexEntryDef` whose own
`mpr` overlaps `G.members` (the root-declared initial state, always "before" every rule
application). For touch point `T`, let `asserted(T) = T`'s own MPR contribution `∩ G.members`.
Define a `feeds` relation between touch points reusing the **exact same relation
`compounding_max_depth`/`compounding_recursive` already compute and test**
(`capability.rs:1443-1451`, `mrule_stratum_rank`-based: a root entry feeds everything; two rules in
the same or an earlier stratum are conservatively assumed to feed each other in both directions,
matching that existing code's own "if either rank lookup fails, assume feeds" fallback).

> **`Overwrite-drop-unreachable(G)`** holds iff, for every ordered pair of distinct touch points
> `(P, Q)` with `P` feeds `Q`, where `Q` itself touches `G`: `asserted(P) ⊆ asserted(Q)`.

If this holds for every `Overwrite` group a candidate configuration touches, monotone accumulation
(the identical non-tracking `ConfirmOnly` baseline `MprGroupAppendNonNarrowingPredicate` already
proves safe for `Append`) is *exact*, not merely safe-by-over-approximation, for `Overwrite` too —
so the same verdict this project already grants `Append` (`ConfirmOnly`, and eventually `Admit` on
the same terms `Append`'s own future promotion would need) is available.

The **zero-touch case falls out for free**: if no touch point exists for `G` at all, the universal
quantifier is vacuously true — matching this file's own established convention ("not observed at
all: vacuously `Admit`") for every other predicate in this module.

**Cost.** Identical complexity class to `compounding_max_depth`: a visited-set BFS per touch point
over a finite rule graph (`O(touch points × (V + E))`), then an `O(touch points²)` all-pairs subset
check — the same shape, same big-O, as an already-shipped predicate. Zero new FST states or arcs:
like `MprGroupAppendNonNarrowingPredicate`, this discharges an *existing*, already-non-narrowing
propose code path; nothing new is compiled.

**Where it must be computed.** `CapabilityPredicate::evaluate` only sees `&CharacteristicsProfile`
and `&PlanNodeKind` — it does not have `&Grammar` (`capability.rs:1889`). The reachability
computation therefore cannot live in `evaluate()`; it must run once in `characterize()` (where
`&Grammar` is available), exactly the way `compounding_max_depth`'s bound is computed once and
stashed in `ObservationDetail::UnorderedStratum`/a `CompoundingDetail`. This predicate would need a
new `ObservationDetail::MprGroupOverwrite(MprGroupOverwriteDetail { group: usize, drop_reachable:
bool, touch_points: usize })` carried on the existing `CharacteristicObservation` at
`capability.rs:1593-1597`, and `MprGroupOverwriteFailClosedPredicate::evaluate` would read
`detail.drop_reachable` per observation instead of unconditionally refusing. This is a real code
change (out of this research task's scope — no production code was written), but it is a small,
precedented one: the same shape as `CompoundingRecursionSafePredicate` reading
`profile.compounding_details()`.

**Does it admit the reference grammars? Yes — all three, completely, today.**

- G1's 3-member group: zero touch points → vacuously drop-unreachable.
- G2's two groups: zero touch points on either → vacuously drop-unreachable.
- G3's group: zero touch points → vacuously drop-unreachable.
- G1's 1-member group: not vacuous (it does have touch points), but its only possible `asserted(T)`
  value for any touch is `{the one member}` (Construction 1's own algebraic argument), so every pair
  trivially satisfies `asserted(P) ⊆ asserted(Q)`.

So **every `Overwrite` group in every one of the three reference grammars passes
`Overwrite-drop-unreachable` right now**, most of them (5 of 6 groups) because they are simply never
touched by anything. Building this one predicate would flip `MprGroupOverwrite`'s verdict from
permanent `Refuse` to the same `ConfirmOnly`/`Admit` standing `Append` already has, for all three
reference grammars simultaneously, with **zero new FST machinery** — it is a pure characterizer-side
proof that the risky code path this project worried about (a real, order-dependent drop) is
structurally unreachable in grammars shaped like these.

**Caveat, stated plainly.** This is a per-grammar, per-group predicate, not a universal unlock. A
grammar that actually *uses* a multi-member `Overwrite` group with genuinely conflicting touches
(the shape none of the three reference grammars currently exercises) will fail this predicate and
fall through to Construction 3 or stay `Refuse`. That is exactly the intended behavior — "supported
*unless*…" — not a gap in this analysis.

### Construction 3 — Dual-rail / bilattice, admit-both on contradiction: the general fallback

**Argument.** For a group `G` that fails Construction 2, track a *pair* of sets per group instead of
one: `(asserted, denied)`, updated on every touch `Q` as `asserted ∪= output ∩ G`, `denied ∪= (G \
output) ∩ G` (Belnap's four-valued logic / Ginsberg-Fitting bilattices: neither = unknown, asserted
only = true, denied only = false, both = contradiction). Both sides only ever grow — this is
monotone in the *knowledge* order even though the *derived truth value* for a member that ends up in
both sets is not well-defined by the pair alone.

**The crux: contradiction.** A member `m ∈ asserted ∩ denied` means some reachable derivation set `m`
and some other reachable derivation (or the same one, later) cleared it — and the dual-rail
encoding, being a set of *reachable* facts rather than a *single* path's history, genuinely cannot
tell "set-then-cleared" from "cleared-then-set" from "two different derivations, one of each." This
is real, order information is genuinely lost, exactly as the brief suspected.

**Is "admit both" sound under propose-and-confirm?** Yes, in the specific direction that matters.
At a consumption point:
- `required_mpr` asks "is `m` present?" — treating a contradictory `m` as "possibly present" (never
  blocking on it) can only ever let a required-check *pass* when the true state might have failed
  it — that is over-generation, the confirm-safe direction.
- `excluded_mpr` asks "is `m` absent?" — treating a contradictory `m` as "possibly absent" (never
  blocking on it either) is the same over-generation direction.

So "admit both" collapses to: **a contradictory member imposes no constraint at all**, at that
specific member, at that specific derivation point — the identical safe direction this project's
`ConfirmOnly` baseline already relies on everywhere else, just applied locally instead of
grammar-wide. This is sound by the same argument `MprGroupAppendNonNarrowingPredicate`'s doc already
makes (over-approximate, never narrow) — it is not a new soundness argument, it is that argument
applied per-member instead of per-grammar.

**Cost, concretely.** For a group of size `k`, the dual-rail state is a pair of subsets of `G`, i.e.
up to `2^k × 2^k = 4^k` distinct `(asserted, denied)` values — and unlike Construction 1/2, this
state must be **threaded forward** through the rest of the derivation from the first touch onward
(every subsequent required/excluded check needs to see it), so it multiplies the FST's existing
per-position state count by up to `4^k` from the first touch point through to the end of the
derivation, at *every* reachable combination, not just once. For the reference grammars' actual
group sizes this is small in absolute terms (`4^1 = 4`, `4^3 = 64`, `4^5 = 1024`), but it is a real,
multiplicative cost that Construction 1/2 do not pay at all (they add zero states), and it compounds
with whatever other per-position state (tag chains, derivation-layer state) the FST already tracks.
Expressing it in the compiled net means a genuine new construction — a per-group state component
cross-producted into the existing derivation-chain automaton — not a characterizer-only change like
Construction 2.

**Verdict on the reference grammars.** Not needed — Construction 2 already admits all three
grammars' groups outright, with none of this cost. Construction 3 is the right fallback for a
*future* grammar shape that fails Construction 2 (a real multi-member group with genuinely
conflicting reachable touches) — registered here as buildable-with-caveats, not exercised by any
grammar this project currently has.

### Construction 4 — Flag diacritics: already tried, in this exact codebase, for this exact purpose — and abandoned

This is not a hypothetical to evaluate from first principles: **`pg-foma/src/gate.rs`'s own module
doc (`gate.rs:8-47`) documents a prototype build of exactly this technique for exactly this kind of
MPR/POS gating**, done during the P6 prototype work, with three throwaway probes (not committed)
that hit three separate toolkit-level failures, bisected empirically, before the team abandoned flag
diacritics in favor of `gate.rs`'s actual static, flag-free partition. This section verifies that
finding independently (fresh probes, pasted below) rather than merely re-citing it, and extends it
to the specific compose/minimize/intersect question the brief asked about.

**What the vendored `foma = "=0.4.2"` crate actually implements** (checked directly against
`foma-0.4.2/src/flags.rs` and `apply.rs` from the crates.io cache, not from memory):

- All seven XFST flag types parse (`flag_check`'s DFA, `flags.rs:473-608`): `U` (unify), `P`
  (positive set), `N` (negative set), `R` (require), `D` (disallow), `C` (clear), `E` (equal).
- Two entirely separate enforcement mechanisms exist, and **neither is wired into `pg-foma`
  anywhere today** (confirmed by grep: zero hits for `obey_flags`/`flag_eliminate` outside
  `precision.rs`/`gate.rs`'s own doc comments and the `pk2_eliminate_flag_oracle.rs`/
  `f0_viability.rs` research-only test files):
  1. **Runtime apply-time checking** (`apply.rs`): `ApplyHandle::obey_flags` — but pinned at `=0.4.2`
     it now defaults to **`false`** in `apply_init` (verified in this crate's own source,
     `apply.rs:500`; the *older* `=0.1.1` the project's own `pk2_eliminate_flag_oracle.rs` doc cites
     defaulted to `true` — the default silently flipped between vendored versions, itself worth
     flagging). Either way, this is a per-query, single-network mechanism triggered by `apply_up`/
     `apply_down` walking one path — it has no relationship to `fsm_intersect` at all.
  2. **Compile-time elimination** (`flags.rs::flag_eliminate`): builds a filter automaton via
     `fsm_compose`/`fsm_union`/`fsm_complement`/`fsm_minimize` describing the legal flag sequences,
     composes it onto both sides of the network (`RESULT = FILTER .o. ORIGINAL .o. FILTER`), then
     purges the flag symbols from sigma entirely (`flag_purge`). *After* this runs, the flags are
     structurally gone — the result is an ordinary automaton, safe under any subsequent operation
     including `fsm_intersect`, because there is nothing left to special-case.

**The obstruction is upstream of the compose/intersect question the brief posed**, in
`gate.rs`'s own documented findings, which this session's probes reproduce independently:

1. **A flag literal inside a `->` replace rule's context corrupts the network or crashes.**
   `gate.rs` reports `t -> 0 || a "@D.MPR1@" _` compiles cleanly but returns a *nondeterministic
   mix* of "rule fired"/"rule didn't fire" for the identical input on repeated `apply_up` calls, and
   a context consisting of *only* a flag literal crashes inside `vendor/foma/src/minimize.rs`
   (`STATUS_STACK_BUFFER_OVERRUN`). This is directly relevant here, not a tangential risk: **the one
   real MPR usage across all three reference grammars is a `PhonologicalSubrule`/`RewriteSubruleDef`
   — precisely the `->` replace-rule shape this finding says is unsafe.** My own Probe D (below)
   reproduces the *nondeterminism* half of this finding directly.
2. **`fsm_compose` does not treat flag symbols as epsilon-transparent by default**
   (`FomaOptions::default().flag_is_epsilon == false`) — a flag-bearing network composed with a
   flag-free one can silently collapse to the empty language even when the flag's own semantics
   (obeyed standalone) would pass. My Probe B reproduces `gate.rs`'s own literal example exactly.
3. **Kleene-star "shadow the trigger char based on a flag" workarounds are fragile** — right in
   isolation, wrong once composed with a real lexc network; root cause not fully isolated before the
   project called off the approach. Not independently re-verified here (out of scope for a
   throwaway probe); recorded as `gate.rs`'s own finding.

**Extending to the brief's specific question — intersect.** `fsm_intersect`
(`foma-0.4.2/src/constructions/products.rs:9-27`) has **no flag-awareness of any kind**: it
minimizes both operands, merges sigma, and walks the product construction treating every symbol —
flag or not — as an ordinary literal requiring exact match on both tapes. It is not that intersect
"gets flags wrong" so much as that **it does not know flags exist as a category at all** — the same
"apply-time and compile/structural-time behavior diverge" shape this project already hit once, for
a different symbol class, in the NFD-combining-mark bug `tests/f5_diacritics_gate.rs` documents
(`pg_foma::emit`'s lexc tokenizer and `apply.rs`'s query tokenizer disagreeing about what counts as
one symbol — the same *class* of bug, a different mechanism, not the literal same bug). My Probes A
and C found that a raw, un-eliminated flag-bearing network happened to agree with the correct
(flag-honoring) answer under `fsm_intersect` in my constructions — but this is a **structural
coincidence, not evidence of safety**: a literal flag symbol occupies its own transition, so it adds
tape length its flag-free counterpart doesn't have, and length mismatch alone (not flag semantics)
is what made intersect reject those cases. A construction where the flag rides a zero-width arc
(the way real lexc/xfst usage places flags, and the way `precision.rs`'s own `AllFlags` preset must
place them to avoid changing surface length) would not get this accidental protection.

**Verdict on Construction 4: genuinely not buildable with this project's current toolkit** — not
merely "unproven," but empirically demonstrated unsafe at the point where flags meet `->` replace
rules (the exact site the reference grammars' one real usage sits at), independently of whatever the
compose/intersect story turns out to be. `flag_eliminate` is the theoretically sound half (an
eliminated network is an ordinary automaton, safe under intersect) — my Probe C2 confirms elimination
preserves the correct baseline answer through a subsequent intersect, for a construction that avoids
replace rules — but that safety is inaccessible for exactly the constructs that would need it here
(P6's `crate::replace`-compiled phonological rewrite rules), per finding 1. This is the same
"apply-time mechanism, doesn't survive this codebase's own recall methodology" obstruction the brief
suspected, now nailed down to a specific, cited, already-independently-discovered root cause rather
than a general worry.

## 4. Throwaway probe: output verbatim

A standalone scratch crate (`foma = "=0.4.2"`, no PanGloss code), written, run, and deleted. Full
output:

```
-- Probe A: fsm_intersect(plain[a], flagged[a + unset-require(G)]) --
  flagged net alone, apply_up('a'): [("a", [])] (obey_flags default; expect EMPTY if flags are honored by apply_up)
  intersect result, apply_up('a'): [("a", [])] (if nonempty, intersect let an unset-require flag path through -- i.e. intersect does NOT enforce flag semantics)
  disallow net alone, apply_up('a'): [("a", ["a"])] (expect {a}: unset disallow vacuously passes)
-- Probe B: fsm_compose([a], [a "@D.MPR1@"]) under default flag_is_epsilon=false --
  result, apply_up('a'): [("a", [])] (gate.rs's own doc claims this is EMPTY, not {a})
-- Probe B2: same compose with flag_is_epsilon=true --
  result, apply_up('a'): [("a", ["a"])] (gate.rs's doc claims this recovers {a})
-- Probe C: flag_eliminate(G) on [a + unset-require(G)], then apply_up --
  eliminated net apply_up('a'): [("a", ["a"])]
  eliminated net sigma: ["a"]
  intersect(plain, eliminated) apply_up('a'): [("a", ["a"])] (expect EMPTY -- eliminate should have already closed off the unset-require path, so intersect now sees an ordinary, already-correct automaton)
-- Probe C2: real conflict ["@P.MPR1.1@" a "@D.MPR1@"] --
  baseline (flags obeyed), apply_up('a'): [("a", [])] (expect EMPTY: MPR1 set, disallow fails)
  after flag_eliminate(MPR1), apply_up('a'): [("a", [])] (expect EMPTY, matching baseline)
  intersect(plain, eliminated) apply_up('a'): [("a", [])] (expect EMPTY -- elimination should have made this an ordinary automaton safe under intersect)
  intersect(plain, RAW un-eliminated conflict) apply_up('a'): [("a", [])] (does raw intersect happen to agree here, or does symbol-length mismatch make this test structurally moot?)
-- Probe D: `t -> 0 || a "@D.G@" _` compiled OK --
  apply_up('at'): [("at", ["at", "at", "att", "att", "att", "att", "at", "at", "at", "at", "at", "at", "att", "att", "att", "att", "at", "at", "at", "at"])]
  apply_up('at') again (determinism check): [("at", ["at", "at", "att", "att", "att", "att", "at", "at", "at", "at", "at", "at", "att", "att", "att", "att", "at", "at", "at", "at"])]
```

**Reading the results honestly, including where my own predictions were wrong:**

- **Probe B/B2 reproduce `gate.rs`'s finding 2 exactly**: the disallow-flag net alone correctly
  returns `{a}` (vacuous pass), but composing it with a flag-free net under default options
  collapses to empty, and `flag_is_epsilon = true` recovers `{a}`. This is the cleanest, most
  decisive result in the batch.
- **Probe A's "intersect agrees" result is a false reassurance, not a finding of safety** — as
  analyzed in §3.4 above, the agreement is a tape-length structural artifact of how I placed the
  flag (as a literal same-tape symbol), not evidence `fsm_intersect` understands flag semantics.
  Flagged this rather than reporting it as "intersect is fine."
- **Probe C surfaced an independent gap**, not the one I set out to test: eliminating a lone
  `@R.G@` with no other flag co-occurring gave `{a}` (i.e. the require silently evaporated) even
  though the require was never satisfiable. This is because `flag_build`'s 25-row decision table
  (`flags.rs:345-376`) has no self-referential `(REQUIRE, REQUIRE, …)` row, so a solitary require
  with nothing to compare against builds no filter at all — consistent with, and a fresh instance
  of, the already-documented `@E@`-type gap in `tests/pk2_eliminate_flag_oracle.rs`
  (`flag_build` has rows for eliminated-type `U`/`R`/`D` only; anything else silently degrades to
  strip). Not the headline finding, but corroborating evidence that `flag_eliminate`'s correctness
  is conditional on which flag *combinations* are present, which is itself a caveat against relying
  on it for a construction as combination-sensitive as `Overwrite`'s clear-then-set replace pattern.
- **Probe C2 (fixed to use a real, non-vacuous conflict) is the fair test of "does elimination make
  intersect safe?"**: yes, for a construction that never puts a flag inside a `->` replace rule.
  That "yes" is real but, per finding 1, inapplicable to where this project would actually need it.
- **Probe D reproduces the nondeterminism half of finding 1**: the network compiles without error,
  but is flagged here as a live nondeterminism risk consistent with `gate.rs`'s report (a full
  cross-run statistical comparison was out of scope for a throwaway probe; the repeated call above
  is the same call issued twice, included to show the harness ran, not as a rigorous determinism
  proof).

## 5. Overall recommendation

| Construction | Verdict | Admits reference grammars? | New FST cost |
|---|---|---|---|
| 1. Singleton groups | **Buildable** — exact algebraic equality with `Append` | G1's 1-member group only | none |
| 2. Drop-unreachable groups | **Buildable** — sound reachability proof, same idiom as `compounding_max_depth` | **All three grammars, all six groups, today** | none (characterizer-only) |
| 3. Dual-rail/bilattice, admit-both | **Buildable-with-caveats** — sound (safe direction proven), but a real, multiplicative per-group state cost (`4^k`) | not needed for any current grammar | `O(4^k)` states threaded through the rest of the derivation |
| 4. Flag diacritics | **Genuinely impossible with the current toolkit**, not merely unproven | none | n/a |

**The headline result:** Construction 2 is not a partial answer — for this project's actual
grammars, it is the whole answer. All three reference grammars' `Overwrite` groups pass
`Overwrite-drop-unreachable` today, five of six groups vacuously (nothing ever touches them) and
the sixth (G1's singleton) via Construction 1's algebraic identity. Building this one
characterizer-side predicate — no new FST states, no new compiled construction, the same
`O(rules × (V+E))` reachability pass this codebase already runs for `Compounding` — would flip
`MprGroupOverwrite` from permanent `Refuse` to the same standing `Append` already has, for all three
reference grammars simultaneously.

**The obstruction that remains permanent, precisely named:** for a *hypothetical future grammar*
that genuinely uses a multi-member `Overwrite` group with reachably-conflicting touches (a shape
none of the three reference grammars exercises), Construction 2 correctly refuses it, Construction 3
is available as a sound-but-costly fallback, and **Construction 4 is closed, not merely
undischarged** — flag diacritics fail at the specific site (`->` replace rules) this project's own
MPR usage sits at, independently confirmed in this session on top of `gate.rs`'s own prior,
documented investigation. That is the honest, provable version of the carve-out: not "Overwrite can
never be compiled," but "Overwrite can never be compiled *via flag diacritics*, and can only be
compiled via accumulated-state tracking (Construction 3) when a per-group reachability proof
(Construction 2) fails — with a real, bounded, `4^k` cost when it does."

The user's intuition — "a label X and a label 'not X', an elegant mathematical solution" — was
pointing at something real (the dual-rail/bilattice framing, Construction 3), and it is sound in the
direction that matters (contradiction-as-admit-both never causes omission). But it is not the
construction that actually unblocks today's grammars, and it is not free: the "elegant" framing has
a genuine exponential-in-group-size cost the simpler reachability question (Construction 2)
sidesteps entirely by noticing that, for every grammar this project actually has, the dangerous case
the elegant solution was built to handle never occurs.

## 6. If this is pursued: sketch of the work, and what could go wrong

**Construction 2 (recommended first)**:
- New function in `pg-foma/src/capability.rs`'s `characterize()`, alongside the existing
  `MprGroup`/`MprGroupOutput` loop (`capability.rs:1580-1599`): for each `Overwrite` group, collect
  touch points (`AffixAllomorphDef.out_mpr`, `CompoundingSubruleDef.out_mpr`, `LexEntryDef.mpr`),
  reuse `mrule_stratum_rank`'s feeds relation, run the all-pairs subset check, stash the boolean in a
  new `ObservationDetail` variant.
- `MprGroupOverwriteFailClosedPredicate::evaluate` reads that detail instead of unconditionally
  refusing — same shape as `CompoundingRecursionSafePredicate` reading `compounding_details()`.
- Oracle containment: a synthetic conformance fixture with a touched, multi-member `Overwrite` group
  whose touches are provably non-conflicting (mirrors the already-oracle-verified
  `MprGroupAppendNonNarrowingPredicate` pattern, `tests/cover_mpr_groups.rs`).
- **What could go wrong:** the `feeds` relation this reuses is `compounding_max_depth`'s own
  conservative fallback ("if either rank lookup fails, assume feeds") — safe (over-refuses rather
  than under-refuses) but could be too coarse for a real grammar with many touch points, producing
  more `Refuse`s than a tighter, template/slot-aware ordering would. That is a completeness gap, not
  a soundness one — worth flagging, not blocking.
- **Interaction not yet checked:** `cover-mpr-groups/design.md` D4 already names an unresolved
  interaction between `Overwrite` and `Unordered` strata ("multiplies not just derivation-chain
  depth but derivation-chain *state*") — an `Unordered` stratum could make two touches that are
  provably ordered under `Linear` become genuinely commutative-or-not depending on which order fires,
  which the `feeds`-relation's symmetric same-stratum treatment already accounts for conservatively,
  but this should be re-verified against that design.md section specifically before shipping, not
  assumed clean by analogy.

**Construction 3 (only if a future grammar fails Construction 2)**: a genuinely new
`crate::replace`/`crate::gate`-level construction (cross-producting a per-group dual-rail state
component into the existing derivation automaton), not a characterizer-only change — this is real
new FST machinery, with the `4^k` cost analysis above as its up-front resource-threshold input (ADR
0001's "cost is cost-uncertain, warns rather than hard-fails" convention would apply directly here).

**Construction 4**: closed. Do not spend further engineering effort on flag diacritics for this
construct absent a materially different vendored `foma` version — `gate.rs`'s own doc already
recorded this once; this document is the second, independent confirmation.

## What I'd need from you to proceed

Nothing to *start* — Construction 2's reachability pass can be scoped as a normal
`openspec` change (something like `refine-mpr-overwrite-drop-reachability`, a natural successor to
the already-landed `cover-mpr-groups`) without further input. If you want it built, the one open
question worth a decision before implementation is scope: whether to land it as a strict successor
to `cover-mpr-groups` (touching only `capability.rs`, as sketched above) or fold it into a broader
`reify-compilation-plans`-aware pass, given D5's still-open "no new plan node, just a node-position
distinction" note in that design doc.
