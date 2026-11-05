# metathesis-middle-context

**Classification: ORACLE DEFECT PINNED (PanGloss issue #162; ledger rows 123 and 127).** A metathesis rule with a middle context, `t i k -> k i t`. Synthesis keeps the `i`; hc.dll's analysis drops it from the search pattern (`AnalysisMetathesisRuleSpec` keeps only the nodes outside the switch groups), so the reorder of root `tika` into `kita` is never recovered. `kita` therefore has one analysis (root `kita`) and `tika` none. PanGloss reproduces this (divergence 127); before the fix it also found `TIKA|kita` and missed the root-only parse of `kita` through the featureless-matching bug of row 122.

Oracle of record: C# `hc-conformance` self-check, sillsdev/machine 4d9b454d, 2026-11-05: PASS. Synthetic data only. Machine issue not yet filed.

FieldWorks-producible: yes, a `PhMetathesisRule` whose `StrucChange` has a context between the switches (`leftSwitch`/`rightSwitch` around it).

Promotion status: upstream_candidate once machine decides whether the drop is intended; if machine fixes it, `kita` gains a second analysis.
