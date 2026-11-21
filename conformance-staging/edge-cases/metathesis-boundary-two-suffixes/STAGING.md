# metathesis-boundary-two-suffixes

Roots `p`, `pa` and `tp`, suffixes `+as` (SA) and `+ta` (ST) on a linear stratum, and the rule `s t -> t s`. With both suffixes the `s` of `+as` meets the `t` of `+ta` across their boundary, and C# swaps them: `patsa`, `p+atsa`, `pat+sa` and `p+at+sa` are all `R0+SA+ST|p+?at+?sa`. The morph order does not change. C# moves only segments, so the boundary between the suffixes stays where it was, with `t` now before it and `s` after it.

Classification: an FST soundness case for PanGloss #374. Rust HC already agrees with C#; the round-4 FST admitted this grammar and missed every swapped word (review finding R4-P1-1), because its refusal looked only for a swap that changes morph order. Words long enough to hold a root and two affixes are the point of the fixture.

Oracle: signatures come from C# HermitCrab at Machine `1ddef2bd2f1503717de95556116df8e4a63846d6` (self-check driver). The word list holds each root alone and with each suffix, the swapped two-suffix words with and without each boundary, and six rejected counterparts (the unswapped `pasta` family). All lexemes are synthetic.

FieldWorks-producible: yes; FieldWorks writes metathesis rules with the later node as `leftSwitch` (HCLoader 2171) and the `+` on the inside edge of a suffix form (HCLoader 1623).

upstream: https://github.com/sillsdev/fieldworks-parser-conformance/pull/50
