# mpr-overwrite-order-dependence

Restored from the oracle pin `aab4c598` (`conformance/edge-cases/mpr-overwrite-order-dependence` there) after the suite
`sillsdev/fieldworks-parser-conformance` retired it as not FieldWorks-producible; see
`conformance/docs/engine-only-fixture-retirement.md`. The suite's reason: 4: an overwrite group and an append group.

It tested: on an unordered stratum, an overwrite group keeps only what the last rule wrote, so the result depends on application order.

PanGloss still pins this behavior in its own tests and keeps the engine behavior, so the fixture
stays here as a `local_regression`: it is not proposed upstream and must not graduate. The grammar
and `words.yaml` are the oracle pin's bytes, unchanged.
