# bistratal-overlapping-segment-representation

Restored from the oracle pin `aab4c598` (`conformance/edge-cases/bistratal-overlapping-segment-representation` there) after the suite
`sillsdev/fieldworks-parser-conformance` retired it as not FieldWorks-producible; see
`conformance/docs/engine-only-fixture-retirement.md`. The suite's reason: 1: two character tables, one per stratum.

It tested: whether a segment spelled differently on two strata is accepted or refused when each stratum has its own lexicon (the refusal side of shared representation).

PanGloss still pins this behavior in its own tests and keeps the engine behavior, so the fixture
stays here as a `local_regression`: it is not proposed upstream and must not graduate. The grammar
and `words.yaml` are the oracle pin's bytes, unchanged.
