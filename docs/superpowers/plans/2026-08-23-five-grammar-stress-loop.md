# Five-Grammar Stress Loop Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Attempt complete contained FST construction and record backend-specific evidence for Indonesian, Amharic, Aweti, Sena, and Mbugwe.

**Architecture:** Reuse the selected-payload worker seam and canonical reports. A small PanGloss-only manifest binds each stress grammar and corpus; production certification remains a separate three-language artifact.

**Tech Stack:** Rust, pg-foma worker, pg-assess reports, private corpus manifest, `pg.ps1` corpus gates.

---

### Task 1: Freeze the five stress cases

**Files:**
- Modify: `rust/tools/corpus-manifest.json`
- Create: `rust/crates/pg-foma/tests/five_grammar_stress_gate.rs`

- [ ] Add one manifest record per grammar with stable grammar/corpus IDs and no embedded private data.
- [ ] Add a failing test requiring all five records, all three backend reports, selected/realized
identity, compile size mode, completion terminal, payload digest, readiness, and containment outcome.
- [ ] Run: `& rust/tools/pg.ps1 -Mode corpus-test -Package pg-foma -TestTarget five_grammar_stress_gate -TestThreads 1`; expected FAIL on missing evidence fields/cases.
- [ ] Commit: `test(foma): freeze five stress grammars`.

### Task 2: Select Error for stress without publishing it

**Files:**
- Modify: `rust/crates/pg-foma/src/backend_selection.rs`
- Modify: `rust/crates/pg-foma/src/completed_build.rs`
- Modify: `rust/crates/pg-foma/src/worker.rs`
- Test: `rust/crates/pg-foma/tests/five_grammar_stress_gate.rs`

- [ ] Add a typed `SelectionPurpose::{Production, DeveloperStress}` parameter. Production admits
only correctness-admitted readiness `<= Warning`; stress may try correctness-admitted Error.
- [ ] Require both purposes to reject Critical, missing payload, live frontier, uncovered/skipped
material, backend mismatch, and parity failure.
- [ ] Preserve every backend report and rank stress candidates by worst severity, finding count,
remedy effort, then committed backend order.
- [ ] Run `backend_selection_contract`, `trusted_selected_build_gate`, and the stress gate; expected PASS.
- [ ] Commit: `feat(foma): select contained stress builds`.

### Task 3: Run and classify all five grammars

**Files:**
- Modify: `rust/crates/pg-foma/tests/five_grammar_stress_gate.rs`
- Create: `docs/fst-plan/2026-08-23-five-grammar-stress-results.md`

- [ ] Before the build, run `pg.ps1 -Mode doctor` and record available memory, CPU/process trees,
and whether one 19 GB managed slot is safe.
- [ ] Run each case single-threaded under the normal envelope, then use developer stress mode only
for correctness-admitted Error. Never use `--allow-unproven` as accuracy evidence.
- [ ] Record exact terminal state, states/arcs/work/probes, payload digest, parity denominator,
warnings/errors, dominant contributors, and ranked remedies for every backend.
- [ ] Treat external ceiling, timeout, live frontier, missing payload, or parity gap as typed failure;
do not copy a partial artifact into results.
- [ ] Commit each newly green language independently using `test(foma): record <language> stress evidence`.

### Task 4: PanGloss-only policy conformance

**Files:**
- Create: `rust/crates/pg-foma/tests/fixtures/pangloss/fst-completeness/error-stress-completes/grammar.xml`
- Create: `rust/crates/pg-foma/tests/fixtures/pangloss/fst-completeness/live-frontier-refuses/grammar.xml`
- Create: `rust/crates/pg-foma/tests/stress_admission_conformance.rs`

- [ ] Prove an Error stress build can complete exactly while staying production-unready.
- [ ] Prove a live frontier and outer containment termination never produce success.
- [ ] Prove these fixtures remain PanGloss-only and are absent from Machine promotion discovery.
- [ ] Run local conformance with `& rust/tools/pg.ps1 -Mode conformance-test -Scope local`, then the
focused stress/worker/selection gates.
- [ ] Commit: `test(foma): pin stress admission policy`.

### Task 5: Authoritative integration

- [ ] Run the focused developer-control, worker, completion, selection, and five-grammar targets.
- [ ] Run the single authoritative `pg-foma` package test through `pg.ps1` after all focused gates pass.
- [ ] Regenerate backend cards only if their static capability catalog changed; do not insert
language measurements into cards.
- [ ] Update the stress results and three-language production report as separate artifacts.
- [ ] Verify `git diff --check`, request independent review, commit, and push the rebased branch.
