# F1 quirk audit — HYBRID_FST_RUST_PLAN.md §4.3

> **LEGACY — superseded by [`foma-fst-plan.md`](foma-fst-plan.md).** This document is part of the
> record of the earlier custom-spun FST prototype (`hc-hybrid`), which PanGloss has sunset (plan
> P5, gate F5, 2026-07-16) in favor of a foma-based FST proposer with the full HermitCrab engine
> confirming/pruning. Kept for historical record only — not current design guidance.

> Companion to `HYBRID_FST_RUST_PLAN.md` §4.3 ("bug-for-bug parity") and its F1 gate ("quirk audit
> done; reviewer signs off that every C# read site of a §4.3-listed member was visited"). This
> document is the evidence trail: for each of the 8 listed quirks, the real C# source location, a
> direct quote/paraphrase of the code (not just the plan's paraphrase), and a verdict —
> **CONFIRMED** (plan's description matches the code exactly), **CONFIRMED + SHARPENED** (matches,
> with detail the plan didn't spell out), or **REFUTED/MODIFIED** (none found this pass).
>
> C# read from `C:\Users\johnm\Documents\repos\machine\.worktrees\fst-oracle\src\SIL.Machine.Morphology.HermitCrab\`
> (branch `fst-oracle`, the frozen oracle ref per `rust/parity-out/golden/fst-advisor/MANIFEST.txt`).
> This audit's job is confirmation + evidence-gathering only — none of these 8 quirks are
> *implemented* yet (that happens in F2+ when each subsystem is actually built); `hc-hybrid` at F1
> is only `token.rs` + scaffold.

## 1. `LockstepPhonologyProposer.HasNonIdentityArcs` — start-state-only inspection

**File:** `LockstepPhonologyProposer.cs:57-67`

```csharp
private static bool HasNonIdentityArcs(InversePhonology pinv)
{
    foreach (InversePhonology.Arc arc in pinv.ArcsFrom(pinv.StartState))
    {
        if (arc.IsEpsilonInput || !arc.SurfaceInput.ValueEquals(arc.UnderlyingOutput))
        {
            return true;
        }
    }
    return false;
}
```

**Verdict: CONFIRMED.** The scan is literally `pinv.ArcsFrom(pinv.StartState)` — only arcs leaving
state 0, never arcs reachable transitively (e.g. after a left-environment chain, `ChainLeftEnvironment`
in `PhonologyRuleCompiler.cs:252-262`, which builds NEW states off state 0 for a non-empty left
environment). Consequence: a rule whose every subrule branch begins with a non-trivial left
environment (so its first arc from state 0 is an *identity* environment-passthrough arc, not the
restoration/substitution arc itself — that arc only appears one or more hops downstream) is invisible
to this check and the whole `LockstepPhonologyProposer` silently reports `_hasArcs = false`,
returning `Enumerable.Empty<WordAnalysis>()` from `AnalyzeWord` (line 48-55) — the v1 proposer
contributes nothing for that rule, with no diagnostic. Called out in `ChainDeletionEpenthesisTests`
per the plan; the default (v1) composite's candidate set depends on this exact scan depth (1 hop),
not "any non-identity arc anywhere in the automaton."

## 2. `PhonologyRuleCompiler` v1 alphabet excludes boundary char-defs

**File:** `PhonologyRuleCompiler.cs:55` (construction) + `:294-308` (`BuildProbeString`, the failure
site) + `:126-130` (the caller's early-exit)

```csharp
_alphabet = table.Where(cd => cd.Type == HCFeatureSystem.Segment).ToList();
...
private string BuildProbeString(List<FeatureStruct> envConstraints)
{
    ...
    CharacterDefinition rep = _alphabet.FirstOrDefault(cd => cd.FeatureStruct.IsUnifiable(fs));
    string str = rep?.Representations.FirstOrDefault();
    if (str == null) { return null; }
    ...
}
...
string leftEnvProbe = BuildProbeString(leftEnv);
string rightEnvProbe = BuildProbeString(rightEnv);
if (leftEnvProbe == null || rightEnvProbe == null)
{
    _unsupportedRuleCount++;
    return;
}
```

**Verdict: CONFIRMED.** `_alphabet` is built once, filtered to `HCFeatureSystem.Segment`-typed
char-defs only (`Boundary`-typed char-defs are excluded by construction, not merely deprioritized).
Any subrule whose left/right environment constraint is *only* unifiable with a boundary char-def's
`FeatureStruct` (a rule that requires "at a morpheme boundary") makes `BuildProbeString` return
`null` for that side (`_alphabet.FirstOrDefault` finds no match), which the caller treats as
unconditionally unsupported (`_unsupportedRuleCount++`, no arcs emitted for that subrule) — v1 has
no code path that ever adds a boundary-conditioned arc. The chain compiler (`RuleInverseCompiler`)
fixes this: its own alphabet (`RuleInverseCompiler.cs:175-177`) is
`cd.Type == HCFeatureSystem.Segment || cd.Type == HCFeatureSystem.Boundary` — confirms the plan's
"the chain compiler fixed this; v1 deliberately didn't change" by direct comparison of the two
constructors.

## 3. `BuildAffixArcs` dedups variants by rendered string, not `FeatureStruct` sequence

**File:** `FstTemplateAnalyzer.cs:1380-1421`

```csharp
private void BuildAffixArcs(State<Shape, ShapeNode> tokenState, State<Shape, ShapeNode> after, InsertSegments insert)
{
    ...
    string underlying = insert.Segments.Representation;
    foreach (string variant in _affixSurfaces(underlying))
    {
        if (variant == underlying) { continue; }   // string comparison
        Shape vshape;
        try { vshape = _table.Segment(variant); }
        catch (InvalidShapeException) { continue; }
        State<Shape, ShapeNode> sv = tokenState;
        foreach (FeatureStruct fs in GetSegments(vshape)) { sv = AddArc(sv, fs); }
        sv.Arcs.Add(after);
        ...
```

**Verdict: CONFIRMED.** `_affixSurfaces` (injected as `Func<string, IReadOnlyCollection<string>>`,
field declared `FstTemplateAnalyzer.cs:63`) is backed by `SurfacePhonology.Variants`, which returns a
**string** set (`HashSet<string>` — every surface variant is deduped as a rendered representation the
moment `SurfacePhonology` produces it, before `BuildAffixArcs` ever sees it). `BuildAffixArcs` itself
adds no further FeatureStruct-level dedup — it re-segments (`_table.Segment(variant)`) each surviving
string independently and builds one full arc chain per distinct string. Two variants that render to
the *same* string via different underlying `FeatureStruct` sequences (e.g. two segments that are
featurally distinct but share every character-definition representation) collapse to one arc chain,
under-representing the true branch count — this is exactly the Phase-H state-count note the plan
cites: state counts for a grammar with such collisions are lower than a FeatureStruct-keyed dedup
would produce. `variant == underlying` (line 1401) is also a plain string comparison, same class of
quirk at the entry-point skip check.

## 4. Chain deletion-restoration cap counts events, not sites

**File:** `RuleInverseCompiler.cs:180-194` (default-cap doc) + `:229-234` (`restorationCap` param doc)

```
/// The `+ 1` mirrors the engine exactly ... Note an honest semantic gap: one ENGINE round can
/// restore several independent deletion sites simultaneously, while this cap counts individual
/// restoration EVENTS in the chain walk — a word with more independent sites of one rule than the
/// cap falls to the engine/unparsed (never wrong, never a hang).
...
/// <param name="restorationCap">I3's knob: the maximum number of deletion-restoration EVENTS (one
/// event = one traversal of one deletion branch, however many segments that branch restores) each
/// rule's automaton may perform per word. Enforced structurally ...</param>
```

**Verdict: CONFIRMED**, with the mechanism spelled out precisely by the C# doc comment itself (not
just inferred): the automaton's "floors" (`CompileRule`, `:302-390`) give a rule with deletion-shaped
subrules `restorationCap + 1` full copies of its automaton; each restoration-event traversal moves up
exactly one floor (`floorBase[f]` → `floorBase[f+1]`), and the top floor has no deletion branches at
all — so the structural bound is on the *count of restoration-branch traversals* along one walk, which
is not the same quantity as "how many independent underlying sites got restored" if the real engine's
one analysis round restores several sites at once (`AnalysisRewriteRule`'s deletion loop, referenced
but not re-derived here). Default cap = `Morpher.DeletionReapplications + 1` (line 193), matching the
engine's own `+1` convention (deletion applies once unconditionally, `DeletionReapplications` counts
further reapplications).

## 5. α-variables: one representative per class, Permissive tier (not per-binding enumeration)

**File:** `RuleInverseCompiler.cs:447-486` (the gate) + `:457-458` (`BuildProbeRepresentative`, one
call, not an enumeration)

```csharp
if (lhs.Any(c => c.FeatureStruct.HasVariables)
    || rhs.Any(c => c.FeatureStruct.HasVariables)
    || HasVariablesAnywhere(subrule.LeftEnvironment)
    || HasVariablesAnywhere(subrule.RightEnvironment))
{
    AddReason(reasons, "alpha-variable");   // reason added, NOT an early return
}

List<FeatureStruct> leftProbe = BuildProbeRepresentative(subrule.LeftEnvironment);
List<FeatureStruct> rightProbe = BuildProbeRepresentative(subrule.RightEnvironment);
...
foreach (CharacterDefinition[] combo in EnumerateLhsCandidates(lhs)) { ... }
if (spec.Candidates.Count == 0) { AddReason(reasons, "no-effect"); return null; }
```

**Verdict: CONFIRMED.** Detecting an α-variable anywhere in Lhs/Rhs/environments adds the
`"alpha-variable"` reason string but does **not** stop compilation — probing continues with
`BuildProbeRepresentative`, which (per its name and single call site, not a combinatorial loop over
variable bindings) returns ONE concrete representative FeatureStruct sequence per environment,
regardless of how many distinct values the variable could bind to. Final tier is computed at
`:1017`: `reasons.Count > 0 ? Permissive : Exact` — so any subrule that reached this point with only
the `"alpha-variable"` reason (probing succeeded, `spec.Candidates.Count > 0`) reports **Permissive**,
never a dedicated "alpha-variable-enumerated" tier; per-binding enumeration is not implemented
anywhere in this file (confirmed by absence — no loop over symbol values keyed to a shared `VarId`
exists in `TryProbeCandidate`/`EnumerateLhsCandidates`). If probing additionally finds
`spec.Candidates.Count == 0` (the representative binding happens to produce no observable rule
effect), `"no-effect"` is appended too and the subrule falls through to one of the `IdentitySkip`
returns (`:921`/`:926`/`:946`/`:961`/`:976`/`:1008-1009`) with reasons `["alpha-variable","no-effect"]`
— exactly the string the plan says the Amharic tier-report gate pins for the CV-merger rule (that
specific tier-report string match is validated by the existing C# `GrammarFstAdvisorTests`/tier-report
test suite, not independently re-run in this audit pass).

## 6. Self-feeding iterative rules: no detection (documented residual)

**File:** `RuleInverseCompiler.cs:196-228` (doc comment on the rule-set `Compile` overload)

**Verdict: CONFIRMED**, and unusually thoroughly self-documented in the C# source itself: the comment
narrates a full history — an earlier heuristic (flag a rule "iterative-self-feeding" whenever its own
Rhs unifies with its own Lhs or environment) was tried and reverted because it fired on essentially
every ordinary substitution/assimilation rule (confirmed regression: downgraded Amharic's "remove
consonant length from lexical forms" rule from Exact to Permissive, added a spurious reason to 2
Indonesian rules), and two narrower alternatives were considered and explicitly rejected as either
requiring machinery this black-box prober doesn't have, or being vacuous/wrong against how HC rules
are actually written (verified via `RewriteRuleTests.AlphaVariableRules`, per the comment). No
detection code exists anywhere in the file for this — grepping `"iterative-self-feeding"`/
`"self-feed"` across `RuleInverseCompiler.cs` surfaces eight more references (`:58,125,201,202,220,
348,504,881`) and every one is a doc comment explaining the absence or cross-referencing this same
decision; none is an `if`/predicate that inspects a rule for this property, confirming the removal
was real, not a docs-only description of code elsewhere. `ApplicationMode
.Iterative` rules that truly self-feed within one word may under-cover via the chain silently (no
reason string names it) — never unsoundly, since `FstReplay` verify is the actual backstop.

## 7. Beam accounting: two debit points (frontier admission + per-matching-arc)

**File:** `FstTemplateAnalyzer.cs:870-884` (frontier axis) + `:958-967` (`CascadeSymbol`, enumeration
axis)

```csharp
// AnalyzeChain's per-segment loop:
foreach (PConfig nc in CascadeSymbol(chain, pc.RuleStates, 0, segment, pc.Lex, budget, pc.InsertionsUsed))
{
    if (seen.Add(PKey(nc)))
    {
        if (!budget.TryDebit()) { break; }   // (a) frontier-axis debit: post-dedup NEW config only
        next.Add(nc);
    }
}
...
// CascadeSymbol, per arc considered:
foreach (InversePhonology.Arc arc in chain[rank].ArcsFrom(ruleStates[rank]))
{
    if (arc.IsEpsilonInput || !arc.SurfaceInput.IsUnifiable(symbol)) { continue; }
    if (!budget.TryDebit()) { yield break; }   // (b) enumeration-axis debit: BEFORE cloning/recursing
    ...
```

**Verdict: CONFIRMED, exact match to the plan's two-site description.** (a) is debited once per
NEW post-dedup frontier config (`seen.Add(PKey(nc))` gates it — a config that collides with one
already in `next` this step is never debited a second time). (b) is debited once per matching arc
INSIDE `CascadeSymbol`, before the state-vector clone/recursion/lexicon-arc scan that follows — the
class doc (`:938-947`) explains why: it is the recursive rank-by-rank fan-out inside `CascadeSymbol`
that can enumerate exponentially many candidate paths for a single input symbol before any of them
reach the frontier's dedup `HashSet`, so debiting only at (a) would let that inner recursion run
unbounded before ever being counted. `EpsilonClosure` (`:648-684`) and `ChainClosure` (`:1032-1168`,
its own doc at `:1032-1033` cross-references the same two axes for the ε-closure side) debit at the
analogous points for ε-driven config admission. Both debit points must be ported for `Overflowed`
counts to match the golden exactly — porting only the frontier axis would systematically undercount.

## 8. `FstReplay` keeps templates/strata/all phonological rules open

**File:** `FstReplay.cs:63-96`

```csharp
morpher.LexEntrySelector = e => e == root || extraRoots.Contains(e);
morpher.RuleSelector = r =>
    r is AffixTemplate
    || r is Stratum
    || r is IPhonologicalRule
    || rules.Contains(r)
    || (extraRoots.Count > 0 && r is CompoundingRule);
```

**Verdict: CONFIRMED**, direct read, no interpretation needed. `RuleSelector` is `true` for: any
`AffixTemplate`, any `Stratum`, any `IPhonologicalRule` (unconditionally — deletion/substitution/
metathesis rules are never gated, matching the design rationale that phonology is an obligatory
deterministic rewrite, not a fan-out choice), any rule literally in the candidate's own `rules` set
(the morphological rules this candidate actually claims to use), and `CompoundingRule` **only** when
`extraRoots.Count > 0` (i.e. only when the candidate itself is a compound — an ordinary word's
`RuleSelector` never opens `CompoundingRule` at all, keeping its fan-out as tight as a non-compound
candidate). `LexEntrySelector` admits exactly the candidate's root plus any compound non-head roots
(`extraRoots`), nothing else. Signature comparison (`Signature`, `:98-113`) keys by per-morpheme
*object identity* (a `Dictionary<IMorpheme,int>` assigning ids on first sight), explicitly because
`Morpheme.Id` is empty for affixes in these grammars (documented in the method's own summary) — this
is the same empty-`Id` fact the F0 `MANIFEST.txt` §1 independently re-discovered for the batch
signature format; `FstReplay`'s signature space and the batch `fst-restricted`/`fst-batch` signature
space are each their own per-run identity-numbering scheme, not literally the same numbers, though
both exist to route around the same underlying defect.

## 9. `ArcCollection` binary-search insertion order (found during F4, added to §4.3's list)

**File:** `src/SIL.Machine/FiniteState/ArcCollection.cs:19-25` (comparer) + `:136-143`
(`AddInternal`)

```csharp
_arcComparer = ProjectionComparer<Arc<TData, TOffset>>.Create(arc => arc.PriorityType).Reverse();
...
private State<TData, TOffset> AddInternal(Arc<TData, TOffset> arc)
{
    int index = _arcs.BinarySearch(arc, _arcComparer);
    if (index < 0)
        index = ~index;
    _arcs.Insert(index, arc);
    return arc.Target;
}
```

**Verdict: CONFIRMED, not in the original 8-item list — F4 is the first milestone that needed raw
arc order** (F3's own structural-dump gate canonicalizes/sorts arc lines before comparing, per
`canon.rs`'s doc, which explicitly predicted this: "F4's own candidate-parity gate ... is an
independent backstop against this gap"). `FstTemplateAnalyzer.cs` never references
`ArcPriorityType`/`priorityType` anywhere (grepped: zero hits), so every arc it adds carries the
same implicit default priority, and every comparison `_arcComparer` ever makes here returns `0`
(tied). .NET's `List<T>.BinarySearch` (`lo=0,hi=count-1; i=lo+((hi-lo)>>1); if
Compare(arr[i],v)==0 return i;`) returns the FIRST probed midpoint the instant a tie is found — with
an all-tied comparer that is the very first comparison ever made per insert, so the insertion index
for the `k`-th arc added to one state (`k` = arcs already present at that state) reduces to the
closed form `0` if `k==0` else `(k-1)/2` (integer division). This reorders non-trivially from the
4th arc added onward and directly determines the WALK's candidate emission order (both walkers
iterate `state.Arcs` forward via the indexer, which maps straight to the reordered `_arcs[index]`).

**Empirical confirmation (not just derived):** implementing this exact closed form in Rust's
`trie.rs` (`arc_insert_index`, replacing plain `Vec::push` with `Vec::insert` at the computed
index) turned F4's Indonesian candidate gate from "112 lines each side, 3 lines out of order" into
byte-identical including line order — on the first attempt, no formula iteration needed. F3's own
gates (StateCount, canonical structural dump) are unaffected by this change (both are order-
independent by construction), confirming this quirk was invisible to every gate before F4 and is
real, not a coincidental reordering that happened to fix the diff.

## Selector read-site inventory (supports quirk #8 and the F1 selector-plumbing item)

Every C# read site of `Morpher.LexEntrySelector`/`Morpher.RuleSelector`, grepped directly (not
assumed) across `src/SIL.Machine.Morphology.HermitCrab/` (excluding compiled `bin`/`obj` binary
hits):

- `Morpher.cs:71-72` — defaults (`entry => true`, `rule => true`).
- `Morpher.cs:105-106` — the two public mutable properties.
- `Morpher.cs:370` — `LexicalLookup`'s `.Where(LexEntrySelector)` (the ONLY `LexEntrySelector` read
  site in the whole codebase).
- `MorpherPool.cs:37-38` — reset to `_ => true` on `Return` (pool hygiene between rentals).
- `FstReplay.cs:73-79` — the verify-time assignment (quirk #8, above).
- `AnalysisLanguageRule.cs:29` — `RuleSelector(_strata[i])`, gates whether a given stratum is even
  descended into on the analysis side.
- `AnalysisAffixTemplateRule.cs:34` — `RuleSelector(_template)`.
- `MorphologicalRules/AnalysisAffixProcessRule.cs:42`, `AnalysisCompoundingRule.cs:42`,
  `AnalysisRealizationalAffixProcessRule.cs:42` — one gate each, top of `Apply`.
- `MorphologicalRules/SynthesisRealizationalAffixProcessRule.cs:43` — synthesis-side realizational
  gate (the only `Synthesis*` MORPHOLOGICAL rule with a `RuleSelector` gate — plain
  `AffixProcessRule`/`CompoundingRule` synthesis has none, confirmed by grep: no hit in
  `SynthesisAffixProcessRule.cs`/`SynthesisCompoundingRule.cs`).
- `PhonologicalRules/AnalysisMetathesisRule.cs:40`, `AnalysisRewriteRule.cs:123`,
  `SynthesisMetathesisRule.cs:37`, `SynthesisRewriteRule.cs:53` — one gate each.
- `SynthesisAffixTemplatesRule.cs:36` — `RuleSelector(_templates[i])` in a synthesis-side template
  loop.
- `SynthesisStratumRule.cs:51` — `!_morpher.RuleSelector(_stratum) || input.RootAllomorph.Morpheme
  .Stratum.Depth > _stratum.Depth` (compound condition; the stratum gate is `||`-combined with a
  depth check, not a bare single-predicate gate like the others).

Total: 1 `LexEntrySelector` site, 13 `RuleSelector` sites (2 defaults/resets + 1 verify-assignment +
10 actual gate checks across analysis and synthesis, both morphological and phonological rule
kinds, plus the stratum-level gate on each side). This is the complete set the Rust `lex_entry_filter`/
`rule_filter` mechanism (§7.1 item 1) must mirror; not yet implemented as of this audit (F1's token.rs
+ scaffold milestone) — tracked as the next commit.

## rustfst evaluation (§7.0, bounded)

Per the plan's own framing ("expected answer: no, for the parity port"), confirmed rather than
assumed: [rustfst](https://github.com/garvys-org/rustfst)'s arc type (`Tr`) is labeled with concrete
`u32` symbols over a fixed `SymbolTable`, and its determinization/minimization/composition algorithms
are defined over that concrete-alphabet model. The hybrid's trie/walk is unification-arc
(`FeatureStruct`-labeled, matched via `IsUnifiable`, e.g. `CascadeSymbol`'s
`arc.SurfaceInput.IsUnifiable(symbol)` above) and deliberately never determinized/minimized
(`HYBRID_FST_FEASIBILITY.md` §5.2 — determinizing across unification arcs would merge genuinely
distinct analysis paths). rustfst cannot represent this arc model without first concretizing every
FeatureStruct to a symbol alphabet, which is exactly the "quotiented chain" idea the plan defers to
§7.0 item 3 / §12.5 (post-parity). **Conclusion: not a viable walker substrate for this port** — same
verdict the plan predicted, now backed by reading rustfst's own arc/symbol model rather than assumed.
Worth mining later regardless: its lazy/delayed composition machinery (state-pair caching, on-demand
arc expansion) is architecturally the same pattern this crate's future `walk.rs`
(`ChainClosure`/`CascadeSymbol`) implements by hand — a design reference, not a dependency, for F7.
