# STAGING: realizational-reduplication-no-proposal-route

## Why this fixture exists

This synthetic HC-XML grammar uses a `RealizationalRule` that copies its complete input twice.
That is the HC-XML-only shape behind `reduplication.no-proposal-route`: the FST reduplication
peeler owns `AffixProcessRule`, while a realizational rule receives no peel proposal and is not
selected for structural composition. The capability gate must refuse the grammar with this
variant's exact id.

The grammar is invented test data and does not represent any language. `words.yaml` carries
answers produced by the C# founding oracle (`hc.dll`) from Machine commit
`18cf242f4b114b0eb9bac304b4b171ca2f499a39`. The oracle returns `ROOT|ta`, `ROOT+RED|tata`, and
no analysis for `tat`; the regression test checks the Rust HermitCrab parser against those same
answers.

## FieldWorks authoring limit

FieldWorks' `HCLoader.cs:976-979` returns an `AffixProcessRule` from its inflectional affix process
loader and marks realizational affix process support as a TODO. The fixture's `RealizationalRule`
cannot be produced by FieldWorks' HCLoader.

Graduation target: `machine/conformance/edge-cases/realizational-reduplication-no-proposal-route/`.

Upstream PR: none (not opened).
