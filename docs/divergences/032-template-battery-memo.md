# 032 — Template-battery memoization

This entry separates implementation status from the evidence needed to trust it across both engines.

## Kind
efficiency.

## Status
Superseded by [045](045-memoization-removed.md): **removed in Rust**, still implemented in C#.
Machine #456 merged. What follows describes the state before removal.

## C# site
`AnalysisAffixTemplatesRule / AnalysisScope`.

## Rust site
None — the memoized wrapper is deleted and `run_template_batch` is now the plain battery
(formerly `run_template_batch_raw`).

## Evidence
`rust/crates/pg-rules/tests/memo_gate.rs::memo_on_equals_memo_off_with_template` and `memo_preserves_nonfinal_template_state_transition_before_final_template` are Rust tests, not Machine conformance grammars. `template-category-sharing` adds exclusivity and homophonous-root controls but does not demonstrate a memo hit.

## Remaining work
None on the Rust side. The unvalidated-replay concern this entry recorded remains open for C#.

## Upstream
[PR #456](https://github.com/sillsdev/machine/pull/456); merge coverage [#505](https://github.com/sillsdev/machine/issues/505), final-template coverage [#507](https://github.com/sillsdev/machine/issues/507).
