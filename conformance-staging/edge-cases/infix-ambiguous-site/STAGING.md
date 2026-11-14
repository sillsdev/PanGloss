# Infix ambiguous site

Regression pin for PanGloss #297: an infix whose environment fits two sites in the stem (`mime`, insert `gu`
after or before a vowel). The two-part shape is the one `HCLoader` writes for a form-allomorph infix. All
lexemes are invented.

C# verdict: `migume` and `mimgue` parse, `mimegu` and `mguime` do not. C# applies an allomorph at one match
only (`SynthesisAffixProcessRule` takes `SingleOrDefault`), and its determinized matcher prefers the greedy
`any*` branch (High-priority quantifier arcs, determinized in arc order). Replayed with `hc-conformance` at machine
`4d9b454d` (six identical runs). The `expect_fail` rows record that the other site is never synthesized.

Source: the Rust port. Its matcher NFA and subset construction differed from C# (arc priority types, arc insertion order, epsilon-closure keying), so the determinized matcher took the first site. The `result_compare` tiebreak is not involved: both engines determinize synthesis patterns.

FieldWorks-producible: yes (`HCLoader.LoadFormAffixProcessAllomorph`, infix branch).
Upstream: staged ahead of fieldworks-parser-conformance branch `fix/infix-empty-tail` (commit a2ead345),
`edge-cases/infix-ambiguous-site`.
