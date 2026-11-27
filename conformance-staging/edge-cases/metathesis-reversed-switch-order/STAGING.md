# metathesis-reversed-switch-order

Roots `ps`, `pas` and `s`, prefix `as+` and suffixes `+t` (SUF0) and `+a` (SUF1) on an unordered stratum, and the rule `s t -> t s` with feature natural classes, whose `leftSwitch` names the earlier node `s`. C# `SynthesisMetathesisRuleSpec.ApplyRhs` (lines 103-109) swaps the two groups so that the later-starting one goes first. With the root's boundary between the switches, `s+t` becomes `t+s` (`SUF0+R2|t+?s`); C# moves only segments, so the boundary stays where it was. Before this change Rust HC kept the authored order and produced `+st`, which C# never produces.

Classification: Rust HC differed from the founding oracle (PanGloss #374, review finding R4 P2-A, Opus); the fix is in this change, so the fixture is green on both engines.

Oracle: signatures come from C# HermitCrab at Machine `1ddef2bd2f1503717de95556116df8e4a63846d6` (self-check driver), from Opus round-4 probe `m6-ltr-edge-after-last`. The word list holds the swapped words with and without each boundary, controls, and ten rejected counterparts, among them every `+st` form the pre-fix Rust HC produced. All lexemes are synthetic.

FieldWorks-producible: no. FieldWorks always names the later group as `leftSwitch` (fieldworks-cannot-say row 5), so this fixture is engine-only and not for graduation; hand-written HermitCrab XML and `pg-grammar-gen`'s metathesis recipe can write it.

upstream: not-producible: row 5 a metathesis whose leftSwitch names the earlier group
