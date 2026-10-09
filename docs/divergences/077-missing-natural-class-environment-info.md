# 077: Missing natural classes drop the whole environment and produce an Info finding

Kind: diagnostic (no parse divergence from FieldWorks or C# HermitCrab).
Status: deliberate reporting addition, requested by the integration lead on 2026-10-09.
Related underdefined policy discussion: [Machine #537](https://github.com/sillsdev/machine/issues/537).

## Measured behavior and owner decision

The synthetic `conformance-staging/underdefined/05-missing-class` witness records an affix
environment `/ [V] _` with no class V. FieldWorks omits the invalid environment from XAMPLE
export; C# HCLoader reports an InvalidEnvironment diagnostic and discards the entire restriction.
Both engines accept `mums` and `muxs`. The defined-class control V={a,u} rejects those words.
`measurement.json`, saved projects and raw engine captures retain the original observations.
Their engine versions and hashes are in the parent `engine-provenance.json`; this lane did not
rerun the Windows engines or infer their installed build identity from an inspected source HEAD.

The lead explicitly retained that whole-environment drop. PanGloss now adds
`grammar.environment.missing-natural-class` at level **Info**, naming the unresolved class token
and explaining that the entire environment, including any other contexts, was discarded. It
retains the existing `grammar.environment.invalid` Warning and its conversion/load outcomes.
It neither fabricates a wildcard class nor preserves a partial environment. No admission,
readiness, resource budget or parsing rule is relaxed.

## Owner and reporting seam

`pg-grammar/src/compile/environment.rs::nodes_from_spanned` owns class-name resolution and
publishes the attempted class token and winner index. `Ctx::environment_resolution` consumes
that cached owner result: an unresolved winner emits one source-identified Info finding per
environment. Grammar health uses `pg-snapshot`'s warning metadata for level, explanation,
navigation and guidance. It does not look up names again or parse diagnostic text to decide
whether a class exists. The original whole-expression failure and load decisions survive.

The FieldWorks subject names its String Representation and points to Grammar > Environments;
guidance directs the author to define or correct the natural class and review the allomorph's
intended distribution. The diagnostic page is rendered from the same runtime metadata.

## Regression coverage

`pg-grammar/src/compile/tests.rs::missing_natural_class_reports_info_and_drops_the_entire_environment`
uses `/ k [Absent] _` on multiple allomorphs. It requires one Info finding, the missing token
and source environment, no retained left/right pattern, and successful parsing after the whole
restriction is dropped. Its repaired `/ k _` control must bind and reject the same word.
The test failed before the reporting implementation: actual finding count 0, expected 1.
Final restored/revert verification and managed gate exits are recorded in the integration report.

The staged case is measured native-engine evidence; the code-constructed regression checks
reporting and whole-environment semantics. These are distinct claims. No new Machine issue or
comment was posted by the closed-network integration lane.
