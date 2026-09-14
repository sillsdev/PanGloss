# STAGING: chained-output-feature-override-loss

## Why this fixture exists

Pins a **known C# parse loss**, not a bug fix landing here — it exists so that HC-Rust's
`fix/exact-analysis-fs` branch (the "exact analysis feature structure" divergence: analysis
un-application now computes the exact inverse of synthesis) shows up as a **conformance FAILURE**
against this fixture the moment that branch's exact mode is exercised by the suite, before any
`sillsdev/machine` PR proposing a C#-side fix exists or is even drafted.

The construct: a morphological rule with no `RequiredHeadFeatures` but with `OutputHeadFeatures`
leaves that written feature sitting on the stem during analysis (un-application). If an inner
suffix's own `OutputHeadFeatures` writes `tense=pres` and an outer suffix's own `OutputHeadFeatures`
overwrites it to `tense=past`, then un-applying the outer suffix leaves `tense=past` on the stem;
un-applying the inner suffix is then refused because its own `OutputHeadFeatures` (`tense=pres`) is
not unifiable with the leftover `tense=past`. **That parse is lost in C# today** — on master (Add
semantics) and under the owner's own research branch `perf/pr494-priority-union`'s `PriorityUnion`
mode alike; neither retracts the stale `OutputHeadFeatures` write before the next rule's
re-unification. HC-Rust's exact mode strips the rule's own output-feature paths off the stem before
re-unifying, and therefore **finds** that parse.

## What it pins

Grammar: one root (`zud`, untensed), three suffix rules on a `linear` stratum —
`mrInner` (no `RequiredHeadFeatures`, writes `tense=pres`, suffix `-i`),
`mrOuter` (no `RequiredHeadFeatures`, writes `tense=past`, suffix `-u`),
`mrOutermost` (`RequiredHeadFeatures` `tense=past`, no output write, suffix `-a`).
`mrOutermost` is the discriminator that makes the loss observable rather than vacuous: it only
admits a derivation that genuinely passed through `mrOuter`'s own `tense=past` write.

- `zud`/`zudi`/`zudu`: bare-root and single-rule controls — the grammar loads and each rule works
  in isolation.
- `zudua` (root+`mrOuter`+`mrOutermost`, `mrInner` skipped): **non-vacuousness control**. The very
  same override-then-gate pair succeeds cleanly whenever `mrInner` is not part of the chain, so the
  loss below is attributable specifically to `mrInner`'s own re-unification, not to some general
  failure of the gate.
- `zudia` (root+`mrInner`+`mrOutermost`, `mrOuter` skipped): negative control that `mrOutermost`'s
  gate is a real feature check (fails because only `tense=pres` was ever written), not vacuously
  satisfied by any three-morph chain of the right shape.
- `zudiua` (the full four-morph chain): **the bug pin**. Zero analyses under the founding oracle,
  even though `zudua` shows the identical override-then-gate pair parses cleanly one rule shorter —
  isolating the loss to `mrInner`'s own re-unification against the leftover `tense=past`.

## Provenance

- `sillsdev/machine` research branch `perf/pr494-priority-union`,
  `src/SIL.Machine.Morphology.HermitCrab/MorphologicalRules/AnalysisSyntacticFeatureMerge.cs` (the
  `Add`/`PriorityUnion`/`Exact` modes) and `docs/pr494-review.md` section 4, which names this exact
  loss `OverrideLoss_TenseFlipFlop` and documents it as a pre-existing master bug, not something
  PR #494 introduces or fixes.
- Adversarial C# test on that branch:
  `tests/SIL.Machine.Morphology.HermitCrab.Tests/MorphologicalRules/AnalysisSyntacticFeatureMergeTests.cs`,
  `OverrideLoss_TenseFlipFlop_AddAndPriorityUnionLoseTheParse_ExactFindsIt` — this fixture's grammar
  shape (root/inner/outer/outermost, suffixes `-i`/`-u`/`-a`) mirrors that test's `MakeSuffixRule`
  setup directly, so the two stay comparable.
- HC-Rust (PanGloss) end-to-end demonstration:
  `rust/crates/pg-parse/tests/exact_analysis_fs_recall.rs` on branch `fix/exact-analysis-fs`.

## Oracle discipline

**Oracle: the C# founding oracle**, run directly — `hc-conformance.exe` self-check mode
(`--fixtures conformance --propose --include-pathological`), built from
`C:\Users\johnm\Documents\repos\machine\src\SIL.Machine.Morphology.HermitCrab.Conformance\bin\Release\net10.0\`,
against the fixture as authored in
`C:\Users\johnm\Documents\repos\machine\.worktrees\conformance-exact-divergence` (branch
`conformance/exact-analysis-fs-divergence`, based on `conformance/fieldworks-witnesses` at
`d3b7643d`). All six signatures/outcomes below are transcribed verbatim from that run — not
hand-derived — and the harness's own comparison genuinely re-verified them (sanity-checked here by
deliberately corrupting one signature and confirming the run reports `[FAIL]` with the correct
proposed replacement before reverting). This `conformance-staging/` copy is a zero-reshaping copy of
that already-oracle-verified fixture; nothing here was authored against `pangloss`/HC-Rust.

## Graduation

Not yet proposed upstream. Candidate destination:
`machine/conformance/edge-cases/chained-output-feature-override-loss/` (already staged there too, on
the `machine` repo's own worktree/branch above — that copy IS the candidate for a future
`sillsdev/machine` PR against `conformance-framework`). On acceptance, bump the `machine` submodule
pin and delete this staged copy in the same change (graduation guard enforces this mechanically).

## Not a fix

This fixture pins a **loss**, on purpose. It is expected to keep passing against C#'s founding
oracle (that is the ground truth it records) and to **fail** against any HC-Rust build/mode that
adopts the `fix/exact-analysis-fs` exact-inverse semantics, until and unless the C# side also adopts
the `Exact` mode from `perf/pr494-priority-union` (not proposed here) or some other reconciliation is
made. Do not "fix" this fixture's expectations to match HC-Rust's exact mode without first deciding,
separately, whether C#'s behavior itself should change.
