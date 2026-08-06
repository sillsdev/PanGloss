## ADDED Requirements

### Requirement: Unbounded quantifiers compile exactly, never as a finite cutoff
A genuinely unbounded quantifier SHALL compile through the backend's own native
unbounded-repetition operators, producing a relation that accepts arbitrarily many occurrences. A
finite expansion SHALL NOT be substituted for it, and the finite-bound preflight ceiling SHALL NOT be
applied to it.

#### Scenario: An unbounded repetition accepts counts beyond the finite ceiling
- **WHEN** a rule declares a repetition with no upper bound
- **THEN** the compiled relation accepts occurrence counts exceeding the finite-bound preflight
  ceiling, and proposer-to-confirm results match the oracle

#### Scenario: A minimum occurrence count is respected exactly
- **WHEN** an unbounded repetition declares a minimum of N occurrences
- **THEN** the compiled relation rejects N-1 occurrences and accepts both N and N+1

### Requirement: An unbounded quantifier alone no longer refuses a grammar
A rule whose only unsupported characteristic was an unbounded quantifier SHALL reach confirm-only
admission rather than refusal, and the capability diagnostic SHALL stop naming that construct as a
cause.

#### Scenario: A previously refused construct is admitted
- **WHEN** a grammar's sole quantifier-related obstacle is an absent upper bound
- **THEN** the capability envelope reports confirm-only for that construct instead of refuse

### Requirement: A right-to-left mirror reverses repetition contents
Building the mirror of a right-to-left rule SHALL reverse the contents of a repetition group as well
as the order of sibling slots, so the mirror's language is the reverse of the original's.

#### Scenario: A repetition group's contents are not palindromic
- **WHEN** a right-to-left rule's environment contains a repetition over two or more distinct
  elements
- **THEN** the mirror requires those elements in reversed order, and the compiled relation matches
  what the rule declares
