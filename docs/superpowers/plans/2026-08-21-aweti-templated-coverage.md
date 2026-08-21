# Aweti Templated-Backend Implementation Plan

> Execute after the Mbugwe correctness foundation. All Rust commands use `rust/tools/pg.ps1`.

**Goal:** Route Aweti away from the 3,093,412-entry eager enumeration path, close the six known templated recall gaps, and prove the selected route against the real corpus.

## Task 1: Reconcile the current 100/106 versus 106/106 evidence

**Files:**

- Modify: `rust/crates/pg-foma/tests/p6_templated_morphotactics_gate.rs`
- Create: `docs/fst-plan/2026-08-21-aweti-templated-results.md`

1. Run the current gate unchanged:

```powershell
.\rust\tools\pg.ps1 -Mode corpus-test -Package pg-foma -TestTarget p6_templated_morphotactics_gate -TestThreads 1
```

2. Record the exact current result for `muʼazan`, `tsãkỹjokwaw`, `moʼazan`, `tsãn`, `moʼaza`, and `kỹjokwaw`.
3. Replace stale expected-miss prose/assertions with a table-driven assertion that compares FST proposals to the HC oracle for each word.
4. Commit: `test(foma): reconcile Aweti templated recall`

## Task 2: Close generic cascade construct gaps

**Files:**

- Modify: `rust/crates/pg-foma/src/emit.rs`
- Modify: `rust/crates/pg-foma/src/backend_runtime.rs`
- Modify as indicated by the failing witness: `rust/crates/pg-foma/src/morphotactics.rs`
- Test: `rust/crates/pg-foma/tests/p6_templated_morphotactics_gate.rs`

For each failing word, reduce the mismatch to the first missing generic construct, add the smallest synthetic test before production code, then implement it without an Aweti-specific branch. Run the synthetic test and the six-word gate after every fix. Commit each independent construct fix separately as `fix(foma): lower <construct> in templated route`.

## Task 3: Select the templated route for Aweti by characteristics

**Files:**

- Modify: `rust/crates/pg-foma/src/backend_selection.rs`
- Modify: `rust/crates/pg-foma/src/capability.rs`
- Test: `rust/crates/pg-foma/tests/strategy_aware_capability_gate.rs`
- Test: `rust/crates/pg-foma/tests/p6_templated_morphotactics_gate.rs`

1. Add a test whose grammar characteristics reproduce the Aweti route choice without checking a language name or path.
2. Preserve the eager route's 3,093,412-entry Error report; select templated only when its completeness certificate is valid and its worst severity is no higher than Warning.
3. Run the focused tests and commit: `feat(foma): select templated route from grammar evidence`.

## Task 4: Scale acceptance

**Files:**

- Modify only if evidence changes: `rust/tools/corpus-manifest.json`
- Modify: `docs/fst-plan/2026-08-21-aweti-templated-results.md`

1. Re-run the enumeration census to preserve the refused-route evidence:

```powershell
.\rust\tools\pg.ps1 -Mode corpus-test -Package pg-foma -Filter aweti_enum_census
```

2. Run the templated gate over every oracle-bearing corpus word, not only the six witnesses.
3. Record completeness certificate, recall, false-positive/confirmation behavior, states/arcs, memory, and elapsed time.
4. Accept only a certified route or a clear typed refusal with no trusted artifact. Commit: `test(foma): record Aweti templated scale evidence`.
