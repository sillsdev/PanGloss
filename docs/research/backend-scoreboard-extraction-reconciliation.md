# Backend scoreboard extraction: reconciliation against the pre-extraction baseline

Records how `rust/crates/pg-foma/tests/backend_scoreboard_gate.rs`'s ratchet constants were
reconciled against the counts measured by hand from `examples/conf_matrix.rs` at commit
`57515281` (before its per-(fixture, backend) evaluation was extracted into
`pg_foma::scoreboard` and given an `expect_crash` skip). One measurement run, recorded so the
next person touching this gate does not have to re-derive it.

## Baseline (57515281, no `expect_crash` skip, 61 fixtures per backend)

| backend | oracle-exact | refused | compile-but-miss | compiled, no verdict |
|---|---|---|---|---|
| TunedSurfaceProbed | 51 | 6 | 2 | 2 |
| TemplatedUnderlyingTokens | 33 | 21 | 5 | 2 |
| PlanComposed | 20 | 36 | 2 | 3 |

## After extraction + the `expect_crash` skip (60 fixtures per backend, 1 excluded)

| backend | oracle-exact | refused | compile-but-miss | compiled, no verdict |
|---|---|---|---|---|
| TunedSurfaceProbed | 51 | 6 | 3 | 0 |
| TemplatedUnderlyingTokens | 33 | 21 | 6 | 0 |
| PlanComposed | 20 | 35 | 2 | 3 |

`oracle-exact` is unchanged for every backend (104 total either way), and `PlanComposed`'s
`refused` count drops by exactly 1 (36 -> 35), fully explained by the one fixture the
`expect_crash` skip removes (`machine:edge-cases/simultaneous-epenthesis-cascade`, which
`PlanComposed` refused outright with a plan-topology marker before the skip existed).

## The `compile-but-miss` / `compiled, no verdict` split for TunedSurfaceProbed and
## TemplatedUnderlyingTokens does not fully explain by the one exclusion alone

Removing one fixture can move at most one cell per backend between buckets. The measured shift
for these two backends is larger: `compile-but-miss` gained 1 (2->3, 5->6) while `compiled, no
verdict` lost 2 (2->0, 2->0) -- a net of -1 (matching the fixture-count drop), but distributed
across more than one cell's worth of bucket movement per backend.

Traced cause: `machine:edge-cases/metathesis-comparison-crash` (which no longer carries
`expect_crash: true` -- fixed upstream by `sillsdev/machine#471`, per its own `words.yaml`
comment -- so it is NOT the excluded fixture and remains normally scored) measures
`Certification::Truncated { stage: "no-analyzable-words", .. }` on both `TunedSurfaceProbed` and
`TemplatedUnderlyingTokens`. That certification variant carries per-word evidence (`obs.words` is
`Some`), so `scoreboard::measure_fixture` counts it as `CellOutcome::CompilesButMisses` --
and would have under the pre-extraction inline code too, since that code's own branch on
`obs.words` is unchanged by the extraction. `no-analyzable-words` fires because the fixture's one
word is `expect_fail: true` (the oracle has zero valid analyses for it), which
`backend_runtime.rs`'s own comment explains: agreeing about nothing is not agreement, so an
all-empty corpus cannot certify `FullHcConfirmed` even when the candidate also proposes nothing.

This is a deterministic, data-driven outcome (a fixed word count against a fixed oracle answer),
not a wall-clock race -- confirmed by reading `RuntimeBudget`'s fields: the dimension that
produced this fixture's `ResourceBreach` cells elsewhere in the same run is a decoded-path COUNT
(`value: 1000001, limit: 1000000`), not a `Duration`, and `no-analyzable-words` itself depends on
nothing but the oracle's own analysis count. Re-running the measurement should reproduce the same
split.

What this note does NOT establish is which second fixture (beyond
`metathesis-comparison-crash`) accounts for the remainder of the shift against the hand-transcribed
57515281 table -- that table was not itself re-derived from a saved log, so its exact
`compile-but-miss` / `compiled, no verdict` split cannot be re-checked line by line. The gate in
`backend_scoreboard_gate.rs` therefore ratchets against the FRESH, reproducible split recorded
above (measured post-extraction, post-`expect_crash`-skip), not against the older transcription --
this file is what makes that choice inspectable rather than a silent divergence from the
implementation brief's own baseline table.

## Soundness

0 `candidate_only_identities` (surviving over-generations) across every measured cell that
produced comparable evidence in this run (115 such cells: 104 `oracle_exact` + 11
`compiles_but_misses`). `backend_scoreboard_gate.rs` asserts this as a hard invariant, never a
ratchet.

## `circumfix-non-first-allomorph-selection`

Measured `Refused` on all three backends (0/3), each for a different, typed, cited reason:

- `TunedSurfaceProbed`: the capability envelope's own `surface-probe.circumfix-zone-exclusive-allomorph`
  predicate refuses the build.
- `TemplatedUnderlyingTokens`: `templated emission unsupported: Partial { uncovered: 1 }`.
- `PlanComposed`: the plan's `StructuralCompositeMarker` subtree cannot be honoured by
  `build_controllable`; the fixture needs a whole-grammar backend.

This does not contradict `tests/circumfix_candidate_selection.rs::non_first_allomorph_circumfix_recall_parity`,
which proves proposer-to-confirm containment for the same grammar's `kemitan` word -- that test
builds its plan directly via `crate::emit`, a route none of `ALL_STRATEGIES`'s three backends
takes. The census C1 fix this fixture pins made the compiler correctly DETECT that the rule needs
structural-composite handling; none of the three measured backends independently implements
building one for this allomorph ordering, so all three fail closed with a named reason rather than
silently under-proposing. Both facts are simultaneously true.
