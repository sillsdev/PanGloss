# 104 — Expansion unapplication loses an RHS match across an interposed Optional

Kind: behavioural (Rust-only defect; GH#8).
Status: fixed-in-rust.

## C# site

None differs. `NarrowAnalysisRewriteRuleSpec.Unapply` in Machine `aab4c598` (which contains
[Machine PR #540](https://github.com/sillsdev/machine/pull/540)) analyses the fixture below correctly.

## Rust site

`pg-rules/src/rewrite.rs::ana_narrow_general` and `build_prule_cache`. Narrowing rules (LHS wider than
RHS) recovered each RHS position from a per-position capture (divergence 083), but expansion rules
(LHS narrower than RHS, e.g. `u -> i i`) still matched a positional span and discarded any span wider
than the RHS. A deletion unapplied earlier in the cascade can interpose an Optional between the two
RHS positions, which widens every span, so `width_matches` rejected all candidates and the parse was
lost. Every non-deletion `Kind::Narrow` subrule now uses the grouped captures.

## Expected parse

Fixture `conformance-staging/edge-cases/interposed-optional-expansion-rhs/`: root `U` (`u`) expands to
`ii`, the `x -> 0 / i _ i` deletion has no `x` to delete, so `ii` parses as `U|ii`. `u`, `i`, `iii`
and `ixi` are negative controls.

## Evidence

- `hc-conformance` self-check built from Machine `aab4c598`: all five words pass, so C# is exact.
- HC-Rust before the change: `conformance_fixtures_gate` fails with `word "ii" ... left (HC-Rust): []
  right (oracle): ["U|ii"]`. After: 33 of 33 tests in that target pass; `pg-rules` 196 of 196.
- The fix-removed run is the pre-change run above, with the fixture present and the change absent.
