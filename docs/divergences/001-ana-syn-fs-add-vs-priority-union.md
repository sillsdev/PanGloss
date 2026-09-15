# 001 — Analysis syntactic-FS fold: Add versus PriorityUnion

This records the earlier analysis-feature port. It is not the current Rust algorithm; entry 002 records its replacement.

## Kind
Behavioural.

## Status
Superseded in Rust by Exact, commit `149f88df`. PriorityUnion was previously ported in `5f06e428`.
This is not a reversion to C# Add and not closure of the current cross-engine difference.

## C# site
`AnalysisAffixProcessRule.Apply` and `AnalysisCompoundingRule.Apply`.
Machine [PR #494](https://github.com/sillsdev/machine/pull/494) proposes PriorityUnion instead of Add.
It remains open at the audit; its current implementation is not Exact.

## Rust site
`rust/crates/pg-rules/src/morph.rs::ana_syn_fs`. See [002](002-ana-syn-fs-exact-inverse.md).

## What differs
Add unions alternatives at a feature. PriorityUnion overwrites a specified feature with the required
value. Neither alone retracts an outer rule's overwritten output paths. A union of two values does
not admit an unrelated third value: the previous version of this entry incorrectly claimed it did.

## Evidence
Historical measurements: `docs/research/2026-09-10-hc-analysis-fs-port-measurements.md`.
Regression tests: `rust/crates/pg-rules/tests/analysis_syn_fs_gate.rs`; the file now tests the current
algorithm, so its presence does not establish that PriorityUnion remains selected.
Finite corpus agreement is not universal proof, and timed-out words provide no completed comparison.

## Upstream
[PR #494](https://github.com/sillsdev/machine/pull/494) is a real posted proposal, contrary to this
entry's former “not proposed” wording. [Issue #504](https://github.com/sillsdev/machine/issues/504)
tracks the remaining Exact question; [#505](https://github.com/sillsdev/machine/issues/505) tracks merge interactions.
