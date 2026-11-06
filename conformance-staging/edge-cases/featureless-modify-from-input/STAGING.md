# featureless-modify-from-input

**Classification: KNOWN PANGLOSS DIVERGENCE (divergence 128, partial-feature grammars; was BUG PIN for issue #121)**

## What it tests

A rule `mrModS` rewrites the vowel after the first consonant (class V = {a i}) to the segment class {i} with
`ModifyFromInput`, copies the rest and appends s. The grammar has no phonological feature system, so a
segment's feature structure is its StrRep alone. (The issue's repro was a reduplication rule with the same
modify; the suffix form pins the same mechanism without the copy-agreement gap the FST scoreboard records for
featureless reduplication.)

C# undoes a modification in analysis by adding the context's `AntiFeatureStruct()` to the node
(`AnalysisMorphologicalTransform.cs:34`, applied at `AddCapturedPartNodes`). `AntiFeatureStruct` negates
symbolic features but copies built-in ones (`HermitCrabExtensions.cs:29`), and StrRep is built in. So the node
ends with StrRep = its own spelling united with the class's spellings, and the lexical root must unify with it.
`kitas` and `sikas` parse because the roots `kita` and `sika` already spell the vowel i; the root `kata` would
also give `kitas` by synthesis but is not recovered. `tisis` and `pisas` (roots `tasi`, `pasa`) do not parse:
the node is restricted to {i}. Widening the class to {a i} would recover them. A grammar with a feature system
never hits this, because a segment with authored features carries no StrRep (`CharacterDefinitionTable.Add`);
the pg-parse test `featureless_modify_from_input` replays that control on the same grammar with one added
feature.

## Results

C# founding oracle, sillsdev/machine 4d9b454d driver (hc-conformance self-check, in-process Morpher),
2026-11-05: 1 passed, 0 failed. A one-feature variant of this grammar (control, pinned in the Rust test) parses `tisis` and `pisas` on the
same driver.

## FieldWorks-producible

Yes, by reading FieldWorks source: `HCLoader` loads `MoModifyFromInput.ModificationRA` through
`TryLoadSimpleContext`, which accepts a segment-list natural class (`IPhNCSegments`) whose phonemes carry no
feature specs. A project with featureless phonemes and a modify rule whose result is a segment class produces
this grammar.

## Provenance

Oracle of record: C# founding oracle, hc-conformance self-check, one run, 2026-11-05. Synthetic data only.

Promotion status: upstream_candidate.
Upstream fixture PR: not opened; candidate destination is machine/conformance/edge-cases/featureless-modify-from-input/.

## PanGloss divergence (divergence 128)

Owner decision 2026-11-05: PanGloss deliberately covers featureless (XAmple-style) grammars through full HermitCrab feature grammars, and keeps the generous behaviour where hc.dll's behaviour comes from missing features. So this fixture's `words.yaml` now records PanGloss's answers and is red against hc.dll on purpose (`# oracle-provenance: forward-synthesis`; exact mismatch pinned in `rust/tools/oracle-conformance-known-divergences.json`).

hc.dll's answers (the text above, 2026-11-05): `kitas` has one analysis (KITA), `tisis` and `pisas` have none. PanGloss: `kitas` has two (KITA, KATA), `tisis` is TASI+MODS and `pisas` is PASA+MODS, as in a grammar with a feature system. `kakas` and `tata` parse in neither. hc.dll mismatch against this `words.yaml`: `kitas` (got KITA only), `tisis` and `pisas` (got none).
