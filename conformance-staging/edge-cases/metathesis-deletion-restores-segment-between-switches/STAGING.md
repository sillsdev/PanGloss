# metathesis-deletion-restores-segment-between-switches

Prefix `aps+`, roots `t`, `pt` and `tap`, the metathesis `s t -> t s` across the prefix boundary, and the deletion `p -> 0`. Pins divergence 132: analysis un-applies the deletion first and restores an optional `p` between the switches, and that `p` must not take a switch's place when the swap is undone.

Classification: an HC-algorithm bug, the mechanism of sillsdev/machine#520 and divergence 050; its own issue is sillsdev/machine#649. C# HermitCrab at Machine `1ddef2bd` generates `ats` from `PRE+R0` and `atsa` from `PRE+R2` (`Morpher.GenerateWords`) but cannot parse either, with or without the boundary written.

Oracle: forward-synthesis for `ats`, `at+s`, `atsa` and `at+sa` (hand-traced in `words.yaml`, confirmed by C#'s own `GenerateWords`); every other row recorded from C# at Machine `1ddef2bd2f1503717de95556116df8e4a63846d6` (self-check driver). The C# replay fails exactly the four derived rows; the exact reason text is in `rust/tools/oracle-conformance-known-divergences.json`. PanGloss replays every row. All lexemes are synthetic.

FieldWorks-producible: yes (`HCLoader.LoadMetathesisRule`, `LoadRewriteRule` for a context-free deletion, and a prefix whose form ends in a boundary).

Upstream: sillsdev/machine#649 (filed by the conformance lead) and the suite copy in the PR below, which records `ats`, `at+s`, `atsa` and `at+sa` as `expect_fail` (hc.dll's answer) with these parses in each row's note. Once a pin includes it, delete this staged copy; PanGloss's four parses are then excused as divergence 132 by the `pangloss_expectations` rows in `rust/tools/oracle-conformance-known-divergences.json`.

upstream: https://github.com/sillsdev/fieldworks-parser-conformance/pull/50
