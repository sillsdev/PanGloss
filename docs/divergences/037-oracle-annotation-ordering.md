# 037 — Oracle annotation ordering instability

This entry separates implementation status from the evidence needed to trust it across both engines.

## Kind
behavioural.

## Status
Open investigation, not an established root cause or a demonstrated Rust bug.

## C# site
`BidirList / tied-node annotation ordering (suspected)`.

## Rust site
`pg-parse oracle comparison dependency; no BidirList port claim`.

## Evidence
#500 reports multiple unseeded outputs and repeatability with a fixed BidirList seed. These are historical PR observations, not fresh runs from this cleanup. No stable shared fixture yet.

## Remaining work
Prove the ordering mechanism with deterministic tie cases and exact identity multisets across fresh processes. A fixed random seed can mask the bug; stability alone does not prove correctness. The concern applies to PanGloss through its C# oracle dependency.

## Upstream
[Issue #506](https://github.com/sillsdev/machine/issues/506), [PR #500](https://github.com/sillsdev/machine/pull/500).
