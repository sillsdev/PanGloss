# prefix-required-environment-after-epenthesis

Kept from the suite pin `0cc97874` (`conformance/edge-cases/prefix-required-environment-after-epenthesis` there) after
`sillsdev/fieldworks-parser-conformance` retired it at `df2b0e7` as not FieldWorks-producible; see
`conformance/docs/engine-only-fixture-retirement.md`. The suite's reason: row 14: a right-hand `RequiredEnvironments` on a lone prefix (HCLoader writes only a left environment for a prefix, folding the right side into the input pattern).

It tested: a required right environment of a prefix with epenthesis between prefix and root (PanGloss #405 follow-up).

PanGloss keeps the engine behavior, so the fixture stays here as a `local_regression`: it is not
proposed upstream and must not graduate. The grammar and `words.yaml` are the old pin's bytes (rows
from the founding oracle Machine e63b8863), except that `fieldworks_producible` is now `false` with the
reason.

upstream: not-producible: row 14 a right-hand RequiredEnvironments on a lone prefix
