# prefixal-discontinuous-slot-dependency

Restored from the oracle pin `aab4c598` (`conformance/languages/prefixal-discontinuous-slot-dependency` there) after the suite
`sillsdev/fieldworks-parser-conformance` retired it as not FieldWorks-producible; see
`conformance/docs/engine-only-fixture-retirement.md`. The suite's reason: 11: MPR output from the slot rule `mrModeTrans`, required by a later slot.

It tested: a prefixing language where a later slot depends on an earlier, non-adjacent one.

PanGloss still pins this behavior in its own tests and keeps the engine behavior, so the fixture
stays here as a `local_regression`: it is not proposed upstream and must not graduate. The grammar
and `words.yaml` are the oracle pin's bytes, unchanged.
