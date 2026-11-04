# Removed FST backends: Templated underlying tokens and Plan composed

Decided 2026-11-03 by the owner. The Templated underlying tokens backend
(`EmissionStrategy::TemplatedUnderlyingTokens`, label `templated-underlying-tokens`) and the Plan
composed backend (`EmissionStrategy::PlanComposed`, label `plan-composed`) are deleted. Tuned
surface probed (`EmissionStrategy::TunedSurfaceProbed`, label `tuned-surface-probed`) is the only
backend left.

## Why

Production selection admits one backend, Tuned surface probed. Measured on the 151 conformance
fixtures at `f4ed795c`, the scoreboard (oracle exact, compiles but misses, refused, unmeasurable)
was:

| Backend | Exact | Miss | Refused | Unmeasurable |
|---|---|---|---|---|
| Tuned surface probed | 122 | 9 | 20 | 0 |
| Templated underlying tokens | 103 | 5 | 43 | 0 |
| Plan composed | 53 | 2 | 93 | 3 |

Keeping two compilers that production never picks cost a build, an oracle, a test surface and a
scoreboard column each, and every change to the shared emitter had to be proved on three backends.
The owner chose to delete them rather than keep them as comparison columns.

They were not strictly dominated. Seven fixtures were oracle exact on a deleted backend and are not on
Tuned surface probed. Those are the comparison cells to recover first if one of them matters:

| Fixture | Tuned surface probed | Templated underlying tokens | Plan composed |
|---|---|---|---|
| `machine:edge-cases/clitic-rule-designation` | miss | miss | exact |
| `machine:edge-cases/final-template-merge-key` | miss | miss | exact |
| `machine:edge-cases/final-template-partial-discriminators` | miss | miss | exact |
| `staging:edge-cases/class-deletion-reinsertion-strrep` | miss | exact | exact |
| `staging:edge-cases/pattern-root-required-environment` | refused | refused | exact |
| `staging:edge-cases/rewrite-default-epenthesis` | miss | exact | refused |
| `staging:edge-cases/rewrite-default-full-set` | miss | exact | refused |

The FST scope is not part of this decision. The FST adds no capability that only a grammar already
carrying a grammar-health error or warning would need (partial morphemes, for example). Grammars
with unrelated warnings still get an FST.

## Where the deleted code last existed, and how to restore it

| What | Commit |
|---|---|
| Last commit on `main` that contains both backends, their tests and their docs | `c241c204` (`main` at v0.15.2 when the deletion was rebased; v0.14.0 is `f4ed795c`) |
| The commit that deletes them | `e29376ddaac7114fe4c0f8a4acb77f4f534a4b5e` (branch `fst/delete-templated-plancomposed`) |

The commit that adds this page sits above the deletion, so reverting the deletion does not conflict with it.
PanGloss lands by squash: after landing, the deletion is the squash commit named in the PR body; the
branch commit above stays reachable at `refs/pull/<number>/head`.

- Restore everything: `git revert e29376dd` (then re-run `pg.ps1 -Mode preland`; the
  scoreboard, ledger golden and the `four_grammar_recipe_evidence` expectations change back).
- Restore one file: `git show e29376dd^:<path> > <path>`.
- List what was deleted: `git show --stat --diff-filter=D e29376dd`.
- Read the old code without restoring: `git show c241c204:<path>`.

## Behaviour that did not change

Tuned surface probed's scoreboard is unchanged by the deletion: 143 exact, 12 miss, 20 refused, 0
other on the 169 conformance fixtures (the suite pin plus staging) at the rebase base, the same counts `main` pins before the
deletion. The pre-deletion cells were measured at `f4ed795c` (122 / 9 / 20 / 0 on 151 fixtures), and
the two backends were not re-measured on the fixtures added since. The other consequences that the
tests now state:

- `recipe-optimize` evaluates one candidate, the baseline, so a run's candidate set, `winner` and
  `pruning` counts shrink (`four_grammar_recipe_evidence`).
- `languages/metathesis-phase-isolation` is no longer confirmed by any candidate: Templated underlying
  tokens confirmed it and Tuned surface probed misses the word `hasaasa` (a known faithfulness
  failure). `four_grammar_recipe_evidence` stopped asserting it confirms.
- Predicates that only the two backends needed were deleted, so Metathesis, RightToLeftRewrite and
  QuantifierPattern characterize as `ConfirmOnly` and a simultaneous rule with disjoint subrules
  composes to `ConfirmOnly` instead of `Admit`.
- Tests that need a second candidate or backend were deleted or reduced to the one candidate:
  `apply_path_refusal_gate` (the 12^k over-generation of the Plan composed net),
  `strategy_coverage_join_gate` (no `CannotRepresent` row is left), the continuation tests of
  `recipe_optimize_continuation`, and the gate-group permutation candidate.

## What was removed

Paths are relative to the repository root; the full list is the deleting commit's diff.

- Compilers. `rust/crates/pg-foma/src/build.rs` and its two test files (the plan interpreter,
  `build_controllable`, that Plan composed ran), `rust/crates/pg-foma/src/oracle.rs`,
  `rust/crates/pg-foma/src/uflexc.rs` (the lexicon emitter Plan composed used) and
  `rust/crates/pg-foma-backend/src/templated_compile.rs`.
- Backend adapters and their dispatch arms in `rust/crates/pg-foma-backend/src/backend.rs`,
  `backend_runtime.rs`, `completed_build.rs`, `witnessed_coverage.rs` and `backend_registry.rs`,
  including the eight seeded families: the seven plan-rewrite families of Plan composed
  (`ordered-morphophonology`, `class-exception-cascade`, `complete-template`, `specialized-branch`,
  `copy-branch`, `bounded-metathesis`, `layered-morphology`) and `token-cascade-morphology`
  (Templated underlying tokens).
- Predicates, refusals and checks that only they used: the marker-material refusal, the
  `rule_cascade_uncompilable_refusal` check in `rust/crates/pg-foma/src/capability_gate.rs`, the
  gate-group fuzz in `plan_interaction_coverage.rs`, the `SimultaneousDisjoint` fixture-coverage
  extension and the net-shape instrument.
- Tests: 31 integration test files under `rust/crates/pg-foma-backend/tests/`, 9 `phase_c_*` and
  `p6_gate_parity` files under `rust/crates/pg-foma/tests/`, and the unit tests of the deleted
  modules. Remaining test files lost only their Plan composed and Templated underlying tokens
  assertions.
- Lab examples: 9 under `rust/crates/pg-foma-backend/examples/lab/` and
  `rust/crates/pg-foma/examples/p6_templated_q3_oracle_bounds.rs`.
- Scoreboard columns and expectations for the two backends in `backend_scoreboard_gate.rs`,
  `backend_seam_gate.rs`, `witnessed_strategy_coverage_gate.rs`,
  `five_language_backend_reports_gate.rs` and `backend_selection_contract.rs`.
- Docs: `docs/fst-plan/backend-cards/plan-composed.md` and
  `docs/fst-plan/backend-cards/templated-underlying-tokens.md`, the two cards in
  `backend_cards_data.rs`, and the `plan-composed-missing-subtrees` entry in
  `rust/crates/pg-foma/assets/backend-advice-v1.toml`.

Historical documents under `docs/research/`, `docs/superpowers/`, `docs/divergences/`, `openspec/`
and the older `docs/fst-plan/` reports still describe the two backends. They are records of what
was measured then and are left as written.

## What stayed, and is generic on purpose

Nothing in the seam names a backend. These pieces take a second backend without a rewrite:

- `Backend` trait, `ALL_BACKENDS` and `backend_for` in
  `rust/crates/pg-foma-backend/src/backend.rs`. One adapter per strategy; `backend_for` is the one
  match on `EmissionStrategy`.
- `EmissionStrategy` in `rust/crates/pg-health/src/strategy.rs`, with its `label`/`from_label` table.
- `ALL_STRATEGIES`, `representation_of` and `StrategyRepresentation` in
  `rust/crates/pg-foma/src/strategy_coverage.rs`: which construct kinds each strategy represents.
- The selection point: `select_backends`/`select_backends_for_grammar` in
  `rust/crates/pg-foma/src/backend_selection.rs`, `default_grammar_wide_checks` and
  `GrammarWideCheck::constrains_strategies` in `capability.rs`, and
  `compose_envelope_for_strategy`, which composes an envelope per strategy and never narrows a
  whole-grammar one.
- `CandidateRole`, `LoweredCandidate` and the registry's `Applicability`/`FamilySearchPolicy`
  (`rust/crates/pg-foma-backend/src/backend_registry.rs`). The one seeded family is
  `FAMILY_SURFACE_PROBE_MORPHOLOGY`; its candidate is always `CandidateRole::Baseline`.
- `CompletedBuildError::UnsupportedStrategy`, which a dispatch arm returns for a strategy it does not
  compile.
- The coverage ledger (`rust/crates/pg-foma-backend/src/coverage_ledger.rs`) and its golden
  `coverage_ledger_golden.json`.

## Coverage the ledger no longer cites

The deleted backends carried the only containment witnesses for six construct kinds. Those kinds now
have no Tuned surface probed containment citation, so `containment_evidence_for` returns `None` for
them and the ledger reports them as unwitnessed rather than claiming coverage it no longer has:
SimultaneousRewrite, RightToLeftRewrite, Metathesis, SubruleGating, MultiTable and
QuantifierPattern. Restoring a witness means adding a test that exercises the kind on Tuned surface
probed and citing it in `coverage_ledger.rs`.

## Restore a backend, or add a comparison backend

1. Variant and label. Add a variant to `EmissionStrategy` and its arms to `label` and `from_label`.
   The compiler then lists every exhaustive match that needs an arm.
2. Adapter. Implement `Backend` for a new unit struct in `backend.rs` (`strategy`,
   `compile_for_measurement`, `realize_accuracy_proposer`, `evaluate_for_corpus`), add it to
   `ALL_BACKENDS` and give `backend_for` its arm. `ALL_BACKENDS` and `backend_for` must stay exhaustive
   in both directions.
3. Strategy table. Add the variant to `ALL_STRATEGIES` and give `representation_of` its row for every
   construct kind. A kind it cannot represent is a `CannotRepresent` row, and the ledger then lists the
   hole per kind.
4. Selection. Decide which grammar-wide checks constrain it (`constrains_strategies`) and whether
   `compose_envelope_for_strategy` needs a refusal for it. Restore any per-strategy refusal from
   `c241c204` (for example the marker-material refusal beside `backend_selection`).
5. Dispatch. Add the arm to `compile_with_backend_for_measurement` in `witnessed_coverage.rs`, to
   `completed_build.rs` (or leave it returning `UnsupportedStrategy`), and to the worker wire in
   `rust/crates/pg-foma-backend/src/worker/` and the pack format if the strategy is selectable in a
   production build.
6. Registry. Seed a family in `backend_registry.rs` whose materializer returns a `LoweredCandidate`
   carrying the strategy. A strategy that reads the plan needs `CandidateRole::Alternative` candidates
   and, in `rust/crates/pg-cli/src/recipe_optimize.rs`, a bare-root id; a whole-grammar strategy keeps
   the `root@label` id.
7. Cards and advice. Add its card to `backend_cards_data.rs` (the build regenerates
   `docs/fst-plan/backend-cards/`) and any advice entries to
   `rust/crates/pg-foma/assets/backend-advice-v1.toml`.
8. Scoreboard and ratchet tests to re-add or extend, each of which now iterates only the one backend:
   - `rust/crates/pg-foma-backend/tests/backend_scoreboard_gate.rs`: add the strategy's `EXPECTED`
     row (exact, miss, refused, other) over the 169 fixtures. This is the comparison column.
   - `backend_seam_gate.rs`: its dispatcher list, partial-class admissions and
     `partial_fixture_rows`.
   - `witnessed_strategy_coverage_gate.rs`, `five_language_backend_reports_gate.rs`,
     `strategy_coverage_join_gate.rs`, `faithfulness_coverage_gate.rs` (`STRATEGIES`) and
     `conformance_coverage_gate.rs`: add the strategy to their strategy lists and re-measure the pinned
     counts.
   - `strategy_aware_capability_gate.rs`: the strategy-blind envelope hole test needs a strategy that
     represents less than the others to show anything.
   - `backend_selection_contract.rs` and `backend_runtime_cache_gate.rs`: a second strategy makes the
     shared-oracle and fallback cases non-trivial.
   - Regenerate the ledger golden with the ignored test `regenerate_coverage_ledger_golden_json`
     in `coverage_ledger.rs`, and keep `tests/coverage_citation_liveness.rs` green: every cited test
     must exist.
   - Recover the deleted tests worth keeping from `c241c204` rather than writing them again: the
     phase C files in `pg-foma/tests/`, `cross_compiler_equivalence_gate.rs`,
     `net_dedup_gate.rs` and `plan_composed_marker_material_gate.rs` are the ones that compare
     backends.

A comparison backend does not need to be selectable in production: leave it out of the pack format and
the worker wire, add it to `ALL_BACKENDS` and the scoreboard, and gate it behind a feature if it
should not ship.
