## ADDED Requirements

### Requirement: Adaptive approximate-first search
The optimizer SHALL begin with bounded search and SHALL select its search strategy from the
characterized feasible space, constraint structure, pilot costs, and caller budget. Strategy
selection and parameters SHALL be recorded and replayable.

#### Scenario: Feasible space is cheaply exhaustive
- **WHEN** the measured upper bound fits within one half of the caller's optimization budget
- **THEN** the optimizer exhaustively evaluates the feasible space and labels the result exact

#### Scenario: Feasible space exceeds the budget
- **WHEN** projected exhaustive cost exceeds the budget
- **THEN** the optimizer uses a deterministic approximate strategy and labels the result approximate

### Requirement: Search strategy is pluggable
The optimizer SHALL expose a common strategy interface supporting at least exhaustive enumeration,
constraint-guided diverse beam search, and one memoizing or branch-and-bound strategy.

#### Scenario: Register a new strategy
- **WHEN** a compatible strategy implementation is registered
- **THEN** policy selection can choose it without changing recipe-family materializers or reporting

### Requirement: Baseline and diversity are preserved
Every optimization SHALL include the current default plan as a baseline and SHALL attempt at least
two content-distinct realizable candidates whenever the grammar admits them.

#### Scenario: Grammar admits multiple families
- **WHEN** two or more content-distinct candidates pass static constraints
- **THEN** the evaluated set includes the baseline and at least one non-baseline candidate

#### Scenario: Grammar admits only one candidate
- **WHEN** all alternatives are proven inapplicable or inadmissible
- **THEN** the run succeeds with one candidate and reports the eliminating constraints

### Requirement: Only fully confirmed finalists are selectable
A candidate MUST be buildable and MUST agree with the full-HC oracle at analysis identity and
multiplicity level before it can be selected. Timeouts, truncation, unsupported constructs,
resource breaches, sampled recall, and partial corpora SHALL be non-certifying.

#### Scenario: Fast candidate loses an analysis
- **WHEN** a low-cost candidate disagrees with full HC on any analysis or multiplicity
- **THEN** it is excluded from selection and the shortest disagreement is reported

#### Scenario: Final confirmation cannot complete
- **WHEN** a finalist times out or breaches a resource budget during full confirmation
- **THEN** it is labeled unconfirmed and cannot replace the baseline

### Requirement: Deterministic Pareto selection
Among fully confirmed candidates, the optimizer SHALL compute the Pareto frontier over states,
arcs, build time, apply time, proposal count, and confirmation work. It SHALL choose
deterministically by minimizing states plus arcs, then build time, then apply time, then proposal
count, then confirmation work, then content address.

#### Scenario: Repeated run with identical inputs
- **WHEN** registry version, grammar, corpus, budgets, measurements, and seed are identical
- **THEN** the same candidate is selected and the same ordering is reported

