# Planner-Owned Health Reintegration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reintegrate the useful evidence, profiling, health, acknowledgement, and attention work from `codex/profiler-health` without restoring its obsolete health-owned backend selection.

**Architecture:** The compiler's strategy planner is the sole authority for semantic disposition and physical-adapter selection. Health consumes the resulting strategy choice, cost envelope, CandidateFilter observations, and HC-authoritative confirmation/profile evidence; it reports those facts but never recomputes capability, selects a backend, validates a rejection proof, or owns a second semantic-completeness denominator.

**Tech Stack:** Rust 2024 workspace, `serde`, existing `pg-foma` planning/capability modules, `pg-parse`, `pg-cli`, `rust/tools/pg.ps1`, and source material retained on `codex/profiler-health` at `45abaffc`.

---

## Frozen boundaries

The data flow is:

```text
Machine obligation projection + GrammarSemantics + planning/resource policy
  -> StrategyPlanner
  -> StrategyChoice { disposition, physical_adapter, plan_id, catalog_revision }
  -> CostEnvelope and measured adapter cost
  -> CandidateFilter evidence
  -> HC confirmation/profile evidence
  -> HealthReport
  -> acknowledgements
  -> attention presentation
```

`Admit | ConfirmOnly | Refuse` is planner evidence. `Healthy | Warning | Critical` is cost/operational presentation. Neither may be converted into the other. An incomplete, censored, bypassed, or budget-exhausted observation stays typed and cannot silently become zero work or semantic success.

Do not merge or cherry-pick `codex/profiler-health` wholesale. Use it as read-only source material and port one contract at a time. In particular, do not transplant `best_case_across_backends`, capability-derived `FomaTier` health logic, `UnknownUnboundedConstruct` as a planner-refusal surrogate, or attention advice that chooses a backend or proposes semantic grammar repair.

## File map

- `rust/crates/pg-evidence/` — stable evidence, context, and observation identities; port first.
- `rust/crates/pg-foma/src/strategy_planner.rs` — authoritative planner output consumed by health (owned by the architecture plan, not health).
- `rust/crates/pg-foma/src/cost_envelope.rs` — projected-cost vocabulary and revision identity (owned by the architecture plan).
- `rust/crates/pg-foma/src/preflight.rs` — adapt to consume `StrategyChoice`; remove health-owned selection.
- `rust/crates/pg-foma/src/health_evaluator.rs` — pure translation of planner/cost/observed evidence into findings.
- `rust/crates/pg-foma/src/health.rs` — versioned report wire schema and deterministic serialization.
- `rust/crates/pg-parse/src/profile.rs` — deterministic HC work profiler with censoring and provenance.
- `rust/crates/pg-cli/src/fst_health.rs` — orchestration/presentation only.
- `rust/crates/pg-health-ack/` — immutable acknowledgement storage after report identities stabilize.
- `rust/crates/pg-attention/` — verified evidence joins and presentation, last.
- `openspec/changes/recipe-scoped-fst-health/{proposal,tasks}.md` — architecture contract and live queue.

## Ordered task queue

### Task 0: Establish the integration base and authoritative identities

**Files:**

- Modify: `openspec/changes/STAGING.md`
- Verify: Machine catalog pointer and accepted filter/circumfix integration tip

- [ ] Record the accepted PanGloss base SHA, Machine catalog commit/digest, catalog schema/profile, and the branch tips supplying CandidateFilter and circumfix work.
- [ ] Import Machine atomic, within-rule configuration, cross-rule interaction, and schedule obligation IDs before defining health fields that refer to semantic coverage.
- [ ] Verify `codex/profiler-health` still resolves at `45abaffc`; treat a changed tip as a new audit input.
- [ ] Commit only the ownership/dependency record.

### Task 1: Port shared evidence identities independently

**Files:**

- Create: `rust/crates/pg-evidence/Cargo.toml`
- Create: `rust/crates/pg-evidence/src/lib.rs`
- Create: `rust/crates/pg-evidence/tests/identity_contract.rs`
- Modify: `rust/Cargo.toml`
- Modify: `rust/Cargo.lock`

- [ ] Port and rebase the behavior from `c29ff254^..a85c0009`, preserving typed context version, construct key, observation key, canonical validation, and anti-forgery tests.
- [ ] Add a failing test that distinct Machine catalog revisions cannot compare as the same evidence context.
- [ ] Run `& rust\tools\pg.ps1 -Mode test -Package pg-evidence`; expect all identity tests to pass.
- [ ] Commit as one evidence-only slice. Do not add health, planner, acknowledgements, or attention dependencies.

### Task 2: Freeze planner and cost evidence contracts

**Files:**

- Create or modify: `rust/crates/pg-foma/src/strategy_planner.rs`
- Create or modify: `rust/crates/pg-foma/src/cost_envelope.rs`
- Test: `rust/crates/pg-foma/tests/strategy_planner_contract.rs`

- [ ] Define planner-owned output equivalent to:

```rust
pub struct StrategyChoice {
    pub disposition: StrategyDisposition,
    pub physical_adapter: Option<PhysicalAdapter>,
    pub plan_id: Option<PlanId>,
    pub machine_catalog_revision: CatalogRevision,
    pub obligations: Vec<MachineObligationId>,
    pub cost: CostEnvelope,
}
```

- [ ] Test that a refusal has no realized adapter, `ConfirmOnly` names HC as authoritative, unknown/high projected cost does not itself change semantic disposition, and every choice is tied to a pinned catalog revision.
- [ ] Route existing selection through this deep interface rather than layering a second advisory planner over `best_case_across_backends`.
- [ ] Run `& rust\tools\pg.ps1 -Mode test -Package pg-foma -TestTarget strategy_planner_contract` and commit the planner contract before health consumers.

### Task 3: Make preflight and health evaluation pure consumers

**Files:**

- Modify: `rust/crates/pg-foma/src/preflight.rs`
- Modify: `rust/crates/pg-foma/src/health_evaluator.rs`
- Test: `rust/crates/pg-foma/tests/health_planner_boundary.rs`

- [ ] Write a failing test that injects a `StrategyChoice` and proves health reports exactly that choice without calling `best_case_across_backends` or interpreting `CharacteristicsProfile`/`FomaTier` as capability.
- [ ] Change the entry point to the explicit boundary:

```rust
pub fn evaluate_health(
    choice: &StrategyChoice,
    projected: &CostEnvelope,
    observed: Option<&ObservedPipelineEvidence>,
) -> HealthReport;
```

- [ ] Retain a thin convenience caller only if it obtains its choice from `StrategyPlanner`; delete health-specific best-backend joins.
- [ ] Test `Admit`, `ConfirmOnly`, `Refuse`, high-cost admitted, and incomplete-observation cases.
- [ ] Run the focused test through `pg.ps1` and commit this boundary separately.

### Task 4: Port deterministic HC profiling with pipeline attribution

**Files:**

- Create: `rust/crates/pg-parse/src/profile.rs`
- Modify: `rust/crates/pg-parse/src/lib.rs`
- Modify: `rust/crates/pg-parse/Cargo.toml`
- Test: focused `pg-parse` profile tests

- [ ] Port the deterministic counters, capped examples, canonical ordering, censorship, and provenance beginning at `05c284e5`, including the later mixed/capped-observation fixes.
- [ ] Extend each report with requested strategy, realized adapter, plan ID, cost-envelope revision, raw proposal count, filter retained/rejected/deferred/bypassed counts, HC confirmation count, and completeness.
- [ ] Test that censored or budgeted counts cannot serialize as complete exact counts and that candidate/filter/HC phases remain distinguishable.
- [ ] Run `& rust\tools\pg.ps1 -Mode test -Package pg-parse` and commit the profiler before wiring CLI health.

### Task 5: Publish HealthReport v2 as a reporting schema

**Files:**

- Modify: `rust/crates/pg-foma/src/health.rs`
- Test: existing health unit tests plus `health_planner_boundary.rs`

- [ ] Adapt the schema work from `b5852ebe` and `c59ae2a4`; do not cherry-pick its old capability consumers.
- [ ] Represent planner choice, projected/measured cost, CandidateFilter completion/proofs/defer summaries, HC evidence, and evidence completeness as separate typed observations.
- [ ] Preserve deterministic canonical serialization, severity, coverage, and admission only as cost/presentation concepts.
- [ ] Add a serialization test proving no general linguistic-quality score or health-owned support verdict exists.
- [ ] Run the focused `pg-foma` health tests and commit the wire schema.

### Task 6: Rewire `fst-health` to the normal pipeline

**Files:**

- Modify: `rust/crates/pg-cli/src/fst_health.rs`
- Test: focused `pg-cli` CLI tests

- [ ] Adapt ideas from `548b9fd2` manually after Tasks 2–5.
- [ ] Make the command obtain one `StrategyChoice`, execute only its selected proposer/filter/HC path, and pass the resulting evidence into `evaluate_health`.
- [ ] Print requested versus realized strategy, filter bypass/incomplete states, HC authority, and projected versus measured cost without suggesting a different semantic backend.
- [ ] Test zero proposals, all candidates filtered, filter bypass, HC incomplete, and planner refusal as distinct outputs.
- [ ] Run `& rust\tools\pg.ps1 -Mode test -Package pg-cli` and commit CLI integration.

### Task 7: Port immutable acknowledgements after report identity freezes

**Files:**

- Create: `rust/crates/pg-health-ack/`
- Modify: workspace manifests
- Test: `pg-health-ack` contract and matching tests

- [ ] Port behavior from `78541c0e^..bd312ced`, rebasing acknowledgement keys onto the final evidence/report identities.
- [ ] Preserve append-only atomic persistence, matching, resurfacing, and lock/transaction-bypass tests.
- [ ] Keep acknowledgement presentation separate from whether a strategy is semantically admitted.
- [ ] Run `& rust\tools\pg.ps1 -Mode test -Package pg-health-ack` and commit storage before CLI presentation.

### Task 8: Port attention last

**Files:**

- Create: `rust/crates/pg-attention/`
- Modify: `rust/crates/pg-cli/src/attention.rs`
- Test: attention contract tests

- [ ] Port the verified-context join model from `7ba04bfa`/`04a0f354`, then select one source-identity hardening lineage; do not merge both `878e4492` and `87b9001d` alternatives.
- [ ] Add typed sources for planner choice, cost envelope, CandidateFilter, and HC-authoritative evidence.
- [ ] Restrict attention to evidence quality, missing/incomplete observations, and operational cost; it may not choose a backend, certify support, or prescribe meaning-changing grammar repair.
- [ ] Run `& rust\tools\pg.ps1 -Mode test -Package pg-attention` plus focused CLI tests and commit.

### Task 9: Authoritative integrated verification

**Files:** none unless a verified failure requires a focused correction.

- [ ] Measure physical memory, commit headroom, CPU load, and live procgov/Cargo trees before builds; keep every Rust command inside `rust/tools/pg.ps1`.
- [ ] Run focused tests for `pg-evidence`, planner boundary, `pg-parse`, `pg-foma` health, `pg-health-ack`, `pg-attention`, and `pg-cli`.
- [ ] Run the CandidateFilter shadow/enforcement gates and the Machine-backed semantic matrix applicable to every strategy claimed by the planner.
- [ ] Inspect JSON/Markdown twice for byte-stable canonical output and verify incomplete evidence remains visible.
- [ ] Require zero call sites where health or attention invokes `best_case_across_backends`, selects a physical adapter, or converts cost severity into semantic disposition.

### Task 10: Retire the audited health source branches

**Files:** none.

- [ ] After Tasks 1–9 pass on the integrated tip, compare `codex/profiler-health`,
  `codex/attention-quality`, and `codex/ack-corrections` against that tip and classify every unique
  commit as ported, superseded, or deliberately rejected in the salvage ledger.
- [ ] Verify no worktree uses those branches and no uncommitted files remain in any associated
  checkout.
- [ ] Delete the three local source branches only after the comparison proves no required code or
  evidence remains reachable solely through them. Delete corresponding remote branches only when
  they exist and the same proof holds.
- [ ] Record the deleted branch names, former tip SHAs, and their integrated/superseded disposition in
  `openspec/changes/STAGING.md` so later archaeology does not depend on reflogs.

## Salvage ledger

- **Port early:** `c29ff254^..a85c0009` (`pg-evidence`).
- **Port after planner:** `05c284e5` profiler lineage, then `b5852ebe`/`c59ae2a4` schema ideas and `548b9fd2` CLI ideas.
- **Port after report identity:** `78541c0e^..bd312ced` acknowledgements.
- **Port last:** `7ba04bfa`, `04a0f354`, and one attention identity-hardening lineage.
- **Optional, separately justified:** project configuration and FieldWorks identity commits; configuration expresses requests, never the final planner choice.
- **Never wholesale:** branch tips or any old health-owned semantic selector.
