# two-merges-cascade-env

**Classification: COVERED (base == head == v1), regression guard**

## What it tests

Tests the real syllabary shape: two narrowing rules with alpha variables (C a -> fused, C + a -> fused), the second feeding the first in analysis, plus an earlier fronting rule (a -> e / _ [+cons vel]) whose right environment must be satisfied by the node standing where the fused K was.

## Results (hc-conformance self-check, 2026-10-09)

base: teK -> TAK+SFX|teK, TAKA|teK; tek -> TAK|tek; KT -> KATA|KT; KTa -> KATA+SFX|KT+?a; taK, taka fail.
head: identical to base (signatures and traced rules).
v1: identical to base.

## Why the expected result is right, and the mechanism

Why base is right: both TAKA (taka -> teka -> teK) and TAK+SFX (tak+a -> tek+a -> teK) are legal forward derivations. The head passes here because the fronting target e is a plain surface node (clean) and only its environment touches the dirty union node; environment constraints carry no Modified=Clean. The union node keeps cons and place (both the fused character and the consonant class with its alpha variable mention them), so the velar environment is still satisfied on all three variants.

## Provenance

Variants: base = sillsdev/machine 18cf242f (#480 head); head = base + PR #539 two-file diff (81c7364d); v1 = head minus every SetDirty in NarrowAnalysisRewriteRuleSpec.Unapply, with AnalysisRewriteRule.cs restored to base. Oracle of record: base (C# founding oracle, hc-conformance self-check). Runner: hc-conformance.dll --fixtures <root> from each worktree, one dotnet process at a time, 2026-10-09. Logs: scratchpad hunt-{base,head,v1}-final.log.

## PanGloss staging

The expected signatures in words.yaml come from the C# founding oracle; its commit and the hunter run evidence are recorded in the fixture metadata and source notes above. PanGloss conformance replays status and complete parse-signature multisets; it does not compare the optional rules annotations.

Promotion status: upstream_candidate.
Upstream fixture PR: not opened; candidate destination is machine/conformance/edge-cases/two-merges-cascade-env/.
Upstream report: [sillsdev/machine#539](https://github.com/sillsdev/machine/pull/539) (review pending). Related capture fix: [sillsdev/machine#540](https://github.com/sillsdev/machine/pull/540).
