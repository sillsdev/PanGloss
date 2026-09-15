# 034 — Stratum analysis-state merging

This entry separates implementation status from the evidence needed to trust it across both engines.

## Kind
behavioural.

## Status
Partially aligned; broader reconciliation remains open. Machine #493 merged as `52d069f845b43f8bc88a95a56a8511aa58def26f`. Rust now uses gate-only template feature handling. State-key/fallback/widening equivalence is not established by this change.

Implementation: PanGloss [`fc357daf`](https://github.com/sillsdev/PanGloss/commit/fc357daf).
Independent Luna spec review and Sol correctness review accepted this narrow slice.

## C# site
`AnalysisStratumRule.MergeEquivalentAnalyses / AnalysisAffixTemplateRule`.

## Rust site
`pg-rules/src/stratum.rs analysis merge and analyze_template`.

## Evidence
Machine merged tests include `MergeEquivalentAnalysesTests`, `AnalysisAffixTemplateRuleTests`, and `SameRuleUsedInMultipleTemplates`. They are unit tests, not a dedicated shared collision grammar.

At Machine conformance commit `a20bce12`, ancestry and source inspection confirm that #493 is
already present. PanGloss `6030f44d` still re-adds template features after slot analysis. The C#
optional-slot tests assert unchanged intermediate syntactic features; final parse signatures alone
cannot establish that invariant. C# also tests a shared suffix with no required features, whereas
PanGloss's `same_rule_used_in_multiple_templates` covers only the constrained-suffix variant.

The gate-only correction removes the saved unification and post-slot feature addition while
retaining compatibility rejection, slot analysis, all widening, memoization and final-template policy.
`stratum::template_analysis_tests` directly tests the production template method: an absent optional
suffix leaves empty features unchanged, a consumed slot preserves its V analysis features, and
an incompatible input is rejected. Before the correction, the first two feature assertions fail
and the rejection control passes; after it, all three pass. Temporarily restoring only the removed
accumulation reproduces both failures; restoring the correction returns all three to green.

Machine commit [`f150e2a0`](https://github.com/sillsdev/machine/commit/f150e2a0) adds
`shared-template-unconstrained-suffix`, mirrored unchanged in staging until the submodule pin
advances. Its ten rows pass the C# oracle with both template orders and memoization on/off
(40 comparisons, no skipped rows). PanGloss's dedicated `template_analysis_conformance` test
passes all 40 comparisons both before and after the correction. This is full-parse preservation coverage,
not a demonstrated full-parse failure on current Rust and not proof that widening is necessary.

Managed verification after the correction: `pg-rules` all-target check passes; private state tests
3/3; `stratum_gate` 15/15 with one existing private-corpus test ignored;
`csharp_port_affix_template` 5/5; `exact_analysis_fs_recall` 1/1;
`template_analysis_conformance` 1/1; `conformance_fixtures_gate` 5/5; and
`divergence_catalogue_gate` 3/3. Generic conformance replay checks 695 words across 66 fixtures,
with three existing exclusions: `deep-optional-affix-nesting` and `backend-template-generic`
are opt-in pathological fixtures; `simultaneous-epenthesis-cascade` pins a crash. This is not
a claim that those excluded cases or the full workspace suite ran. Machine's focused template
and manifest test selection also passes 22/22 without skips.

## Remaining work
#493 also registers a canonical state only after output insertion succeeds. Audit that equivalence
separately from the now-aligned template feature handling. Do not describe the merged C# fix as
union widening. Issue #505 remains open for actual same-key collision reachability, downstream
positive and negative continuations, and feature-correlation evidence; memory representation is
outside this correction.

## Upstream
[Issue #505](https://github.com/sillsdev/machine/issues/505), merged [PR #493](https://github.com/sillsdev/machine/pull/493).

Published implementation evidence: [#505 update](https://github.com/sillsdev/machine/issues/505#issuecomment-5686179488)
and [conformance PR update](https://github.com/sillsdev/machine/pull/480#issuecomment-5686179895).
