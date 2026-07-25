## ADDED Requirements

### Requirement: A single, checkable promotion ladder governs every non-Proven construct

Every `CharacteristicKind` configuration that is not `Proven` SHALL be classified against exactly one
promotion ladder (`Refuse` → `ConfirmOnly`/permanent rest → `Admit`, ADR 0001), with `ConfirmOnly`
documented as a legitimate permanent resting state and `Admit` documented as a separate, optional,
non-blocking optimization that is never required to close a construct's coverage.

#### Scenario: A ConfirmOnly configuration is treated as closed

- **WHEN** a construct's configuration has a passing conformance fixture and a curated containment-test
  citation at `ConfirmOnly`
- **THEN** it counts as closed for coverage-completion purposes, and no further work item requires
  promoting it to `Admit`

#### Scenario: A Refuse configuration is judged by the same four-part promotion criteria

- **WHEN** a construct's configuration is `Refuse`
- **THEN** its plan entry names all four promotion criteria (a structural over-approximating
  construction, a dedicated containment test, a passing conformance fixture, and a Stage-0A gate
  cross-check) and states which are missing, rather than a vague "needs work" note

### Requirement: The plan enumerates a verdict for every non-Proven CharacteristicKind

The plan SHALL contain one row per non-`Proven` `CharacteristicKind` (14 rows, per
`capability.rs::default_disposition`'s actual count), each naming its disposition, its specific
unsupported split, what would close that split, the conformance fixture(s) needed, and exactly one
verdict: PROVABLE, NEEDS-ORACLE, PERMANENT CARVE-OUT, or NEEDS-DECISION.

#### Scenario: A row cannot be honestly verdicted from available evidence

- **WHEN** the plan's author cannot determine from existing code/docs whether a construct's open split
  is PROVABLE or a PERMANENT CARVE-OUT
- **THEN** the row is marked NEEDS-DECISION and names the specific open question for a human, rather
  than defaulting to either verdict

### Requirement: Fixture enumeration is bounded by the reified plan tree, not combinatorial in gaps

The plan SHALL derive its fixture-authoring rule from the closed, 7-shape `legal_adjacency_tuples()` set
and the existing orthogonality retirements (`plan_interaction_coverage::retired_interactions`), such
that closing an open (construct, configuration) cell requires exactly one new fixture, never a
cross-product against other constructs or other tree positions.

#### Scenario: A new fixture closes an open construct configuration

- **WHEN** a new conformance fixture is authored to close a PROVABLE row's open split
- **THEN** `plan_interaction_coverage`'s adjacency-tuple set stays at exactly 7 shapes and both existing
  retirements still hold, confirming the new evidence rode on an existing tuple rather than growing the
  required set

### Requirement: The four Unmappable constructs are scheduled as an explicit upstream task

The plan SHALL name a `sillsdev/machine` `constructs.txt` PR, with proposed row text, as the explicit,
separate prerequisite for `LeftToRightRewrite`, `RightToLeftRewrite`, `SubruleGating`, and `MultiTable`
ever leaving `Unmappable` — no in-repo fixture work SHALL be claimed to close these four without it.

#### Scenario: An Unmappable construct's fixture is proposed before the upstream row exists

- **WHEN** someone proposes a conformance fixture intended to cover `MultiTable` (or any of the other
  three Unmappable kinds)
- **THEN** the plan flags that the fixture cannot resolve `Unmappable` status until the corresponding
  `constructs.txt` row lands upstream and the `machine` submodule pointer is bumped

### Requirement: Oracle-dependent promotions are distinguished from self-verifiable ones

The plan SHALL identify which open configurations require independent C# HermitCrab ground truth
(`add-reference-hermitcrab-parity`) versus which can be closed against this repo's own confirm engine,
citing `IMPLEMENTATION-READINESS.md` R1's completeness assumption and any ADR-0001-named exception.

#### Scenario: A configuration's oracle is unverified

- **WHEN** ADR 0001 itself names a configuration as never independently pinned against `hc.dll`
  (`SimultaneousRewrite`'s overlapping-subrule case)
- **THEN** the plan marks that configuration NEEDS-ORACLE and does not schedule engineering work to
  close it ahead of the C# oracle harness existing

### Requirement: Full coverage has a crisp, checkable definition of done

The plan SHALL state the exact end-state conditions for "full coverage": zero un-evidenced ledger rows,
zero `Unmappable` rows, zero unresolved NEEDS-DECISION rows, and the conformance-coverage cross-check
flipped from advisory to build-breaking over the full ledger (not a `Proven`-only subset).

#### Scenario: The conformance-coverage cross-check is scoped too narrowly to flip safely

- **WHEN** the existing `conformance_coverage::supported_kinds()` scopes itself to `Proven` kinds only
  while the ledger (`coverage_ledger.rs`) already computes status over all 20 kinds
- **THEN** the plan requires fixing that scope gap before the cross-check may be flipped to
  build-breaking, so the flip does not silently under-cover `ConfigPredicate`/`ConfirmOnly` rows
