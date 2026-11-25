# prefix-required-environment-after-epenthesis-later-compound

Kept from the suite pin `0cc97874` (`conformance/edge-cases/prefix-required-environment-after-epenthesis-later-compound` there) after
`sillsdev/fieldworks-parser-conformance` retired it at `df2b0e7` as not FieldWorks-producible; see
`conformance/docs/engine-only-fixture-retirement.md`. The suite's reason: row 14: as above.

It tested: the same environment with a compounding rule rebuilding the shape in a later stratum.

PanGloss keeps the engine behavior, so the fixture stays here as a `local_regression`: it is not
proposed upstream and must not graduate. The grammar and `words.yaml` are the old pin's bytes (rows
from the founding oracle Machine e63b8863), except that `fieldworks_producible` is now `false` with the
reason.

upstream: not-producible: row 14 as above
