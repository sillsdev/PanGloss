# metathesis-boundary-context-right

Root `st`, suffix `+a` (SUF) and the left-to-right rule `s t -> t s / _ a`. The boundary sits between the right switch `t` and the context `a`. C# gives a skipped boundary to the pattern node matched just before it, here the switch `t`; only segments move, so `s` moves after `t` and the boundary: C# accepts `tsa` and `t+sa` as `Z+SUF|t+?sa` and rejects `ts+a`. The pre-fix Rust HC left the boundary before the context and gave `Z+SUF|ts+?a`.

Classification: Rust HC differed from the founding oracle (PanGloss #374, review finding on boundary attribution next to a context node); the fix is in this change, so the fixture is green on both engines. Red on revert: the 02fa51bb Rust HC fails `tsa`, `t+sa` and `ts+a`.

Oracle: signatures come from C# HermitCrab at Machine `1ddef2bd2f1503717de95556116df8e4a63846d6` (self-check driver). The word list holds every word over `a`, `p`, `t`, `s` and `+` up to length four that C# accepts, plus three rejected counterparts. All lexemes are synthetic.

FieldWorks-producible: yes; FieldWorks writes metathesis environments (`fw-metathesis-switch-indices`) and the `+` on the inside edge of a suffix form (HCLoader 1623).

upstream: https://github.com/sillsdev/fieldworks-parser-conformance/pull/50
