# 033 — Final-template analysis pruning

This entry separates implementation status from the evidence needed to trust it across both engines.

## Kind
efficiency.

## Status
Rust port exists, including conservative partial-grammar guards and invocation-role handling. Machine #491 remains open. Safe defaults and forced overrides are not equivalent.

## C# site
`AnalysisStratumRule / Word.FinalTemplateState`.

## Rust site
`pg-rules/src/stratum.rs final-template policy`.

## Evidence
`conformance-staging/edge-cases/final-template-partial-discriminators` has ten words. Published on Machine's `integrate-conformance-framework` branch in [commit a20bce12](https://github.com/sillsdev/machine/commit/a20bce12), through [PR #480](https://github.com/sillsdev/machine/pull/480). Freshly self-checked at base `8bad1934`: both new fixtures pass with memoization on and off, zero skipped. The PanGloss pin is unchanged; staged copies remain until its pin includes the fixtures. `docs/superpowers/plans/2026-09-03-final-template-prune-results.md` records earlier corpus measurements.

## Remaining work
The guard must imply that no valid continuation remains. Default runs with zero prunes establish parity only, not pruning coverage. Actual dual-role invocation, partial roots and lower-stratum continuations need discriminating fixtures; forced override results cannot justify default safety.

Since entry 102 (Machine #529) the template state is no longer reset at every stratum: it crosses
non-blocking strata. 102's "Interaction with 033" gives the argument that the per-stratum policy
adds no case this entry did not already have.

## Upstream
[Issue #507](https://github.com/sillsdev/machine/issues/507), [PR #491](https://github.com/sillsdev/machine/pull/491).
