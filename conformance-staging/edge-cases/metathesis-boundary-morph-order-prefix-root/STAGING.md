# metathesis-boundary-morph-order-prefix-root

Prefix `s` (PRE), root `t` (Z) and the rule `s t -> t s`. After the swap C# lists the morphs in surface order, so `ts` is `Z+PRE`; the pre-fix Rust HC listed `PRE+Z`. No boundary is written, so this isolates morph order (issue A).

Classification: Rust HC differed from the founding oracle (PanGloss #374); the fix is in this change, so the fixture is green on both engines. The metathesis rule has a two-node structural description and no environment.

Oracle: signatures come from C# HermitCrab at Machine `1ddef2bd2f1503717de95556116df8e4a63846d6` (self-check driver, suite `a0094a3f97ddaa4a4dc4c4718ba6ea44869e254d` layout). The word list holds every word over `p`, `t`, `s` and `+` up to length four that C# accepts, plus the listed rejected counterparts. All lexemes are synthetic.

FieldWorks-producible: yes for the grammar shape (a metathesis rule over a stem or affix whose lexical form writes a boundary); the boundary-writing forms are the point of the fixture.

upstream: https://github.com/sillsdev/fieldworks-parser-conformance/pull/50
