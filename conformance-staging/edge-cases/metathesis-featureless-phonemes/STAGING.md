# metathesis-featureless-phonemes

**Classification: BUG PIN (PanGloss issue #162; ledger row 122).** A metathesis rule `a t k -> a k t` between featureless phonemes (no phonological features). hc.dll finds `akta` (root `atka`); HC-Rust found nothing for `akta`, `itka` or `tika`, because its metathesis matcher compared feature lanes only and a featureless segment class matched every segment, so the obligatory rule reordered every root.

Words: `akta` is the one reordered root; `atka` and `ikta` are negative controls; `itka` and `tika` are roots the rule must not touch.

Oracle of record: C# `hc-conformance` self-check, sillsdev/machine 4d9b454d, 2026-11-05: PASS. Source: `AnalysisMetathesisRuleSpec.cs` and `SynthesisMetathesisRuleSpec.cs` match segment natural classes by `StrRep` on a featureless grammar. Synthetic data only.

FieldWorks-producible: yes. A project whose phonemes carry no features and a `PhMetathesisRule` with `StrucChange` `0 2 -1 1 -1` over three segment-list classes (the `fw-metathesis` project of `pg-parse/tests/data` with the identity features removed).

Promotion status: upstream_candidate.
