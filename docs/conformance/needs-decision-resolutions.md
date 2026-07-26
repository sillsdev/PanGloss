# NEEDS-DECISION resolutions

Resolves the two rows `openspec/changes/plan-construct-coverage-completion/design.md` §D2 marked
**NEEDS-DECISION** (rows 8 and 14), plus §D6's oracle-re-verification-policy question that was
explicitly held for the same decision point (§D7 step 4).

Both rows resolve to **PROVABLE — build it**. Neither becomes a carve-out. In both cases the row's
stated blocker turned out to rest on a premise that does not survive checking the code: one assumed a
construction would have to be written from scratch (it does not — an existing construction in this
repo transfers directly), the other assumed unbounded semantics might be structurally inexpressible
(it is not — the FST backend has a native, exact construction for it).

Decided 2026-07-25. Standing authorization: "Make your own decisions on these — I want everything."

---

## Row 14 — `QuantifierPattern`, unbounded sub-split (`max == -1`)

**Open question as recorded:** is true-unbounded quantifier compilation (foma's native Kleene star, no
cutoff) structurally infeasible for some reason not yet written down, or simply unattempted?

**Resolution: simply unattempted. Build it.**

### Evidence

1. **The FST backend has a native, exact unbounded construction.** `nfst-xre` parses `E^>N`
   (`RepeatNPlus`, `nfst-xre-0.1.0/src/ast.rs:73`) and `E*` (`Token::Star`), and `foma` builds them as
   `concat(concat_n(net, N), kleene_plus(net))` (`foma-0.4.2/src/regex.rs:258-268`) and
   `fsm_kleene_star(fsm_minimize(net))` (`regex.rs:302`) respectively. Both are finite-size nets over a
   genuinely infinite regular language — there is no cutoff anywhere in either construction. So
   `min = N, max = -1` lowers exactly: `[inner]*` when `N == 0`, `[inner]^>{N-1}` when `N >= 1`.
2. **`lower.rs`'s refusal is a scope line, not a feasibility finding.** `slots_from_nodes`'s
   `PatternNode::Quantifier` arm (`rust/crates/pg-foma/src/lower.rs:303-309`) returns `None` for
   `max == None` with the comment "ADR 0001: a finite cutoff must never masquerade as unbounded
   semantics." That reasoning is sound *against clamping* — it rules out silently rewriting `max=-1`
   as `max=512`. It says nothing against emitting an actually-unbounded net, which is precisely what
   ADR 0001 would prefer. The refusal came out of `compile-bounded-fst-quantifiers`, a change whose own
   title scoped it to the bounded case; unbounded was never evaluated on its merits.
3. **Unbounded is the DTD's DEFAULT, not an exotic configuration.** `XmlLanguageLoader.cs:1408-1409`
   reads `max` as `-1` when the attribute is *absent*
   (`int max = string.IsNullOrEmpty(maxStr) ? -1 : int.Parse(maxStr);`), and
   `HermitCrabInput.dtd:560-568` documents `max` as `#IMPLIED`, "an integer, -1 or higher". So every
   `<OptionalSegmentSequence>` authored without an explicit `max` is unbounded. Refusing the unbounded
   split is therefore a coverage hole on the *most common* authored shape of the construct, not on a
   rare tail case — which inverts the cost/benefit the row was originally weighed under.
4. **The C# test corpus itself relies on it.** `GuesserSignatureTests.cs:82` uses
   `<OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></...>` as the
   guesser's own "any nonempty stem" placeholder. This also links row 14 to gap G3 (guesser surface):
   the guesser's canonical pattern *is* an unbounded quantifier.

### Implementation shape

- `Slot::Repeat`'s `max: u32` becomes `max: Option<u32>` (`None` = unbounded), and `render_slots`
  (`lower.rs:555-570`) renders `None` as `[inner]*` / `[inner]^>{min-1}` instead of `^{min,max}`.
- `MAX_QUANTIFIER_BOUND` (`lower.rs:232`) keeps applying to *finite* bounds only. An unbounded
  quantifier is not "a bound above the ceiling"; it does not need the ceiling at all, because the
  native star's net size is independent of any repetition count. This is the whole point.
- `slot_candidates` (used by the metathesis path, `replace.rs:1117-1122`) enumerates concrete
  candidates and so still cannot accept a `Slot::Repeat` — that stays `None`, unchanged and honest.
- Everything that currently reads a `Slot::Repeat`'s `max` for a finite size preflight must be audited
  for a "no finite max" path rather than defaulting to a number.

### Termination is not at risk, and here is why

A rewrite rule's compiled relation being infinite does not make `apply` non-terminating: `apply_up` /
`apply_down` run over a *finite input word*, and for an ε-free relation each input yields finitely
many outputs regardless of loops in the net. The one shape that could produce unbounded output is an
unbounded quantifier in a rule's **RHS** (insert arbitrarily many copies). That shape is checked
separately and, if reachable, stays refused on its own merits — it is a distinct question from the
LHS/environment case, which is what rows 14 and the DTD default are actually about. ADR 0003's apply
budgets remain the backstop either way.

### Disposition after the build

`ConfirmOnly` (the construction is exact, but promotion to `Admit` is the separate optional track per
§D1) with the `quantifier.unbounded-expansion` configuration key. Fixture:
`unbounded-iterative-quantifier-expansion`, pattern basis already authored in
`docs/conformance/representative-typology-basis.md` §"QuantifierPattern — unbounded".

---

## Row 8 — `Metathesis`, `Dir::RightToLeft`

**Open question as recorded:** is a from-scratch RTL-metathesis construction worth building at all, or
is this a candidate for a declared permanent scope boundary the way `MprGroupOverwrite` is?

**Resolution: build it. It is not from scratch.**

### Evidence

1. **The premise "no existing partial attempt to extend, unlike RTL rewrite" is what fails.** The row
   distinguishes RTL metathesis (nothing built) from RTL rewrite (partially built). But the *shape* of
   the RTL-rewrite construction is fully built and directly reusable:
   `rust/crates/pg-foma/src/replace.rs`'s module doc §"Scope" (lines 71-107) documents
   `reverse ∘ compile(mirror rule) ∘ reverse`, implemented as `reversed_slots` (`replace.rs:394`) plus
   `fsm_reverse` (imported at `replace.rs:245`), with the final net taken as
   `fsm_union(plain_net, reversed_net)` (`replace.rs:457`). None of that machinery is
   rewrite-rule-specific — it operates on `Vec<Slot>` and `Fsm`.
2. **C# treats the two directions identically at the mechanism level.** `SynthesisMetathesisRule`
   builds an `IterativePhonologicalPatternRule` with `Direction = rule.Direction`
   (`SynthesisMetathesisRule.cs`, `MatcherSettings.Direction`) — exactly the same
   `IterativePhonologicalPatternRule` + `Direction` pairing the rewrite path uses. Direction is an
   iteration-order setting over overlapping matches, not a different rule semantics. So the same
   mirror-and-reverse argument that makes RTL rewrite faithful applies unchanged to RTL metathesis.
3. **Cost is small and bounded.** `compile_metathesis_rule` (`replace.rs:1085-1096`) currently
   early-returns `Ok(None)` on `!matches!(rule.dir, Dir::LeftToRight)`. The change is: build the mirror
   pattern (reverse the slots, and correspondingly remap `left_switch`/`right_switch` to their mirrored
   indices), run the existing swap construction on it, `fsm_reverse` the result, and union with the
   plain net — the same four moves `compile_rtl_branch_net` already makes.
4. **A permanent carve-out would be the *expensive* option here, not the cheap one.** A carve-out has
   to be written, justified against a construction we can demonstrably build, and then defended every
   time someone reads the ledger and asks why. `MprGroupOverwrite`'s carve-out is justified because the
   construct's semantics are genuinely outside what a compiled proposer can represent. Nothing
   comparable is true of RTL metathesis.

### Disposition after the build

`ConfirmOnly`, matching `RightToLeftRewrite` exactly and for the identical reason: the union
`plain ∪ reverse(mirror)` is a **superset** of the true RTL relation, which is sound under the
propose-and-confirm invariant (the proposer may overapproximate; it may never omit) and is precisely
why RTL rewrite is `ConfirmOnly` rather than `Admit`. Configuration key
`metathesis.faithful-reversal-swap`, alongside the existing `metathesis.faithful-swap-construction`.
Fixture: `right-to-left-metathesis-reversal`, pattern basis already authored in
`docs/conformance/representative-typology-basis.md` §"Metathesis — right-to-left".

---

## §D6's held question — does either promotion need a C# oracle pass?

**Resolution: no C#-oracle precondition for either. `pangloss` is the oracle, per the standing rule.**

§D6 flagged as a judgment call whether these two rows, "given how novel and rare both configurations
are," should get a genuine `hc.dll` re-verification before promotion. The rarity premise does not hold
for row 14 (evidence point 3: unbounded is the DTD default), and for row 8 the construction is a
*reuse* of one already accepted under the repo's normal oracle discipline rather than a novel one. So
neither is a special case, and both close under the `conformance-grammars` skill's standing rule:
`pangloss` is the oracle for a staged fixture until re-verified, which is the discipline every other
staged fixture in this repo already operates under.

The one row §D6 names as genuinely oracle-blocked — `SimultaneousRewrite`'s overlapping-subrule
configuration, the single case ADR 0001 itself names as "never pinned against `hc.dll`" — is
**unchanged by this decision** and stays oracle-blocked. It is not swept in.

---

## Consequences for the finish line

`design.md` §D7's "Definition of done" requires zero unresolved NEEDS-DECISION rows. That condition is
now satisfiable by *building*, with no carve-out added: both rows move from NEEDS-DECISION to
PROVABLE, and each closes with the standard Stage-2 kit (construction + containment test + conformance
fixture + ledger-row update). Neither is a precondition for the ledger-wide cross-check flip (§D7 step
7) becoming build-breaking, but both must be closed before "full coverage" can be claimed.
