# Backend Advice and Historical Five-Language Verification Plan

> Execute after completeness certificates exist. Advice describes compiler evidence; it never authorizes a linguistically invalid grammar edit.

> **Historical/superseded plan.** The active deliverable is now the Indonesian/Amharic/Aweti
> three-language slice defined by [`2026-08-22-filter-reach-shipping-spec.md`](../../fst-plan/2026-08-22-filter-reach-shipping-spec.md).
> This plan's five-language matrix and measurements remain historical reference; Mbugwe is deferred
> and is not a current acceptance blocker. The tasks below are not being marked complete by this
> notice.

**Historical goal:** Produce stable findings and shared remedies for every backend, select only proven candidates, and publish an authority matrix for the five reference grammars. This goal is superseded for delivery: the current slice is Indonesian/Amharic/Aweti, and no language in that slice has a trusted shipped FST yet.

## Task 1: Add the versioned advice catalog

**Files:**

- Create: `rust/crates/pg-foma/src/advice_catalog.rs`
- Create: `rust/crates/pg-foma/assets/backend-advice-v1.toml`
- Modify: `rust/crates/pg-foma/src/lib.rs`
- Modify: `rust/crates/pg-foma/src/health.rs`
- Test: `rust/crates/pg-foma/src/advice_catalog.rs`

1. Define stable `shape_key`, backend/route, failed predicate, typed evidence references, remedy key, `easy|medium|hard` effort per remedy×shape, prerequisites, contraindications, and equivalence caveat.
2. Seed the approved shapes: unordered interactions, structural deletion/truncation, null cycle, late structural reachability, repeated application, broad phonology, optional-slot branching, and nonregular/process morphology.
3. Require every rendered remedy group to contain “would make this backend work for your language” conditionally and “Don't make any change that would make your language invalid!” exactly.
4. Test catalog versioning, required evidence, shared remedies, deterministic order, and missing-key failure. Commit: `feat(foma): add backend advice catalog`.

## Task 2: Report every backend and choose only admissible candidates

**Files:**

- Modify: `rust/crates/pg-foma/src/backend_selection.rs`
- Modify: `rust/crates/pg-foma/src/health_evaluator.rs`
- Test: `rust/crates/pg-foma/src/backend_selection.rs`
- Test: `rust/crates/pg-foma/tests/strategy_aware_capability_gate.rs`

1. Extend compatibility reports with correctness, worst severity, stable findings, failed predicates, shapes, cost evidence, and advice references.
2. Retain Ideal/Info/Warning/Error/Critical reports for all backends, including refused ones.
3. Select zero, one, or two normally admissible candidates. Exclude Error/Critical. Sort by clean report, worst severity, finding count, then committed backend preference.
4. Sort blocking remedy sets by count of hard, medium, then easy remedies; do not let remedy effort override correctness.
5. Test all tie-breaks and commit: `feat(foma): rank complete backend reports`.

## Task 3: Persist the full report and concise build summary

**Files:**

- Modify: `rust/crates/pg-cli/src/fst_health.rs`
- Modify: `rust/crates/pg-cli/src/pack.rs`
- Modify: `rust/crates/pg-cli/src/make_report.rs`
- Test: corresponding module tests and CLI snapshots

1. Persist a machine-readable full report containing every backend and finding.
2. During build, emit only checked-backend count and warning/error/critical counts plus the saved report path.
3. Show detailed findings inline only when no backend is clean; presentation polish beyond this compiler-like contract remains out of scope.
4. Run `pg.ps1 -Mode test -Package pg-cli` and commit: `feat(cli): save backend compatibility reports`.

## Task 4: Historical five-language authority matrix (not a current shipping gate)

This task records the former five-language target for traceability only. It is not a current
acceptance obligation and must not be used to claim a trusted artifact. The active replacement is
the three-language evidence/status document linked above; Mbugwe is deferred.

**Files:**

- Modify: `rust/tools/corpus-manifest.json`
- Create: `docs/fst-plan/2026-08-21-five-language-acceptance.md`

1. Run the manifest's `requiring_tests` via `pg.ps1 -Mode corpus-test`, one managed build at a time.
2. Indonesian: run `p6_gate_parity` and record exhaustive oracle/FST parity or an explicit bounded-evidence limitation.
3. Sena: reconcile 308 skipped words, record the 123-word HC timeout tail, and run a full FST recall gate rather than the first 120 only.
4. Amharic: run `f3_interdigitation_gate`, migrate hardcoded paths to the corpus helper, and record broader timeout-aware parity.
5. For the historical matrix, import only separately measured scale results; do not infer them from miniature fixtures. Do not run or add a Mbugwe result for the current three-language slice.
6. For every language record grammar/corpus identity, selected backend, every backend report, certificate/refusal, recall, skipped, timeout, states/arcs, elapsed time, and exact command.
7. Run package suites:

```powershell
.\rust\tools\pg.ps1 -Mode test -Package pg-foma
.\rust\tools\pg.ps1 -Mode test -Package pg-cli
```

8. Commit: `test(foma): publish five-language acceptance matrix`.
