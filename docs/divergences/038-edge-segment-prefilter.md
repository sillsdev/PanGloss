# 038 — Edge-segment prefilter candidate

This entry separates implementation status from the evidence needed to trust it across both engines.

## Kind
efficiency.

## Status
Open candidate only. Not implemented in PanGloss; no accepted Machine parser fix is claimed.

## C# site
`AnalysisAffixProcessAllomorphRuleSpec candidate matching`.

## Rust site
`No current Rust port identified`.

## Evidence
Machine's [optimization record #490](https://github.com/sillsdev/machine/pull/490) records previous attempts and negative results. No dedicated pass/fail conformance grammar identified for this candidate.

## Remaining work
Before implementing, derive a necessary condition from the actual matcher semantics (optional/empty segments, natural classes and rewrites included). Demonstrate both parity and avoided matcher calls. This cleanup does not restart sparse-forest or memory work.

## Upstream
[PR #490](https://github.com/sillsdev/machine/pull/490), broader performance [issue #485](https://github.com/sillsdev/machine/issues/485).
