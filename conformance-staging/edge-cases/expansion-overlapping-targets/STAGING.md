# expansion-overlapping-targets

**Classification: PR REGRESSION, fixed by V1 (head != base, v1 == base)**

## What it tests

Tests analysis-side unapplication of an expansion rule (1 -> 2, A -> a a, leftToRightIterative) whose two-segment output overlaps with itself in kaaa.

## Results (hc-conformance self-check, 2026-10-09)

base: kaaa -> KAA1|kaaa, KAA2|kaaa, KAAA|kaaa; kaa -> KA|kaa, KAA3|kaa; kAa, kA fail.
head: kaaa -> KAA2|kaaa, KAAA|kaaa (KAA1 lost).
v1: identical to base.

## Why the expected result is right, and the mechanism

Why base is right: both kAa and kaA derive kaaa by one application of the obligatory rule.

Mechanism at head: the PR removes mode = Simultaneous; reapplyType = Deletion from the expansion branch of AnalysisRewriteRule, so expansion runs through IterativePhonologicalPatternRule with one pass (the rule is not simultaneous, so no SelfOpaquing loop). The analysis matcher for a left-to-right rule runs right to left; it finds the site at positions 2-3 first, unions the long vowel into node 2, makes node 3 optional, and restarts left of the match, where no second a a remains. The site at positions 1-2 is never undone, so node 1 stays a short a and kAa cannot be found. The base Simultaneous rule collects both sites with AllMatches before applying either. V1 restores the base branch, so the new union encoding is applied at both sites (U (U) (a)) and all three roots are found. This is a different failure from fable-report F1 (the hang), which needs multApplic=simultaneous; here the rule is iterative and terminates, but silently loses a parse.

## Provenance

Variants: base = sillsdev/machine 18cf242f (#480 head); head = base + PR #539 two-file diff (81c7364d); v1 = head minus every SetDirty in NarrowAnalysisRewriteRuleSpec.Unapply, with AnalysisRewriteRule.cs restored to base. Oracle of record: base (C# founding oracle, hc-conformance self-check). Runner: hc-conformance.dll --fixtures <root> from each worktree, one dotnet process at a time, 2026-10-09. Logs: scratchpad hunt-{base,head,v1}-final.log.

## PanGloss staging

The expected signatures in words.yaml come from the C# founding oracle; its commit and the hunter run evidence are recorded in the fixture metadata and source notes above. PanGloss conformance replays status and complete parse-signature multisets; it does not compare the optional rules annotations.

Promotion status: upstream_candidate.
Upstream fixture PR: not opened; candidate destination is machine/conformance/edge-cases/expansion-overlapping-targets/.
Upstream report: [sillsdev/machine#539](https://github.com/sillsdev/machine/pull/539) (review pending). Related capture fix: [sillsdev/machine#540](https://github.com/sillsdev/machine/pull/540).
