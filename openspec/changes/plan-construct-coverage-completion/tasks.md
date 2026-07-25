## 1. The plan itself (this change)

- [x] 1.1 Read the ground truth: `pangloss coverage` output, `capability.rs`, `coverage_ledger.rs`,
      `conformance_coverage.rs`, `plan_interaction_coverage.rs`, `constructs.txt`, ADR 0001/0002/0005,
      `add-reference-hermitcrab-parity`'s `tasks.md`, the `conformance-grammars` skill, and
      `STAGING.md` — no number or claim invented (design.md's own "Ground truth" section)
- [x] 1.2 Define the promotion ladder once, distinguishing the mandatory `ConfirmOnly` rest from the
      optional, non-blocking `Admit` optimization (design.md D1)
- [x] 1.3 Produce the per-construct table for all 14 non-`Proven` kinds with disposition, split,
      closing evidence, fixture(s), and verdict (design.md D2)
- [x] 1.4 Report the `constructs.txt` ↔ `CharacteristicKind` mismatch shape in both directions
      (design.md D3)
- [x] 1.5 Specify the tree-bounded fixture-enumeration method (design.md D4)
- [x] 1.6 Name the upstream `constructs.txt` PR with proposed row text (design.md D5)
- [x] 1.7 Identify which promotions need the C# oracle harness vs. which can proceed against the
      in-repo confirm engine (design.md D6)
- [x] 1.8 Sequence the work and state a crisp definition of done (design.md D7)
- [x] 1.9 Add this change to `STAGING.md`'s spine (successor to Stage 2/3) and to its "Still open"
      section
- [x] 1.10 `openspec validate plan-construct-coverage-completion --strict` and
      `openspec validate --all --strict` both pass

## 2. Upstream constructs.txt task (hand-off, tracked here — not implemented by this change)

- [ ] 2.1 Open a PR against `sillsdev/machine` (`conformance-framework` branch) adding constructs.txt
      rows for rewrite direction, subrule-level MPR/POS gating, and multi-table threading (design.md D5)
- [ ] 2.2 On acceptance, bump the `machine` submodule pointer and update `conformance_coverage.rs::
      construct_ids_for`'s four empty-slice arms to the new ids

## 3. Fix the conformance-coverage gate's scope before flipping it (hand-off)

- [ ] 3.1 Widen `conformance_coverage.rs`'s cross-check from `supported_kinds()` (Proven-only) to a
      ledger-wide check over all 20 `CharacteristicKind`s, excluding permanently-carved-out `Refuse`
      configurations the same way `plan_interaction_coverage::TupleStatus::ContainsUnsupported`
      excludes them (design.md D7 step 2)

## 4. Close PROVABLE rows, one construct at a time (hand-off, Stage-2-style full kits)

- [ ] 4.1 `Compounding.recursive`: rule-graph reachability + depth-budgeted construction + containment
      test + new `edge-cases` fixture
- [ ] 4.2 `RightToLeftRewrite`: extend `compile_rtl_branch_net` to currently-excluded pattern shapes,
      one shape at a time, each with its own fixture
- [ ] 4.3 `CircumfixOutputAction`: census which allomorph shapes fail `is_structural_rule`/
      `build_structural_composites` today, then extend the builder to cover them
- [ ] 4.4 `MultiTable`: design and build a disjoint-token-range encoding across `CharacterDefinitionTable`s
      to close the shared-representation `Refuse` split

## 5. Escalate NEEDS-DECISION and NEEDS-ORACLE items (hand-off)

- [ ] 5.1 Record a human/architect decision (new short ADR or dated `STAGING.md` note) on: `Metathesis`'s
      from-scratch RTL swap construction (build it, or declare a permanent scope boundary?);
      `QuantifierPattern`'s genuinely-unbounded case (structurally infeasible, or simply unattempted?);
      whether either, if greenlit, also warrants a C#-oracle re-verification pass
- [ ] 5.2 Resume `add-reference-hermitcrab-parity` §§2-5 far enough to independently verify
      `SimultaneousRewrite`'s overlapping-subrule configuration against `hc.dll` (design.md D6)

## 6. The finish line (hand-off)

- [ ] 6.1 Re-run `plan_interaction_coverage`'s report after every promotion in section 4; confirm the
      7-tuple set stays closed and both existing retirements still hold
- [ ] 6.2 Flip the ledger-wide conformance-coverage cross-check (section 3) from advisory to
      build-breaking
- [ ] 6.3 Flip `plan_interaction_coverage`'s own gate from advisory to build-breaking
- [ ] 6.4 Confirm the definition of done (design.md D7): zero un-evidenced ledger rows, zero
      `Unmappable` rows, zero unresolved NEEDS-DECISION rows, both gates build-breaking
