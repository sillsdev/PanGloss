# unspecified-feature-minus-target

**Classification: KNOWN PANGLOSS DIVERGENCE (divergence 129, partial-feature grammars; was BUG PIN for issue #180, target half)**

A rewrite rule whose target pins `[nasal:-]` must not delete or rewrite a segment with no nasal value. C# founding
oracle, sillsdev/machine pin 4d9b454d, 2026-11-05: ata surfaces as aa (only t is deleted). Source
and mechanism are as in the sibling fixture: synthesis matches by subsumption
(`FeatureStruct.SubsumesImpl` fails on an absent feature without a default); HC-Rust treated the
unspecified feature as compatible with any pin and applied the rule to a and deleted it.

FieldWorks-producible: yes, a phonological rule with a feature natural class as its input is ordinary
FieldWorks data; the staged grammar is hand-minimised HermitCrab XML and no project.fwdata is staged.
Oracle of record: C# founding oracle, one run. Synthetic data only. Upstream PR: not opened.

## PanGloss divergence (divergence 129)

Owner decision 2026-11-05: see the sibling `unspecified-feature-minus-environment`. `words.yaml` records PanGloss's answers and is red against hc.dll on purpose (`# oracle-provenance: forward-synthesis`; exact mismatch pinned in `rust/tools/oracle-conformance-known-divergences.json`).

hc.dll (subsumption): `ata` surfaces as `aa` (only t deleted) and `pa` as `a`. PanGloss: the unspecified `a` also satisfies `[nasal:-]`, so it is deleted too: `aa` and `a` have no analysis, and neither do `ata` or `pa` (obligatory rule). An empty surface cannot be tested.
