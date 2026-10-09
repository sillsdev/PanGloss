# boundary-mid-in-narrowing-lhs-strrep

**Classification: V1 REGRESSION (v1 != base; head != base the same way)**

## What it tests

Zero-feature grammar, rule n + b -> n m: boundary in position 2 of a 3-segment target with a 2-segment output, so the boundary still falls inside the first |output| positions.

## Results (hc-conformance self-check, 2026-10-09)

base: kanmo -> KAN+BO|kanmo, KAN+MO|kan+?mo; katbo -> KAT+BO|kat+?bo; katmo -> KAT+MO|kat+?mo; kan -> KAN|kan; kat -> KAT|kat; kanbo fails.
head: kanmo -> KAN+MO|kan+?mo only (KAN+BO lost).
v1: kanmo -> KAN+MO|kan+?mo only (KAN+BO lost).

## Why the expected result is right, and the mechanism

Same mechanism as boundary-first-in-narrowing-lhs-strrep: union(n, n) = n, union(m, +) = {Type: {Segment, Boundary}, StrRep: {m, +}} mandatory, leftover (b). The suffix b cannot consume the {m, +} node and the root cannot skip it. Shows the hazard is positional (first |output| LHS positions), not specific to a boundary-initial target.

## Provenance

Variants: base = sillsdev/machine 18cf242f (#480 head); head = base + PR #539 two-file diff (81c7364d); v1 = head minus every SetDirty in NarrowAnalysisRewriteRuleSpec.Unapply, with AnalysisRewriteRule.cs restored to base. Oracle of record: base (C# founding oracle, hc-conformance self-check). Runner: hc-conformance.dll --fixtures <root> from each worktree, one dotnet process at a time, 2026-10-09. Logs: scratchpad hunt-{base,head,v1}-final.log.

## PanGloss staging

The expected signatures in words.yaml come from the C# founding oracle; its commit and the hunter run evidence are recorded in the fixture metadata and source notes above. PanGloss conformance replays status and complete parse-signature multisets; it does not compare the optional rules annotations.

Promotion status: upstream_candidate.
Upstream fixture PR: not opened; candidate destination is machine/conformance/edge-cases/boundary-mid-in-narrowing-lhs-strrep/.
Upstream report: [sillsdev/machine#539](https://github.com/sillsdev/machine/pull/539) (review pending). Related capture fix: [sillsdev/machine#540](https://github.com/sillsdev/machine/pull/540).
