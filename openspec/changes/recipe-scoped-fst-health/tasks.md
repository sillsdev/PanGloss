# Planner-owned health reintegration queue

Detailed TDD steps, file ownership, source commits, and verification commands live in
`docs/superpowers/plans/2026-08-13-planner-owned-health-reintegration.md`.

## 0. Prerequisite integration and ownership

- [ ] 0.1 Record accepted PanGloss/filter/circumfix tips and the pinned Machine catalog revision.
- [ ] 0.2 Import stable Machine atomic/configuration/interaction/schedule obligation identities.
- [ ] 0.3 Freeze the StrategyPlanner, StrategyChoice, physical-adapter, plan-ID, and CostEnvelope boundary.

## 1. Independent evidence substrate

- [ ] 1.1 Port `pg-evidence` from `c29ff254^..a85c0009` with its anti-forgery/canonical tests.
- [ ] 1.2 Bind evidence context to the Machine catalog revision without adding a health completeness ledger.

## 2. Remove health-owned selection

- [ ] 2.1 Change `preflight.rs` and `health_evaluator.rs` to consume `StrategyChoice` and cost evidence.
- [ ] 2.2 Remove `best_case_across_backends` and capability-derived `FomaTier` interpretation from health.
- [ ] 2.3 Prove high cost cannot change semantic disposition and planner refusal is not encoded as an unknown-cost finding.

## 3. Port HC and pipeline profiling

- [ ] 3.1 Port deterministic HC profiler mechanics beginning at `05c284e5` plus censoring/provenance fixes.
- [ ] 3.2 Add requested/realized strategy, adapter, plan, cost revision, proposal, filter, confirmation, and completeness fields.
- [ ] 3.3 Prove incomplete/censored/budgeted observations never serialize as exact zero work.

## 4. Publish health schema and CLI

- [ ] 4.1 Adapt HealthReport v2 ideas from `b5852ebe`/`c59ae2a4` after planner fields freeze.
- [ ] 4.2 Adapt `fst-health` ideas from `548b9fd2` to execute and report the one planner-selected pipeline.
- [ ] 4.3 Distinguish planner refusal, zero proposals, all-filtered, filter bypass, and HC incomplete outcomes.
- [ ] 4.4 Keep severity/admission strictly on the cost and presentation axis.

## 5. Downstream evidence consumers

- [ ] 5.1 Port immutable acknowledgements from `78541c0e^..bd312ced` after report identity freezes.
- [ ] 5.2 Port attention last; add planner/cost/filter/HC typed sources and select one identity-hardening lineage.
- [ ] 5.3 Prove neither acknowledgements nor attention selects a backend, certifies support, or prescribes meaning-changing repair.

## 6. Integration gate

- [ ] 6.1 Run all focused crate tests through `rust/tools/pg.ps1` after measuring managed-build headroom.
- [ ] 6.2 Run CandidateFilter gates and the applicable Machine-backed semantic matrix.
- [ ] 6.3 Search for and eliminate health/attention calls to `best_case_across_backends` and any cost-to-capability conversion.
- [ ] 6.4 Inspect canonical JSON/Markdown output and verify incomplete evidence stays explicit.

## 7. Retire health source branches after verified integration

- [ ] 7.1 After Sections 0–6 pass, compare `codex/profiler-health`, `codex/attention-quality`, and
      `codex/ack-corrections` with the integrated tip and mark every unique commit ported,
      superseded, or deliberately rejected.
- [ ] 7.2 Verify no worktree uses those branches and delete their local branches; delete corresponding
      remote branches only if they exist and contain no unique required work.
- [ ] 7.3 Record former tips and dispositions in `openspec/changes/STAGING.md` before branch deletion.

## Explicitly rejected integration paths

- [ ] Never merge or cherry-pick the `codex/profiler-health` tip wholesale.
- [ ] Never restore a health-owned “best backend” computation.
- [ ] Never use `UnknownUnboundedConstruct` for planner refusal or semantic incompleteness.
- [ ] Never count health rows as the Machine semantic-completeness denominator.
