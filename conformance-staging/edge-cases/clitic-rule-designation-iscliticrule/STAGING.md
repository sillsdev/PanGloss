# clitic-rule-designation-iscliticrule

The oracle pin's `edge-cases/clitic-rule-designation` (`aab4c598`), under a distinct name. The suite
keeps a fixture called `clitic-rule-designation` but reshaped it: FieldWorks never sets
`isCliticRule`, so the suite's version dropped it, and with it the words `sagdt` and `sagds` that
`clitic_template_state_gate` and `final_template_pruning_gate` pin. See
`conformance/docs/engine-only-fixture-retirement.md`.

This copy keeps the `isCliticRule` grammar and its words. It is a `local_regression`: PanGloss still
implements `isCliticRule`, it is not proposed upstream and must not graduate. The graduation guard
forbids one name in both roots, which is why this copy has its own name.
