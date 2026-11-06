# 127: hc.dll drops the middle context of a metathesis rule in analysis; PanGloss keeps it

Kind: behavioural (PanGloss is correct; hc.dll has a C# defect).
Status: temporary-permanent: remove when sillsdev/machine#619 lands and the oracle pin moves. Then delete this entry, the `forward-synthesis` marker in the staged fixture and its `oracle-conformance-known-divergences.json` row. Issue #162 (ledger rows 122 and 123).

## Decision

A metathesis rule with a segment between its two switches (`t i k -> k i t`) is applied by synthesis with the middle segment left in place, but un-applied by hc.dll's analysis as if the switches were adjacent. hc.dll therefore never recovers the reorder it produced: root `tika` surfaces as `kita`, and `kita` gets only the analysis of root `kita`. PanGloss keeps the middle segment in its analysis pattern, so `kita` has two analyses (`KITA|kita`, `TIKA|kita`) and `tika` none.

Owner decision 2026-11-05: this is a plain C# bug, filed as [sillsdev/machine#619](https://github.com/sillsdev/machine/issues/619) with a machine fix PR being prepared. PanGloss must not copy it. (PanGloss 0.16.0 briefly copied it; that is reverted.) A middle context is authorable in FieldWorks (the rule's `StrucChange` can hold a context between the switches).

Until the fix lands PanGloss gives a parse hc.dll does not: `kita` -> `TIKA|kita`. That is an over-parse relative to the oracle, by design.

## C# site

`AnalysisMetathesisRuleSpec` (`src/SIL.Machine.Morphology.HermitCrab/PhonologicalRules/AnalysisMetathesisRuleSpec.cs`), constructor: the rebuilt pattern is the nodes before the first `Group`, the two switch groups, and the nodes after the last `Group` (`TakeWhile(n => !(n is Group))` from each end). Everything between the groups is lost. `SynthesisMetathesisRuleSpec.ApplyRhs` (`MoveNodesAfter`) leaves it in its slot.

## Rust site

`pg-rules/src/metathesis.rs`, `build_analysis_pattern` (keeps every middle node except a boundary, which can never appear in the analysis match sequence).

## Verification

C# replay on `hc.dll` (machine `4d9b454d`, `hc-conformance` self-check) of `conformance-staging/edge-cases/metathesis-middle-context`: `kita` has exactly one analysis (`KITA|kita`), `tika` none. The fixture now records the forward-synthesis answer (`# oracle-provenance: forward-synthesis`) and is listed in `rust/tools/oracle-conformance-known-divergences.json` as an exact reviewed mismatch. Unit pin `pg-rules/tests/metathesis_gate.rs::metathesis_middle_context_node_round_trips`.
