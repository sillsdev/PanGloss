# cross-table-root-respelling

Restored from the oracle pin `aab4c598` (`conformance/edge-cases/cross-table-root-respelling` there) after the suite
`sillsdev/fieldworks-parser-conformance` retired it as not FieldWorks-producible; see
`conformance/docs/engine-only-fixture-retirement.md`. The suite's reason: 1: tables `tIn` and `tFin`.

It tested: a root entered on an inner stratum surfaces spelled by the final table's segment with the same feature bundle, with no phonological rule.

PanGloss still pins this behavior in its own tests and keeps the engine behavior, so the fixture
stays here as a `local_regression`: it is not proposed upstream and must not graduate. The grammar
and `words.yaml` are the oracle pin's bytes, unchanged.
