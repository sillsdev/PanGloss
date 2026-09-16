# 031 — Analysis cascade memoization

This entry separates implementation status from the evidence needed to trust it across both engines.

## Kind
efficiency.

## Status
Superseded — memoization is being removed from HC-Rust entirely (branch feat/remove-memoization), so the memo-side mechanism this entry compares no longer exists as a target; kept for history per the ledger's never-delete rule.

Former status: Implemented in both engines; implementations are not asserted identical. Machine #456 merged. Rust's memo work has independent history.

## C# site
`AnalysisStratumRule / AnalysisScope`.

## Rust site
`pg-rules/src/stratum.rs::memo_apply_rules`.

## Evidence
`rust/crates/pg-rules/tests/memo_gate.rs` includes `memo_on_equals_memo_off_unordered` and counter tests. Machine #456 includes counter-guarded parity unit tests. A dedicated exported conformance grammar demonstrating an actual cache hit is not identified.

## Remaining work
Same-key replay must preserve the entire continuation result, including identities and multiplicities. Add a grammar-level memo-on/off discriminator with actual hits before declaring conformance coverage complete.

## Upstream
[PR #456](https://github.com/sillsdev/machine/pull/456); shared performance investigation [#485](https://github.com/sillsdev/machine/issues/485). No new correctness bug is claimed.
