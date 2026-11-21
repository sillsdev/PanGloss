# metathesis-boundary-context-left-right-to-left

Prefix `a+` (PRE), root `st` and the right-to-left rule `s t -> t s / a _`. The boundary sits between the context `a` and the left switch `s`. Right to left, C# matches `t`, then `s`, then steps over the boundary and gives it to the group of `s`, the node matched just before it; only segments move, so `t` moves in front of the boundary: C# accepts `ats` and `at+s` as `PRE+Z|at+?s` and rejects `a+ts`. The pre-fix Rust HC left the boundary after the context and gave `PRE+Z|a+?ts`.

Classification: Rust HC differed from the founding oracle (PanGloss #374, review finding on boundary attribution next to a context node); the fix is in this change, so the fixture is green on both engines. Red on revert: the 02fa51bb Rust HC fails `ats`, `at+s` and `a+ts`.

Oracle: signatures come from C# HermitCrab at Machine `1ddef2bd2f1503717de95556116df8e4a63846d6` (self-check driver). The word list holds every word over `a`, `p`, `t`, `s` and `+` up to length four that C# accepts, plus three rejected counterparts. All lexemes are synthetic.

FieldWorks-producible: yes; `multipleApplicationOrder` is written by HCLoader 2152-2170, metathesis environments by the same loader, and the `+` on the inside edge of a prefix form at HCLoader 1649.

upstream: https://github.com/sillsdev/fieldworks-parser-conformance/pull/50
