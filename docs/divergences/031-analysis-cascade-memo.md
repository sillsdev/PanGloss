# 031 — Analysis cascade memoization

This entry separates implementation status from the evidence needed to trust it across both engines.

## Kind
efficiency.

## Status
Superseded by [040](040-memoization-removed.md): **removed in Rust**, still implemented in C#.
Machine #456 merged. What follows describes the state before removal.

## C# site
`AnalysisStratumRule / AnalysisScope`.

## Rust site
None — `memo_apply_rules` and the `pg-memo` crate are deleted. Was
`pg-rules/src/stratum.rs::memo_apply_rules`.

## Evidence
`rust/crates/pg-rules/tests/memo_gate.rs` includes `memo_on_equals_memo_off_unordered` and counter tests. Machine #456 includes counter-guarded parity unit tests. A dedicated exported conformance grammar demonstrating an actual cache hit is not identified.

## Remaining work
None on the Rust side. The conformance-coverage gap this entry recorded (no grammar-level
discriminator with actual cache hits) was never closed, and is now unclosable in Rust — it stands
as an open coverage question for C# alone.

## Upstream
[PR #456](https://github.com/sillsdev/machine/pull/456); shared performance investigation [#485](https://github.com/sillsdev/machine/issues/485). No new correctness bug is claimed.
