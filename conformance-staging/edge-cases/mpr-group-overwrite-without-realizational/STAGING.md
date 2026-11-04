# mpr-group-overwrite-without-realizational

Restored from the oracle pin `aab4c598` (`conformance/edge-cases/mpr-group-overwrite-without-realizational` there) after the suite
`sillsdev/fieldworks-parser-conformance` retired it as not FieldWorks-producible; see
`conformance/docs/engine-only-fixture-retirement.md`. The suite's reason: 4: a custom-named group (`ThemeGroup`).

It tested: an overwrite-mode MPR group in a grammar with no realizational rule.

PanGloss still pins this behavior in its own tests and keeps the engine behavior, so the fixture
stays here as a `local_regression`: it is not proposed upstream and must not graduate. The grammar
and `words.yaml` are the oracle pin's bytes, unchanged.
