# Architecture and Testing Assurance Plan

> **For agentic workers:** Use `agent-handoff` for bounded Luna research and four independent Sol/xhigh review lanes. Implement confirmed fixes with a failing regression first, then review specification compliance and code quality.

**Goal:** Audit the architecture, seams, and demonstrated testing of PanGloss's additions around its HermitCrab core, correct confirmed local defects, and separately disconnect the optional whole-grammar Foma subsystem from the ordinary solution.

**Architecture:** Inspect a fixed baseline before integrating changes. Each finding names an owning module, observable contract, and discriminating test; shared policy must be called or extracted from its owner. Preserve the HermitCrab core's internal finite-state rule implementation while removing the separate Foma proposer/compiler from ordinary deployment dependencies.

**Tech Stack:** Rust workspace, managed PowerShell launcher, C# HermitCrab oracle, upstream and staged conformance grammars, SQLite statistics, native C ABI and WASM.

## Baseline and operating constraints

- Superproject: `ac13fc17a5bef39c5419b97ea02886113b1178b4` (`main`).
- Superproject's pinned Machine revision: `f412c252172d6589339d8f25ff6a7258ff33b432`.
- Main checkout's actual Machine revision: `34215889c7adf3012f700c9ee1c1d6712c056d15`; it initially differed from the pin. The later explicit user request authorized updating both main and review Machine checkouts to PR 480 head `d8ff628dc53b3ba9c96cffc712cc5909fdfe961b`.
- Isolated implementation branch: `review/architecture-assurance`, under `.worktrees/architecture-assurance`.
- Existing candidate worktrees have unrelated untracked or modified files; none will be repurposed.
- Initial resources: 66,825,620 KiB physical memory, 30,565,348 KiB free, 5% CPU, 20 logical processors; only the permanent sccache daemon is present. Admit at most two managed builds; reassess before raising concurrency.
- Native agent capacity is four including the coordinator. Schedule four Sol reviewers and up to eight Luna investigators in waves; read-only investigations may share the baseline.
- The ordinary sandbox launcher stalled on read-only commands. An escalated read probe succeeds; the diagnostic reports no direct runtime validation failure. This is infrastructure evidence, not a repository test result.

## Task 1: Establish executable evidence

- [x] Record source, submodule, working-tree, existing-worktree and resource baselines.
- [x] Verify the managed worktree's initialized conformance revision and fixture inventory.
- [x] Run `pwsh -NoProfile -File rust/tools/pg.ps1 -Mode check` in the isolated worktree and record its exact scope and result.
- [x] Inventory relevant unit, integration, corpus, oracle, host-target and PowerShell tests; distinguish execution from fixture presence.

## Task 2: Schedule eight bounded Luna sessions

- [x] `review_scope`: selected literal text, provenance, and later exact Sena environment witnesses.
- [x] `fst_default_scope`: Foma boundaries and native fallback health/pack review.
- [x] `luna_stats_timing`: baseline measurements and bounded observability implementation.
- [x] `luna_foma_disconnect_impl`: isolated structural Foma implementation.
- [x] `luna_artifact_integrity_impl`: isolated assessment/report integrity implementation.
- [x] `health-seams`, `try-word-seams`, `snapshot-pack-seams`: attempted read-only CLI handoffs, unavailable because their local tools stalled; owned runners terminated and status retained.
- [x] Cover unavailable investigations through bounded native Luna fallback, independent Sol lanes, and primary source inspection.

This records eight sessions, five successful native handoffs and three unavailable CLI attempts. It does not claim eight successful independent investigations. Research-only handoffs made no source mutations; implementation sessions had separate worktrees and at most two concurrent Workspace writers. The primary inspected every delegated diff and claim.

## Task 3: Complete four independent Sol review lanes

- [x] S1: project-to-grammar architecture, auto-created phonology, provenance and policy ownership.
- [x] S2: execution and public interfaces, try-a-word, host consistency, lifecycle and concurrency.
- [x] S3: measurements and diagnostics, statistics/timing/health correctness and report identity.
- [x] S4: architecture/test assurance, oracle standards, false greens, missing evidence and managed CI scope.

Each Sol first assesses its lane independently. Supply Luna findings afterward for challenge, not as trusted conclusions. The coordinator inspects every claim and resolves overlap and disagreements.

## Task 4: Build the behavior-to-evidence matrix

- [x] Cover import, construction, auto-created phonology, analysis/generation, try-a-word, statistics, timing, health, assessment, packs and host interfaces.
- [x] For each workflow identify the owning seam, promised behavior, existing exact tests, actual runs, provenance, positive/negative controls, and remaining evidence gaps.
- [x] Trace at least one cross-feature project flow, checking that grammar identity and effective options agree across stages.
- [x] Require complete word rows, statuses and parse-identity multisets; caps, timeouts, skips, missing inputs and baselined divergences remain explicit.

## Task 5: Integrate confirmed corrections

- [x] Record each adjudicated finding in `reports/architecture-testing-review/README.md` before changing its implementation.
- [x] For each behavior correction write and execute a regression that fails for the claimed defect, implement at the owning seam, and rerun it.
- [x] Keep Foma disconnection as a separate change with dependency-graph and runtime-effect evidence; do not change correctness/refusal/containment thresholds to achieve it.
- [x] Inspect every delegated diff personally, reject out-of-scope mutations, and obtain independent Sol review of consequential integration.

## Task 6: Authoritative verification and final report

- [x] Run managed `check`, then appropriate `quick` and narrowed integration/conformance checks; run supported host-target checks and relevant PowerShell tests.
- [x] Attempt founding-oracle verification through `rust/tools/oracle-conformance.ps1`; distinguish unavailable oracle from actual and baselined divergences and record the actual C# revision.
- [x] Record exact commands, pass/fail/skip counts, scope, relevant resource observations and untested cases.
- [x] Publish the findings ledger and behavior-to-evidence matrix with corrected defects, accepted architecture risks and remaining coverage gaps separately.
- [x] Verify unrelated main-checkout files remain unchanged and its Machine checkout equals the explicitly requested new PR 480 revision.

Final C# verification at the requested new head is blocked by the upstream conformance project's CS0535 interface mismatch; historical results remain separately labeled.

Final evidence: full default Rust 1615 PASS / 0 FAIL / 64 SKIP; strengthened real-project gate 4 PASS; PowerShell 29 files PASS; generated WASM smoke 3 PASS / 2 private-data SKIP; all four optional Foma feature checks PASS. Required managed check ran before every commit. Main remains on its existing branch with only the authorized Machine gitlink change; reviewed code is committed on `review/architecture-assurance`.
