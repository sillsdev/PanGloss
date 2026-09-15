# 034 — Stratum analysis-state merging

This entry separates implementation status from the evidence needed to trust it across both engines.

## Kind
behavioural.

## Status
Open reconciliation. Machine #493 merged as `52d069f845b43f8bc88a95a56a8511aa58def26f`. Rust has state-key/fallback/widening work, but not the same template-feature treatment.

## C# site
`AnalysisStratumRule.MergeEquivalentAnalyses / AnalysisAffixTemplateRule`.

## Rust site
`pg-rules/src/stratum.rs analysis merge and analyze_template`.

## Evidence
Machine merged tests include `MergeEquivalentAnalysesTests`, `AnalysisAffixTemplateRuleTests`, and `SameRuleUsedInMultipleTemplates`. They are unit tests, not a dedicated shared collision grammar.

## Remaining work
#493 registers a canonical state only after output insertion succeeds and stops adding template-required syntactic features to analysis output. Rust still adds those features. Do not describe the merged C# fix as union widening, or assume the earlier Rust port closes this difference. Reproduce the shared-template case and retain both positive and negative continuations.

## Upstream
[Issue #505](https://github.com/sillsdev/machine/issues/505), merged [PR #493](https://github.com/sillsdev/machine/pull/493).
