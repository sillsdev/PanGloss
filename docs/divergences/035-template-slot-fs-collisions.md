# 035 — Template/slot feature collisions and widening

This entry separates implementation status from the evidence needed to trust it across both engines.

## Kind
behavioural.

## Status
Open research. Rust widening is implemented; its necessity and complete safety are not established. No equivalent widening patch is claimed merged in Machine.

## C# site
`AnalysisStratumRule` template `RuleBatch` / `AnalysisAffixTemplateRule` slot `RuleBatch`.

## Rust site
`pg-rules/src/stratum.rs::run_template_batch_raw / apply_slot_batch`.

## Evidence
`docs/research/pg-rules-analysis-syn-fs-gate-notes.md` records that both existing template tests still pass with both widening sites disabled. No load-bearing shared conformance collision grammar is identified. `template-category-sharing` is NOT that grammar.

Machine `a20bce12` has no `AnalysisAffixTemplatesRule` class: the template battery uses `RuleBatch`.
The unconstrained-suffix variant of C# `SameRuleUsedInMultipleTemplates` now has a shared grammar:
Machine [`f150e2a0`](https://github.com/sillsdev/machine/commit/f150e2a0),
`edge-cases/shared-template-unconstrained-suffix`, mirrored unchanged in PanGloss staging.
All ten words pass C# in both template orders with memoization on/off (40 comparisons, no skips).
The same complete multisets pass current Rust before gate-only alignment, so this is preservation
coverage, not a load-bearing widening witness. Entry 034 records the separate intermediate-state
red/green correction. This change leaves template, slot and stratum widening untouched.

The six-word template-exclusivity and homophonous-identity control is published in Machine
[commit a20bce12](https://github.com/sillsdev/machine/commit/a20bce12), on
`integrate-conformance-framework` through [PR #480](https://github.com/sillsdev/machine/pull/480).
It passes with memoization on and off; adding its slot-only rules to the ordinary stratum list
produces the predicted 4/6 mismatches. This strengthens structural coverage, not collision coverage.

## Remaining work
Construct a real same-key collision followed by a distinguishing downstream gate. Check soundness as well as recall, including feature correlations and ordering. Reconcile with #493 and Exact: removing unnecessary Rust accumulation/widening is a possible result, not just porting more widening upstream.

## Upstream
[Issue #505](https://github.com/sillsdev/machine/issues/505), [PR #493](https://github.com/sillsdev/machine/pull/493), [PR #494](https://github.com/sillsdev/machine/pull/494).
