# metathesis-boundary-context-segments

Roots `st` and `pst`, suffixes `+a` (SUF0), `+p` (SUF1) and `+ap` (SUF2) on an unordered stratum, and the rule `s t -> t s / _ a p`, every node a one-member segment natural class. With `+a+p` or `+ap` after the root, the right context crosses the root's boundary. C# gives that skipped boundary to the node matched just before it, the switch `t`, so the swap puts `t` before the boundary and `s` after it: `tsap` is `R0+SUF0+SUF1|t+?sa+?p` and `R0+SUF2|t+?sap`. C# moves only segments; the boundary stays where it was.

Classification: an FST soundness case for PanGloss #374. Rust HC already agrees with C#; the round-4 FST admitted this grammar and missed every swapped word (review finding R4 P1, Opus), because the context-node fixtures with feature classes were refused for an unrelated reason.

Oracle: signatures come from C# HermitCrab at Machine `1ddef2bd2f1503717de95556116df8e4a63846d6` (self-check driver). The word list holds the unswapped words, the swapped words with and without each boundary, and six rejected counterparts. All lexemes are synthetic.

FieldWorks-producible: yes; FieldWorks writes metathesis environments (`fw-metathesis-switch-indices`), segment natural classes (HCLoader 2848-2866) and the `+` on the inside edge of a suffix form (HCLoader 1623).

upstream: https://github.com/sillsdev/fieldworks-parser-conformance/pull/50
