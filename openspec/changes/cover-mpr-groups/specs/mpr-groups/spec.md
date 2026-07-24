## ADDED Requirements

### Requirement: MPR-group capability is split at configuration-predicate granularity, by output policy

MPR-group capability SHALL be evaluated as two distinct configuration predicates, never as one
blanket verdict for `MprGroup`: `mpr-group.append-output` (every `MprGroup` a configuration touches
has `output == Append`) and `mpr-group.overwrite-output` (at least one touched `MprGroup` has
`output == Overwrite`). Each predicate SHALL be independently registered with the capability
characteristics check and independently promotable.

#### Scenario: A configuration touches both an Append-output and an Overwrite-output group

- **WHEN** a grammar's rule graph reaches one `MprGroup` with `Append` output and a separate
  `MprGroup` with `Overwrite` output
- **THEN** the two groups receive independent capability verdicts, and neither is inferred from the
  other

### Requirement: Append-output MPR-group proposal over-approximates by non-narrowing; admission-filtering is a distinct, unproven step

Where `mpr-group.append-output` proposal is implemented, it SHALL NOT use tracked accumulated
MPR-group state to reject a candidate unless a separate proof establishes that the admission filter
has no false negatives. Absent that proof, proposal SHALL leave every `required_mpr`/`excluded_mpr`
gate downstream of an `out_mpr`-bearing allomorph to confirmation.

#### Scenario: A candidate's accumulated MPR state would fail a later gate under a naive filter

- **WHEN** a candidate's over-approximate accumulated MPR set does not yet reflect an `out_mpr`
  addition confirmation would apply
- **THEN** proposal still emits the candidate, and confirmation performs the exact
  `mpr_group_ok`/`mpr_add_output` evaluation

### Requirement: Overwrite-output MPR groups never back an FST admission filter without a replace-semantics proof

`mpr-group.overwrite-output` SHALL remain `FailClosed` until a proof characterizes the group's
history-dependent replace semantics as a sound admission filter. A monotone-accumulation argument
(the basis for `mpr-group.append-output`'s eventual `Admit` candidacy) SHALL NOT be applied to an
`Overwrite`-policy group.

#### Scenario: A grammar's rule graph reaches an Overwrite-policy group

- **WHEN** a configuration's touched `MprGroup` set includes any group with `output == Overwrite`
- **THEN** compilation fails closed for that configuration's admission-filtering path unless an
  explicit capability override is present, independent of any `Append`-group verdict elsewhere in the
  grammar

### Requirement: Compounding's group-unaware rule-level restrictions are out of scope for these predicates

`MprSet::compound_match` (the group-unaware flat-intersect test used for `CompoundingRuleDef`'s
rule-level restriction fields) SHALL NOT be evaluated by `mpr-group.append-output` or
`mpr-group.overwrite-output`. These predicates SHALL apply only to consumption sites that evaluate
`required_mpr`/`excluded_mpr`/`out_mpr` through the group-aware
`mpr_group_buckets`/`mpr_required_ok`/`mpr_excluded_ok`/`mpr_add_output` helpers.

#### Scenario: A compounding rule's rule-level restriction is evaluated

- **WHEN** a `CompoundingRuleDef`'s `head_prod_restrictions_mpr`, `non_head_prod_restrictions_mpr`, or
  `output_prod_restrictions_mpr` is evaluated against a candidate stem
- **THEN** that evaluation uses `compound_match` and is unaffected by either MPR-group predicate's
  verdict

### Requirement: Order-(in)dependence of accumulated MPR-group state is characterized before combining with unordered rule application

An `Append`-policy group's accumulated state SHALL be treated as order-invariant across any
admissible rule ordering. An `Overwrite`-policy group's accumulated state SHALL NOT be treated as
order-invariant: a configuration combining `MorphRuleOrder::Unordered` with a touched `Overwrite`
group SHALL require its own proven interaction predicate, independent of `mpr-group.overwrite-output`'s
own verdict in isolation.

#### Scenario: An Unordered stratum's rules touch only Append-policy groups

- **WHEN** every `MprGroup` reachable from an `Unordered` stratum's rules has `output == Append`
- **THEN** the accumulated MPR state is identical across every admissible ordering, and the
  ordering-union proposal composes with `mpr-group.append-output`'s verdict without an additional
  interaction predicate

#### Scenario: An Unordered stratum's rules touch an Overwrite-policy group

- **WHEN** an `Unordered` stratum's rules can reach an `MprGroup` with `output == Overwrite`
- **THEN** compilation fails closed for that configuration absent a proven interaction predicate
  characterizing the combination of ordering multiplicity and history-dependent state
