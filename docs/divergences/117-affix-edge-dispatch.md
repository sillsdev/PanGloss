# 117 — Affix edge-dispatch index: PanGloss skips allomorphs and rules that cannot match; C# tries each

Kind: efficiency (behavioural only under a step or work cap).
Status: permanent — a Rust-only optimisation. Issue: [sillsdev/PanGloss#90](https://github.com/sillsdev/PanGloss/issues/90) (owner decision Q22, 2026-10-30).

## C# site

Machine revision `74ee09bf8521a6e0765949ea8ba7c4ec74236602`: `AnalysisAffixProcessRule.cs` (`Apply`: compiles one matcher per allomorph and tries every allomorph of every rule on every analysis state). C# has no shared edge index.

## Rust site

- `pg-rules/src/morph/edge_dispatch.rs`: `EdgeDispatch`, the per-rule index; `Admission`, one word's answer.
- `pg-rules/src/morph/edge_prefilter.rs`: the edge constraint each allomorph's analysis pattern must meet.
- `pg-rules/src/cache.rs` (`RuleCache::build`, `edge_dispatch_bytes`): builds the index once with the matchers.
- `pg-rules/src/morph.rs` (`ana_affix_cached`, `ana_affix_cached_traced`, `affix_rule_edge_rejected`): the allomorph loop and the whole-rule check.
- `pg-rules/src/stratum.rs` (`apply_one_mrule_uncounted`): skips a rule before its attempt is reserved.

## Difference

At cache build, each affix allomorph's analysis pattern yields a mandatory first-edge and last-edge constraint (the existing prefilter's rule: an optional, branching or unknown edge yields none). Distinct constraints are interned per rule, at most 63, with a wildcard id 0 for everything else. For a word the index evaluates each distinct constraint once against the word's first and last segment, giving a bit per constraint. The allomorph loop then skips an allomorph whose constraint bit is clear, before the work reservation. When no allomorph is admitted the rule is skipped before its attempt is reserved.

Why it cannot change a parse: an allomorph is skipped only when the matcher's own mandatory edge cannot unify with the word's mandatory edge segment, the same test the in-loop prefilter already used (Machine #574), so the matcher would have returned no match. An optional word edge, an optional, quantified-to-zero or branching pattern edge, an allomorph with no compiled pattern, and every constraint past the 63rd all fall to the wildcard and are tried as before. The skipped allomorph reports the same `reach` row and, when tracing, the same `Pattern` failure event as before.

What does change, observably:

- `work_steps` drops: a skipped allomorph reserves no unit, and a skipped rule reserves none.
- Analysis attempts (`steps`) drop: a rule skipped as a whole is not an attempt, so the per-rule `attempts`, `reach` and fact `work` rows of a stats run have no entry for it. A traced run still walks the skipped rule so its per-allomorph events are reported, reserving and recording nothing, so tracing does not change the step counts.
- Under `--step-cap` or `--work-cap`, a word that hit the cap may now finish, and a capped word's partial state differs. Complete parses are identical.

## Memory

The index is compile-once and per grammar, not per word, so it is not the memoisation entry 045 removed. `Morpher::edge_dispatch_bytes` reports its retained heap bytes: 4,160 B at its largest over the conformance corpus, and the figure for the Cebuano project is recorded in the PR.

## Evidence

- Fixture: `conformance-staging/edge-cases/affix-edge-dispatch` (suffixes ending in d, s, ts, ay, a prefix, a total reduplication, an enclitic g), replayed on C# `hc.dll` (`hc-conformance` self-check passes) and on HC-Rust with the index on.
- `pg-parse/tests/edge_prefilter_gate.rs::the_dispatch_index_keeps_every_conformance_identity_and_never_adds_work`: over every discovered conformance fixture and settings variant, identical analyses and shape verdict with the index on and off, and never more attempts or work.
- `pg-rules/src/morph/edge_dispatch.rs` unit tests: the index agrees with the per-allomorph prefilter on every word, optional edges admit everything, constraints past the limit fall back to the wildcard.
