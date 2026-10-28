# 038 — Edge-segment prefilter candidate

This entry separates implementation status from the evidence needed to trust it across both engines.

## Kind
efficiency.

## Status
Implemented in PanGloss as a pure speed-up: parses and failed-unapplication trace events are identical with the prefilter on and off. Upstream [machine #574](https://github.com/sillsdev/machine/pull/574) is open, so no accepted Machine change is claimed.

## C# site
`AnalysisAffixProcessAllomorphRuleSpec candidate matching`.

## Rust site
`pg-rules/src/morph/edge_prefilter.rs` (`AnalysisLhs::edge_rejects`), called from the cached affix analysis paths in `pg-rules/src/morph.rs`. `RuleCache::with_edge_prefilter(false)` and `Morpher::with_edge_prefilter(false)` turn it off for differential runs; the uncached `ana_affix` path never prefilters.

## Evidence
Machine's [optimization record #490](https://github.com/sillsdev/machine/pull/490) records previous attempts and negative results. No dedicated pass/fail conformance grammar identified for this candidate.

## Necessary condition
The matcher is anchored at both ends and may skip only an optional input segment. A mandatory first (last) word segment must therefore unify with the pattern's first (last) constraint, which the analysis LHS exposes as the first (last) node, looking through groups and through quantifiers with a lower bound of one. Optional word edges, optional or branching pattern edges and empty patterns never reject. The prefilter stores nothing per rule and adds no work step of its own.

## Measured effect
`work_steps` on the first 100 words of each reference corpus (words that hit a 5M cap or fail to segment are left out of both arms; analyses identical in every compared word), off then on: Indonesian 1,765,894 to 1,641,756 (-7.0%); Aweti 38,466,419 to 19,800,119 (-48.5%); Amharic 7,704,677 to 6,880,683 (-10.7%); Mbugwe 18,489,245 to 15,865,920 (-14.2%); Sena 2,971,451 to 2,400,890 (-19.2%). Reproduce with the ignored `edge_prefilter_gate` corpus test.

## Remaining work
Originally: derive a necessary condition from the actual matcher semantics (optional/empty segments, natural classes and rewrites included). Demonstrate both parity and avoided matcher calls. This cleanup does not restart sparse-forest or memory work.

## Upstream
[PR #490](https://github.com/sillsdev/machine/pull/490), broader performance [issue #485](https://github.com/sillsdev/machine/issues/485).
