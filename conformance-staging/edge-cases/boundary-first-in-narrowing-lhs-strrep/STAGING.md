# boundary-first-in-narrowing-lhs-strrep

**Classification: V1 REGRESSION (v1 != base; head != base the same way)**

## What it tests

Zero-feature twin of boundary-first-in-narrowing-lhs: no PhonologicalFeatureSystem, so every segment is {Type: Segment, StrRep: {x}}. Rule + b -> m / [+nas] _ with the boundary in the first target position.

## Results (hc-conformance self-check, 2026-10-09)

base: kanmo -> KAN+BO|kanmo, KAN+MO|kan+?mo; katbo -> KAT+BO|kat+?bo; katmo -> KAT+MO|kat+?mo; kan -> KAN|kan; kat -> KAT|kat; kanbo fails.
head: kanmo -> KAN+MO|kan+?mo only (KAN+BO lost). Everything else as base.
v1: kanmo -> KAN+MO|kan+?mo only (KAN+BO lost). Everything else as base.

## Why the expected result is right, and the mechanism

Why base is right: kan+bo -> kanmo is a legal forward derivation (the rule is obligatory after a nasal), so kanmo must analyse as KAN+BO; base finds it because its unapplication leaves (m)(+)(b) o, where the suffix pattern b o consumes the optional reconstructed b and the root kan skips (m)(+).

Mechanism (verified by reading): NarrowAnalysisRewriteRuleSpec.Unapply at head and v1 unions the first |RHS| LHS constraints INTO the matched surface nodes; the first LHS constraint is the boundary {Type: Boundary, StrRep: +}, the matched node is {Type: Segment, StrRep: m}. FeatureStruct.UnionImpl (FeatureStruct.cs:384-411) keeps the keys present on both sides and unions their values, so the node becomes {Type: {Segment, Boundary}, StrRep: {m, +}} and stays MANDATORY; only the second constraint b becomes an optional leftover. String features unify by set intersection (StringFeatureValue.IntersectWith), so neither the suffix b (b against {m, +}) nor the root-final n can consume that node, and no analysis path reaches KAN+BO. The dirty marks play no part (v1 has none), so this is a property of the union encoding itself: the superset argument in fable-report section 3 silently assumed the LHS constraint is a segment; a boundary constraint shares only Type and StrRep with the node, and in a zero-feature grammar StrRep is the whole identity.

Who is affected: any grammar with no phonological features (the default state of a FieldWorks project that has not defined features, where rules are written over phonemes) whose rewrite rule has a BoundaryMarker inside the first |output| positions of its input. Real-grammar check: the Fidel-style rule c + v -> F has the boundary in position 2 of 3 with a 1-node output, so it is a leftover and safe (see boundary-leftover-in-narrowing-lhs-strrep).

Minimality: one rule, two roots, two suffixes; the KAT words and kan/kat are controls showing the rule and both suffixes work when the rule does not fire.

## Provenance

Variants: base = sillsdev/machine 18cf242f (#480 head); head = base + PR #539 two-file diff (81c7364d); v1 = head minus every SetDirty in NarrowAnalysisRewriteRuleSpec.Unapply, with AnalysisRewriteRule.cs restored to base. Oracle of record: base (C# founding oracle, hc-conformance self-check). Runner: hc-conformance.dll --fixtures <root> from each worktree, one dotnet process at a time, 2026-10-09. Logs: scratchpad hunt-{base,head,v1}-final.log.

## PanGloss staging

The expected signatures in words.yaml come from the C# founding oracle; its commit and the hunter run evidence are recorded in the fixture metadata and source notes above. PanGloss conformance replays status and complete parse-signature multisets; it does not compare the optional rules annotations.

Promotion status: upstream_candidate.
Upstream fixture PR: not opened; candidate destination is machine/conformance/edge-cases/boundary-first-in-narrowing-lhs-strrep/.
Upstream report: [sillsdev/machine#539](https://github.com/sillsdev/machine/pull/539) (review pending). Related capture fix: [sillsdev/machine#540](https://github.com/sillsdev/machine/pull/540).
