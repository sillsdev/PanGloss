# merge-subrules-pos-rtl-template

**Classification: COVERED (base == head == v1), regression guard**

## What it tests

Tests a narrowing rule applied rightToLeftIterative with two subrules gated by requiredPartsOfSpeech (nouns fuse t a to F, verbs to G), inside a verb AffixTemplate whose mandatory slot adds s (template-only rule).

## Results (hc-conformance self-check, 2026-10-09)

base: F -> TA_N|F; Gs -> TA_V+S|Gs; FF -> TATA|FF; G, Fs, ta, tas, tata fail.
head: identical to base.
v1: identical to base.

## Why the expected result is right, and the mechanism

Why base is right: each surface has exactly one legal forward derivation under the part-of-speech gating and the mandatory template slot. No later-in-analysis rule has a Modified=Clean target here, so the dirty marks cannot bite, and the union node (F-or-t / G-or-t) is consumed by the root t on every variant.

## Provenance

Variants: base = sillsdev/machine 18cf242f (#480 head); head = base + PR #539 two-file diff (81c7364d); v1 = head minus every SetDirty in NarrowAnalysisRewriteRuleSpec.Unapply, with AnalysisRewriteRule.cs restored to base. Oracle of record: base (C# founding oracle, hc-conformance self-check). Runner: hc-conformance.dll --fixtures <root> from each worktree, one dotnet process at a time, 2026-10-09. Logs: scratchpad hunt-{base,head,v1}-final.log.

## PanGloss staging

The expected signatures in words.yaml come from the C# founding oracle; its commit and the hunter run evidence are recorded in the fixture metadata and source notes above. PanGloss conformance replays status and complete parse-signature multisets; it does not compare the optional rules annotations.

Promotion status: upstream_candidate.
Upstream fixture PR: not opened; candidate destination is machine/conformance/edge-cases/merge-subrules-pos-rtl-template/.
Upstream report: [sillsdev/machine#539](https://github.com/sillsdev/machine/pull/539) (review pending). Related capture fix: [sillsdev/machine#540](https://github.com/sillsdev/machine/pull/540).
