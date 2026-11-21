# metathesis-boundary-between-switches-root

One root `t+p` (Z) with an inner boundary, no compounding rule, and the rule `t p -> p t`. C# accepts `pt` and `p+t` and rejects `tp` and `t+p`; the pre-fix Rust HC did the reverse (issue B).

Classification: Rust HC differed from the founding oracle (PanGloss #374); the fix is in this change, so the fixture is green on both engines. The metathesis rule has a two-node structural description and no environment.

Oracle: signatures come from C# HermitCrab at Machine `1ddef2bd2f1503717de95556116df8e4a63846d6` (self-check driver, suite `a0094a3f97ddaa4a4dc4c4718ba6ea44869e254d` layout). The word list holds every word over `p`, `t`, `s` and `+` up to length four that C# accepts, plus the listed rejected counterparts. `rules:` holds the C# self-check rule trace. All lexemes are synthetic.

FieldWorks-producible: no. The root spells `+` between two of its own segments (fieldworks-cannot-say row 29), so this fixture is engine-only and not for graduation.

upstream: not-producible: row 29 a root spells + between two of its own segments
