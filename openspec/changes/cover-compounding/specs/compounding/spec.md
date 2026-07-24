## ADDED Requirements

### Requirement: Compounding capability is split at configuration-predicate granularity

Compounding capability SHALL be evaluated as two distinct configuration predicates, never as one
blanket verdict for `MorphRuleDef::Compounding`: `compounding.non-recursive` (a `CompoundingRuleDef`
whose head/non-head stem search never admits the output of a `Compounding` application) and
`compounding.recursive` (self-feeding/nested compounding). Each predicate SHALL be independently
registered with the capability characteristics check and independently promotable.

#### Scenario: A non-recursive compounding rule is evaluated independently of a recursive one

- **WHEN** a grammar contains both a non-recursive compounding rule and a rule whose stem search can
  admit another compounding rule's output
- **THEN** the two configurations receive independent capability verdicts, and neither rule's verdict
  is inferred from the other's

### Requirement: Compounding proposal over-approximates by license only, never by pattern match

Where `compounding.non-recursive` proposal is implemented, it SHALL propose the full licensed cross
product of head-eligible and non-head-eligible stems for each subrule — gated only by
`head_prod_restrictions_mpr`/`non_head_prod_restrictions_mpr`/`output_prod_restrictions_mpr` and the
subrule's `required_mpr`/`excluded_mpr` — and SHALL NOT attempt to match `head_lhs`/`non_head_lhs`
patterns, `out_syn_fs`, or `obligatory_features` during proposal. Confirmation SHALL perform that
narrowing.

#### Scenario: A licensed pair fails the exact pattern match

- **WHEN** a head/non-head stem pair is licensed by the coarse propose-side gates but does not
  satisfy the subrule's `head_lhs`/`non_head_lhs` pattern match
- **THEN** proposal still emits the candidate, and confirmation rejects it

### Requirement: Compounding's rule-level MPR restrictions use group-unaware matching; subrule MPR gates use group-aware matching

`head_prod_restrictions_mpr`, `non_head_prod_restrictions_mpr`, and `output_prod_restrictions_mpr`
SHALL be evaluated with the group-unaware flat-intersect test (`MprSet::compound_match`). A
`CompoundingSubruleDef`'s `required_mpr`/`excluded_mpr` SHALL be evaluated with the group-aware
bucketed test (`mpr_group_ok`). Neither test SHALL be substituted for the other.

#### Scenario: A stem satisfies the flat-intersect restriction but not a group-aware reading

- **WHEN** a candidate stem's MPR features satisfy `compound_match` against a rule-level restriction
  set but would fail an `All`-type group-aware reading of the same set
- **THEN** the stem is still admitted, matching the flat-intersect semantics the rule-level
  restriction actually uses

### Requirement: Recursive compounding stays fail-closed pending a chain-depth interaction proof

`compounding.recursive` SHALL remain `FailClosed` until a proof characterizes its interaction with
the derivation-chain-depth budget. It SHALL NOT be promoted to `ConfirmOnly` or `Admit` by this
change.

#### Scenario: A grammar exercises self-feeding compounding

- **WHEN** a grammar's compounding rule can admit another compounding rule's output as a head or
  non-head stem
- **THEN** compilation fails closed for that configuration unless an explicit capability override is
  present

### Requirement: Compounding cost is measured and thresholded, never assumed

Before `compounding.non-recursive` is promoted to supported, its `Compose(head-trie, non_head-trie)`
plan node's `(states + arcs)` SHALL be measured or estimated against a proposed resource threshold. A
threshold breach SHALL warn, not hard-fail the capability verdict.

#### Scenario: Compose cost exceeds the proposed threshold

- **WHEN** a grammar's licensed head/non-head cross product exceeds the calibrated resource threshold
- **THEN** compilation reports a cost warning and still completes, distinct from any capability
  hard-fail

### Requirement: Compounding-into-template composition stays fail-closed until proven

A composition node combining a compounding rule's output with an affix-template slot SHALL require a
proven interaction predicate before being anything but fail-closed, independent of
`compounding.non-recursive`'s own verdict in isolation.

#### Scenario: A compounding rule's output feeds a template slot

- **WHEN** a `SlotDef` references a `Compounding` rule id and the grammar is otherwise
  `compounding.non-recursive`-eligible
- **THEN** the composition node still fails closed absent a proven interaction predicate for that
  node
