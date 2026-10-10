# 087 — Narrowing unapplication unions the LHS into the matched RHS nodes

Kind: optimization (argued parse-preserving; shared upstream proposal).
Status: open — implemented in PanGloss; the C# change is proposed in Machine PR #539, whose review
recommends exactly this variant.

## C# site

`NarrowAnalysisRewriteRuleSpec.Unapply` (`PhonologicalRules/NarrowAnalysisRewriteRuleSpec.cs`). On
Machine master, unapplying a narrowing rule `l1..ln -> r1..rm` (n > m > 0) inserts all of `l1..ln` as
Optional nodes after the match and makes `r1..rm` Optional. Optional nodes multiply the paths the
root lookup and later rules explore, so grammars whose every character is a merge output (Amharic
C+V -> Fidel) take orders of magnitude longer than they need to.

[Machine PR #539](https://github.com/sillsdev/machine/pull/539) instead unions each LHS constraint
into the matched RHS node and inserts only the leftover LHS nodes as Optional. As posted it also adds
dirty marks and moves expansion rules to an Iterative path; both lose parses or fail to terminate
(Simultaneous expansion `u -> i i` hangs; a feature rule ordered before a merge cannot unapply on
the merged nodes). PanGloss implements only the parse-preserving core, called V1b in the review:

- union each **Segment-typed** LHS constraint, in order, into the next matched RHS node;
- insert every other LHS node, including every **Boundary**, as an Optional leftover after the match;
- no dirty marks; expansion and deletion unchanged.

## Rust site

`pg-rules/src/rewrite.rs::ana_narrow_general` (true-narrowing branch) and `union_narrowing_node`. The
matched RHS nodes come from the per-position captures added for divergence 083, so the union lands on
the real RHS node even across an interposed Optional skip. Union is lane-wise OR after alpha-variable
resolution (an unconstrained lane is its full mask, so a key absent on either side stays
unconstrained, as in C# `FeatureStruct.Union`); StrRep identity is unioned when both sides carry one
and becomes unrestricted otherwise.

## Why it cannot change a parse

`FeatureStruct.Union` generalizes: the union node unifies with every segment the old RHS node and the
old LHS node each unified with. For any alignment, choosing none of the Optional leftovers reproduces
the RHS reading and choosing all of them reproduces the LHS reading; mixed choices are junk candidates
that synthesis rejects in both encodings. Alpha values are substituted before the union. The argument
does not hold for a Boundary constraint, which shares nothing with a segment but `Type` and, in a
grammar without phonological features, `StrRep`: `union(m, +)` is a mandatory node no segment can
consume. V1b therefore never unions a boundary.

This is an argued property, not a proof for every grammar. Evidence:

- C# (Machine `18cf242f` + PR head with the dirty marks removed): on the Amharic corpus, identical parse
  sets to master on all 587 words both finished; the PR as posted loses every parse on 17 of 673.
  Amharic's only boundary rule has its boundary as a leftover, so V1b and V1 coincide there.
- C# V1b on the conformance suite of Machine PR #480 plus the fixtures below: every fixture equals
  master except `interposed-optional-multi-rhs` (divergence 083, needs Machine PR #540) and a
  traced-rules-only difference on `boundary-leftover-in-narrowing-lhs-strrep` (parse sets equal);
  534 HermitCrab tests pass.
- PanGloss: the gates and all-scope conformance replay in `docs/research/narrow-unapply/`.

## Fixtures

`conformance-staging/edge-cases/`: `simultaneous-expansion-terminates`, `feature-rule-before-merge`,
`epenthesis-fed-merge`, `geminate-under-merge`, `default-symbol-after-merge` (forward-derived guards
against the PR's regressions), and nine regression-hunt fixtures whose expectations come from C#
master: `expansion-overlapping-targets`, `merge-then-metathesis`, `two-merges-cascade-env`,
`boundary-first-in-narrowing-lhs`, `boundary-first-in-narrowing-lhs-strrep`,
`boundary-mid-in-narrowing-lhs-strrep`, `boundary-leftover-in-narrowing-lhs-strrep`,
`deletion-reinserted-feature-unapply`, `merge-subrules-pos-rtl-template`.

The last two exposed two older PanGloss bugs, independent of this change and fixed with their
staging, so they are not divergences: a bare `#` environment did not skip Optional segments
(`bak` missed `BAG` behind the re-inserted `t`), and a right-to-left synthesis target was
compiled unreversed, so a multi-segment `rightToLeftIterative` LHS never matched (`F`, `Gs`,
`FF` found no parse).

FST backends: the new fixtures enter the backend scoreboard ratchet
(`pg-foma-backend/tests/backend_scoreboard_gate.rs`). HC-Rust is exact on all of them; the
`PlanComposed` backend refuses eleven (typed refusals), and `TunedSurfaceProbed` misses a parse on
`boundary-first-in-narrowing-lhs-strrep`, an FST coverage gap on a feature-less boundary rule.
Both FST strategies also miss on `merge-subrules-pos-rtl-template` in every application order,
a separate gap with part-of-speech-gated narrowing subrules.

Related: 083 (interposed Optional; [Machine PR #540](https://github.com/sillsdev/machine/pull/540)).
