# boundary-leftover-in-narrowing-lhs-strrep

**Classification: COVERED for parse identity (base == head == v1 signatures); traced-rule list differs on one word (see below)**

## What it tests

Zero-feature grammar, rule n + b -> m: boundary in position 2 of a 3-segment target with a 1-segment output, so the boundary is a leftover, the shape of the real Fidel-style c + v -> F rule.

## Results (hc-conformance self-check, 2026-10-09)

base: kamo -> KA+MO|ka+?mo, KAN+BO|kamo; kanmo -> KAN+MO|kan+?mo; katbo -> KAT+BO|kat+?bo; katmo -> KAT+MO|kat+?mo; kan, kat, ka -> themselves; kanbo fails.
head: same signatures; the self-check traced-rules check differs for kanmo (base attributes prPostNasal to the KAN+MO analysis, head attributes only mrSfxMo).
v1: same as head.

## Why the expected result is right, and the mechanism

Why no parse is lost: union(m, n) = {Type: Segment, StrRep: {m, n}} is mandatory but the root-final n consumes it; the boundary and b are optional leftovers, as in base. The traced-rule difference is not a parse-set change: the self-check credits every parse of a word with each phonological rule that fired in ANY candidate synthesis of that word (TraceRuleAttributor.WordLevelRuleIds reads PhonologicalRuleSynthesis nodes of the whole trace). For kanmo, base generates the junk candidate KAN+BO (root kan skipping the all-optional (m)(n)(+), suffix bo on the reconstructed (b)), synthesises kamo, rejects it at the surface check, but the trace keeps prPostNasal; head and v1 never generate that candidate because the mandatory {m, n} node cannot be skipped. Fewer junk candidates is the intended effect of the PR. The words.yaml records the base attribution, so the fixture is red on head/v1 for that reason only; if this guard graduates, drop prPostNasal from that rules list or accept the trace change.

## Provenance

Variants: base = sillsdev/machine 18cf242f (#480 head); head = base + PR #539 two-file diff (81c7364d); v1 = head minus every SetDirty in NarrowAnalysisRewriteRuleSpec.Unapply, with AnalysisRewriteRule.cs restored to base. Oracle of record: base (C# founding oracle, hc-conformance self-check). Runner: hc-conformance.dll --fixtures <root> from each worktree, one dotnet process at a time, 2026-10-09. Logs: scratchpad hunt-{base,head,v1}-final.log.

## PanGloss staging

The expected signatures in words.yaml come from the C# founding oracle; its commit and the hunter run evidence are recorded in the fixture metadata and source notes above. PanGloss conformance replays status and complete parse-signature multisets; it does not compare the optional rules annotations.

Promotion status: upstream_candidate.
Upstream fixture PR: not opened; candidate destination is machine/conformance/edge-cases/boundary-leftover-in-narrowing-lhs-strrep/.
Upstream report: [sillsdev/machine#539](https://github.com/sillsdev/machine/pull/539) (review pending). Related capture fix: [sillsdev/machine#540](https://github.com/sillsdev/machine/pull/540).

The kanmo base-versus-head rule-trace attribution difference is a discarded junk synthesis candidate, not a parse-set difference. This fixture keeps the founding-oracle parse signatures as the assertion target; PanGloss compares those signatures and statuses only.
