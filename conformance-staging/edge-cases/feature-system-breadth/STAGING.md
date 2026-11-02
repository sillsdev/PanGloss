# feature-system-breadth

Restored from the oracle pin `aab4c598` (`conformance/edge-cases/feature-system-breadth` there) after the suite
`sillsdev/fieldworks-parser-conformance` retired it as not FieldWorks-producible; see
`conformance/docs/engine-only-fixture-retirement.md`. The suite's reason: 7: `isActive="no"` on a symbolic feature, a complex feature, a feature value, both natural class kinds and a lexical entry; 20: `ExcludedEnvironments` on the allomorph of `eSal`.

It tested: deactivation of feature-system elements, complex features, right-to-left metathesis and an alpha variable.

PanGloss still pins this behavior in its own tests and keeps the engine behavior, so the fixture
stays here as a `local_regression`: it is not proposed upstream and must not graduate. The grammar
and `words.yaml` are the oracle pin's bytes, unchanged.
