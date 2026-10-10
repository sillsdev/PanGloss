# deletion-reinserted-feature-unapply

**Classification: PR REGRESSION, fixed by V1 (head != base, v1 == base)**

## What it tests

Tests a feature rule (final devoicing) ordered before a deletion rule (final t after a consonant) in synthesis. Analysing bag must re-insert an optional t (undo the deletion) and then undo the devoicing on that re-inserted node to reach BAGD.

## Results (hc-conformance self-check, 2026-10-09)

base: bag -> BAGD|bag, BAGT|bag; bak -> BAG|bak, BAK|bak; bagd, bagt fail.
head: bag -> BAGT|bag only (BAGD lost). Everything else as base.
v1: identical to base.

## Why the expected result is right, and the mechanism

Why base is right: BAGD -> bagd -> (final devoicing) bagt -> (t deletion after consonant) bag is a legal forward derivation, so bag must analyse as BAGD.

Mechanism at head: a deletion rule is a NarrowAnalysisRewriteRuleSpec with an empty target; its expanding flag (LHS count 1 > target count 0) is true, so every re-inserted optional node is marked Modified=Dirty. FeatureAnalysisRewriteRuleSpec adds Modified=Clean to its target constraint, so the devoicing rule cannot match the re-inserted t and the union t/d is never formed; the root lookup then needs bagd to match b a g (t) and fails. SimultaneousPhonologicalPatternRule never resets dirty marks, and no later iterative rule succeeds on this word, so nothing clears them. This mechanism is separate from the merge-rule dirt that fable-report F2 describes: it fires on every deletion rule, with no narrowing rule in the grammar at all (a real grammar with a 1->0 rule ordered after a feature rule has it).

## Provenance

Variants: base = sillsdev/machine 18cf242f (#480 head); head = base + PR #539 two-file diff (81c7364d); v1 = head minus every SetDirty in NarrowAnalysisRewriteRuleSpec.Unapply, with AnalysisRewriteRule.cs restored to base. Oracle of record: base (C# founding oracle, hc-conformance self-check). Runner: hc-conformance.dll --fixtures <root> from each worktree, one dotnet process at a time, 2026-10-09. Logs: scratchpad hunt-{base,head,v1}-final.log.

## PanGloss staging

The expected signatures in words.yaml come from the C# founding oracle; its commit and the hunter run evidence are recorded in the fixture metadata and source notes above. PanGloss conformance replays status and complete parse-signature multisets; it does not compare the optional rules annotations.

Promotion status: upstream_candidate.
Upstream fixture PR: not opened; candidate destination is machine/conformance/edge-cases/deletion-reinserted-feature-unapply/.
Upstream report: [sillsdev/machine#539](https://github.com/sillsdev/machine/pull/539) (review pending). Related capture fix: [sillsdev/machine#540](https://github.com/sillsdev/machine/pull/540).
