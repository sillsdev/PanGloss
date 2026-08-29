# Which backends handle which conformance fixtures, and why

Measured at `496a6f3c` by `rust/crates/pg-foma/examples/conf_matrix.rs`. Reproduce:

```
rust/tools/pg.ps1 -Mode run -Example conf_matrix
```

61 fixtures x 3 `EmissionStrategy` = 183 cells. `select_backends` is bypassed by construction
(`LoweringAdapter::for_strategy`), so a capability-refused backend still compiles and is measured --
otherwise the measurement reports the refusal rather than the backend.

## Legend

| Code | Meaning | Is it a defect? |
|---|---|---|
| **OK** | compiles, `FullHcConfirmed` -- oracle-exact confirmed output | no |
| **MISS r=N** | compiles, but misses N oracle-required identities | **yes** -- ADR-0001's "never miss" |
| **NOBUILD** | refused with a typed capability diagnostic | **no** -- correct behaviour |
| **NODATA** | compiles, but produced no usable per-word evidence | **yes** |
| **TRUNC** | the ORACLE found zero analyses corpus-wide | not a backend result |

`OK` never means "proposed nothing extra". Legal pre-confirm over-generation totalled **2443** across
the measured cells and is permitted by ADR-0001 -- the confirm step prunes it. Judging a proposer by
raw acceptance instead of confirmed output is what made 7 of 9 fixtures look broken when they were
not.

## Totals

| | cells |
|---|---|
| Works | 104 |
| Honestly refuses | 63 |
| **Silently wrong** | **12** (9 `MISS` + 3 `NODATA`) |
| Oracle-empty (`TRUNC`) | 4 |

**Soundness violations: 0.** Not one over-generation survived confirmation, on any backend, in any
cell. Every defect below is a recall miss or an absence of evidence.

## Why each backend refuses

- **`PlanComposed` -- 36 cells, all ONE shape.** The plan requires a `CompositeEmissionMarker` /
  `StructuralCompositeMarker` subtree that `build_controllable` cannot build. One capability gap
  accounts for over half of every unbuildable cell in the matrix.
- **`TemplatedUnderlyingTokens` -- 21 cells.** Mostly `BuildFailed: templated emission unsupported:
  Partial{uncovered:N}`, plus 2x "no phonological rule compiled".
- **`TunedSurfaceProbed` -- 6 cells**, each a named capability-envelope refusal: 3x
  `rep-variant-overflow` (root shape exceeds 64 representation variants), 1x
  `standalone-rule-claimed`, 1x `finite-closure-bound`, 1x `circumfix-zone-exclusive-allomorph`.

## The 12 silently-wrong cells -- the real defect surface

| Fixture | Backend | Detail |
|---|---|---|
| `morphotactic-attribute-breadth` | **all three** | MISS r=4 / r=1 / r=1 -- every backend misses |
| `feature-gating-breadth` | PlanComposed | MISS r=2 |
| `feature-system-breadth` | Templated | MISS r=1 |
| `loader-isactive-breadth` | TunedSurface | MISS r=1 |
| `mpr-overwrite-order-dependence` | Templated | MISS r=4 |
| `strrep-identity` | Templated | MISS r=2 |
| `truncate-morphotactic` | Templated | MISS r=2 |
| `deep-optional-affix-nesting` | PlanComposed | NODATA -- `ResourceBreach`, apply_up path budget |
| `backend-template-generic` | PlanComposed | NODATA -- `ResourceBreach` |
| `loader-pattern-shapes` | PlanComposed | NODATA -- `Truncated{empty-network}` |

## The five all-refused fixtures

One is passing by design; four are real gaps.

| Fixture | Verdict |
|---|---|
| `circumfix-non-first-allomorph-selection` | **intended refusal -- PASSING.** `STAGING.md` authors it to pin an honest fail-closed recall gap |
| `process-morphology-in-place-mutation` | unintended gap -- the grammar's own comment says the design target is to compile |
| `polysynthetic-stratal-derivation-chain` | unintended gap -- refused by the open `rep-variant-overflow` limit, unrelated to the fixture's purpose |
| `suffixing-extension-slot-ordering` | unintended gap -- trips `MprGroupOverwrite`'s unconditional `FailClosed` as a side effect |
| `backend-strata-generic` | unintended gap -- `STAGING.md` expects it to become buildable |

## Full table

PC = `PlanComposed`, TSP = `TunedSurfaceProbed`, TUT = `TemplatedUnderlyingTokens`.

| Fixture | PC | TSP | TUT |
|---|---|---|---|
| machine:edge-cases/alpha-variable-name-collision | NOBUILD | OK | NOBUILD |
| machine:edge-cases/bistratal-overlapping-segment-representation | OK | OK | NOBUILD |
| machine:edge-cases/compounding-breadth | OK | OK | OK |
| machine:edge-cases/deep-optional-affix-nesting | NODATA | OK | OK |
| machine:edge-cases/diacritic-segments | OK | OK | OK |
| machine:edge-cases/disjunctive-recheck | OK | OK | OK |
| machine:edge-cases/feature-gating-breadth | MISS r=2 | OK | OK |
| machine:edge-cases/feature-system-breadth | NOBUILD | OK | MISS r=1 |
| machine:edge-cases/free-fluctuating-allomorph-pair | OK | OK | OK |
| machine:edge-cases/loader-default-symbol | NOBUILD | OK | OK |
| machine:edge-cases/loader-isactive | OK | OK | OK |
| machine:edge-cases/loader-isactive-breadth | OK | MISS r=1 | OK |
| machine:edge-cases/loader-pattern-shapes | NODATA | OK | NOBUILD |
| machine:edge-cases/metathesis-comparison-crash | NOBUILD | TRUNC | TRUNC |
| machine:edge-cases/morphotactic-attribute-breadth | MISS r=4 | MISS r=1 | MISS r=1 |
| machine:edge-cases/mpr-gated-exception | NOBUILD | OK | OK |
| machine:edge-cases/mpr-group-overwrite-without-realizational | OK | OK | OK |
| machine:edge-cases/mpr-overwrite-order-dependence | OK | OK | MISS r=4 |
| machine:edge-cases/process-morphology-in-place-mutation | NOBUILD | NOBUILD | NOBUILD |
| machine:edge-cases/right-to-left-anchor-environment | NOBUILD | OK | OK |
| machine:edge-cases/simultaneous-epenthesis-cascade | NOBUILD | TRUNC | TRUNC |
| machine:edge-cases/stem-name-restricted-root-allomorph | OK | OK | OK |
| machine:edge-cases/strrep-identity | OK | OK | MISS r=2 |
| machine:edge-cases/subrule-morphosyntactic-gating | NOBUILD | OK | OK |
| machine:edge-cases/truncate-morphotactic | NOBUILD | OK | MISS r=2 |
| machine:languages/fusional-realizational-morphology | NOBUILD | OK | NOBUILD |
| machine:languages/metathesis-phase-isolation | NOBUILD | OK | NOBUILD |
| machine:languages/polysynthetic-stratal-derivation-chain | NOBUILD | NOBUILD | NOBUILD |
| machine:languages/prefixal-discontinuous-slot-dependency | OK | OK | OK |
| machine:languages/suffixing-evidential-adjacency-chain | OK | OK | OK |
| machine:languages/suffixing-extension-slot-ordering | NOBUILD | NOBUILD | NOBUILD |
| machine:languages/suffixing-vowel-harmony | NOBUILD | OK | NOBUILD |
| machine:languages/templatic-root-modification | NOBUILD | OK | NOBUILD |
| staging:edge-cases/backend-gated-generic | NOBUILD | OK | OK |
| staging:edge-cases/backend-ordered-generic | NOBUILD | OK | NOBUILD |
| staging:edge-cases/backend-strata-generic | NOBUILD | NOBUILD | NOBUILD |
| staging:edge-cases/backend-template-generic | NODATA | OK | OK |
| staging:edge-cases/circumfix-cross-product-and-infix-drop | NOBUILD | OK | NOBUILD |
| staging:edge-cases/circumfix-in-template-slot | NOBUILD | OK | OK |
| staging:edge-cases/circumfix-infix-interior-action-precedence | NOBUILD | OK | NOBUILD |
| staging:edge-cases/circumfix-non-first-allomorph-selection | NOBUILD | NOBUILD | NOBUILD |
| staging:edge-cases/circumfix-reduplication-precedence | NOBUILD | OK | NOBUILD |
| staging:edge-cases/compounding-non-recursive | OK | OK | OK |
| staging:edge-cases/cross-stem-material-determination | OK | OK | OK |
| staging:edge-cases/deletion-reduplication-exception-composite | NOBUILD | OK | NOBUILD |
| staging:edge-cases/guesser-pattern-root-fallback | OK | NOBUILD | NOBUILD |
| staging:edge-cases/head-ambiguous-compounding | OK | OK | OK |
| staging:edge-cases/infix-interdigitation | NOBUILD | OK | NOBUILD |
| staging:edge-cases/multi-table-metathesis-shared-representation | NOBUILD | OK | OK |
| staging:edge-cases/optional-template-composite | NOBUILD | OK | OK |
| staging:edge-cases/recursive-endocentric-compounding | OK | OK | OK |
| staging:edge-cases/right-to-left-bounded-quantifier-rewrite | NOBUILD | OK | OK |
| staging:edge-cases/right-to-left-cross-table-segments-environment | NOBUILD | OK | OK |
| staging:edge-cases/right-to-left-metathesis-reversal | NOBUILD | OK | OK |
| staging:edge-cases/right-to-left-segments-environment | NOBUILD | OK | OK |
| staging:edge-cases/segment-natural-class-table-binding | NOBUILD | OK | NOBUILD |
| staging:edge-cases/simultaneous-subrule-genuine-overlap | NOBUILD | OK | NOBUILD |
| staging:edge-cases/standalone-combining-mark | OK | OK | OK |
| staging:edge-cases/template-category-sharing | OK | OK | OK |
| staging:edge-cases/two-table-shared-representation-recall | NOBUILD | OK | OK |
| staging:edge-cases/unbounded-iterative-quantifier-expansion | NOBUILD | OK | OK |

## Per-backend totals

| Backend | Compiles | Oracle-exact (of compiled) | Recall misses | Soundness |
|---|---|---|---|---|
| `TunedSurfaceProbed` | 55/61 (90%) | 51/55 | 2 cells | 0 |
| `TemplatedUnderlyingTokens` | 40/61 (66%) | 33/40 | 5 cells | 0 |
| `PlanComposed` | 25/61 (41%) | 20/22 measured | 2 cells | 0 |
