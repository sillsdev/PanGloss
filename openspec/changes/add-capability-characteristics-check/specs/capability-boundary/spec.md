## ADDED Requirements

### Requirement: Compilation is gated by a characteristics check

Compilation SHALL project a grammar plus its stem data into a characteristics profile, compose a
capability envelope from per-stage and interaction predicates over the reified compilation plan,
match the profile against the envelope, and either compile a recall-preserving proposer or **fail at
compile time** with a typed diagnostic naming what cannot be done faithfully. Silent
overapproximation that loses a valid HermitCrab analysis SHALL never occur.

#### Scenario: An unproven construct hard-fails

- **WHEN** a grammar uses a construct whose configuration is not proven faithful and no override is
  present
- **THEN** compilation fails at compile time with a typed diagnostic naming the construct and
  configuration, and emits no proposer

### Requirement: The characterizer is exhaustive over the frozen model with no catch-all

The characterizer SHALL match every variant of every frozen `model.rs` enum with no catch-all arm,
so that adding a model variant breaks the build until it is characterized. Its first act SHALL mark
`MorphRuleDef::Compounding`, `MorphRuleOrder::Unordered`, `MprGroup`, and every not-yet-proven
configuration fail-closed.

#### Scenario: A new model variant breaks the build

- **WHEN** a new variant is added to a frozen-model enum
- **THEN** the characterizer fails to compile until the variant is given an explicit capability
  disposition

### Requirement: Capability predicates are conservative proof obligations

Each capability predicate SHALL return `Admit`, `ConfirmOnly`, or `Refuse` for a profile at a plan
node, and SHALL be conservative: it may over-refuse but SHALL never admit a configuration that could
omit a valid analysis. `ConfirmOnly` SHALL be returned whenever the construct is recall-preserving
only if the proposer proposes the superset (no proven no-false-negative admission filter). Every
`FailClosed` or `ConfigPredicate` characteristic SHALL be discharged by at least one registered
predicate, else the build breaks.

#### Scenario: Overwrite MPR group without a safe filter proof

- **WHEN** an `MprGroupOutput::Overwrite` group is present and no proof shows an FST admission filter
  cannot drop a valid analysis
- **THEN** the predicate returns `ConfirmOnly`, the proposer proposes the superset, and confirm
  prunes

#### Scenario: Simultaneous subrule overlap

- **WHEN** a simultaneous rewrite rule has two subrules whose environments can match at the same
  input position and MPR gates do not make them disjoint
- **THEN** the `simultaneous.subrule-overlap` predicate returns `Refuse` with a witness

### Requirement: Interactions are proven, not composed for free

At a composition node whose children carry independently-safe constructs, the envelope SHALL require
a proven interaction predicate (parallel-independence / critical-pair non-overlap; feeding/bleeding
disjointness as the phonological special case) before composing by union; absent the proof the node
SHALL fail closed. A node verdict SHALL be the meet of its children's verdicts and its own predicate,
with `Refuse` dominating and any `ConfirmOnly` demoting the subtree.

#### Scenario: Two safe branches with an unproven interaction

- **WHEN** two branches each pass their own predicates but their interaction at a union node is
  unproven
- **THEN** the union node fails closed until an interaction predicate proves non-interference

### Requirement: Capability override with an indelible degraded-trust signal

An explicit capability override SHALL force-compile a refused grammar, recording an indelible
unproven/recall-unsafe stamp (who, when, why, which configurations) in the pack manifest override
record and broadcasting a pack-level `unproven` load signal plus a per-analysis degraded-trust flag.
An overridden artifact SHALL never pass conformance; only genuine proof plus a clean recompile clears
the stamp.

#### Scenario: Overridden pack loads with a trust signal

- **WHEN** an overridden pack is loaded at runtime
- **THEN** the runtime broadcasts the `unproven` signal at load and flags every analysis result as
  degraded-trust

### Requirement: Supported status is gated on passing conformance coverage

A construct or configuration SHALL be markable as supported in the capability registry only when a
passing synthetic `machine/conformance/` fixture exercises it. CI SHALL cross-check the capability
registry against conformance coverage and break the build if anything is marked supported without a
covering, passing fixture.

#### Scenario: Supported without coverage breaks the build

- **WHEN** a configuration is marked supported in the capability registry but no passing conformance
  fixture exercises it
- **THEN** CI fails the build
