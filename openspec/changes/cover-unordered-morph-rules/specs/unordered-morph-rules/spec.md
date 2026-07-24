## ADDED Requirements

### Requirement: Unordered morphological rule application capability is split at a chain-depth cardinality bound

Unordered morphological rule application capability SHALL be evaluated as two distinct configuration
predicates, never as one blanket verdict for `MorphRuleOrder::Unordered`:
`unordered-application.chain-depth-bounded` (a stratum whose rule count and derivation-chain-depth
stay within a calibrated joint bound) and `unordered-application.unbounded` (a stratum exceeding that
bound, or for which no bound has yet been calibrated). Each predicate SHALL be independently
registered with the capability characteristics check and independently promotable.

#### Scenario: A grammar contains both a small and a large Unordered stratum

- **WHEN** a grammar declares one `Unordered` stratum within the calibrated bound and another
  exceeding it
- **THEN** the two strata receive independent capability verdicts, and neither is inferred from the
  other

### Requirement: Unordered proposal over-approximates by the union over admissible orderings, never a single fixed order

Where `unordered-application.chain-depth-bounded` proposal is implemented, it SHALL propose the union
of every ordering and subset of the stratum's rules admissible under the engine's own
any-order/any-subset application semantics, mirroring the exact reachable-derivation set of the
combination-cascade confirm path. It SHALL NOT restrict proposal to a single fixed rule order.

#### Scenario: A surface form is reachable only via a non-document-order application sequence

- **WHEN** a word's analysis requires the stratum's rules to have applied in an order other than
  their declared document order
- **THEN** proposal still emits a candidate covering that ordering, and confirmation verifies it
  against the exact combination-cascade semantics

### Requirement: The existing morphotactic-legality over-approximation is not treated as a proposal-language proof

The existing composite-chain legality automaton's convention of exploring rule-attachment sequences
without enforcing document order SHALL NOT, by itself, be treated as satisfying
`unordered-application.chain-depth-bounded`'s proposal requirement. That convention characterizes
chain-attachment legality only; a separate proof SHALL establish that the resulting composed
proposal's language equals the union over every admissible ordering's surface output.

#### Scenario: A grammar's composite chain builder already explores non-document-order attachments

- **WHEN** a grammar's composite/structural proposal path already recurses over stratum rules without
  enforcing a non-decreasing rule index
- **THEN** `unordered-application.chain-depth-bounded` still requires its own proposal-language proof
  before promotion, independent of that pre-existing legality convention

### Requirement: Unordered application's chain-depth multiplication is a required ADR 0003 budget extension, not an open question

Before `unordered-application.chain-depth-bounded` is promoted to supported, the ADR 0003
derivation-chain-depth budget SHALL be extended with a calibrated dimension accounting for the
ordering-multiplicity of `Unordered` application, distinct from ordinary chain length.

#### Scenario: A stratum's rule count makes ordering multiplication the binding constraint

- **WHEN** an `Unordered` stratum's admissible-ordering count exceeds the calibrated ordering-
  multiplicity bound even though its chain length alone would be within the plain chain-depth budget
- **THEN** compilation reports the ordering-multiplicity dimension as the named blocking dimension, not
  the plain chain-depth one

### Requirement: Order-dependence of Overwrite-policy MPR-group state requires a proven interaction predicate

A configuration combining `MorphRuleOrder::Unordered` with rules that can reach an `MprGroup` whose
`output` is `Overwrite` SHALL require a proven interaction predicate characterizing the combination of
ordering multiplicity and history-dependent group state, independent of
`unordered-application.chain-depth-bounded`'s own verdict in isolation. A configuration whose reachable
`MprGroup`s are all `Append`-policy SHALL NOT require this additional interaction predicate.

#### Scenario: An Unordered stratum's rules can reach only Append-policy MPR groups

- **WHEN** every `MprGroup` reachable from an `Unordered` stratum's rules has `output == Append`
- **THEN** the ordering-union proposal composes with the MPR-group verdict without requiring a
  separate interaction predicate

#### Scenario: An Unordered stratum's rules can reach an Overwrite-policy MPR group

- **WHEN** an `Unordered` stratum's rules can reach an `MprGroup` with `output == Overwrite`
- **THEN** compilation fails closed for that configuration absent a proven interaction predicate
