# Five-Grammar Stress Loop Implementation Plan

> **Status: superseded/historical.** This plan is retained as a record of the five-grammar
> comparison intent. Its normal-envelope and developer-stress-mode routes are not current marching
> orders; any future measurement must use finite `ExecutionLimits`, exact completion, and the
> current worker/containment boundary.

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

### Task 2: Run and classify all five grammars under finite limits

**Files:**
- Modify: `rust/crates/pg-foma/tests/five_grammar_stress_gate.rs`
- Create: `docs/fst-plan/2026-08-23-five-grammar-stress-results.md`

- [ ] Before the build, run `pg.ps1 -Mode doctor` and record available memory, CPU/process trees,
and whether one 19 GB managed slot is safe.
- [ ] Run each case single-threaded under finite `ExecutionLimits`, retaining exact completion and
 containment evidence. Never use `--allow-unproven` as accuracy evidence; it remains local
 developer/testing generation only and never publishes.
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

- [ ] Run the focused worker, completion, selection, and five-grammar targets.
- [ ] Run the single authoritative `pg-foma` package test through `pg.ps1` after all focused gates pass.
- [ ] Regenerate backend cards only if their static capability catalog changed; do not insert
language measurements into cards.
- [ ] Update the stress results and three-language production report as separate artifacts.
- [ ] Verify `git diff --check`, request independent review, commit, and push the rebased branch.
