# loader-isactive-breadth

Restored from the oracle pin `aab4c598` (`conformance/edge-cases/loader-isactive-breadth` there) after the suite
`sillsdev/fieldworks-parser-conformance` retired it as not FieldWorks-producible; see
`conformance/docs/engine-only-fixture-retirement.md`. The suite's reason: 1, 7, 8: three character tables, `isActive="no"` on more than a dozen kinds, `family` twice.

It tested: an inactive decoy beside the live declaration for each collection the XML loader filters.

PanGloss still pins this behavior in its own tests and keeps the engine behavior, so the fixture
stays here as a `local_regression`: it is not proposed upstream and must not graduate. The grammar
and `words.yaml` are the oracle pin's bytes, unchanged.
