# adhoc-prohibition-whynot

Synthetic. One ad hoc allomorph prohibition on a prefix: the prefix ta- (`subTa`) excludes the stem allomorph `aDa`, the plainest form `HCLoader.LoadAllomorphCoOccurrenceRules` writes (HCLoader.cs:2163-2189). The fixture carries the suite's first why-not row.

## What it pins

- `da`: parses. `taga`: parses (ga has no prohibition).
- `tada`: no analysis; the only derivation TA+DA is excluded by the prohibition.
- `whynot.yaml`, row for `tada`: asked for TA+DA, the first failure is `adhoc_prohibition` / `AllomorphCoOccurrenceRules`, naming `subTa` and then `aDa`. Format: PROTOCOL.md section 11 in the conformance suite repository.

## Status

Copied from the suite at 89ff086 for the PanGloss why-not runner test. The parse rows come from the reference engine (`words.yaml` header). The why-not row is `status: pending` in the suite; PanGloss's runner answers it as `whynot.yaml` states (`tests/why_not_cli.rs`), and the copied `whynot.yaml` is not read by any code.

## FieldWorks witness

None. `fieldworks_producible: true` is a claim read from `HCLoader.cs`, not a round trip.
