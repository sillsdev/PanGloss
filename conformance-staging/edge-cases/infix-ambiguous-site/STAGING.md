# Infix ambiguous site

Regression pin for PanGloss #297: an infix whose environment fits two sites in the stem (`mime`, insert `gu`
after or before a vowel). The two-part shape is the one `HCLoader` writes for a form-allomorph infix. All
lexemes are invented.

C# verdict: `migume` and `mimgue` parse, `mimegu` and `mguime` do not. C# applies an allomorph at one match
only (`SynthesisAffixProcessRule` takes `SingleOrDefault`), and `Fst.ResultCompare` breaks the tie between
equal-span matches on arc priority, so the greedy `any*` wins. Replayed with `hc-conformance` at machine
`4d9b454d` (six identical runs). The `expect_fail` rows record that the other site is never synthesized.

Source: the Rust port. `result_compare` dropped the arc-priority tiebreak, so ties fell to traversal order.

FieldWorks-producible: yes (`HCLoader.LoadFormAffixProcessAllomorph`, infix branch).
Upstream: staged ahead of fieldworks-parser-conformance branch `fix/infix-empty-tail` (commit a2ead345),
`edge-cases/infix-ambiguous-site`.
