# family-blocked-generation

Kept from the suite pin `0cc97874` (`conformance/edge-cases/family-blocked-generation` there) after
`sillsdev/fieldworks-parser-conformance` retired it at `df2b0e7` as not FieldWorks-producible; see
`conformance/docs/engine-only-fixture-retirement.md`. The suite's reason: row 8: `family="famF"` on two entries (`LoadLexEntry` never reads a family).

It tested: a family relative blocking a derivation that shares its surface: `aat` parses as B twice (the plain lookup and the blocked A+T). PanGloss reports one (#423, #432; docs/divergences/134).

PanGloss keeps the engine behavior, so the fixture stays here as a `local_regression`: it is not
proposed upstream and must not graduate. The grammar and `words.yaml` rows are the old pin's bytes (rows
from the founding oracle Machine e63b8863; the `aat` row keeps C#'s two `B|aat`), except that
`fieldworks_producible` is now `false` with the reason. PanGloss reports one `B|aat`; that is recorded as
divergence 134 in `pangloss_expectations` (`staging:edge-cases/family-blocked-generation`).

upstream: not-producible: row 8 family on two entries
