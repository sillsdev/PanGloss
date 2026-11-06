# metathesis-middle-context

**Classification: KNOWN ORACLE DIVERGENCE, PanGloss correct (PanGloss issue #162; sillsdev/machine#619; divergence 127).** A metathesis rule with a middle context, `t i k -> k i t`. Synthesis keeps the `i`; hc.dll's analysis drops it from the search pattern (`AnalysisMetathesisRuleSpec` keeps only the nodes outside the switch groups), so the reorder of root `tika` into `kita` is never recovered. The correct answer is two analyses of `kita` (`KITA|kita`, `TIKA|kita`) and none for `tika`. hc.dll gives only `KITA|kita`.

Oracle: forward-synthesis (`# oracle-provenance: forward-synthesis`). hc.dll (self-check, sillsdev/machine 4d9b454d, 2026-11-05) reports the single-analysis `kita`; the matching entry is in `rust/tools/oracle-conformance-known-divergences.json`. Synthetic data only.

FieldWorks-producible: yes, a `PhMetathesisRule` whose `StrucChange` has a context between the switches.

Graduation: remove the known-divergence entry and the provenance marker once machine#619 is fixed and the oracle pin moves; then the fixture graduates to `conformance/edge-cases/`.
