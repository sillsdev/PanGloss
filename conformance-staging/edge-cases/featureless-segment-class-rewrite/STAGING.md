# featureless-segment-class-rewrite

**Classification: BUG PIN (PanGloss issue #120; HC-Rust differed from the oracle on 6 of 11 words before the fix)**

## What it tests

A rewrite rule written with segment natural classes, `[k t] -> [s z] / [a i] _ [a i]`, in a grammar
with no phonological feature system. With no features a segment's feature structure is its StrRep
alone, and a segment class carries the union of its members' StrReps (`SegmentNaturalClass.cs`,
`CharacterDefinitionTable.cs`), so the rule rewrites the StrRep of the matched node.

`asa`, `aza`, `isa` and `iza` need the rule to change `k`/`t` to the right-hand class in synthesis
and to restore the left-hand class in analysis. `aka`, `ita`, `ata` and `aki` are negative controls:
the rule is obligatory, so the unchanged surface is not a parse. `aska`, `kaa` and `aa` check that
the environment still binds.

## Results

C# founding oracle, sillsdev/machine aab4c598 (hc-conformance self-check, in-process Morpher),
2026-11-03: 1 passed, 0 failed. The oracle prints the surface of a parse as `a[sz]a`.

HC-Rust at main f4ed795c, before the fix: `asa`, `aza`, `isa`, `iza` gave no parse and `aka`, `ita`
parsed (6 of 11 words wrong). After the fix it matches on all 11 words.

## FieldWorks-producible

`HCLoader` emits a `SegmentNaturalClass` for any segment-list class and passes `fs = null` for a
phoneme with no feature specs, so a project whose phonemes carry no features and whose rule uses
natural classes made of segment lists produces this grammar. Whether the FLEx rule editor lets a
user put a segment-list class in the Result box is not known; LCM stores it either way.

## Provenance

Oracle of record: C# founding oracle, `hc-conformance --fixtures <root>` self-check built from
sillsdev/machine aab4c598, one run, 2026-11-03. Synthetic data only.

Promotion status: upstream_candidate.
Upstream fixture PR: not opened; candidate destination is machine/conformance/edge-cases/featureless-segment-class-rewrite/.
