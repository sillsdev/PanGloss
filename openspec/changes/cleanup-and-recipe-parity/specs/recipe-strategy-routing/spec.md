## ADDED Requirements

### Requirement: Templated grammars are offered a template-aware underlying candidate
The recipe registry SHALL offer the `TemplatedUnderlyingTokens` whole-grammar strategy to any
grammar whose morphotactics are template-bearing (at least one affix template), regardless of
whether the grammar declares phonological rules. The applicability predicate SHALL be derived from
grammar structure, never from a grammar or language name.

#### Scenario: Templated, phonology-free grammar gets the token-cascade candidate
- **WHEN** the registry enumerates candidates for a synthetic grammar with affix templates and an
  empty `prules` list
- **THEN** the offered candidate set includes a `TemplatedUnderlyingTokens` whole-grammar
  candidate in addition to the plan-composed baseline

#### Scenario: Existing phonology-bearing offering is unchanged
- **WHEN** the registry enumerates candidates for a grammar with a non-empty `prules` list
- **THEN** the `TemplatedUnderlyingTokens` candidate is offered exactly as before this change

### Requirement: The self-looping underlying emitter is never the sole underlying model for a templated grammar
For a template-bearing grammar, the candidate set SHALL NOT consist only of plan-composed
candidates whose lexicon text comes from the deliberately-minimal self-looping emitter
(`uflexc`); at least one candidate SHALL carry template-aware morphotactic structure
(slot ordering and bounded slot occupancy).

#### Scenario: Templated fixture is not uflexc-only
- **WHEN** candidates are materialized for a synthetic template-bearing fixture
- **THEN** at least one materialized candidate's emission strategy is template-aware, and the
  evaluation report records which strategy was realized for the winner

### Requirement: Routing changes preserve proposer recall
Widening or re-routing strategy offerings SHALL NOT reduce end-to-end recall: on the conformance
fixtures, every analysis confirmed by the full-HC oracle before the change SHALL still be
proposed and confirmed after it, at analysis identity and multiplicity level.

#### Scenario: Conformance parity is unchanged by routing
- **WHEN** the full conformance suite runs after the routing change
- **THEN** pass/divergence results are identical to the pre-change baseline
