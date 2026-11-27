# 129: A segment with no value for a feature satisfies a `[feature:-]` pin in synthesis

Kind: behavioural (partial-feature grammars; see the policy in `README.md` and `docs/adr/0009-partial-feature-grammars.md`).
Status: permanent, owner decision 2026-11-05. v0.16.0 (#180, PR #188) had aligned with hc.dll; that alignment is reverted.

## Decision

In a synthesis rewrite rule, a segment that has no value for a feature satisfies a `[feature:-]` condition, in the rule's target and in its environments, as before 0.16.0 and as in analysis. In `unspecified-feature-minus-environment`, `N+ata` surfaces as `mata` (the vowel `a` has no nasal value, so it satisfies `[nasal:-]`) and `Nata` has no analysis; in `unspecified-feature-minus-target`, `[nasal:-] -> (deleted)` also deletes the unspecified `a`. hc.dll gives `Nata` and `aa`.

Why: a grammar may specify a feature on some phonemes only (an obstruent series marked `nasal:-` while vowels never needed the feature). A phoneme that has not been given a feature value must behave like a phoneme the feature does not apply to, not silently fail every condition that names the feature. Treating "no value" as compatible lets phonemes gain features one at a time without a parse changing under the author. hc.dll's behaviour comes from the missing feature, so PanGloss keeps the generous result as a true, recorded divergence.

## C# site

Synthesis rewrite matchers keep `MatchingMethod.Subsumption` (the `MatcherSettings` default): `SynthesisRewriteRule` (target and environment patterns) matches through `FeatureStruct.SubsumesImpl`, which returns false when the node lacks a pinned feature and the feature declares no default. Analysis matchers use `Unification`, where the missing feature is compatible. (`FeatureStruct.cs`, `Input.cs`.)

## Rust site

`pg-rules/src/rewrite.rs`: synthesis targets and environments match by unification, like analysis; `pattern_defaults_ok` applies only a declared default (the `UseDefaults` flag), with no subsumption mode and no phantom-bit environment logic. Research note: `docs/research/rewrite-usedefaults-confirm.md`.

## Verification

hc.dll (machine `4d9b454d`, `hc-conformance` self-check) on the fixtures' PanGloss expectations: `unspecified-feature-minus-environment` mismatches on `Nata` and `mata`; `unspecified-feature-minus-target` on `aa` and `a`; pinned in `rust/tools/oracle-conformance-known-divergences.json`. Tests: `pg-parse/tests/csharp_port_rewrite.rs` (`unspecified_feature_satisfies_a_minus_environment`, `unspecified_feature_satisfies_a_minus_target`) and `pg-rules/tests/rewrite_gate.rs` (`defaults_allow_matching_the_declared_default_and_absent_default`).
