# 032 — Template-battery memoization

This entry separates implementation status from the evidence needed to trust it across both engines.

## Kind
efficiency.

## Status
Implemented in both engines; Machine #456 merged. This is separate from cascade memoization and from template-output merging.

## C# site
`AnalysisAffixTemplatesRule / AnalysisScope`.

## Rust site
`pg-rules/src/stratum.rs::run_template_batch`.

## Evidence
`rust/crates/pg-rules/tests/memo_gate.rs::memo_on_equals_memo_off_with_template` and `memo_preserves_nonfinal_template_state_transition_before_final_template` are Rust tests, not Machine conformance grammars. `template-category-sharing` adds exclusivity and homophonous-root controls but does not demonstrate a memo hit.

## Remaining work
Key/replay must preserve template state and all observable alternatives. Record cache hits and complete memo-on/off identity multisets; a passing cache-free run cannot validate replay.

## Upstream
[PR #456](https://github.com/sillsdev/machine/pull/456); merge coverage [#505](https://github.com/sillsdev/machine/issues/505), final-template coverage [#507](https://github.com/sillsdev/machine/issues/507).
