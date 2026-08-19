# Candidate filter assessment — 2026-08-19

Scoped honestly to what is actually built today, per the certification ladder in
`docs/superpowers/specs/2026-08-11-candidate-filter-contract.md`. This is not the full Task 11 this
plan originally described (see "What Task 11 has not reached" below) — Tasks 5-10's own passes were
never built, so there is no promotion table spanning them to write.

## Promotion table

| Pass | Status | Profile membership | Evidence |
|---|---|---|---|
| `structural.ownership.v1` | Built, `ShadowOnly` | `StructuralV1` | `candidate_filter_model_check.rs` (2/2), `candidate_filter_shadow_gate.rs` (101/101, 11 skipped) — both structural-passes-only |
| `structural.transition.v1` | Built, `ShadowOnly` | `StructuralV1` | same two gates as above |
| `surface.consistency.v1` | Built, `ShadowOnly` | **none of `StructuralV1`/`SymbolicV1`/`BoundaryLocalV1`** — added outside that taxonomy, unclassified against it | See "Surface consistency" below |
| symbolic partner / co-occurrence / static-signature | **Not built** | `SymbolicV1` names these; none exist | Tasks 5, 7 of `2026-08-11-candidate-filter-first.md` never executed |
| local allomorph / exact-span / environment | **Not built** | `BoundaryLocalV1` names these | Task 6 never executed |
| compiled trace DFA | **Not built** | N/A (`Hybrid growth path`) | `candidate_filter_dfa_equivalence.rs` does not exist |
| `candidate_filter_promotion_gate.rs` | **Does not exist** | — | Task 9/10's own promotion-gate test was never written |

`StructuralV1`, `SymbolicV1`, and `BoundaryLocalV1` remain exactly as specified in the contract —
no profile has been promoted past `ShadowOnly`, and `Enforce` is unavailable for all of them pending
the proof-verifier-liveness and oracle-survival gates the contract's certification ladder requires.

## Surface consistency: the actual news

`surface.consistency.v1` (`rust/crates/pg-foma/src/candidate_filter/passes/surface_consistency.rs`)
is the first pass in this whole project to produce a real, nonzero, sound rejection on a real
grammar. Every structural pass above defers on every real candidate today, because a sound pass must
defer when every `TraceFact` arrives `Deferred` — this plan's own generator-blocker (`2026-08-11-
candidate-filter-first.md`, "Achieved saving is 0.00%"). Surface consistency needs no such fact —
only the candidate's own morpheme-id sequence, root index, and the grammar's static allomorph
tables — so it can fire without waiting on that generator.

**Measured** (`docs/superpowers/plans/2026-08-11-candidate-filter-first.md`'s "Surface consistency is
measured" section — read that section for the full numbers and the false-positive bug found and
fixed while wiring it in):

| grammar | doomed candidates | catches | of removable steps |
|---|---|---|---|
| Sena | 3331 | 1029 (30.9%) | 2884/19331 (**14.9%**) |
| Amharic | 56 | 0 (0.0%) | 0/53 (0.0%) |
| Indonesian | 4 | 1 (25.0%) | 0/3 (0.0%) |
| Aweti | 0 (harness defect, pre-existing) | — | — |
| Mbugwe | 0 (harness defect, pre-existing) | — | — |

Only Sena shows a real, positive removable-work result. The first measurement (before the false-
positive fix) showed all three of Sena/Amharic/Indonesian firing; the corrected numbers show Amharic
collapsed to zero and Indonesian's candidate-level catch does not clear a whole chunk. **This is a
one-grammar result, not a five-grammar one, and two of the five are entirely unmeasured through this
path.**

**Its own conformance fixture is `producer-blocked`, not `wired`**
(`conformance-staging/filter-passes/surface-consistency/filter-expectation.json`): the
`candidate_filter_fixture_weight.rs` harness only ever presents candidates `pg_parse::Morpher`
already confirmed, which is by construction never surface-infeasible, so the fixture pins the floor a
future over-generating producer's candidate would meet, not a count this harness measures today —
the same shape as the pre-existing `structural.ownership.v1` fixture. The real, positive evidence
above comes from a *different*, over-generating FST/enumeration producer
(`rust/crates/pg-foma/examples/filter_ceiling_census.rs`) against the private corpora, not from the
conformance suite.

**Against the certification ladder:**

1. Unit tests (positive/negative/ambiguous/missing-metadata/invalid-proof) — met, via
   `candidate_filter_passes.rs`'s generic `ImpossibleSurfaceComposition` coverage plus the dedicated
   fixture's positive/negative words.
2. Model/property tests against an exhaustive reference predicate — **not built**;
   `candidate_filter_model_check.rs` only covers the two structural passes.
3. Synthetic integration proof that the pass fires and reduces candidates before HC — **not met
   through the conformance harness** (producer-blocked, as above); met through the private-corpus
   census instead, which is evidence of a different, non-repeatable-in-CI kind.
4. Shadow tests proving every would-reject candidate gets zero HC confirmations — **not built**;
   `candidate_filter_shadow_gate.rs` does not exercise this pass.
5. All oracle-positive analyses survive across the private corpora — implied by construction (the
   pass is a proven sound under-approximation; see the module's own doc for the soundness argument
   and its two found-and-fixed false-positive classes) but not exercised by the fail-closed
   `pg.ps1 -Mode corpus-test` gate specifically for this pass.
6. `Off`/`Enforce` identical output — not specifically gated for this pass.
7. Deterministic counters proving nonzero firing on named inputs — **met**, via the corrected census
   numbers above (Sena: 1029 verified catches, 2884 removable steps).

So: real and evidenced on step 7, and sound by its own construction, but steps 2, 3 (through this
repo's own test infrastructure), 4, and 6 remain open. It is **`ShadowOnly`**, wired into
`composite.rs`'s shadow-mode filter call (`filter_into_with_word`) alongside the structural passes,
and no closer to any named `Enforce` profile than the structural passes are.

## What Task 11 has not reached

The plan's own Task 11 describes a promotion table spanning every pass Tasks 5-10 were meant to
build (symbolic partner/co-occurrence/static-signature, local allomorph/exact-span/environment, a
compiled DFA backend) plus running `candidate_filter_promotion_gate.rs` and the full private-corpus
oracle-survival + privacy-boundary gates. None of Tasks 5-10 were executed — only `structural.rs`
(pre-existing) and `surface_consistency.rs` (this session) exist. `candidate_filter_promotion_gate.rs`
and `candidate_filter_dfa_equivalence.rs` do not exist. This document does not pretend otherwise: it
is the honest state of a two-pass project, not the ten-pass one originally scoped.

The private-corpus oracle-survival gate (`candidate_filter_oracle_survival`, needing
`PANGLOSS_CORPUS_ROOT`) and the private-data boundary test
(`candidate-filter-private-data.tests.ps1`) were not run for this assessment — both are real,
larger, separate steps and remain open.

## Open decision, not made here

Whether one real, corrected, positive result — on Sena alone, with Aweti and Mbugwe still
unmeasured through this path due to a pre-existing census-harness defect — is sufficient evidence to
authorize the plan's Tasks 5-10 (the remaining structural/symbolic/local passes), or whether that
needs a broader signal first, is a decision for a human to make. This document states the evidence;
it does not decide.
