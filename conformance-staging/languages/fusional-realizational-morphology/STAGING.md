# fusional-realizational-morphology

Restored from the oracle pin `aab4c598` (`conformance/languages/fusional-realizational-morphology` there) after the suite
`sillsdev/fieldworks-parser-conformance` retired it as not FieldWorks-producible; see
`conformance/docs/engine-only-fixture-retirement.md`. The suite's reason: 3, 4, 9, 10: `RealizationalRule` `rrPast`, three custom MPR groups, `outputObligatoryFeatures` on a compound, required and excluded MPR features on the compound head (`mrCompoundConstrained`).

It tested: a fusional language with realizational tense, ablaut, stem names and constrained compounding.

PanGloss still pins this behavior in its own tests and keeps the engine behavior, so the fixture
stays here as a `local_regression`: it is not proposed upstream and must not graduate. The grammar
and `words.yaml` are the oracle pin's bytes, unchanged.
