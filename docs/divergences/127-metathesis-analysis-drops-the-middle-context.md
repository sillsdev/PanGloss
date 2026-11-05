# 127: Metathesis analysis drops the middle context, as hc.dll does

Kind: decision record (PanGloss reproduces a C# defect so it never parses differently from hc.dll).
Status: permanent until sillsdev/machine fixes it; then remove `build_analysis_pattern`'s drop and the staged fixture's `kita` expectation together. Issue #162 (ledger rows 122 and 123).

## Decision

A metathesis rule with a segment between its two switches (`t i k -> k i t`) is applied by synthesis with the middle segment left in place, but un-applied by analysis as if the switches were adjacent. So hc.dll never recovers the reorder it produced: root `tika` surfaces as `kita` and `kita` has only the analysis of root `kita`. PanGloss 0.15.2 kept the middle segment in its analysis pattern and so found `kita` for `tika`. That is the better answer, but it is a parse hc.dll does not give, and the oracle rule wins. PanGloss now drops the middle context too.

This is plainly a C# defect (synthesis and analysis disagree about the same rule); it is copied here deliberately, not silently. A middle context is authorable in FieldWorks (the rule's `StrucChange` can hold a context between the switches).

## C# site

`AnalysisMetathesisRuleSpec` (`src/SIL.Machine.Morphology.HermitCrab/PhonologicalRules/AnalysisMetathesisRuleSpec.cs`), constructor: the rebuilt pattern is the nodes before the first `Group`, the two switch groups, and the nodes after the last `Group` (`TakeWhile(n => !(n is Group))` from each end). Everything between the groups is lost. `SynthesisMetathesisRuleSpec.ApplyRhs` (`MoveNodesAfter`) leaves it in its slot.

## Rust site

`pg-rules/src/metathesis.rs`, `build_analysis_pattern`.

## Verification

C# replay on `hc.dll` (machine `4d9b454d`, `hc-conformance` self-check) of `conformance-staging/edge-cases/metathesis-middle-context`: `kita` has exactly one analysis (`KITA|kita`), `tika` none. Unit pin `pg-rules/tests/metathesis_gate.rs::metathesis_middle_context_is_not_unapplied_as_in_csharp`. No Machine issue filed yet; to file: "Analysis metathesis drops the middle context that synthesis keeps".
