# 035 — Template/slot feature collisions and widening

This entry separates implementation status from the evidence needed to trust it across both engines.

## Kind
behavioural.

## Status
Open research. Rust widening is implemented; its necessity and complete safety are not established. No equivalent widening patch is claimed merged in Machine.

## C# site
`AnalysisAffixTemplatesRule / AnalysisAffixTemplateRule`.

## Rust site
`pg-rules/src/stratum.rs::run_template_batch_raw / apply_slot_batch`.

## Evidence
`docs/research/pg-rules-analysis-syn-fs-gate-notes.md` records that both existing template tests still pass with both widening sites disabled. No load-bearing shared conformance collision grammar is identified. `template-category-sharing` is NOT that grammar.

The six-word template-exclusivity and homophonous-identity control is published in Machine
[commit a20bce12](https://github.com/sillsdev/machine/commit/a20bce12), on
`integrate-conformance-framework` through [PR #480](https://github.com/sillsdev/machine/pull/480).
It passes with memoization on and off; adding its slot-only rules to the ordinary stratum list
produces the predicted 4/6 mismatches. This strengthens structural coverage, not collision coverage.

## Remaining work
Construct a real same-key collision followed by a distinguishing downstream gate. Check soundness as well as recall, including feature correlations and ordering. Reconcile with #493 and Exact: removing unnecessary Rust accumulation/widening is a possible result, not just porting more widening upstream.

## Upstream
[Issue #505](https://github.com/sillsdev/machine/issues/505), [PR #493](https://github.com/sillsdev/machine/pull/493), [PR #494](https://github.com/sillsdev/machine/pull/494).
