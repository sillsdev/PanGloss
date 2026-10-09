# 083 — Narrowing unapplication loses an RHS match across an interposed Optional

Kind: behavioural (shared C#/Rust bug; expected parse is independently forward-derived).
Status: open — fixed in PanGloss; the C# fix is Machine PR #540.

## C# site

`NarrowAnalysisRewriteRuleSpec.Unapply` (`PhonologicalRules/NarrowAnalysisRewriteRuleSpec.cs`) finds the
matched RHS nodes by stepping `range.Start.Next` once per RHS constraint. The analysis matcher skips
Optional nodes and filters out boundaries (`AnalysisRewriteRule`'s Segment|Anchor filter), so a
multi-node RHS match can span an interposed Optional node or boundary; the walk then marks that node
instead of the real RHS node, which stays mandatory, and the LHS reading is lost. Verified on
Machine master `b9e7db44` by Machine PR #540's tests (segment and boundary variants).

Machine fix PR: [Machine PR #540](https://github.com/sillsdev/machine/pull/540).

## Rust site

`pg-rules/src/rewrite.rs::ana_narrow_general` handles only narrowing/expansion rules with a
non-empty RHS. It compiles the same RHS target lanes inside one named capture per RHS position and
uses `grouped_target_positions` to recover the real matched segment for each position. The
uncached, cached, and traced analysis routes use the same grouped target. The pure-deletion branch
and synthesis/expansion paths are unchanged.

## Expected parse and reason for the divergence

Fixture: `conformance-staging/edge-cases/interposed-optional-multi-rhs/`.

The independently justified forward derivation is:

1. Lexical entry `ABC` has shape `abc`.
2. The `merge` rewrite maps `abc` to `de`.
3. The `x -> 0 / d _ e` deletion has no `x` to delete, so the surface stays `de`.

Therefore `de` has analysis `ABC|de`. During analysis, unapplying the deletion can insert Optional
`x` between `d` and `e`. The old `ana_narrow_general` used the enclosing match span as if every
segment in that span were an RHS position. The span was wider than the two-position RHS and was
dropped by `width_matches`, losing `ABC|de`. Grouped captures retain the target's two real
positions, so the existing environment, binding, reconstruction, and Optional-marking steps can
run on the correct nodes.

The changed matcher retains the same per-position RHS constraints, environments, alpha bindings,
and reconstruction. A tight match recovers the same nodes the old contiguous span selected. A match
that the old width guard discarded is now applied only to the nodes captured for its RHS positions;
the interposed Optional node stays Optional and is not marked as part of the RHS. This recovers
previously discarded candidates and corrects target-node selection without widening the rule's
pattern. The staged positive proves recall; its four negative controls pin the grammar's rejection
of `abc`, `dex`, `dxe`, and `dd`.

No boundary-inside-RHS variant is included. Analysis target matching uses `MutShape::segs(false)`,
which omits physical boundary nodes. A boundary therefore cannot serve as an interposed captured
position in this matcher; treating a boundary RHS row as a segment lane would probe separate
boundary semantics.

## Regression evidence

The base binary was run before the code change:

```sh
/home/johnm/work/narrow539/bin/pangloss-main batch \
  conformance-staging/edge-cases/interposed-optional-multi-rhs/grammar.xml \
  /home/johnm/work/narrow539/f3-words.txt \
  /home/johnm/work/narrow539/f3-main.tsv \
  --threads 1 --word-timeout-ms 5000
```

It completed all five rows with status `ok`; `de` had signature `-`, as did all four negative
controls. The full T4 replay, focused package gates, and final `-Mode check` results are recorded
in `docs/research/narrow-unapply/f3-grouped-capture.md` after their serial-queue runs.

The expected positive is `forward-synthesis`, not a transcription of the C# output. The C# loss is
confirmed separately: Machine PR #540 adds `RewriteRuleTests.MergeRuleUnappliesAcrossInterposedOptionalSegment`
(the same shape in the test language), which fails on master `b9e7db44` and passes with its fix.
