# metathesis-boundary-null-prefix

Prefix `as+` (PRE), a null prefix `^0+` (NUL, no segment), roots `t`, `ta`, `tt` and the rule `s t -> t s`. When the null prefix sits between `as+` and the root, three boundaries (`+ ^0 +`) lie between the switches. C# gives them to the group of `s`, the switch matched before them; only segments move, so `t` moves in front of them and C# lists the null prefix after the root: `ats` has the analyses `NUL+PRE+R0`, `PRE+R0` and `PRE+R0+NUL`, and `at++s`, `at^0s`, `at^0+s` and `at+^0+s` have only `PRE+R0+NUL`.

Classification: Rust HC differed from the founding oracle before this change (14 words fail on origin/main); the fix is in this change, so the fixture is green on both engines. The FST route at 02fa51bb admitted this grammar and missed `PRE+R0+NUL` (8 of 20 words); it now refuses it with `metathesis_across_morphs`, because a morph with no segment can be overtaken by a switch (PanGloss #374).

Oracle: signatures come from C# HermitCrab at Machine `1ddef2bd2f1503717de95556116df8e4a63846d6` (self-check driver). The words are chosen from a sweep over `a`, `t`, `s`, `+` and `^0` that C# accepts, plus two rejected counterparts. All lexemes are synthetic.

FieldWorks-producible: yes; HCLoader writes a null allomorph `^0` of a prefix as `^0+` (1649, the null boundary with its four spellings at 2759) and the `+` on the inside edge of `as+`.

upstream: https://github.com/sillsdev/fieldworks-parser-conformance/pull/50
