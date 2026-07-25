## Why

`pangloss coverage` (schema v1, `rust/crates/pg-foma/src/coverage_ledger.rs`) already prints the honest
current state: 20 `CharacteristicKind`s, 6 `Proven` / 3 `ConfirmOnly` / 10 `ConfigPredicate` /
1 `FailClosed`, 12/20 with a registered discharging predicate, 19/20 with curated containment-test
evidence, 16/20 mapped to a `machine/conformance/constructs.txt` id and `Covered` by a passing fixture,
4 permanently `Unmappable`. Stage 2 shipped every construct's fail-closed-to-predicate kit; Stage 3
(`add-pairwise-grammar-interaction-coverage` → `plan_interaction_coverage.rs`) shipped the tree-structured
node/subtree interaction instrument this task's own framing invokes ("this is what we made the poly-FST
tree structure for"). What has never existed is the **consolidated plan** that reads those two artifacts
together and says, per remaining construct, exactly what evidence closes it out — as opposed to leaving
"finish coverage" as an unscoped standing intention that Stage 2/3 changes gesture at but never total up.

Without this plan, "full coverage" has no checkable finish line: nobody can say today whether a given
`ConfigPredicate` row's `Refuse` split is a genuine open proof obligation, a permanent architectural
carve-out (ADR 0001 says `ConfirmOnly` is a legitimate *permanent* rest — full coverage does not mean
everything becomes `Proven`), or a question only the never-built C# oracle harness can answer. Left
unscoped, "cover every construct" degenerates into either premature closure (declaring gaps permanent
without proof) or unbounded fuzzing (the exact failure mode ADR 0001's own "Considered and rejected"
section names: "strict n-way interaction proof... never ships a real language"). The reified plan tree
plus its two already-proven orthogonality retirements (`plan_interaction_coverage::retired_interactions`)
is the mechanism that keeps this finite; this change is the plan that actually uses it.

## What Changes

- Define the promotion ladder once, per ADR 0001: what evidence moves a construct's specific
  configuration from `Refuse` to a closed, evidenced rest (`ConfirmOnly`/`Admit`), and state plainly
  that `ConfirmOnly` is a legitimate **permanent** landing spot for architecturally confirm-dependent
  constructs — never a synonym for "not done yet." `Admit` (the FST proposer narrowing its own output,
  proven no-false-negative) is an explicitly **optional, non-blocking optimization** on top of an
  already-closed `ConfirmOnly` rung (ADR 0001's own words) — promoting `ConfirmOnly` → `Admit` is never
  required for "full coverage" and is out of scope for this plan's own finish line.
- Produce a per-construct table for all 14 non-`Proven` `CharacteristicKind`s (not 13 — the actual count
  per `capability.rs::default_disposition` and `pangloss coverage`'s own printed totals, 3 + 10 + 1):
  disposition, the specific unsupported split, what would close it, which conformance fixture(s) are
  needed, and a verdict of PROVABLE / NEEDS-ORACLE / PERMANENT CARVE-OUT / NEEDS-DECISION.
- Specify how the fixture set for closing each open split is bounded by the reified plan tree: the
  7-shape closed `legal_adjacency_tuples()` set never grows as constructs are promoted (most non-`Proven`
  characteristics fold onto the single representative `Gate` node, not their own `PlanNodeKind`), so
  closing a `Refuse` gap adds one evidenced fixture per open (construct, configuration) cell, never a
  cross-product; the two existing orthogonality retirements demonstrate the mechanism that keeps this
  converging rather than growing.
- Name the upstream `constructs.txt` task: `LeftToRightRewrite`, `RightToLeftRewrite`, `SubruleGating`,
  and `MultiTable` can never leave `Unmappable` without new rows landing in `sillsdev/machine`'s
  `constructs.txt` first — an explicit PR, not something this repo can unblock alone.
- Identify which promotions are blocked on the C# HermitCrab oracle harness
  (`add-reference-hermitcrab-parity` §§2-5, currently zero code) versus which can proceed against this
  repo's own confirm engine (assumed complete per `IMPLEMENTATION-READINESS.md` R1).
- Give a sequenced work order and a crisp, checkable definition of "full coverage" — including that the
  conformance-coverage cross-check must be fixed to be **ledger-wide** (today's
  `conformance_coverage::supported_kinds()` scopes itself to the 6 `Proven` kinds only, which is
  narrower than the 16/20-row ledger `pangloss coverage` already reports) before it can honestly flip
  from advisory to build-breaking — that flip is the actual finish line.
- Add this change to `openspec/changes/STAGING.md`'s spine as the successor to Stage 2/3's per-construct
  work, and record it under "Still open."

## Impact

Documentation and planning only — no `.rs` file is touched. Affected specs: new capability
`construct-coverage-completion` (this change's own `specs/` directory). Downstream, the plan's
sequencing hands off PROVABLE items to future per-construct worktrees (Stage 2's own one-construct-
one-kit discipline), NEEDS-DECISION items to a human/architect decision record, and the oracle-dependent
item to whichever change resumes `add-reference-hermitcrab-parity`.
