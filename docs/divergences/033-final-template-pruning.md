# 033 — Final-template analysis pruning

This entry separates implementation status from the evidence needed to trust it across both engines.

## Kind
efficiency.

## Status
Rust source policy exists, including conservative partial-rule guards and invocation-role handling. The staged #553 partial-root fixture now has runtime status/identity evidence below; Machine #491 remains open, and family-stem substitution remains unverified. Safe defaults and forced overrides are not equivalent.

## C# site
`AnalysisStratumRule / Word.FinalTemplateState`.

## Rust site
`pg-rules/src/stratum.rs final-template policy`.

## Evidence
`conformance-staging/edge-cases/final-template-partial-discriminators` has ten words. Published on Machine's `integrate-conformance-framework` branch in [commit a20bce12](https://github.com/sillsdev/machine/commit/a20bce12), through [PR #480](https://github.com/sillsdev/machine/pull/480). Freshly self-checked at base `8bad1934`: both new fixtures pass with memoization on and off, zero skipped. The PanGloss pin is unchanged; staged copies remain until its pin includes the fixtures. `docs/superpowers/plans/2026-09-03-final-template-prune-results.md` records earlier corpus measurements.

## Remaining work
The guard must imply that no valid continuation remains. Default runs with zero prunes establish parity only, not pruning coverage. Partial roots are now covered by the staged fixture and counter distinction below; dual-role invocation, family-stem substitution, and any unrepresented lower-stratum continuations need separate evidence. Forced override results cannot justify default safety.

Since entry 102 (Machine #529) the template state is no longer reset at every stratum: it crosses
non-blocking strata. 102's "Interaction with 033" gives the argument that the per-stratum policy
adds no case this entry did not already have.

## Upstream
[Issue #507](https://github.com/sillsdev/machine/issues/507), [PR #491](https://github.com/sillsdev/machine/pull/491).

## 2026-10-23 #553 runtime verification

The #553 partial-rule policy and partial-root template gate were already present in Rust at base HEAD 14975991f65dcbd290c2a58c30ecd894247d0851: FinalTemplatePruneFacts is derived from partial rules in pg-grammar-model/src/model.rs, the parser selects the policy in pg-parse/src/morpher.rs, and root_is_partial is checked in pg-rules/src/stratum.rs. This is source-presence evidence; the run below is separate runtime evidence.

The exact Machine #480 final-template-partial-root grammar and words are staged under conformance-staging/edge-cases/final-template-partial-root because they are absent from the current pin. The managed final_template_partial_root_gate target passed all ten base rows and every declared settings variant, including statuses and complete identity multisets; its partial-rule control fixture and pilfbvb positive also passed. The staged copy remains until upstream acceptance and a pin update. See [Machine issue #553](https://github.com/sillsdev/machine/issues/553); the staged words retain their existing #551 provenance, with no duplicate issue opened.

For pakfaga, the partial-root path has no analysis and an observed template_batteries_skipped effect. It does not increment final_templates_skipped: all ten staged partial-root words reported zero for that different counter. The exact final_templates_skipped mechanism was separately demonstrated by the clitic-rule-designation sagdt test, with sagds as the zero-count control; that test now counts only final_templates_skipped. These counters are not used as substitutes for each other.

The later managed `conformance-test -Scope all` run also passed with the staged partial-root gate included. This is runtime regression coverage on the staged copy; the Machine #553 fixture remains outside the pinned submodule.

Inflectional-family stem substitution remains unverified. The local staged fixtures contain no family entry, and choose_inflectional_stem requires a family relative whose syntactic feature structure is compatible and strictly more specific; no minimal oracle-checked fixture was established in this slice. No #553 product code changed.
