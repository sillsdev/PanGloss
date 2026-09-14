# STAGING: chained-output-feature-override-loss

## Why this fixture exists

Pins a **known C# parse loss**, and — for the one word that exercises it (`zudiua`) — records the
answer forward synthesis proves is correct rather than what hc.dll returns. It passes under
HC-Rust's `fix/exact-analysis-fs` branch (the "exact analysis feature structure" divergence:
analysis un-application now computes the exact inverse of synthesis) and **fails against hc.dll**,
on purpose, until a `sillsdev/machine` PR proposing the same fix on the C# side exists and lands.
The twin fixture upstream lives on branch `conformance/exact-analysis-fs-divergence` in
`C:\Users\johnm\Documents\repos\machine\.worktrees\conformance-exact-divergence`.

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
- `zudiua` (the full four-morph chain): **the bug pin, and the one word that does not record hc.dll's
  own output**. The founding oracle finds zero analyses, even though `zudua` shows the identical
  override-then-gate pair parses cleanly one rule shorter — isolating the loss to `mrInner`'s own
  re-unification against the leftover `tense=past`. The entry instead records
  `ZUD+INNER+OUTER+OUTERMOST|zudiua`, the answer forward synthesis proves correct (root, then
  `mrInner` tense=pres, then `mrOuter` tense=past, then `mrOutermost`'s tense=past gate satisfied),
  so this fixture is red against hc.dll and green against HC-Rust's `fix/exact-analysis-fs` Exact
  fold by design.

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

**Oracle: the C# founding oracle for five of six words**, run directly — `hc-conformance.exe`
self-check mode (`--fixtures conformance --propose --include-pathological`), built from
`C:\Users\johnm\Documents\repos\machine\src\SIL.Machine.Morphology.HermitCrab.Conformance\bin\Release\net10.0\`,
against the fixture as authored in
`C:\Users\johnm\Documents\repos\machine\.worktrees\conformance-exact-divergence` (branch
`conformance/exact-analysis-fs-divergence`, based on `conformance/fieldworks-witnesses` at
`d3b7643d`). `zud`/`zudi`/`zudu`/`zudua`/`zudia`'s signatures/outcomes are transcribed verbatim from
that run — not hand-derived — and the harness's own comparison genuinely re-verified them
(sanity-checked here by deliberately corrupting one signature and confirming the run reports
`[FAIL]` with the correct proposed replacement before reverting). `zudiua` is the deliberate
exception: the oracle returns no analysis at all, and its `words.yaml` entry instead records the
answer traced by hand from forward synthesis (see "What it pins" above) — the one case this repo's
`conformance-grammars` skill and `docs/divergences/002-*` sanction a fixture disagreeing with the
oracle, because the answer is independently checkable without either engine. This
`conformance-staging/` copy is a zero-reshaping copy of that already-oracle-verified (except
`zudiua`) fixture; nothing here was authored against `pangloss`/HC-Rust for the other five words.

## Graduation

Not yet proposed upstream. Candidate destination:
`machine/conformance/edge-cases/chained-output-feature-override-loss/` (already staged there too, on
the `machine` repo's own worktree/branch above — that copy IS the candidate for a future
`sillsdev/machine` PR against `conformance-framework`). On acceptance, bump the `machine` submodule
pin and delete this staged copy in the same change (graduation guard enforces this mechanically).

## Current status: passes under Exact, fails against hc.dll — on purpose

This fixture pins the **correct** answer for `zudiua`, not hc.dll's current one. Under this
branch's (`fix/exact-analysis-fs`) `Exact` analysis fold it **passes** end to end. Run directly
against the C# founding oracle it **fails** on `zudiua` alone, because hc.dll has not adopted the
`Exact` mode from `perf/pr494-priority-union` (or any other retraction fix) yet — see
`docs/divergences/001-verification.md`/`001-three-way-confirmation.md` (the empirical settling of
which fold is right) and `docs/divergences/002-exact-validation.md` (the validation of `Exact`
across 11 adversarial grammars). A red result against hc.dll here is the expected state, not a
broken fixture, and not something to "fix" by reverting `zudiua`'s expectation back to the oracle's
empty result. The five other words remain oracle-recorded and are expected to agree with hc.dll
under every fold.
