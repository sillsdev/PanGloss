# Which backends handle which conformance fixtures, and why

Measured at `9d1a9d76` by `rust/crates/pg-foma/examples/conf_matrix.rs`, scored against the HC-Rust
oracle, 61 fixtures x 3 `EmissionStrategy` = 183 cells. Reproduce:

```
$env:PANGLOSS_CONFORMANCE_SCOPE = 'all'
rust/tools/pg.ps1 -Mode run -Example conf_matrix
```

`PANGLOSS_CONFORMANCE_SCOPE=all` is required first — `pg_conformance_fixtures::discover` panics if
it is unset (see `CLAUDE.md`, "A conformance run must claim what it covers"), and `local` scope would
silently drop the 21 upstream `machine:` fixtures from the 61.

## Legend

| Code | Meaning | Is it a defect? |
|---|---|---|
| **OK** | compiles, `FullHcConfirmed` -- oracle-exact confirmed output | no |
| **MISS r=N** | compiles, but misses N oracle-required identities | **yes** -- ADR-0001's "never miss" |
| **NOBUILD** | refused with a typed capability diagnostic | **no** -- correct behaviour |
| **NODATA** | compiles, but produced no usable per-word evidence | **yes** |
| **TRUNC** | the ORACLE found zero analyses corpus-wide | not a backend result |

## Per-backend totals

| Backend | Oracle-exact | Refused | Compile-but-miss | Compiled, no verdict |
|---|---|---|---|---|
| `TunedSurfaceProbed` | 51 | 6 | 2 | 2 |
| `TemplatedUnderlyingTokens` | 33 | 21 | 5 | 2 |
| `PlanComposed` | 20 | 36 | 2 | 3 |
| **total** | **104** | **63** | **9** | **7** |

104 + 63 + 9 + 7 = 183 = 61 fixtures x 3 backends, and the compile-but-miss column now sums to the
9-cell itemized list below.

**Corrected on re-derivation.** An earlier revision of this table reported 4 / 7 / 5 compile-but-miss
and flagged that it would not reconcile with the itemized 9. It did not reconcile because that column
was computed as "compiled but not oracle-exact", which folds in `IdentityMismatch`, `Truncated` and
no-data cells alongside genuine recall misses. Those are now the separate right-hand column. A miss
is a recall defect against the oracle; "no verdict" is an absence of evidence, and ADR-0001 treats
those as different questions.

**0 soundness violations** across the 117 cells that produced a recall/soundness verdict (120
compiled, of which 3 yielded no per-word evidence at all). Not one over-generation survived
confirmation, on any backend, in any cell. Every defect below is a recall miss or an absence of
evidence, never a wrong answer accepted as right.

## Per-fixture headline

Of 61 fixtures: **15 pass on all three** backends, **21 on two**, **17 on one**, **8 on none**, **0
unmeasurable**.

## The 9 cells that compile but miss oracle analyses

| Fixture | Backend | Recall miss |
|---|---|---|
| `feature-gating-breadth` | PlanComposed | r=2 |
| `feature-system-breadth` | TemplatedUnderlyingTokens | r=1 |
| `loader-isactive-breadth` | TunedSurfaceProbed | r=1 |
| `morphotactic-attribute-breadth` | PlanComposed | r=4 |
| `morphotactic-attribute-breadth` | TunedSurfaceProbed | r=1 |
| `morphotactic-attribute-breadth` | TemplatedUnderlyingTokens | r=1 |
| `mpr-overwrite-order-dependence` | TemplatedUnderlyingTokens | r=4 |
| `strrep-identity` | TemplatedUnderlyingTokens | r=2 |
| `truncate-morphotactic` | TemplatedUnderlyingTokens | r=2 |

**`morphotactic-attribute-breadth` is the only fixture that misses on all three backends** -- flag it
as the shared-gap suspect: whatever it exercises is not a single-backend weakness.

## The 8 fixtures no backend handles cleanly

Not all 8 are the same kind of gap. Checked against each fixture's own `STAGING.md` (staged fixtures)
or its `grammar.xml` header comment (upstream `machine:` fixtures, which carry no `STAGING.md`):

| Fixture | Root | Verdict |
|---|---|---|
| `circumfix-non-first-allomorph-selection` | staging | **Intended refusal.** Its `STAGING.md` says this pins a real, honest, fail-closed recall gap on purpose -- the affected allomorph is deliberately never reachable from the proposer, and that is the point of the fixture. |
| `metathesis-comparison-crash` | machine | **Not a meaningful defect for this matrix.** Its `grammar.xml` header states it "pins a C# engine defect, not a grammar error" -- the founding oracle itself throws on this input, so there is no ground truth to confirm against on 2 of 3 backends (`TRUNC`); `PlanComposed`'s `NOBUILD` is the same generic marker-subtree gap that accounts for most `PlanComposed` refusals, unrelated to this fixture's purpose. |
| `simultaneous-epenthesis-cascade` | machine | **Unverified -- do not assume.** Same `NOBUILD`/`TRUNC`/`TRUNC` shape as the crash fixture above, but its `grammar.xml` header carries no comparable statement explaining the `TRUNC` result or naming an expected backend. No `STAGING.md` exists to check. Left unclassified rather than guessed. |
| `morphotactic-attribute-breadth` | machine | **Real defect** (see above -- the only all-backend `MISS`, not a refusal at all). |
| `process-morphology-in-place-mutation` | machine | **Unintended gap.** Its `grammar.xml` comment names "the ProcessMorphology compile path (`crate::emit::is_structural_rule` admitting `Role::Process`...)" as the exact mechanism this fixture means to exercise -- the design target is for a backend to compile it, and none currently does. |
| `polysynthetic-stratal-derivation-chain` | machine | **Unintended gap.** Refused by the open `rep-variant-overflow` limit (see `emit::VariantLimit`), unrelated to the fixture's own stated purpose (a derivation-then-inflection stratum chain). |
| `suffixing-extension-slot-ordering` | machine | **Unintended gap.** Trips `MprGroupOverwrite`'s unconditional `FailClosed` as a side effect of exercising `mpr-groups/output-overwrite`, not because the fixture means to test that refusal. |
| `backend-strata-generic` | staging | **Unintended gap, still open.** Its `STAGING.md` requires promotion to produce "either a content-distinct buildable backend or an explicit elimination report" -- i.e. this fixture is expected to eventually compile somewhere; a permanent refusal is not the documented intent. |

So: **1 of the 8 is a documented intended refusal, 1 is not a meaningful defect (oracle-empty by the
fixture's own design), 1 is unverified, and the remaining 5 are real, undocumented gaps** (including
the all-backend miss). Do not report "8 defects" without this split.

## Why each backend refuses

- **`PlanComposed`.** Most refusals trace to one shape: a plan requiring a `CompositeEmissionMarker`
  / `StructuralCompositeMarker` subtree that `build_controllable` cannot build.
- **`TemplatedUnderlyingTokens`.** Mostly `BuildFailed: templated emission unsupported:
  Partial{uncovered:N}`, plus phonological-rule-compilation failures on a smaller number of fixtures.
- **`TunedSurfaceProbed`.** Named capability-envelope refusals -- root-shape/representation-variant
  overflow, standalone-rule claims, finite-closure bounds, and zone-exclusive-allomorph conflicts,
  each a distinct typed diagnostic rather than one undifferentiated failure.

Legal pre-confirm over-generation is permitted by ADR-0001 -- the confirm step prunes it. Judging a
proposer by raw acceptance instead of confirmed output overstates how broken a backend looks.
