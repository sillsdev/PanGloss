# 036 — Zero-width morpheme identity preservation

This entry separates implementation status from the evidence needed to trust it across both engines.

## Kind
behavioural.

## Status
Open upstream fix. Machine #500 proposes preserving a wrapped morpheme before adding the fallback zero-width morph. PanGloss was reported correct on the reproduction; no new Rust patch is needed on that evidence alone.

## C# site
`SynthesisAffixProcessAllomorphRuleSpec.ApplyRhs / MarkMorph`.

## Rust site
`pg-rules/src/morph.rs::attribute_morphs`.

## Evidence
#500's `sagui` reproduction must retain A2B and B2A in both analyses, with THIRD added only to the longer derivation. The PR contains unit tests and reported results. A stable shared conformance fixture is still missing; those reports were not rerun during this documentation cleanup.

## Remaining work
Do not equate correct surface or parse count with correct morpheme identity. Separate the dropped-ID fix from unresolved process-to-process annotation ordering (entry 037).

## Upstream
[PR #500](https://github.com/sillsdev/machine/pull/500); related remaining instability [issue #506](https://github.com/sillsdev/machine/issues/506).
