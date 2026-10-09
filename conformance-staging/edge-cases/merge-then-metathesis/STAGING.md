# merge-then-metathesis

**Classification: PR REGRESSION, fixed by V1 (head != base, v1 == base)**

## What it tests

Tests a metathesis rule (t s -> s t) ordered before a merge rule (s t -> c) in synthesis, so that the metathesis must unapply on the nodes the merge unapplication recovered.

## Results (hc-conformance self-check, 2026-10-09)

base: aca -> ACA|aca, ASTA|aca, ATSA|aca; atsa, asta fail.
head: aca -> ACA|aca, ASTA|aca (ATSA lost).
v1: identical to base.

## Why the expected result is right, and the mechanism

Why base is right: ATSA -> atsa -> (metathesis) asta -> (merge) aca is a legal forward derivation.

Mechanism at head: the merge unapplication unions s into the surface c (U = c-or-s) and inserts an optional t, and because expanding is true for narrowing it marks both nodes Dirty. AnalysisMetathesisRuleSpec adds Modified=Clean to every constraint of its pattern, so the pattern s t cannot match U (t); the metathesis is never undone and the root atsa cannot match a U (t) a (t has strid- while U keeps strid+ from c and s). V1 drops the dirty marks and the metathesis matches U (t) as it matched (c)(s)(t) at base. Fable-report F2 covers feature and epenthesis rules; this fixture adds metathesis to the set of later-in-analysis rule types blocked by the marks.

## Provenance

Variants: base = sillsdev/machine 18cf242f (#480 head); head = base + PR #539 two-file diff (81c7364d); v1 = head minus every SetDirty in NarrowAnalysisRewriteRuleSpec.Unapply, with AnalysisRewriteRule.cs restored to base. Oracle of record: base (C# founding oracle, hc-conformance self-check). Runner: hc-conformance.dll --fixtures <root> from each worktree, one dotnet process at a time, 2026-10-09. Logs: scratchpad hunt-{base,head,v1}-final.log.

## PanGloss staging

The expected signatures in words.yaml come from the C# founding oracle; its commit and the hunter run evidence are recorded in the fixture metadata and source notes above. PanGloss conformance replays status and complete parse-signature multisets; it does not compare the optional rules annotations.

Promotion status: upstream_candidate.
Upstream fixture PR: not opened; candidate destination is machine/conformance/edge-cases/merge-then-metathesis/.
Upstream report: [sillsdev/machine#539](https://github.com/sillsdev/machine/pull/539) (review pending). Related capture fix: [sillsdev/machine#540](https://github.com/sillsdev/machine/pull/540).
