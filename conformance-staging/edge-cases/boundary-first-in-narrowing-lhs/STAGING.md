# boundary-first-in-narrowing-lhs

**Classification: COVERED (base == head == v1), regression guard; the discriminating twin is boundary-first-in-narrowing-lhs-strrep**

## What it tests

Tests a narrowing rule whose target starts with a morpheme boundary (+ b -> m / [+nas] _) in a grammar WITH a phonological feature system.

## Results (hc-conformance self-check, 2026-10-09)

base: kanmo -> KAN+BO|kanmo, KAN+MO|kan+?mo; katbo -> KAT+BO|kat+?bo; katmo -> KAT+MO|kat+?mo; kan -> KAN|kan; kat -> KAT|kat; kanbo fails.
head: identical to base.
v1: identical to base.

## Why the expected result is right, and the mechanism

Why base is right: kan+bo -> kanmo is the only derivation of KAN+BO and kan+mo is KAN+MO unchanged.

Why the union is harmless here: with a feature system a segment node is {Type: Segment, phonological features} and carries no StrRep (CharacterDefinitionTable.Add only builds a StrRep structure when the loader passes fs == null, which XmlLanguageLoader does only when PhonologicalFeatureSystem.Count == 0). A boundary is always {Type: Boundary, StrRep: +}. FeatureStruct.Union keeps only the keys both sides carry, so union(m, +) = {Type: {Segment, Boundary}}: a wildcard that b and n both unify with.

## Provenance

Variants: base = sillsdev/machine 18cf242f (#480 head); head = base + PR #539 two-file diff (81c7364d); v1 = head minus every SetDirty in NarrowAnalysisRewriteRuleSpec.Unapply, with AnalysisRewriteRule.cs restored to base. Oracle of record: base (C# founding oracle, hc-conformance self-check). Runner: hc-conformance.dll --fixtures <root> from each worktree, one dotnet process at a time, 2026-10-09. Logs: scratchpad hunt-{base,head,v1}-final.log.

## PanGloss staging

The expected signatures in words.yaml come from the C# founding oracle; its commit and the hunter run evidence are recorded in the fixture metadata and source notes above. PanGloss conformance replays status and complete parse-signature multisets; it does not compare the optional rules annotations.

Promotion status: upstream_candidate.
Upstream fixture PR: not opened; candidate destination is machine/conformance/edge-cases/boundary-first-in-narrowing-lhs/.
Upstream report: [sillsdev/machine#539](https://github.com/sillsdev/machine/pull/539) (review pending). Related capture fix: [sillsdev/machine#540](https://github.com/sillsdev/machine/pull/540).
