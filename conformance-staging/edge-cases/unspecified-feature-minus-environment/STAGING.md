# unspecified-feature-minus-environment

**Classification: BUG PIN (PanGloss issue #180; HC-Rust differed from the oracle on 2 of 6 words before the fix)**

## What it tests

A segment with no value for a feature must not satisfy a rewrite-rule pin on that value. The rule
`[arch:+] -> [nasal:+, arch:-] / _ + [nasal:-]` turns the archiphoneme prefix N into m before a
stem that starts with a nasal:- segment (p). The vowel `a` has no nasal value, so a stem starting
with `a` does not trigger it: `Nata` parses (N unchanged) and `mata` does not. PanGloss 0.15.2
parsed `mata` and not `Nata`.

## C# verdict and source

C# founding oracle, sillsdev/machine pin 4d9b454d (hc-conformance self-check, in-process Morpher),
2026-11-05: 1 passed, 0 failed. Source: HC-Rust port bug, not an HC algorithm bug. Synthesis rewrite
rules use `MatchingMethod.Subsumption` (the `MatcherSettings` default; `SynthesisRewriteRule.cs`
sets only `UseDefaults`), so `Input.Matches` calls `FeatureStruct.Subsumes`, whose
`SubsumesImpl` returns false when the node lacks a pinned feature and the feature has no default
(`FeatureStruct.cs`, final `else`). Analysis rewrite rules set `MatchingMethod.Unification`, where
an unspecified feature is compatible. HC-Rust compiled both through unification and only
confirmed defaults afterwards. No sillsdev/machine issue: C# is correct.

## FieldWorks-producible

Yes: the issue's grammar was exported by FieldWorks GenerateHCConfig (nasal:+ and nasal:- phonemes,
a vowel with no nasal value, an N prefix, one simultaneous feature-change rule). This fixture is
the hand-minimised HermitCrab XML of it; no `fieldworks/project.fwdata` is staged because the
authoring tool needs a Windows FieldWorks build.

## Provenance

Oracle of record: C# founding oracle, one run, 2026-11-05. Synthetic data only.
Promotion status: upstream_candidate. Upstream fixture PR: not opened.

Sibling fixture for the rule target: `unspecified-feature-minus-target`.
