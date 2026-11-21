# swap-across-a-deleted-node-keeps-the-gap-before-suffix

Pins swap across a deleted node keeps the gap before suffix (a boundary prefix, roots R0 and R1, a suffix; a deletion rule, then a swap rule whose switches sit on both sides of the deleted node). C# SynthesisMetathesisRuleSpec.ApplyRhs moves only the two switch groups, so a deleted node between them keeps its position; a Rust port that moves the deleted-node marks with the swap puts the null prefix in the wrong place.

C# verdict: rows are `hc-conformance --propose` output from machine 1ddef2bd (source: Fable's g4-gap-then-swap-across-the-gap probe of the #405 review). Source of the bug: the Rust port; no Machine issue is warranted.

FieldWorks-producible: ordinary affix-process rules plus rewrite and metathesis or compounding rules.
Upstream PR: not opened; the fixture graduates with the next `conformance.commit` bump after it is proposed to sillsdev/fieldworks-parser-conformance.
