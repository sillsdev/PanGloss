# P13 — `RewriteMode::Simultaneous` port: design

Status: **design only** (plan `rust-optimizations-phase2.md` §P13, [FABLE-PLAN then SONNET]).
Decided in scope 2026-07-10 (Open scope decisions #4): "PORT IT, fully." John: "I want complete
grammar coverage for even hypothetical grammars that HC can parse. Create a synthetic oracle if
needed." No engine code changes accompany this doc — `pg-rules/src/rewrite.rs` and
`pg-grammar/src/load.rs` are read-only throughout this pass. Target implementer: a Sonnet-tier
agent working mechanically from §4-§5, using the fixtures in §6.

Oracle: `.worktrees/parse-opt/src/SIL.Machine.Morphology.HermitCrab/PhonologicalRules/
{RewriteRule,SynthesisRewriteRule,AnalysisRewriteRule,IterativePhonologicalPatternRule,
SimultaneousPhonologicalPatternRule,PhonologicalPatternRule,RewriteRuleSpec}.cs` (the live C#
engine, `.worktrees/parse-opt` @ working tree as of 2026-07-10) plus the live built tool
`.worktrees/parse-opt/src/SIL.Machine.Morphology.HermitCrab.Tool/bin/Release/net10.0/hc.dll`, used
throughout this pass to generate real oracle output (§6). Literal test oracle:
`RewriteRuleTests.MultipleApplicationRules` (RewriteRuleTests.cs:1809-1862) and
`RewriteRuleTests.EpenthesisRules` sub-case (1) (RewriteRuleTests.cs:1191-1202).

---

## 1. What the C# does, read end-to-end

### 1.1 The two pattern-rule classes

`RewriteRule.ApplicationMode` (`RewriteRule.cs:9-13`) is a bare enum, `Iterative` or `Simultaneous`,
set from the DTD's `multipleApplicationOrder` (`leftToRightIterative`/`rightToLeftIterative`
→ Iterative + a `Direction`; `simultaneous` → Simultaneous, direction fixed left-to-right —
`XmlLanguageLoader.cs:67-79`). Both `SynthesisRewriteRule` and `AnalysisRewriteRule` switch on it
to pick between two sibling `PhonologicalPatternRule` subclasses that share a base (`Matcher`,
`RuleSpec` — `PhonologicalPatternRule.cs`) but implement `Apply` completely differently:

```csharp
// IterativePhonologicalPatternRule.cs:17-55 (abridged)
public override IEnumerable<Word> Apply(Word input) {
    Match targetMatch = Matcher.Match(input);          // ONE match: leftmost from `start`
    while (targetMatch.Success) {
        if (RuleSpec.MatchSubrule(this, targetMatch, out srMatch)) {
            ShapeNode matchEndNode = GetEndNode(targetMatch.Range, ...);   // resolved BEFORE mutating
            srMatch.SubruleSpec.ApplyRhs(targetMatch, srMatch.Range, ...); // MUTATES input now
            start = matchEndNode.GetNext(direction);        // re-read AFTER mutation
        } else {
            start = GetStartNode(targetMatch.Range, ...).GetNext(direction);
        }
        if (start == null) break;
        targetMatch = Matcher.Match(input, MatchStartOffset(start, direction));  // re-match on the MUTATED input
    }
    ...
}

// SimultaneousPhonologicalPatternRule.cs:22-36 (complete)
public override IEnumerable<Word> Apply(Word input) {
    var matches = new List<Tuple<Match, PhonologicalSubruleMatch>>();
    foreach (Match targetMatch in Matcher.AllMatches(input))       // ALL matches against input AS-IS
        if (RuleSpec.MatchSubrule(this, targetMatch, out srMatch)) // env check also against input AS-IS
            matches.Add(Tuple.Create(targetMatch, srMatch));
    foreach (var match in matches)                                  // THEN apply every accepted one
        match.Item2.SubruleSpec.ApplyRhs(match.Item1, match.Item2.Range, match.Item2.VariableBindings);
    return input.ToEnumerable();
}
```

**The one fact this whole design pass exists to pin down:** Iterative finds one match, applies it
(mutating the shared `Word`), and *then* looks for the next match starting from just past the
mutation — so every match after the first is found and environment-checked against a
partially-rewritten shape. Simultaneous finds and environment-checks *every* candidate position in
one `AllMatches` pass over the untouched input, and only afterward applies all of them. A rewrite
under Simultaneous can never feed (enable) or bleed (disable) another match within the same rule
application; under Iterative it always can, because the cursor's re-`Match` call after each
application reads live, current node state.

Verified empirically (not just derived) against `RewriteRuleTests.MultipleApplicationRules`, ported
as this task's `rewrite/simultaneous-feeding` / `simultaneous-feeding-control-iterative` fixtures
(§6.1) — `gigugu` parses only under Simultaneous, `gigugi` only under Iterative, on the *same* rule.

`Matcher.AllMatches` (`Fst.cs:301-327`, private `Transduce(..., allMatches: true, ...)`) tries a
match starting at *every* annotation position in the input in increasing order
(`Fst.cs:348-399`'s `while (annIndex < ...)` loop with no post-match skip-ahead), so distinct
matches can in principle be adjacent or overlap; results are `Distinct()`-deduped only to collapse
multiple *arcs* reaching the same match at nondeterministic FSAs, never to enforce non-overlap
across different start positions. None of this task's fixtures needed an overlapping-target case
to make the point (single-node targets sufficed), but the design must not assume non-overlap.

### 1.2 Subrule dispatch is a single shared scan, not one scan per subrule

`RewriteRuleSpec.Pattern` (built once per rule in `SynthesisRewriteRuleSpec`'s/
`FeatureAnalysisRewriteRuleSpec`'s constructor from `rule.Lhs`) is the **one** target pattern the
**one** `Matcher` on a `PhonologicalPatternRule` matches against. A rule's multiple
`RewriteSubrule`s do not each get their own independent match pass — `RewriteRuleSpec.MatchSubrule`
(`RewriteRuleSpec.cs:37-123`) is called once per *accepted target match* and internally loops over
`_subruleSpecs` **in declaration order**, returning the first subrule whose left/right environment
also holds. So: one shared scan discovers *where* the rule's LHS matches; per matched position, the
first environment-satisfying subrule (in list order) wins and is the only one applied there. This
holds identically for both Iterative and Simultaneous — the difference in §1.1 is entirely about
*when* matches/environments are (re-)computed relative to mutation, not about how subrules are
selected.

This matters for the Rust port specifically because **Rust's current `synthesize_with_mpr` loops
`for sr in &rule.subrules`, running each subrule to its own completion (its own internal
find-leftmost-then-rescan loop) before moving to the next subrule** (`pg-rules/src/rewrite.rs:
893-917`) — a different shape from C#'s single-scan-then-per-position-dispatch. For **Iterative**
mode this is an accidental-but-real equivalence: subrule 1's pass only ever touches positions where
subrule 1's own environment holds, marking them `dirty`; subrule 2's pass then only considers
not-yet-`dirty` positions, so the net effect is "subrule 1 wins wherever it applies, subrule 2 gets
whatever's left" — the same outcome as C#'s per-position first-match-wins, given subrules are
processed in declaration order. **This equivalence does NOT automatically carry over to a new
Simultaneous execution path** if that path is implemented as "one collect-then-apply pass per
subrule" (the natural-looking generalization of the existing per-subrule loop) instead of "one
collect-then-apply pass across the WHOLE rule, with per-position first-applicable-subrule dispatch."
A grammar with two Simultaneous subrules whose environments overlap at the same target position
would silently diverge under the naive per-subrule design (C# applies only the first subrule there;
a naive port might apply both, or apply the second because the first subrule's own snapshot-based
pass doesn't mark anything dirty before the second subrule's snapshot is taken). See §4.1's warning
and §7's open risk — **no fixture in this pass exercises multi-subrule Simultaneous disjunction**;
it is a named requirement, not an empirically-covered one.

### 1.3 Analysis side: the mode's effect is much narrower than synthesis's

This is the single most important simplification this design pass found, and it changes the
implementation's shape substantially. `AnalysisRewriteRule`'s constructor (`AnalysisRewriteRule.cs:
26-104`) dispatches per subrule on **shape** (`Lhs.Children.Count` vs `sr.Rhs.Children.Count`), not
directly on `rule.ApplicationMode`:

```csharp
var mode = RewriteApplicationMode.Iterative;      // default, most branches never touch this
var reapplyType = ReapplyType.Normal;
if (Lhs.Count == Rhs.Count) {                     // Feature
    ruleSpec = new FeatureAnalysisRewriteRuleSpec(...);
    if (rule.ApplicationMode == Simultaneous)
        foreach (rhsSegmentConstraint)
            if (!IsUnifiable(constraint, sr.LeftEnvironment) || !IsUnifiable(constraint, sr.RightEnvironment))
                { reapplyType = SelfOpaquing; break; }   // mode STILL stays Iterative
} else if (Lhs.Count > Rhs.Count) {               // Narrow / deletion
    ruleSpec = new NarrowAnalysisRewriteRuleSpec(...);
    mode = Simultaneous;                          // ALWAYS, regardless of rule.ApplicationMode
    reapplyType = Deletion;
} else if (Lhs.Count == 0) {                      // Epenthesis
    ruleSpec = new EpenthesisAnalysisRewriteRuleSpec(...);
    if (rule.ApplicationMode == Simultaneous)
        reapplyType = SelfOpaquing;                // mode STILL stays Iterative
} else {                                          // Expansion (0 < Lhs.Count < Rhs.Count)
    ruleSpec = new NarrowAnalysisRewriteRuleSpec(...);  // "works for expansion, too"
    mode = Simultaneous;                          // ALWAYS
    reapplyType = Deletion;
}
patternRule = mode == Iterative ? new IterativePhonologicalPatternRule(ruleSpec, settings)
                                 : new SimultaneousPhonologicalPatternRule(ruleSpec, settings);
```

Then `AnalysisRewriteRule.Apply` (`cs:122-196`) dispatches on `reapplyType`:
- **Normal** (Feature, mode unifiable with environment, or `ApplicationMode==Iterative`): call
  `patternRule.Apply(input)` exactly once.
- **SelfOpaquing** (Feature/Epenthesis, only when `rule.ApplicationMode==Simultaneous` and — for
  Feature only — the extra unifiability precheck above trips): repeat
  `patternRule.Apply(...)` — itself always the (internally full-shape-sweeping) *Iterative* pattern
  rule — on the previous result, until a call makes no change. This is a fixpoint wrapper around an
  Iterative sweep, not a Simultaneous pattern rule at all.
- **Deletion** (Narrow/Expansion, unconditionally): repeat the (always *Simultaneous*) pattern
  rule's `Apply` up to `1 + Morpher.DeletionReapplications` times (default 0 ⇒ exactly once).

**Consequence:** `rule.ApplicationMode` has **zero effect on which `PhonologicalPatternRule`
analysis uses** for Narrow/Expansion subrules (always Simultaneous) and **zero effect on which one
it uses** for Feature/Epenthesis subrules (always Iterative) — its only analysis-side effect is
whether Feature/Epenthesis get a fixpoint-repeat wrapper. Synthesis, by contrast, honors
`rule.ApplicationMode` directly and uniformly across all three subrule kinds
(`SynthesisRewriteRule.cs:40-49`). This asymmetry is real, in the live source, and reshapes the
implementation plan (§4.3-4.4): most of the "Simultaneous" *matching mechanism* the analysis side
needs already has a home (Narrow's `SimultaneousPhonologicalPatternRule` never varies with the
rule's tag), while what the rule's tag actually gates on the analysis side is a repeat-loop, not a
different matcher.

### 1.4 The `IsUnifiable` self-opaquing precheck (Feature analysis only)

`AnalysisRewriteRule.cs:106-120`'s `IsUnifiable` checks whether each RHS segment constraint's
feature struct is unifiable with every `Segment`-typed node in the subrule's own left/right
environment patterns. If **any** RHS constraint is *not* unifiable with either environment, the
rule is "self-opaquing": its own output could, in principle, satisfy or reshape a nearby match, so
repeated reapplication is needed to reach a fixpoint. If every RHS constraint unifies cleanly with
both environments, one pass suffices (the rule's output can never create or destroy an adjacent
match) and `reapplyType` stays `Normal` even under `Simultaneous`. This is a **static,
compile-time** check over the rule's own patterns (no per-word state) — port it as a
`self_opaquing: bool` computed once per (rule, subrule) pair at grammar-load or rule-cache-build
time, not per parse.

---

## 2. Current Rust state

### 2.1 The load-time lint

`pg_grammar::load::load_rewrite_rule` (`pg-grammar/src/load.rs:1053-1076`) parses
`multipleApplicationOrder` and, if it is exactly `"simultaneous"`, immediately returns
`GrammarError::Unsupported("PhonologicalRule multipleApplicationOrder=\"simultaneous\"")` — a
deliberate stopgap (W1.4) chosen over silently running Simultaneous-tagged grammars as Iterative.
`RewriteMode` (`pg-grammar/src/model.rs:339`, `Simultaneous`/`Iterative`) and `RewriteRuleDef.mode`
already exist and already round-trip correctly for the *rejected* value (confirmed by the existing
unit test `rewrite_mode_simultaneous_lints_unsupported`, `load.rs:2540+`) — only the lint itself
needs removing once execution exists (§4.5).

### 2.2 `pg-rules/src/rewrite.rs`'s existing shapes — narrower gap than expected

Reading every rewrite-execution function against the C# read above:

| Function | Current shape | C# equivalent shape | Gap for Simultaneous |
|---|---|---|---|
| `syn_feature` (1104) | `loop { rescan; find first match; apply; break }` — re-derives `ms.segs()` after every single application | `IterativePhonologicalPatternRule` exactly | **Real gap.** Needs a sibling that computes all matches once, applies once. |
| `syn_narrow` (1467) | same re-scan-per-application shape | same | **Real gap**, same shape as above. |
| `syn_epenthesis` (1748) | collects all sites in ONE call against one `ms.segs()` snapshot, applies all (descending) — **no outer re-scan loop at all** | — | **Already Simultaneous-shaped**, mislabeled as serving both modes. See §2.3 — this is a pre-existing, orthogonal finding this pass surfaced, not something P13 needs to fix to add mode support, but a correctness gap for a *faithful Iterative* path if bug-for-bug parity is later wanted (§7). |
| `ana_feature` (1260) | same re-scan-per-application loop shape as `syn_feature` | `IterativePhonologicalPatternRule`, always (§1.3) | **No gap for the matcher itself** — C# also always uses Iterative here. The gap is purely the *outer* SelfOpaquing repeat wrapper (§1.3/§1.4), entirely absent today. |
| `ana_narrow_deletion` (1578) | collects all sites in one pass against one snapshot, applies all (descending) | `SimultaneousPhonologicalPatternRule`, **always**, regardless of `rule.mode` (§1.3) | **No gap.** Already exactly right, unconditionally — this is *why* the two ignored tests this P13 unblocks were only blocked by the *load-time lint*, not by wrong analysis semantics. |
| `ana_narrow_general` (1654) | same one-pass-collect-then-apply-descending shape | same, always | **No gap**, same as above. |
| `ana_epenthesis` (1810) | one pass, one snapshot, applies all matches that are "nonvacuous" (not already fully Optional) | `EpenthesisAnalysisRewriteRuleSpec` inside the always-Iterative pattern rule, plus the SelfOpaquing outer wrapper when `rule.mode==Simultaneous` (§1.3) | **Partial gap**: the per-call matching shape already happens to look like one Simultaneous-style pass (fine for a single call), but the **outer repeat-until-fixpoint wrapper is entirely missing** — `analyze`/`analyze_cached` call each of `ana_feature`/`ana_epenthesis` exactly once, with no loop. |

The practical upshot: **the synthesis side needs two genuinely new functions** (`sim_feature`,
`sim_narrow` — §4.2), while **the analysis side needs no new matching functions at all**, only (a)
the load-time lint removed, and (b) a small outer repeat-wrapper added around `ana_feature`/
`ana_epenthesis`, gated by a new `self_opaquing`/`mode` field read (§4.3-4.4). This asymmetry
mirrors §1.3's C# asymmetry exactly, and is why this design doc's implementation plan (§5) is much
smaller on the analysis side than a naive "port Simultaneous" framing would suggest.

### 2.3 A related, pre-existing, orthogonal finding: `syn_epenthesis` is not faithfully Iterative

Verified empirically as part of building the `rewrite/simultaneous-epenthesis-cascade` fixture
(§6.2): a hand-designed epenthesis rule whose own output re-satisfies its own trigger environment
(insert an HFU vowel after any high vowel — the inserted vowel is itself high) causes the live C#
oracle, under **Iterative** mode, to crash with an uncaught `InfiniteLoopException` (`
EpenthesisSynthesisRewriteSubruleSpec.cs`'s 256-node safety cap) — because Iterative's cursor
resumes matching at the just-inserted node (§1.1), which re-satisfies the same environment, forever.
Running the *identical* grammar (mode attribute stripped, so it loads and runs on today's Rust
engine) through `pangloss batch` produces a clean, instant `-` (no parse), no crash, no hang: today's
`syn_epenthesis` cannot cascade, because it collects all sites against one snapshot before applying
any of them (§2.2's table). This means today's default (and only) epenthesis synthesis path is
already effectively Simultaneous-shaped, not Iterative-shaped — a latent, pre-existing divergence
from C# that no reference-corpus grammar happens to trigger (none has a self-referential epenthesis
environment), surfaced here only because this pass specifically went looking for one. Flagged as an
explicit, named scope decision for the implementer (§7), not silently absorbed into "Simultaneous
epenthesis is free, reuse `syn_epenthesis` as-is" — reusing it as-is for *Simultaneous* mode is
in fact correct (§4.2); the gap is only in what a *faithful Iterative* epenthesis path would need.

---

## 3. A confirmed bug in the C# oracle's own nogood-cache (not a fixture-construction artifact)

While building the `rewrite/simultaneous-epenthesis` fixture (§6.3), this pass found that the live
C# oracle's **default** parse path (used by `BatchCommand` and any non-tracing caller) disagrees
with its own **traced** path on the exact same grammar and word: `Morpher.ParseWord`
(`Morpher.cs:240-253`) only installs `AnalysisScope`'s nogood-memoization cascade when *not*
tracing, and for this grammar the memoized path fails to find a parse the unmemoized (traced) path
finds cleanly.

This was **not** taken at face value. The first hypothesis (raised on review) was that the fixture
itself was unfaithful to the real, passing C# unit test it was ported from — a real concern, since
an earlier draft of this fixture *was* unfaithful (single stratum instead of two; the wrong root
shape, missing root 19's actual `"b+ubu"` morpheme boundary; a table missing the boundary character
entirely). Chasing that hypothesis down fully is what actually found those three real construction
bugs (§6.3 documents each). But after fixing all three — the grammar now matches every structural
detail of `HermitCrabTestBase.cs` this pass could find — the divergence **persisted**. Three
independent checks then confirmed it is a genuine oracle bug, not a remaining fixture defect:

1. The real NUnit test (`RewriteRuleTests.EpenthesisRules`, run directly via `dotnet test`, default
   non-tracing `TraceManager`) passes.
2. A from-scratch in-memory reconstruction of the identical scenario — same two-stratum split,
   same boundary-bearing table, same rule built via the same `NaturalClass`/`SimpleContext`/
   `Constraint` construction path the XML loader itself uses, in a standalone program that only
   *references* the already-built oracle library (no oracle source modified) — also succeeds
   non-traced.
3. The SAME loaded `grammar.xml` `Language` object, run through two `Morpher`s differing only in
   `TraceManager.IsTracing`, gives different answers (0 vs. 1 result) for `"buibui"` specifically;
   the fixture's other two words give the same (correct) answer either way, so this is not a
   blanket "tracing changes everything" effect.

The failure is specific to the Simultaneous-mode `SelfOpaquing` reapplication path for epenthesis
(§1.3) — never exercised by any reference-corpus grammar, hence never caught before. The *exact*
mechanism (what the nogood cache keys on, and why it over-prunes here) was not isolated within this
pass despite substantial bisection (table sharing vs. separate tables, feature-system richness,
natural-class construction style, stratum count all ruled out as the differentiator) — this remains
a named open question (§7), not a solved one. Full transcripts and the bisection trail are in
`rust/conformance/rewrite/simultaneous-epenthesis/README.md`.

This is a finding about the **oracle**, not a Rust requirement, but it directly bears on the Rust
implementation: Rust's own analysis path has its own memo/nogood machinery (parse-optimization.md
Phases 2/9/10). **Whoever implements the analysis-side `SelfOpaquing` repeat-wrapper (§4.4) should
specifically test it against Rust's own memo cache using a case shaped like this fixture**, to
confirm Rust's memoization does not reproduce the same class of soundness gap — even without
knowing the C# mechanism's exact trigger, the *shape* of the risk (a repeat-until-fixpoint
reapplication loop interacting badly with memoization) is precise enough to test for directly.
Because three independent lines of evidence agree the correct answer is `19` (not `-`), this
fixture's `expected.tsv` freezes the **traced/correct** signature for the affected row rather than
the default path's buggy output — see §6.3 and that fixture's README for the exact value and the
principle ("never freeze a value known to be semantically wrong") behind that choice.

---

## 4. Design

### 4.1 Synthesis: `sim_feature` / `sim_narrow` (new), `syn_epenthesis` (reused)

Two new functions parallel to `syn_feature`/`syn_narrow`, sharing their helper machinery
(`node_pins`, `resolve_bindings`, `pattern_defaults_ok`, `all_spans`, `width_matches`) but with a
different top-level loop shape:

```
fn sim_feature(g, table, rule, sr, ms, target, left, right) -> bool {
    let (segs, node_of) = ms.segs(true);                 // ONE snapshot, before any mutation
    let mut accepted = Vec::new();
    for (s, e) in all_spans(target, &segs) {              // every candidate span against that snapshot
        ... same width/dirty/environment/binding/defaults checks as syn_feature, but reading
            only the snapshot's segs/node_of, never ms.nodes directly for gating ...
        if all checks pass { accepted.push((target_nodes, bindings)); }
    }
    if accepted.is_empty() { return false; }
    for (target_nodes, bindings) in accepted {             // THEN apply every accepted one
        ... identical per-node rewrite body to syn_feature's applying loop ...
    }
    true
}
```

`sim_narrow` is the analogous transform of `syn_narrow`'s body: collect all accepted spans against
one snapshot, then splice/delete for all of them in **descending** node-index order (the existing
`ana_narrow_general`/`ana_narrow_deletion` convention for exactly this reason — earlier splices
must not invalidate not-yet-applied later matches' captured indices). Note the applying order
matters here in a way it does not for `syn_feature` (feature rewrites mutate node contents in place,
never indices) — get this from the already-proven analysis-side pattern, not by re-deriving it.

`syn_epenthesis` needs **no new function** for Simultaneous mode — its existing one-snapshot-
collect-then-apply-descending shape (§2.2, §2.3) already matches
`SimultaneousPhonologicalPatternRule`'s semantics exactly. Dispatch `Kind::Epenthesis` to the same
`syn_epenthesis` regardless of `rule.mode` (only `Kind::Feature`/`Kind::Narrow` branch on mode).

**Warning carried over from §1.2:** all of the above, as sketched, still operates **per subrule**
(one call per `sr` in the rule's subrule list), matching Rust's existing per-subrule-outer-loop
architecture. This reproduces C#'s per-position first-subrule-wins semantics correctly **only** for
the common case (subrules that never have overlapping candidate positions — true for every
single-subrule rule and every reference-grammar rule seen so far). A rule with multiple Simultaneous
subrules whose target+environment can both hold at the *same* position needs the collect-then-apply
snapshot to be taken **once across all subrules of the rule**, with per-position dispatch to the
first subrule (in declaration order) whose environment holds — mirroring `MatchSubrule`'s inner
loop exactly (§1.2). Implementer: before wiring `sim_feature`/`sim_narrow` into
`synthesize_with_mpr`'s per-subrule loop, write a multi-subrule-disjunctive-Simultaneous test first
(TDD) modeled on the existing `conformance/rewrite/disjunctive/` fixture (iterative-only today) —
if it passes trivially under the per-subrule design, that's fine (no overlap in that grammar); if
you cannot construct a case where it *would* matter, still leave a comment recording that this was
checked, not merely assumed. Do not skip this check silently.

### 4.2 Dispatch: `RewriteRuleDef.mode` selects the function pair, per `Kind`

In `synthesize_with_mpr`/`synthesize_with_mpr_cached`'s existing `match classify(rule, sr)`:

```rust
let did = match (classify(rule, sr), rule.mode) {
    (Kind::Feature, RewriteMode::Iterative)    => syn_feature(...),
    (Kind::Feature, RewriteMode::Simultaneous) => sim_feature(...),
    (Kind::Narrow,  RewriteMode::Iterative)    => syn_narrow(...),
    (Kind::Narrow,  RewriteMode::Simultaneous) => sim_narrow(...),
    (Kind::Epenthesis, _)                      => syn_epenthesis(...),  // both modes, see §4.1
};
```

`RuleCache`/`PruleCache` (`pg-rules/src/cache.rs`, referenced by `synthesize_with_mpr_cached`) needs
no new compiled-artifact fields — `sim_feature`/`sim_narrow` reuse the exact same compiled
`target`/`left`/`right` FSTs `syn_feature`/`syn_narrow` already use; only the *driving loop* differs
per mode, not what gets compiled. Confirm this when implementing (no cache schema change expected).

### 4.3 Analysis: `self_opaquing` as a loaded/cached fact, not a per-parse computation

Add `RewriteSubruleDef.self_opaquing: bool` (Feature subrules only; irrelevant/`false` for
Narrow/Expansion, which are unconditionally Simultaneous+Deletion-reapply regardless — §1.3), and a
rule-level `RewriteRuleDef.epenthesis_self_opaquing` equivalent is unnecessary — for Epenthesis
(`Lhs.Children.Count == 0`), C# sets `reapplyType = SelfOpaquing` **unconditionally** whenever
`rule.ApplicationMode == Simultaneous` (`AnalysisRewriteRule.cs:75-80`; no unifiability precheck for
this branch, unlike Feature's). So the gating fact needed is:

- Feature subrule: `self_opaquing = rule.mode == Simultaneous && !all_rhs_pins_unifiable_with_envs`
  — compute the unifiability half once at grammar-load or rule-cache-build time (§1.4), matching
  `IsUnifiable` (`AnalysisRewriteRule.cs:106-120`) against `node_pins`'s already-computed RHS pins
  and the subrule's own left/right environment `Pattern` nodes (not the compiled FST — this check
  is over the *pattern*, done once, not per match).
- Epenthesis subrule: `self_opaquing = rule.mode == Simultaneous` (no additional check).
- Narrow/Expansion subrule: irrelevant field — always goes through the existing
  `ana_narrow_deletion`/`ana_narrow_general` unconditionally (§2.2 — no change needed there at all).

### 4.4 Analysis: the repeat-until-fixpoint wrapper

In `analyze`/`analyze_cached`, wrap the `Kind::Feature`/`Kind::Epenthesis` calls:

```rust
let did = match classify(rule, sr) {
    Kind::Feature if sr.self_opaquing => {
        let mut any = false;
        while ana_feature(g, table, rule, sr, ms, &target, &names, &left, &right) { any = true; }
        any
    }
    Kind::Feature => ana_feature(...),               // unchanged, Normal reapply
    Kind::Epenthesis if sr.self_opaquing => {
        let mut any = false;
        while ana_epenthesis(ms, target.as_ref(), sr.rhs.nodes.len(), &left, &right) { any = true; }
        any
    }
    Kind::Epenthesis => ana_epenthesis(...),         // unchanged
    Kind::Narrow => { /* unchanged: already unconditionally the right shape, §2.2 */ }
};
```

This matches C#'s `while (data != null) { applied = true; data = sr.Item2.Apply(data)
.SingleOrDefault(); }` exactly: repeat calling the (unchanged) single-pass function until a call
makes no further change. Both `ana_feature` and `ana_epenthesis` already return `bool` ("did
anything change"), so the `while` condition falls out directly — no new return type needed.
`Morpher.DeletionReapplications`'s cap (default 0, already a documented Rust gap per
`csharp_port_rewrite.rs`'s `deletion_rules_multi_position_reinsertion` doc) does not apply here —
that cap is specific to the *Deletion* reapply type (Narrow/Expansion), not SelfOpaquing; do not
conflate the two loops or their caps. SelfOpaquing's own natural termination is "no further match" —
confirm there is no separate C# iteration cap for it (none found in `AnalysisRewriteRule.cs`; the
loop is genuinely unbounded, matching `IsUnapplicationNonvacuous`'s job of guaranteeing progress
each iteration — `ana_feature`'s existing `nonvacuous` check and `ana_epenthesis`'s existing
"skip already-fully-Optional" check already provide this termination guarantee, so no new step-cap
plumbing should be needed here beyond whatever global step/word-timeout budget the engine already
enforces).

### 4.5 Loader change

`load_rewrite_rule` (`pg-grammar/src/load.rs:1053-1076`): remove the `if mult == "simultaneous"`
early-return block entirely; the existing `match mult { "simultaneous" => RewriteMode::Simultaneous,
_ => RewriteMode::Iterative }` below it already does the right parse once the lint is gone. Update/
remove `rewrite_mode_simultaneous_lints_unsupported` (`load.rs:2540+`) to instead assert the mode
loads correctly and round-trips into `RewriteRuleDef.mode` (do not just delete the test — replace
its assertion). No DTD change (the grammar-authoring surface is unchanged; only Rust's own
acceptance of it changes).

---

## 5. Ordered implementation plan (landable chunks)

1. **`pg-grammar`: remove the load-time lint** (§4.5). Inert by itself — `RewriteRuleDef.mode` is
   already threaded everywhere it's stored; nothing downstream reads it yet, so this alone changes
   no behavior except that Simultaneous-tagged grammars now load (and then silently misexecute as
   Iterative, exactly the W1.4 stopgap's original documented risk) — **land this together with at
   least step 2**, not alone, to avoid a silent-misexecution window.
2. **`pg-rules`: `sim_feature`/`sim_narrow`** (§4.1-4.2), dispatched by `rule.mode` in
   `synthesize_with_mpr`/`synthesize_with_mpr_cached`. Write the multi-subrule-disjunction check
   (§4.1's warning) as a test *before* wiring the dispatch. Gate tests:
   `rewrite/simultaneous-feeding` + `simultaneous-feeding-control-iterative` (§6.1) must both pass
   byte-identically; the existing `epenthesis_rules`/`multiple_application_rules` `#[ignore]`d
   tests (`pg-parse/tests/csharp_port_rewrite.rs`) should be re-examined (some sub-cases may still
   have unrelated findings — re-verify each, don't assume un-ignoring is automatic).
3. **`pg-grammar`/`pg-rules`: `self_opaquing`** (§4.3) — computed at load or rule-cache-build time,
   stored on `RewriteSubruleDef`/wherever the cache keys per-subrule facts today. Unit-test the
   `IsUnifiable`-equivalent computation directly against hand-built patterns (mirror C#'s own
   logic, don't just eyeball agreement).
4. **`pg-rules`: the analysis repeat-wrapper** (§4.4) in `analyze`/`analyze_cached`. Gate test:
   `rewrite/simultaneous-epenthesis` (§6.3) — **expect this to reproduce the frozen `-` results**
   (matching the live oracle's default, memo-affected output per §3), not the "more correct" traced
   result; if Rust's own engine instead finds the parse the traced C# path finds, that is a
   **known, acceptable, and arguably better** divergence — flag it explicitly in the test's own doc
   comment rather than either silently accepting or silently "fixing" Rust to match the buggy
   oracle output.
5. **Un-ignore + re-verify** `epenthesis_rules`/`multiple_application_rules`
   (`csharp_port_rewrite.rs`) and any other test whose ignore note cites W1.4/the Simultaneous lint
   specifically. Re-check each sub-case against its own doc comment — some (e.g. `anchor_rules`'s
   sub-case (1)) have unrelated, still-open findings layered on top; do not assume a green run
   means every historical note is now moot.
6. **Wire the 4 fixtures from this pass** into `pg-parse/tests/rewrite_conformance.rs` (same
   convention as `merge_matches_oracle` etc.) — `simultaneous-feeding-control-iterative` can be
   wired **immediately, independent of steps 1-5** (it's a plain Iterative grammar, loads today).
7. **Corpus regression**: full Indonesian/Sena/Amharic runs before/after must be byte-identical —
   none of the 3 reference grammars uses `multipleApplicationOrder="simultaneous"` (W9.3), so this
   is a pure no-op-on-real-corpora check, but run it anyway per this plan's standing acceptance
   gates.

Estimated total: **M** (steps 2 and 4 are the bulk; step 1 is trivial; steps 3, 5, 6 are each S).

---

## 6. Synthetic oracle fixtures built in this pass

All four live under `rust/conformance/rewrite/`, each with `grammar.xml` + `words.txt` +
`expected.tsv` (real `hc.dll` output, verified 2026-07-10) + a README documenting the derivation,
the exact generating command, and — for two of them — an additional live-oracle finding beyond the
original Simultaneous-vs-Iterative question. **None can be replayed against Rust yet** (the mode
isn't implemented); `simultaneous-feeding-control-iterative` is the one exception (plain Iterative,
loads and runs today — wire it immediately per §5 step 6).

### 6.1 `simultaneous-feeding` + `simultaneous-feeding-control-iterative`

Direct port of `RewriteRuleTests.MultipleApplicationRules`. Proves §1.1's headline algorithmic fact
in both directions on one rule: `gigugu` parses under Simultaneous but not Iterative; `gigugi`
parses under Iterative but not Simultaneous. Both outcomes independently oracle-verified (2 grammar
variants × 3 words each). This is the primary, cleanest, highest-confidence fixture — no
complications, no open questions, a real C# unit test transcribed rather than hand-invented.

### 6.2 `simultaneous-epenthesis-cascade`

Hand-designed (not from a C# test): an epenthesis rule whose own RHS output re-satisfies its own
trigger environment. Proves the *other* direction of §1.1's fact is not just a subtle
ordering difference but a genuine **safety property**: under Iterative, this rule provably crashes
the live oracle with an uncaught `InfiniteLoopException` (real transcript in the README); under
Simultaneous it structurally cannot cascade (matches are all computed against one pristine
snapshot, so a freshly-inserted node is never itself a candidate). Also surfaced §2.3's orthogonal
finding: Rust's current (Iterative-labeled, only) `syn_epenthesis` already behaves like the
Simultaneous case here (no crash, no cascade) — a pre-existing gap in Iterative-epenthesis fidelity
that this design's fixture is the first to actually distinguish, flagged as an explicit scope
decision for the implementer (§7), not silently folded into "cascade behavior is out of scope."

### 6.3 `simultaneous-epenthesis`

Direct port of `RewriteRuleTests.EpenthesisRules` sub-case (1), tagged Simultaneous, run against a
3-word list. Intended to be the positive-parse counterpart to 6.2 (same trigger rule, no cascade
risk under Simultaneous, full word-level round trip). Getting there took three real construction
fixes over earlier drafts (two strata, not one; root 19's actual `"b+ubu"` shape with its morpheme
boundary, not a simplified `"bubu"`; a boundary-bearing table for the root's own stratum, separate
from the rule's stratum's table) — each found by comparing against `HermitCrabTestBase.cs` and each
individually necessary, none sufficient. Even with all three applied, this exact grammar, loaded
the ordinary way and parsed with tracing off (i.e. exactly how `hc.dll batch` operates), still
returns zero results for `"buibui"` — confirmed (§3) to be a genuine, reproducible oracle bug via
three independent checks (the real NUnit test passing; a from-scratch in-memory reconstruction
succeeding; the same loaded grammar object flipping from 0 to 1 result purely on
`TraceManager.IsTracing`), not a remaining defect in this fixture. `expected.tsv` freezes the
traced/correct signature for the affected row (`|b+?uibui`) rather than the default path's buggy
`-`, with the deviation from the standard "generate via plain `batch`" convention documented
explicitly in the fixture's own README, per the principle that a conformance fixture must never
freeze a value known to be semantically wrong.

### 6.4 What was deliberately not built

A dedicated multi-subrule-disjunctive-Simultaneous fixture (§1.2/§4.1's warning) was not built in
this pass — the risk is real and named, but constructing a clean, hand-verifiable case (and
confirming empirically which of several plausible C# outcomes actually happens, per this pass's own
"verify empirically, don't assume" discipline) would have meaningfully extended this already-long
design pass without a proportionate design-doc payoff; it is called out in §4.1/§5 step 2 as a
required TDD step for the implementer instead of a fixture handed down in advance. A Narrow/
Expansion-specific Simultaneous fixture was also not built — §2.2 establishes by direct code
reading that this path needs no new Rust code at all (already unconditionally correct), so an
oracle fixture there would confirm the loader-lint removal alone, not any new execution logic;
`deletion_rules_multi_position_reinsertion` (already green, already oracle-fixtured via
`rewrite/deletion-reinsertion/`) already exercises this exact mechanism end-to-end.

---

## 7. Open questions / risks for the implementer

1. **Multi-subrule Simultaneous disjunction** (§1.2, §4.1) — no fixture covers it; write one (TDD)
   before wiring `sim_feature`/`sim_narrow` into the per-subrule dispatch loop, per §5 step 2.
2. **Faithful-Iterative epenthesis cascade** (§2.3, §6.2) — today's `syn_epenthesis` cannot
   reproduce C#'s self-feeding-cascade-then-`InfiniteLoopException` behavior for a genuinely
   self-referential Iterative epenthesis rule. No reference grammar needs this. Decide and document
   (not silently skip) whether this is an accepted permanent scope cut or a follow-up ticket —
   it predates and is independent of this P13 work, merely surfaced by it.
3. **Nogood-cache soundness under `SelfOpaquing`** (§3, §6.3) — test Rust's own memo cache against
   the `simultaneous-epenthesis` fixture's shape specifically; do not assume Rust's cache is immune
   just because it uses different memo keys than C#'s `AnalysisScope`. Note this is a **confirmed**
   oracle bug (three independent checks, §3), not a hypothesis — the open part is only the exact
   C# trigger mechanism, which this pass could not isolate despite substantial bisection.
4. **The exact C# nogood-cache trigger** (§3, §6.3) — bisection ruled out table sharing vs.
   separate tables, feature-system richness, natural-class construction style, and stratum count as
   the differentiator between the failing (loaded-XML) and succeeding (in-memory-constructed) paths
   for the *same* logical grammar. If anyone wants to chase this further in the oracle worktree, the
   `simultaneous-epenthesis` fixture's README documents the exact reproduction and the diagnostic
   harness approach (a standalone project referencing the built oracle library, not modifying it).
5. **`RewriteSubruleDef.self_opaquing` storage location** (§4.3) — this doc assumes it's computed
   once (load or cache-build time) and stored per-subrule; confirm against `pg-grammar`'s existing
   convention for similar per-subrule derived facts (e.g. how `required_pos`/`required_mpr` are
   already stored) rather than inventing a new pattern.
