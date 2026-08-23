# Historical five-language backend characterization (not current acceptance)

> **Historical pre-route baseline (2026-08-21).** This document records the measured static
> five-language matrix from before the current route work. It is retained as historical evidence;
> it is not the current shipping acceptance document, and none of its rows claims a trusted shipped
> FST. The active acceptance slice is Indonesian, Amharic, and Aweti; Mbugwe is deferred and does
> not block that slice. Historical measurements below are intentionally unchanged.

## Decision

PanGloss now makes an exact, fail-closed static backend decision for all five reference grammars. The
default selector retains a report for every backend and emits no trusted FST when every report is
Error or Critical.

| Language | TunedSurface | TemplatedUnderlyingTokens | PlanComposed | Default result |
|---|---|---|---|---|
| Indonesian | Error: closure work exceeds the default envelope | Critical: nonregular-process coverage gap | Critical: required plan subtrees are not materialized | No path |
| Sena | Ideal | Critical: nonregular-process coverage gap | Ideal | TunedSurface preferred; PlanComposed also admissible |
| Amharic | Error: closure work exceeds the default envelope | Critical: nonregular-process coverage gap | Critical: required plan subtrees are not materialized | No path |
| Aweti | Error: closure work exceeds the default envelope | Critical: nonregular-process coverage gap | Critical: required plan subtrees are not materialized | No path |
| Mbugwe | Error: closure work exceeds the default envelope | Critical: nonregular-process coverage gap | Critical: required plan subtrees are not materialized | No path |

The TunedSurface Error is `PGF0009 ProvenBoundExceedsBudget`, shape
`tuned-surface-resource-envelope`. The two correctness refusals are
`PGF0013 BackendCoverageIncomplete`, with shapes `nonregular-process-morphology` and
`plan-composed-missing-subtrees`. Error and Critical reports remain visible with their evidence and
advice; neither is selected for normal generation.

## Corpus identity

| Language | Grammar input | Grammar SHA-256 prefix | Word list SHA-256 prefix |
|---|---|---:|---:|
| Indonesian | `indonesian-hc.xml` | `e450110eac48` | `004d6aa362b8` |
| Sena | `sena-hc.xml` | `dac6fdba75b5` | `42f4b6d6bda0` |
| Amharic | `amharic-hc.xml` | `d5156ea82c6c` | `33124870ea1e` |
| Aweti | `aweti.json` | `f4d5426f177b` | `e888ce23f926` |
| Mbugwe | `mbugwe.fwdata` | `feceab0c9fde` | `ffa9acf34a42` |

The managed corpus wrapper printed and validated these identities before running the gate. Aweti's
optional fwdata input was `12eebb3beebb`; the acceptance grammar was the required JSON snapshot.

## Indonesian explicit retry

Indonesian is the one default Error with a demonstrated small retry envelope. Complete static
closure characterization visited 3,290 structural rule pairs, synthesized 3,072 successors,
reached depth 5, and emptied its worklist in 216 ms. A test-scoped retry with a 10,000-unit
closure-work limit admits TunedSurface as Ideal. This is a clean rerun from grammar state, not an
override and not an automatic escalation. The low-level selector currently accepts the numeric
limit directly; a product-facing catalog of named resource envelopes is not implemented here.

The retry result is static backend admission. It does not by itself claim that a full build or
corpus runtime finished. The existing Indonesian P6 corpus gate separately passed 114 corpus cases,
but that corpus-scoped result does not discharge the grammar-wide PlanComposed marker refusal.

## Scale evidence and non-results

- Sena is the only grammar with a clean default FST route. Its dedicated large-lexicon gate owns
  payload size, recall, and runtime measurements, but the current gate covers only its first 120
  words. Full-corpus Sena runtime acceptance remains not determined by this static matrix.
- Amharic's complete structural characterization exceeded 3,000,000 visited rule pairs and was
  still growing at depth 16. No larger retry was attempted.
- Aweti's legacy eager construction measured 3,093,412 composite entries. The default selector
  refuses before constructing that artifact.
- Mbugwe imports successfully and its bounded full-engine smoke gate passed 20 cases. Fresh runs
  have taken about one minute to a minute and a half. That proves oracle viability, not FST
  viability.
- States, arcs, FST recall, and FST elapsed time are not applicable where selection returns no path,
  because no trusted artifact is constructed. A refusal must not be reported as a zero-sized or
  zero-recall FST.

## Runtime evidence boundary

| Language | Completeness certificate | FST recall / skipped / timeout | States / arcs | FST build elapsed |
|---|---|---|---|---|
| Indonesian | Not determined by this static gate | Bounded P6 corpus evidence only; not grammar-wide certification | Not determined | Not determined |
| Sena | Not determined by this static gate | First-120 gate exists; full-corpus result not determined | Not determined | Not determined |
| Amharic | N/A: no backend selected | N/A | N/A | N/A |
| Aweti | N/A: no backend selected | N/A | N/A | N/A |
| Mbugwe | N/A: no backend selected | N/A | N/A | N/A |

`Not determined` means that this document does not have authoritative runtime evidence for that
field. `N/A` means the fail-closed selector returned no normal backend, so PanGloss deliberately did
not construct an FST to measure.

## Conformance evidence

The PanGloss-only completeness fixtures remain under
`rust/crates/pg-foma/tests/fixtures/pangloss/fst-completeness/` and are not Machine promotion
inputs. Current managed results:

- `late_structural_anchor_recall`: 10 passed, including the five-rule late anchor, later complex
  allomorph, repeated application, and structural/ordinary/structural slot sequence.
- `closure_unbounded_realizational`: 6 passed, including regular loop acceptance and no-artifact
  refusals for unbounded or incomplete eager closure.
- `phase_c_chain_scale::ordinary_affix_depth_five_and_ten_are_not_health_violations`: passed;
  ordinary depth 5 and 10 remained fully represented and Ideal.
- `five_language_backend_reports_gate`: 5 passed and recorded one corpus case per grammar.

## Reproduction

All Rust commands run through `rust/tools/pg.ps1` with the managed target and corpus root. The key
targets are:

```powershell
rust/tools/pg.ps1 -Mode corpus-test -Package pg-foma -TestTarget five_language_backend_reports_gate -TestThreads 1
rust/tools/pg.ps1 -Mode corpus-test -Package pg-foma -TestTarget mbugwe_corpus_smoke_gate -TestThreads 1
rust/tools/pg.ps1 -Mode test -Package pg-foma -TestTarget late_structural_anchor_recall
rust/tools/pg.ps1 -Mode test -Package pg-foma -TestTarget closure_unbounded_realizational
rust/tools/pg.ps1 -Mode test -Package pg-foma -TestTarget phase_c_chain_scale -Filter ordinary_affix_depth_five_and_ten_are_not_health_violations
```
