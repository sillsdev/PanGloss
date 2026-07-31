## ADDED Requirements

### Requirement: No unconstructible strategy variants
The plan vocabulary SHALL contain only strategy variants that at least one builder can construct.
`ComposeStrategy::Lazy` and `ComposeStrategy::LazyLookahead` SHALL be removed, along with their
rendering labels and the panic guards that existed only to reject them.

#### Scenario: Compile-time exhaustiveness after removal
- **WHEN** the workspace builds after the removal
- **THEN** every `match` over `ComposeStrategy` is exhaustive without dead arms, and no
  diagram/coverage label for lazy composition remains

### Requirement: Dead report signals are not presented as live
`SearchAccounting.pruned` SHALL either be driven by a real admissible bound (branch-and-bound
with a wired `exact_objective`) or be explicitly documented-and-asserted as structurally zero in
production, so no reader or downstream tool can mistake it for a measured signal.

#### Scenario: The pruned field cannot silently mislead
- **WHEN** a production optimizer run completes
- **THEN** either `pruned` reflects genuine bound-based pruning, or a test asserts it is zero and
  the report schema documents why

### Requirement: Family identities are compiler-checked at decision sites
Decision sites that select or order candidates by family (baseline detection, sort order) SHALL
reference shared constants or an enum defined next to the registry's seed table, so a family
rename fails to compile rather than silently changing behavior.

#### Scenario: Rename breaks the build, not the sort
- **WHEN** a registry family identifier is renamed
- **THEN** every decision site referencing it fails to compile until updated

### Requirement: Failure scores share one constructor
All evaluation outcomes that produce zeroed/failure `Score` values SHALL construct them through
the shared constructor so a new score field cannot be silently omitted from a failure path.

#### Scenario: New score field propagates to failure paths
- **WHEN** a field is added to `Score`
- **THEN** failure-path construction compiles only after the shared constructor accounts for it
  (no duplicated inline literals remain)

### Requirement: Superseded plan documents say so
`docs/fst-plan/large-lexicon-proposal-explosion.md` SHALL carry a superseded header pointing at
the landed fix, and `docs/fst-plan/four-grammar-recipe-evidence-2026-07-28.md` SHALL carry a
historical banner disambiguating its four synthetic fixtures from the four language corpora.

#### Scenario: A reader cannot mistake stale numbers for current
- **WHEN** either document is opened
- **THEN** its first visible section states its status and links the superseding evidence
